#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

//! `[concurrency] max_waiting_per_index` and `max_waiting` through the real
//! binary, no database: `check`, `build` and `run` judge them with the one
//! function, a refused value says where it came from, and neither reaches the
//! lock. Each command runs as a child process with its own environment, so
//! setting `FLUSSO_MAX_WAITING*` here races with nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A project whose `flusso.toml` ends with `tail` (a `[concurrency]` table
/// or nothing).
fn project(name: &str, tail: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("max_waiting")
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
             enabled = true\n\n{tail}"
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
        .env_remove("FLUSSO_MAX_WAITING_PER_INDEX")
        .env_remove("FLUSSO_MAX_WAITING")
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

/// A bound `run` refuses, `check` and `build` refuse with the same words.
#[test]
fn check_build_and_run_refuse_the_same_bounds_alike() {
    for (name, table, said) in [
        (
            "zero",
            "max_waiting_per_index = 0",
            "[concurrency] max_waiting_per_index = 0 (set in flusso.toml's [concurrency] \
             max_waiting_per_index)",
        ),
        (
            "below",
            "max_waiting_per_index = 600",
            "[concurrency] max_waiting = 512 (the default, 2 × builds × [batch] max_changes) is \
             below max_waiting_per_index = 600",
        ),
    ] {
        let dir = project(
            name,
            &format!("[batch]\nmax_changes = 64\n\n[concurrency]\nbuilds = 4\n{table}\n"),
        );
        for args in [&["check", "--offline"][..], &["build"][..], RUN] {
            let output = flusso(&dir, args, &[]);
            assert!(!output.status.success(), "{args:?}: {}", text(&output));
            assert!(text(&output).contains(said), "{args:?}: {}", text(&output));
        }
    }
}

/// Bounds that make sense check and build, and never reach the lock.
#[test]
fn bounds_in_range_check_and_build_and_stay_out_of_the_lock() {
    let dir = project(
        "set",
        "[concurrency]\nmax_waiting_per_index = 100\nmax_waiting = 300\n",
    );
    for args in [&["check", "--offline"][..], &["build"][..]] {
        let output = flusso(&dir, args, &[]);
        assert!(output.status.success(), "{args:?}: {}", text(&output));
    }
    let lock = std::fs::read_to_string(dir.join("flusso.lock")).unwrap();
    assert!(!lock.contains("max_waiting"), "{lock}");
}

/// The variable wins over the file, the flag over the variable, and the
/// refusal names the one that won.
#[test]
fn the_variable_beats_the_file_and_the_flag_beats_the_variable() {
    let dir = project("layers", "[concurrency]\nmax_waiting = 4000\n");

    let output = flusso(&dir, RUN, &[("FLUSSO_MAX_WAITING", "0")]);
    assert!(
        text(&output).contains("max_waiting = 0 (set by FLUSSO_MAX_WAITING)"),
        "{}",
        text(&output)
    );

    let mut args = RUN.to_vec();
    args.extend(["--max-waiting-per-index", "0"]);
    let output = flusso(&dir, &args, &[("FLUSSO_MAX_WAITING_PER_INDEX", "100")]);
    assert!(
        text(&output).contains("max_waiting_per_index = 0 (set by --max-waiting-per-index)"),
        "{}",
        text(&output)
    );

    let mut args = RUN.to_vec();
    args.extend(["--max-waiting", "lots"]);
    let output = flusso(&dir, &args, &[]);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("`lots` is not a whole number"),
        "{}",
        text(&output)
    );
}
