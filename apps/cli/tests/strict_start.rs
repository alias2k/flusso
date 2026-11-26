//! `flusso run --strict` (issue #485) through the real binary over a real
//! Postgres: an error `flusso check` fails on but that does not stop a plain
//! start (a `text[]` column read without `array: true`) refuses a strict
//! start, from the config and from a `--locked` lock alike, with `check`'s
//! report, before anything is staged (no slot, no publication, the replica
//! identity untouched); a plain start goes on, logs the error at `error` with
//! its index, kind and field, and sets `flusso.index.schema_errors`. A gap in
//! capture `run` won't close is refused and reported the same way.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(strict_start)'
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const SETUP: &str = "CREATE TABLE t (id int PRIMARY KEY, tags text[]);
INSERT INTO t VALUES (1, ARRAY['a', 'b'])";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn strict_refuses_an_error_a_plain_start_goes_on_with() {
    let (_postgres, url) = common::start_postgres(SETUP).await;
    let dir = project(&url, "strict");

    let checked = flusso(&dir, &["check", "--config", "flusso.toml"], &[]);
    assert!(!checked.status.success(), "{}", text(&checked));
    assert!(
        text(&checked).contains("[error] t.tags"),
        "{}",
        text(&checked)
    );

    for args in [
        &["--config", "flusso.toml"][..],
        &["--locked", "--lock", "flusso.lock"][..],
    ] {
        let ran = run(&dir, args, &[("FLUSSO_STRICT", "true")]).await;
        let ran_text = text(&ran);
        assert!(!ran.status.success(), "{args:?}: {ran_text}");
        assert!(ran_text.contains("[error] t.tags"), "{args:?}: {ran_text}");
        assert!(
            ran_text.contains("refused under --strict"),
            "{args:?}: {ran_text}"
        );
    }
    let flagged = run(&dir, &["--config", "flusso.toml", "--strict"], &[]).await;
    assert!(!flagged.status.success(), "{}", text(&flagged));

    let wanted = r#"flusso_index_schema_errors{index="t",kind="undeclared_array_column"} 1"#;
    let (metrics, seen) = plain_start(&dir, wanted, None).await;
    assert!(
        metrics.contains(wanted),
        "{wanted} missing from:\n{metrics}"
    );
    let event = seen
        .lines()
        .find(|line| line.contains(r#""kind":"undeclared_array_column""#))
        .unwrap_or_else(|| panic!("no error event for t.tags in:\n{seen}"));
    let event: serde_json::Value = serde_json::from_str(event).unwrap();
    assert_eq!(event["level"], "ERROR", "{event}");
    assert_eq!(event["fields"]["index"], "t", "{event}");
    assert_eq!(event["fields"]["field"], "tags", "{event}");
    assert!(seen.contains("starting anyway"), "{seen}");
}

/// A plain `flusso run` of `dir`, logging JSON, stopped once `/metrics`
/// shows `wanted` and, when `slot` names a database and a slot, that slot
/// exists (so `prepare` has run), or after 60 s: what `/metrics` last showed,
/// and every line it logged.
async fn plain_start(dir: &Path, wanted: &str, slot: Option<(&str, &str)>) -> (String, String) {
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(dir)
        .args([
            "run",
            "--config",
            "flusso.toml",
            "--public-address",
            &format!("127.0.0.1:{port}"),
            "--private-address",
            "127.0.0.1:0",
        ])
        .env("FLUSSO_LOG_FORMAT", "json")
        .env_remove("FLUSSO_STRICT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let logged = std::thread::spawn(move || {
        let mut seen = String::new();
        for line in BufReader::new(stderr).lines() {
            seen.push_str(&line.unwrap());
            seen.push('\n');
        }
        seen
    });

    let pool = match slot {
        Some((url, _)) => Some(
            sqlx::postgres::PgPoolOptions::new()
                .connect(url)
                .await
                .unwrap(),
        ),
        None => None,
    };
    let http = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut metrics = String::new();
    let mut prepared = slot.is_none();
    while Instant::now() < deadline && !(metrics.contains(wanted) && prepared) {
        if let Ok(response) = http
            .get(format!("http://127.0.0.1:{port}/metrics"))
            .send()
            .await
        {
            metrics = response.text().await.unwrap_or_default();
        }
        if let (Some(pool), Some((_, name))) = (&pool, slot) {
            let slots: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_replication_slots WHERE slot_name = $1",
            )
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
            prepared = slots > 0;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    child.kill().unwrap();
    child.wait().unwrap();
    (metrics, logged.join().unwrap())
}

/// A project of one index over `t`, reading `tags` as one keyword, writing
/// to stdout through slot `s_<name>` and publication `p_<name>`, with its
/// lock built.
fn project(url: &str, name: &str) -> PathBuf {
    project_with(url, name, "  - keyword: tags\n    required: false\n", "")
}

/// A project of one index over `t` whose `tags` field is `tags`, with
/// `source` added to its `[source]` table.
fn project_with(url: &str, name: &str, tags: &str, source: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        format!(
            "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n{tags}"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"{url}\"\nslot = \"s_{name}\"\n\
             publication = \"p_{name}\"\n{source}\n[sinks.out]\ntype = \"stdout\"\n\n[[index]]\n\
             name = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
        ),
    )
    .unwrap();
    let built = flusso(&dir, &["build", "--config", "flusso.toml"], &[]);
    assert!(built.status.success(), "{}", text(&built));
    dir
}

/// A strict refusal comes before anything is staged: no replication slot,
/// no publication, and the table keeps its default replica identity, from a
/// config and from a `--locked` lock alike.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_strict_refusal_creates_no_slot_or_publication() {
    let (_postgres, url) = common::start_postgres(SETUP).await;
    let dir = project(&url, "untouched");
    for args in [
        &["--config", "flusso.toml", "--strict"][..],
        &["--locked", "--lock", "flusso.lock", "--strict"][..],
    ] {
        let ran = run(&dir, args, &[]).await;
        assert!(!ran.status.success(), "{args:?}: {}", text(&ran));
        assert_untouched(&url, "untouched", args).await;
    }
}

/// A gap in capture `flusso run` won't close (issue #485): with
/// `manage_publication = false` and no publication, no change to `t` would
/// reach its index. `check` fails on it; `run --strict` refuses before
/// anything is staged; a plain start goes on, logs the gap once as an `error`
/// event (`prepare` doesn't warn about it again) and sets
/// `flusso.index.schema_errors` to `publication_gap`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_gap_run_wont_close_fails_check_and_a_strict_start() {
    let (_postgres, url) = common::start_postgres(SETUP).await;
    let dir = project_with(
        &url,
        "gap",
        "  - keyword: tags\n    array: true\n    required: false\n",
        "manage_publication = false\n",
    );

    let checked = flusso(&dir, &["check", "--config", "flusso.toml"], &[]);
    assert!(!checked.status.success(), "{}", text(&checked));
    assert!(
        text(&checked).contains("[error] public.t  `public.t` is not in the publication"),
        "{}",
        text(&checked)
    );
    let json = flusso(
        &dir,
        &["check", "--config", "flusso.toml", "--format", "json"],
        &[],
    );
    let document: serde_json::Value =
        serde_json::from_slice(&json.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&json)));
    assert_eq!(document["gaps"][0]["kind"], "publication_gap", "{document}");
    assert_eq!(document["coverage"]["status"], "problem", "{document}");
    assert_eq!(document["summary"]["errors"], 1, "{document}");

    for args in [
        &["--config", "flusso.toml", "--strict"][..],
        &["--locked", "--lock", "flusso.lock", "--strict"][..],
    ] {
        let ran = run(&dir, args, &[]).await;
        let ran_text = text(&ran);
        assert!(!ran.status.success(), "{args:?}: {ran_text}");
        assert!(
            ran_text.contains("1 gap(s) above, refused under --strict"),
            "{args:?}: {ran_text}"
        );
        assert_untouched(&url, "gap", args).await;
    }

    let wanted = r#"flusso_index_schema_errors{index="t",kind="publication_gap"} 1"#;
    let (metrics, seen) = plain_start(&dir, wanted, Some((&url, "s_gap"))).await;
    assert!(
        metrics.contains(wanted),
        "{wanted} missing from:\n{metrics}"
    );
    let events: Vec<&str> = seen
        .lines()
        .filter(|line| line.contains(r#""kind":"publication_gap""#))
        .collect();
    assert_eq!(events.len(), 1, "one error event for the gap in:\n{seen}");
    assert!(
        !seen.contains("publication is missing tables"),
        "prepare warned about the gap the start reported:\n{seen}"
    );
}

/// No slot `s_<name>`, no publication `p_<name>`, and `t` still at its
/// default replica identity: nothing was staged.
async fn assert_untouched(url: &str, name: &str, args: &[&str]) {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(url)
        .await
        .unwrap();
    let slots: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_replication_slots WHERE slot_name = $1")
            .bind(format!("s_{name}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let publications: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_publication WHERE pubname = $1")
            .bind(format!("p_{name}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let identity: String =
        sqlx::query_scalar("SELECT relreplident::text FROM pg_class WHERE oid = 't'::regclass")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(slots, 0, "{args:?}: a slot was created");
    assert_eq!(publications, 0, "{args:?}: a publication was created");
    assert_eq!(identity, "d", "{args:?}: the replica identity was changed");
    pool.close().await;
}

/// `flusso run` with `args`, expected to exit on its own.
async fn run(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let dir = dir.to_owned();
    let mut all = vec![
        "run",
        "--public-address",
        "127.0.0.1:0",
        "--private-address",
        "127.0.0.1:0",
    ];
    all.extend_from_slice(args);
    let all: Vec<String> = all.into_iter().map(str::to_owned).collect();
    let env: Vec<(String, String)> = env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let ran = tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = all.iter().map(String::as_str).collect();
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        flusso(&dir, &args, &env)
    });
    tokio::time::timeout(Duration::from_secs(60), ran)
        .await
        .expect("run refuses at start instead of running")
        .unwrap()
}

fn flusso(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_flusso"));
    command
        .current_dir(dir)
        .args(args)
        .env_remove("FLUSSO_STRICT")
        .stdin(Stdio::null());
    for (var, value) in env {
        command.env(var, value);
    }
    command.output().unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
