#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

//! `flusso check`'s human report through the real binary, no database
//! (issue #485): errors are printed last, after every other section, the last
//! line counts them, and colour is on only when asked for or on a terminal.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A project whose one index has two namesakes that only warn (`name`) and two
/// that fail every build (`tags`: a list of scalars before an object).
fn project() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("check_report");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n\
         \x20 - keyword: name\n    required: false\n  - keyword: name\n    required: false\n\
         \x20 - keyword: tags\n    array: true\n    required: false\n\
         \x20 - object: tags\n    fields:\n      - keyword: a\n        required: false\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://nobody:hunter2@127.0.0.1:1/none\"\n\n\
         [sinks.out]\ntype = \"stdout\"\n\n[[index]]\nname = \"t\"\nschema = \"t.schema.yml\"\n\
         enabled = true\n",
    )
    .unwrap();
    dir
}

fn check(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_flusso"));
    command
        .current_dir(project())
        .args(["check", "--offline"])
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("FLUSSO_COLOR")
        .stdin(Stdio::null());
    for (var, value) in env {
        command.env(var, value);
    }
    command.output().unwrap()
}

#[test]
fn errors_are_printed_last_and_counted() {
    let output = check(&[], &[]);
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let error = stdout.find("[error] t.tags").expect(&stdout);
    for before in ["✓ config valid", "! offline", "[warning] t.name"] {
        assert!(
            stdout.find(before).expect(&stdout) < error,
            "{before}:\n{stdout}"
        );
    }
    assert_eq!(
        stdout.trim_end().lines().last(),
        Some("✗ check failed: 1 error, 1 warning"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("Index  t"),
        "no mapping in check: {stdout}"
    );
}

#[test]
fn a_report_into_a_pipe_has_no_colour() {
    for env in [&[][..], &[("NO_COLOR", "1")][..]] {
        let output = check(&[], env);
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("[error]"), "{stdout}");
        assert!(!stdout.contains('\x1b'), "{env:?}: {stdout:?}");
    }
    let never = check(&["--color", "never"], &[]);
    assert!(!String::from_utf8(never.stdout).unwrap().contains('\x1b'));
}

#[test]
fn a_forced_colour_paints_errors_and_warnings() {
    for (args, env) in [
        (&["--color", "always"][..], &[][..]),
        (&[][..], &[("FLUSSO_COLOR", "always")][..]),
    ] {
        let output = check(args, env);
        let stdout = String::from_utf8(output.stdout).unwrap();
        for painted in [
            "\x1b[1;31m[error]\x1b[0m",
            "\x1b[1;33m[warning]\x1b[0m",
            "\x1b[1;31m✗ check failed: 1 error, 1 warning\x1b[0m",
        ] {
            assert!(stdout.contains(painted), "{painted:?} missing: {stdout:?}");
        }
    }
}

/// `check --format json` carries what the human report says and nothing more:
/// no config dump, no mapping, so no secret (the URL's password here).
#[test]
fn the_json_report_holds_no_config_mapping_or_secret() {
    let output = check(&["--format", "json"], &[]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let document: serde_json::Value = serde_json::from_str(&stdout).expect(&stdout);
    for gone in ["config", "mappings"] {
        assert!(document.get(gone).is_none(), "{gone}: {stdout}");
    }
    assert!(!stdout.contains("hunter2"), "{stdout}");
    assert_eq!(document["summary"]["errors"], 1, "{stdout}");
    assert_eq!(document["summary"]["warnings"], 1, "{stdout}");
    assert_eq!(document["summary"]["database_checked"], false, "{stdout}");
    assert_eq!(
        document["summary"]["line"], "✗ check failed: 1 error, 1 warning",
        "{stdout}"
    );
    assert!(document["server"].is_null(), "{stdout}");
}
