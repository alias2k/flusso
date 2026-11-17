//! A fenced read rides out a restart of flusso (issue #377): the real `flusso
//! run` binary over a real Postgres and OpenSearch, read through
//! `flusso-query`.
//!
//! A row is committed and flusso stopped at once; a second row is committed
//! while nothing listens on flusso's address. Fenced reads are issued in the
//! gap: a fetch by id for each row (applied) and a count after the second
//! (searchable). flusso starts again seconds later on the same address; every
//! read returns the committed state, which for the second row only the new
//! process can have indexed.
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

use flusso_query::{Client, Search, position};
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn fenced_reads_made_while_flusso_restarts_return_the_committed_rows() {
    let (_postgres, pg_url) =
        common::start_postgres("CREATE TABLE t (id int PRIMARY KEY, name text NOT NULL)").await;
    let (_opensearch, os_url) = common::start_opensearch().await;
    let pool = PgPoolOptions::new().connect(&pg_url).await.unwrap();
    let dir = project(&pg_url, &os_url);
    let public = free_address();

    let mut flusso = start(&dir, public);
    ready(public, &mut flusso, &dir).await;
    let hash = schema_hash(&os_url).await;
    let before = commit(&pool, 0, "committed before the stop").await;
    stop(&mut flusso);
    let during = commit(&pool, 1, "committed while flusso was down").await;

    let client = Client::connect(&os_url)
        .unwrap()
        .flusso(format!("http://{public}"))
        .unwrap()
        .fence_sink("primary")
        .fence_timeout(Duration::from_secs(90));
    let started = Instant::now();
    let get = |position: String, id: i32| {
        let (client, hash) = (client.clone(), hash.clone());
        tokio::spawn(async move {
            client
                .after(position)
                .get_one::<Value>("t", &hash, id)
                .await
        })
    };
    let get_before = get(before, 0);
    let get_during = get(during.clone(), 1);
    let count_during = {
        let (client, hash) = (client.clone(), hash.clone());
        tokio::spawn(async move {
            Search::<Value>::new("t", &hash)
                .count(&client.after(during))
                .await
        })
    };

    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        !get_during.is_finished() && !count_during.is_finished(),
        "both levels wait through the gap"
    );
    let mut flusso = start(&dir, public);

    let within = Duration::from_secs(120);
    let get_before = tokio::time::timeout(within, get_before).await;
    let get_during = tokio::time::timeout(within, get_during).await;
    let count_during = tokio::time::timeout(within, count_during).await;
    let elapsed = started.elapsed();
    stop(&mut flusso);
    let log = stderr(&dir);
    let get_before = get_before
        .expect("returns")
        .unwrap()
        .unwrap_or_else(|e| panic!("{e}: {log}"));
    let get_during = get_during
        .expect("returns")
        .unwrap()
        .unwrap_or_else(|e| panic!("{e}: {log}"));
    let count_during = count_during
        .expect("returns")
        .unwrap()
        .unwrap_or_else(|e| panic!("{e}: {log}"));

    assert_eq!(
        get_before,
        Some(json!({ "id": 0, "name": "committed before the stop" })),
        "applied, a position committed before the stop"
    );
    assert_eq!(
        get_during,
        Some(json!({ "id": 1, "name": "committed while flusso was down" })),
        "applied, a position committed in the gap"
    );
    assert_eq!(count_during, 2, "searchable: a search sees both rows");
    assert!(elapsed >= Duration::from_secs(3), "{elapsed:?}");
}

/// Insert a row and return the source position read after its commit.
async fn commit(pool: &PgPool, id: i32, name: &str) -> String {
    sqlx::query("INSERT INTO t VALUES ($1, $2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query_scalar(position::CURRENT_SQL)
        .fetch_one(pool)
        .await
        .unwrap()
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
            "--shutdown-timeout-secs",
            "10",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .unwrap()
}

/// Stop flusso as a deploy does, with `SIGTERM`, and wait for it to exit: its
/// drain is given 10 s (`start`), the kill comes well after.
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
