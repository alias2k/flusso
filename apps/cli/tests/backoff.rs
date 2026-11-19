#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

//! `[backoff]` through the real binary, no database: `check`, `build` and
//! `run` resolve and judge it with the one function, and a value says where
//! it came from. Each command runs as a child process with its own
//! environment, so setting `FLUSSO_BACKOFF_*` here races with nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A project whose `flusso.toml` ends with `backoff` (a `[backoff]` table or
/// nothing).
fn project(name: &str, backoff: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("backoff")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://nobody@127.0.0.1:1/none\"\n\n\
             [sinks.out]\ntype = \"stdout\"\n\n[[index]]\nname = \"t\"\nschema = \"t.schema.yml\"\n\
             enabled = true\n\n{backoff}"
        ),
    )
    .unwrap();
    dir
}

fn flusso(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_flusso"));
    command
        .current_dir(dir)
        .args(args)
        .env_remove("FLUSSO_BACKOFF_INITIAL_MS")
        .env_remove("FLUSSO_BACKOFF_MAX_MS")
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

const RUN: &[&str] = &[
    "run",
    "--public-address",
    "127.0.0.1:0",
    "--private-address",
    "127.0.0.1:0",
];

/// A file `run` refuses, `check` and `build` refuse with the same words.
#[test]
fn check_build_and_run_refuse_the_same_file_alike() {
    let dir = project("inverted", "[backoff]\ninitial_ms = 200\nmax_ms = 100\n");
    let said = "[backoff] max_ms = 100 (set in flusso.toml's [backoff] max_ms) is below \
                initial_ms = 200 (set in flusso.toml's [backoff] initial_ms)";
    for args in [&["check", "--offline"][..], &["build"][..], RUN] {
        let output = flusso(&dir, args, &[]);
        assert!(!output.status.success(), "{args:?}: {}", text(&output));
        assert!(text(&output).contains(said), "{args:?}: {}", text(&output));
    }
}

/// Raising the first wait alone passes `check` and `build`: the cap rises
/// with it.
#[test]
fn a_file_that_raises_only_the_first_wait_checks_and_builds() {
    let dir = project("raised", "[backoff]\ninitial_ms = 120000\n");
    for args in [&["check", "--offline"][..], &["build"][..]] {
        let output = flusso(&dir, args, &[]);
        assert!(output.status.success(), "{args:?}: {}", text(&output));
    }
}

/// The variable wins over the file, the flag over the variable, and the
/// refusal names the one that won.
#[test]
fn the_variable_beats_the_file_and_the_flag_beats_the_variable() {
    let dir = project("layers", "[backoff]\ninitial_ms = 200\nmax_ms = 800\n");

    let output = flusso(&dir, RUN, &[("FLUSSO_BACKOFF_MAX_MS", "100")]);
    assert!(
        text(&output)
            .contains("max_ms = 100 (set by FLUSSO_BACKOFF_MAX_MS) is below initial_ms = 200"),
        "{}",
        text(&output)
    );

    let mut args = RUN.to_vec();
    args.extend(["--backoff-max-ms", "50"]);
    let output = flusso(&dir, &args, &[("FLUSSO_BACKOFF_MAX_MS", "100")]);
    assert!(
        text(&output).contains("max_ms = 50 (set by --backoff-max-ms) is below initial_ms = 200"),
        "{}",
        text(&output)
    );

    let output = flusso(
        &dir,
        RUN,
        &[
            ("FLUSSO_BACKOFF_INITIAL_MS", "900"),
            ("FLUSSO_BACKOFF_MAX_MS", ""),
        ],
    );
    assert!(
        text(&output).contains("max_ms = 800 (set in flusso.toml's [backoff] max_ms) is below initial_ms = 900 (set by FLUSSO_BACKOFF_INITIAL_MS)"),
        "{}",
        text(&output)
    );
}
