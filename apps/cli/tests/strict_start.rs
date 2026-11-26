//! `flusso run --strict` (issue #485) through the real binary over a real
//! Postgres: an error `flusso check` fails on but that does not stop a plain
//! start (a `text[]` column read without `array: true`) refuses a strict
//! start, from the config and from a `--locked` lock alike, with `check`'s
//! report; a plain start goes on, logs the error at `error` with its index,
//! kind and field, and sets `flusso.index.schema_errors`.
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
    let dir = project(&url);

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

    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(&dir)
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
            let line = line.unwrap();
            seen.push_str(&line);
            seen.push('\n');
            if line.contains(r#""kind":"undeclared_array_column""#) {
                return (true, seen);
            }
        }
        (false, seen)
    });

    let http = reqwest::Client::new();
    let wanted = r#"flusso_index_schema_errors{index="t",kind="undeclared_array_column"} 1"#;
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut metrics = String::new();
    while Instant::now() < deadline && !metrics.contains(wanted) {
        if let Ok(response) = http
            .get(format!("http://127.0.0.1:{port}/metrics"))
            .send()
            .await
        {
            metrics = response.text().await.unwrap_or_default();
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let (found, seen) = logged.join().unwrap();
    assert!(
        metrics.contains(wanted),
        "{wanted} missing from:\n{metrics}"
    );
    assert!(found, "no error event for t.tags in:\n{seen}");
    let event = seen
        .lines()
        .find(|line| line.contains(r#""kind":"undeclared_array_column""#))
        .unwrap();
    let event: serde_json::Value = serde_json::from_str(event).unwrap();
    assert_eq!(event["level"], "ERROR", "{event}");
    assert_eq!(event["fields"]["index"], "t", "{event}");
    assert_eq!(event["fields"]["field"], "tags", "{event}");
    assert!(seen.contains("starting anyway"), "{seen}");
}

/// A project of one index over `t`, reading `tags` as one keyword, writing
/// to stdout, with its lock built.
fn project(url: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-strict-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - keyword: tags\n    required: false\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"{url}\"\nslot = \"s_strict\"\n\
             publication = \"p_strict\"\n\n[sinks.out]\ntype = \"stdout\"\n\n[[index]]\n\
             name = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
        ),
    )
    .unwrap();
    let built = flusso(&dir, &["build", "--config", "flusso.toml"], &[]);
    assert!(built.status.success(), "{}", text(&built));
    dir
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
