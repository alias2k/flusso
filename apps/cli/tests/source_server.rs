//! `flusso check` and `flusso run` judge the source server alike (issue #343):
//! a server that would make flusso a synchronous standby is refused by both,
//! with the same message, and a plain primary passes both.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(source_server)'
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn check_and_run_refuse_a_server_that_would_make_flusso_a_synchronous_standby() {
    let (_postgres, url) =
        common::start_postgres("CREATE TABLE t (id int PRIMARY KEY, name text)").await;
    let dir = project(&url, "standby");

    let passed = flusso(&dir, &["check", "--config", "flusso.toml"]);
    assert!(
        passed.status.success(),
        "a plain primary passes check: {}",
        text(&passed)
    );

    standby_names(&url, "ANY 1 (*)").await;
    let checked = flusso(&dir, &["check", "--config", "flusso.toml"]);
    let checked_text = text(&checked);
    assert!(
        !checked.status.success(),
        "check fails where run refuses: {checked_text}"
    );
    assert!(
        checked_text.contains("synchronous_standby_names = 'ANY 1 (*)'")
            && checked_text.contains("Replace '*' with the names of the synchronous standbys"),
        "check names the setting and the remedy: {checked_text}"
    );
    assert!(
        !checked_text.contains("check passed"),
        "check does not say it passed: {checked_text}"
    );

    let json = flusso(
        &dir,
        &["check", "--config", "flusso.toml", "--format", "json"],
    );
    let stdout = String::from_utf8_lossy(&json.stdout);
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("a JSON document");
    assert_eq!(document["server"]["ok"], false, "{document}");
    assert!(
        document["server"]["problem"]
            .as_str()
            .unwrap()
            .contains("synchronous_standby_names"),
        "{document}"
    );

    let ran = tokio::task::spawn_blocking(move || {
        flusso(
            &dir,
            &[
                "run",
                "--config",
                "flusso.toml",
                "--public-address",
                "127.0.0.1:0",
                "--private-address",
                "127.0.0.1:0",
            ],
        )
    });
    let ran = tokio::time::timeout(Duration::from_secs(60), ran)
        .await
        .expect("run refuses at start instead of running")
        .unwrap();
    let ran_text = text(&ran);
    assert!(!ran.status.success(), "{ran_text}");
    assert!(
        ran_text.contains("synchronous_standby_names = 'ANY 1 (*)'"),
        "run names the setting: {ran_text}"
    );
}

/// A project of one index over `t`, writing to stdout.
fn project(url: &str, name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-server-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - keyword: name\n    required: false\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"{url}\"\nslot = \"s_{name}\"\n\
             publication = \"p_{name}\"\n\n[sinks.out]\ntype = \"stdout\"\n\n[[index]]\n\
             name = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
        ),
    )
    .unwrap();
    dir
}

fn flusso(dir: &PathBuf, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Set `synchronous_standby_names` and wait until a new session reads it.
async fn standby_names(url: &str, names: &str) {
    let pool = PgPoolOptions::new().connect(url).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER SYSTEM SET synchronous_standby_names = '{names}'"
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("SELECT pg_reload_conf()")
        .execute(&pool)
        .await
        .unwrap();
    for _ in 0..100 {
        let fresh = PgPoolOptions::new()
            .max_connections(1)
            .connect(url)
            .await
            .unwrap();
        let shown: String = sqlx::query_scalar("SHOW synchronous_standby_names")
            .fetch_one(&fresh)
            .await
            .unwrap();
        if shown == names {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("synchronous_standby_names never read '{names}'");
}
