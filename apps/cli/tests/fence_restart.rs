//! A fenced read rides out a restart of flusso (issue #377): the real `flusso
//! run` binary over a real Postgres and OpenSearch, read through
//! `flusso-query`.
//!
//! flusso is stopped, a row is committed and its position read, and a fenced
//! read is issued while nothing listens on flusso's address. flusso starts
//! again seconds later on the same address; the read returns the committed
//! document, which only the new process can have indexed.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(fence_restart)'
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use flusso_query::{Client, position};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_fenced_read_made_while_flusso_restarts_returns_the_committed_row() {
    let (_postgres, pg_url) =
        common::start_postgres("CREATE TABLE t (id int PRIMARY KEY, name text NOT NULL)").await;
    let (_opensearch, os_url) = common::start_opensearch().await;
    let pool = PgPoolOptions::new().connect(&pg_url).await.unwrap();
    let dir = project(&pg_url, &os_url);
    let public = free_address();

    let mut flusso = start(&dir, public);
    ready(public, &mut flusso, &dir).await;
    let hash = schema_hash(&os_url).await;
    stop(&mut flusso);

    sqlx::query("INSERT INTO t VALUES (1, 'committed while flusso was down')")
        .execute(&pool)
        .await
        .unwrap();
    let position: String = sqlx::query_scalar(position::CURRENT_SQL)
        .fetch_one(&pool)
        .await
        .unwrap();

    let client = Client::connect(&os_url)
        .unwrap()
        .flusso(format!("http://{public}"))
        .unwrap()
        .fence_sink("primary")
        .fence_timeout(Duration::from_secs(90));
    let started = Instant::now();
    let read =
        tokio::spawn(async move { client.after(position).get_one::<Value>("t", &hash, 1).await });

    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(!read.is_finished(), "the read waits through the gap");
    let mut flusso = start(&dir, public);

    let row = tokio::time::timeout(Duration::from_secs(120), read)
        .await
        .expect("the read returns once flusso is back")
        .unwrap();
    let elapsed = started.elapsed();
    stop(&mut flusso);
    let row = row.unwrap_or_else(|error| panic!("{error}: {}", stderr(&dir)));
    assert_eq!(
        row,
        Some(json!({ "id": 1, "name": "committed while flusso was down" }))
    );
    assert!(elapsed >= Duration::from_secs(3), "{elapsed:?}");
}

/// One index `t` written to the OpenSearch sink `primary`.
fn project(pg_url: &str, os_url: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-fence-restart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - keyword: name\n    required: true\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"{pg_url}\"\nslot = \"s_restart\"\n\
             publication = \"p_restart\"\n\n[sinks.primary]\ntype = \"opensearch\"\nurl = \"{os_url}\"\n\
             number_of_replicas = 0\n\n[[index]]\nname = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
        ),
    )
    .unwrap();
    dir
}

/// An address nothing listens on, for both runs of flusso to bind.
fn free_address() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

fn start(dir: &Path, public: SocketAddr) -> Child {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("stderr.log"))
        .unwrap();
    Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(dir)
        .args([
            "run",
            "--config",
            "flusso.toml",
            "--public-address",
            &public.to_string(),
            "--private-address",
            "127.0.0.1:0",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .unwrap()
}

/// Stop flusso as a deploy does, with `SIGTERM`, and wait for it to exit.
fn stop(flusso: &mut Child) {
    let _ = Command::new("kill")
        .args(["-TERM", &flusso.id().to_string()])
        .status();
    let deadline = Instant::now() + Duration::from_secs(30);
    while flusso.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            let _ = flusso.kill();
            let _ = flusso.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn ready(public: SocketAddr, flusso: &mut Child, dir: &Path) {
    let http = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        assert!(
            flusso.try_wait().unwrap().is_none(),
            "flusso exited: {}",
            stderr(dir)
        );
        let ok = http
            .get(format!("http://{public}/readyz"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if ok {
            return;
        }
        assert!(Instant::now() < deadline, "never ready: {}", stderr(dir));
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The schema hash of index `t`, from its hash alias `t_<hash>`.
async fn schema_hash(os_url: &str) -> String {
    let aliases: serde_json::Value = reqwest::get(format!("{os_url}/_alias"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    aliases
        .as_object()
        .unwrap()
        .values()
        .filter_map(|index| index["aliases"].as_object())
        .flat_map(|aliases| aliases.keys())
        .find_map(|alias| alias.strip_prefix("t_"))
        .expect("a hash alias t_<hash>")
        .to_owned()
}

fn stderr(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("stderr.log")).unwrap_or_default()
}
