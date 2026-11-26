#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

//! `flusso check`'s human report through the real binary, no database
//! (issue #485): errors are printed last, after every other section, the last
//! line counts them, and colour is on only when asked for or on a terminal.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A project of its own (`name`: one per test, since tests run at once and a
/// `check` reading files another test is rewriting reads them truncated),
/// whose one index has two namesakes that only warn (`name`) and two that
/// fail every build (`tags`: a list of scalars before an object).
fn project(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("check_report")
        .join(name);
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

/// One `flusso check --offline` run: its stdout, and the whole run for a
/// failure message.
struct Checked {
    stdout: String,
    whole: String,
}

/// Run `flusso check --offline` with `args` in the project `name`, under
/// `env`, with every colour variable unset otherwise.
fn check(name: &str, args: &[&str], env: &[(&str, &str)]) -> Checked {
    let mut command = Command::new(env!("CARGO_BIN_EXE_flusso"));
    command
        .current_dir(project(name))
        .args(["check", "--offline"])
        .args(args)
        .env_remove("NO_COLOR")
        .env_remove("FLUSSO_COLOR")
        .env_remove("FLUSSO_FORMAT")
        .env_remove("FLUSSO_CONFIG")
        .env_remove("FLUSSO_LOCK")
        .stdin(Stdio::null());
    for (var, value) in env {
        command.env(var, value);
    }
    let output: Output = command.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let whole = format!(
        "{args:?} {env:?}: {}\n--- stdout ---\n{stdout}\n--- stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "the report has an error: {whole}");
    Checked { stdout, whole }
}

#[test]
fn errors_are_printed_last_and_counted() {
    let run = check("last", &[], &[]);
    let stdout = &run.stdout;
    let at = |text: &str| {
        stdout
            .find(text)
            .unwrap_or_else(|| panic!("{text:?} missing: {}", run.whole))
    };
    let error = at("[error] t.tags");
    for before in ["✓ config valid", "! offline", "[warning] t.name"] {
        assert!(
            at(before) < error,
            "{before} after the error: {}",
            run.whole
        );
    }
    assert_eq!(
        stdout.trim_end().lines().last(),
        Some("✗ check failed: 1 error, 1 warning"),
        "{}",
        run.whole
    );
    assert!(
        !stdout.contains("Index  t"),
        "no mapping in check: {}",
        run.whole
    );
}

#[test]
fn a_report_into_a_pipe_has_no_colour() {
    for env in [&[][..], &[("NO_COLOR", "1")][..]] {
        let run = check("pipe", &[], env);
        assert!(run.stdout.contains("[error]"), "{}", run.whole);
        assert!(!run.stdout.contains('\x1b'), "{}", run.whole);
    }
    let never = check("pipe", &["--color", "never"], &[]);
    assert!(!never.stdout.contains('\x1b'), "{}", never.whole);
}

#[test]
fn a_forced_colour_paints_errors_and_warnings() {
    for (args, env) in [
        (&["--color", "always"][..], &[][..]),
        (&[][..], &[("FLUSSO_COLOR", "always")][..]),
    ] {
        let run = check("forced", args, env);
        for painted in [
            "\x1b[1;31m[error]\x1b[0m",
            "\x1b[1;33m[warning]\x1b[0m",
            "\x1b[1;31m✗ check failed: 1 error, 1 warning\x1b[0m",
        ] {
            assert!(
                run.stdout.contains(painted),
                "{painted:?} missing: {}",
                run.whole
            );
        }
    }
}

/// `check --format json` carries what the human report says and nothing more:
/// no config dump, no mapping, so no secret (the URL's password here).
#[test]
fn the_json_report_holds_no_config_mapping_or_secret() {
    let run = check("json", &["--format", "json"], &[]);
    let document: serde_json::Value =
        serde_json::from_str(&run.stdout).unwrap_or_else(|error| panic!("{error}: {}", run.whole));
    for gone in ["config", "mappings"] {
        assert!(document.get(gone).is_none(), "{gone}: {}", run.whole);
    }
    assert!(!run.stdout.contains("hunter2"), "{}", run.whole);
    assert_eq!(document["summary"]["errors"], 1, "{}", run.whole);
    assert_eq!(document["summary"]["warnings"], 1, "{}", run.whole);
    assert_eq!(
        document["summary"]["database_checked"], false,
        "{}",
        run.whole
    );
    assert_eq!(
        document["summary"]["line"], "✗ check failed: 1 error, 1 warning",
        "{}",
        run.whole
    );
    assert!(document["server"].is_null(), "{}", run.whole);
}

/// A project of its own whose index passes, with its lock built.
fn locked_project(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("check_report")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        "[source]\ntype = \"postgres\"\n\n[sinks.out]\ntype = \"stdout\"\n\n[[index]]\nname = \"t\"\n\
         schema = \"t.schema.yml\"\nenabled = true\n",
    )
    .unwrap();
    let built = flusso(&dir, &["build"]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    dir
}

fn flusso(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(dir)
        .args(args)
        .env_remove("FLUSSO_CONFIG")
        .env_remove("FLUSSO_LOCK")
        .env_remove("FLUSSO_FORMAT")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn shown(output: &Output) -> String {
    format!(
        "{}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// `check` compares the lock with what the config compiles to (issue #485):
/// a current one is a line, a stale one fails the check as an error, and with
/// no config the lock alone is checked, with nothing to compare.
#[test]
fn check_says_whether_the_lock_is_current() {
    let dir = locked_project("lock");
    let current = flusso(&dir, &["check", "--offline"]);
    assert!(current.status.success(), "{}", shown(&current));
    assert!(
        String::from_utf8_lossy(&current.stdout).contains("✓ flusso.lock is current"),
        "{}",
        shown(&current)
    );

    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n\
         \x20 - keyword: name\n    required: false\n",
    )
    .unwrap();
    let stale = flusso(&dir, &["check", "--offline"]);
    assert!(!stale.status.success(), "{}", shown(&stale));
    let printed = String::from_utf8_lossy(&stale.stdout);
    assert!(
        printed.contains("[error] flusso.lock") && printed.contains("the index `t` changed"),
        "{}",
        shown(&stale)
    );
    let json = flusso(&dir, &["check", "--offline", "--format", "json"]);
    let document: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(document["lock"]["state"], "lock_stale", "{document}");
    assert_eq!(document["summary"]["errors"], 1, "{document}");

    std::fs::remove_file(dir.join("flusso.toml")).unwrap();
    let alone = flusso(&dir, &["check", "--offline"]);
    assert!(alone.status.success(), "{}", shown(&alone));
    assert!(
        String::from_utf8_lossy(&alone.stdout)
            .contains("no config to compare flusso.lock with: the lock is what was checked"),
        "{}",
        shown(&alone)
    );
}
