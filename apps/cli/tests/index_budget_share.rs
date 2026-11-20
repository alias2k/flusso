#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

//! `[concurrency] index_budget_share` through the real binary, no database:
//! `check`, `build` and `run` judge it with the one function, and a refused
//! value says where it came from. Each command runs as a child process with
//! its own environment, so setting `FLUSSO_INDEX_BUDGET_SHARE` here races
//! with nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A project whose `flusso.toml` ends with `tail` (a `[concurrency]` table
/// or nothing).
fn project(name: &str, tail: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("index_budget_share")
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
        .env_remove("FLUSSO_INDEX_BUDGET_SHARE")
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

/// A share `run` refuses, `check` and `build` refuse with the same words.
#[test]
fn check_build_and_run_refuse_the_same_share_alike() {
    let dir = project("zero", "[concurrency]\nindex_budget_share = 0\n");
    let said = "[concurrency] index_budget_share = 0 (set in flusso.toml's [concurrency] \
                index_budget_share) is outside (0, 1]";
    for args in [&["check", "--offline"][..], &["build"][..], RUN] {
        let output = flusso(&dir, args, &[]);
        assert!(!output.status.success(), "{args:?}: {}", text(&output));
        assert!(text(&output).contains(said), "{args:?}: {}", text(&output));
    }
}

/// A share in range checks and builds, and never reaches the lock.
#[test]
fn a_share_in_range_checks_and_builds_and_stays_out_of_the_lock() {
    let dir = project("quarter", "[concurrency]\nindex_budget_share = 0.25\n");
    for args in [&["check", "--offline"][..], &["build"][..]] {
        let output = flusso(&dir, args, &[]);
        assert!(output.status.success(), "{args:?}: {}", text(&output));
    }
    let lock = std::fs::read_to_string(dir.join("flusso.lock")).unwrap();
    assert!(!lock.contains("index_budget_share"), "{lock}");
}

/// The variable wins over the file, the flag over the variable, and the
/// refusal names the one that won.
#[test]
fn the_variable_beats_the_file_and_the_flag_beats_the_variable() {
    let dir = project("layers", "[concurrency]\nindex_budget_share = 0.5\n");

    let output = flusso(&dir, RUN, &[("FLUSSO_INDEX_BUDGET_SHARE", "2")]);
    assert!(
        text(&output).contains("index_budget_share = 2 (set by FLUSSO_INDEX_BUDGET_SHARE)"),
        "{}",
        text(&output)
    );

    let mut args = RUN.to_vec();
    args.extend(["--index-budget-share", "0"]);
    let output = flusso(&dir, &args, &[("FLUSSO_INDEX_BUDGET_SHARE", "0.3")]);
    assert!(
        text(&output).contains("index_budget_share = 0 (set by --index-budget-share)"),
        "{}",
        text(&output)
    );

    let mut args = RUN.to_vec();
    args.extend(["--index-budget-share", "most"]);
    let output = flusso(&dir, &args, &[]);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("`most` is not a number"),
        "{}",
        text(&output)
    );
}
