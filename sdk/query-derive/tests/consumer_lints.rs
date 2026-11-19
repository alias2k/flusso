//! The derives' output passes the lints of a strict consumer (issue #431).
//!
//! A derive's code lands in the consumer's crate, and much of it carries the
//! consumer's own spans (so a failed check points at their field). The lints
//! then treat it as the consumer's code: a consumer denying warnings must not
//! stop compiling because of what `#[derive(FlussoRoot)]`, `FlussoFragment`,
//! `FlussoValue` or `FlussoMap` emitted.
//!
//! This test runs `cargo clippy` over three packages, every lint in
//! [`STRICT`] denied:
//!
//! - `ui-pass`: every `tests/ui_pass/*.rs` as a binary (the shapes that must
//!   compile: flatten, `opaque`, transparent and array newtypes, maps,
//!   nested/object levels, read-time fields, raw identifiers), plus
//!   `tests/ui_no_subfields/map_exact_family.rs` under its own config. These
//!   are test programs whose own code trips a few lints no derive output is
//!   about ([`UI_PASS_ALLOWS`]: unread fields, an undocumented crate,
//!   `unwrap()`), so those are allowed there.
//! - `public`: `tests/consumer/public.rs`, a documented library exporting
//!   every kind of derived type, where every lint is denied.
//! - `private`: `tests/consumer/private.rs`, a binary whose derived types are
//!   private and used, where every lint is denied too.
//!
//! The two consumer packages check what `ui-pass` allows (`missing_docs`,
//! `dead_code`, `unreachable_pub`, …) on what the derives generate.
//!
//! The packages live in `target/tests/flusso-consumer-lints/` with a target
//! directory of their own, so this never waits on another harness's lock, and
//! cargo's own fingerprints keep a rerun on an unchanged derive to a replay of
//! cached results.
#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::process::Command;

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// What a strict consumer denies: the workspace's own `[workspace.lints]`, the
/// rustc lints such crates commonly add, and clippy's default and pedantic
/// sets. A group (`clippy::all`) is laid under the single lints.
const STRICT: &[&str] = &[
    "warnings",
    "rust_2018_idioms",
    "unsafe_code",
    "missing_docs",
    "missing_debug_implementations",
    "missing_copy_implementations",
    "non_snake_case",
    "non_camel_case_types",
    "non_upper_case_globals",
    "unreachable_patterns",
    "unreachable_pub",
    "unused_qualifications",
    "unused_results",
    "unused_lifetimes",
    "single_use_lifetimes",
    "unnameable_types",
    "trivial_casts",
    "trivial_numeric_casts",
    "clippy::all",
    "clippy::pedantic",
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::indexing_slicing",
    "clippy::print_stdout",
    "clippy::print_stderr",
    "clippy::allow_attributes",
    "clippy::allow_attributes_without_reason",
];

/// Lint groups, laid under the single lints so an allowance wins over them.
const GROUPS: &[&str] = &[
    "warnings",
    "rust_2018_idioms",
    "clippy::all",
    "clippy::pedantic",
];

/// What the `ui_pass` programs' own code trips; the two consumer packages
/// deny these on the derives' output.
const UI_PASS_ALLOWS: &[&str] = &[
    "dead_code",
    "missing_docs",
    "unreachable_pub",
    "unused_qualifications",
    "unused_results",
    "clippy::box_collection",
    "clippy::doc_markdown",
    "clippy::indexing_slicing",
    "clippy::unwrap_used",
];

#[test]
fn the_derives_output_passes_a_strict_consumers_lints() {
    let project = Project::new();
    let tests = Path::new(MANIFEST_DIR).join("tests");
    let fixtures = tests.join("fixtures");

    let mut ui_pass: Vec<PathBuf> = std::fs::read_dir(tests.join("ui_pass"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    ui_pass.sort();
    project.package("ui-pass", Target::Bins(&ui_pass), UI_PASS_ALLOWS);
    project.package(
        "ui-no-subfields",
        Target::Bins(&[tests.join("ui_no_subfields/map_exact_family.rs")]),
        UI_PASS_ALLOWS,
    );
    project.package(
        "public",
        Target::Lib(&tests.join("consumer/public.rs")),
        &[],
    );
    project.package(
        "private",
        Target::Bins(&[tests.join("consumer/private.rs")]),
        &[],
    );
    project.workspace(&["ui-pass", "ui-no-subfields", "public", "private"]);

    let mut found = project.clippy(
        &["--workspace", "--exclude", "ui-no-subfields"],
        &fixtures.join("flusso.toml"),
    );
    found.extend(project.clippy(
        &["--package", "ui-no-subfields"],
        &fixtures.join("no_subfields.toml"),
    ));
    assert!(
        found.is_empty(),
        "the derives' output trips a strict consumer's lints:\n\n{}",
        found.join("\n")
    );
}

/// What a package builds.
enum Target<'a> {
    Lib(&'a Path),
    Bins(&'a [PathBuf]),
}

struct Project {
    dir: PathBuf,
    target: PathBuf,
}

impl Project {
    fn new() -> Project {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .parent()
            .unwrap()
            .join("tests")
            .join("flusso-consumer-lints");
        let dir = root.join("project");
        std::fs::create_dir_all(&dir).unwrap();
        let lock = std::fs::read_to_string(workspace().join("Cargo.lock")).unwrap();
        write_if_changed(&dir.join("Cargo.lock"), &lock);
        Project {
            dir,
            target: root.join("target"),
        }
    }

    fn workspace(&self, members: &[&str]) {
        let members = members
            .iter()
            .map(|member| format!("{member:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        write_if_changed(
            &self.dir.join("Cargo.toml"),
            &format!("[workspace]\nmembers = [{members}]\nresolver = \"3\"\n"),
        );
    }

    fn package(&self, name: &str, target: Target<'_>, allows: &[&str]) {
        let query = workspace().join("sdk/query");
        let mut manifest = format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
             publish = false\n\n[dependencies]\n\
             flusso-query = {{ path = {:?}, features = [\"derive\", \"uuid\", \"decimal\", \"rc\"] }}\n\
             serde = {{ version = \"1\", features = [\"derive\"] }}\n\
             serde_json = \"1\"\n",
            query.display().to_string(),
        );
        manifest.push('\n');
        manifest.push_str(&lints(allows));
        match target {
            Target::Lib(path) => {
                manifest.push_str(&format!(
                    "\n[lib]\npath = {:?}\n",
                    path.display().to_string()
                ));
            }
            Target::Bins(paths) => {
                for path in paths {
                    let stem = path.file_stem().unwrap().to_str().unwrap();
                    manifest.push_str(&format!(
                        "\n[[bin]]\nname = \"{stem}\"\npath = {:?}\n",
                        path.display().to_string()
                    ));
                }
            }
        }
        let dir = self.dir.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        write_if_changed(&dir.join("Cargo.toml"), &manifest);
    }

    /// `cargo clippy` with `args`, the derive reading `config`: every
    /// diagnostic it reports, rendered.
    fn clippy(&self, args: &[&str], config: &Path) -> Vec<String> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .current_dir(&self.dir)
            .env("CARGO_TARGET_DIR", &self.target)
            .env("CARGO_INCREMENTAL", "0")
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .env("FLUSSO_CONFIG", config)
            .args([
                "clippy",
                "--offline",
                "--quiet",
                "--keep-going",
                "--message-format=json",
                "-j3",
            ])
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let found: Vec<String> = stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|message| message["reason"] == "compiler-message")
            .filter(|message| {
                matches!(
                    message["message"]["level"].as_str(),
                    Some("error" | "warning")
                ) && !message["message"]["code"].is_null()
            })
            .map(|message| {
                format!(
                    "[{}] {}",
                    message["target"]["name"].as_str().unwrap_or_default(),
                    message["message"]["rendered"].as_str().unwrap_or_default()
                )
            })
            .collect();
        assert!(
            output.status.success() || !found.is_empty(),
            "cargo clippy failed without a lint to show:\n{}\n{}",
            String::from_utf8_lossy(&output.stderr),
            stdout
        );
        found
    }
}

/// The `[lints]` tables: every [`STRICT`] lint denied, `allows` allowed.
fn lints(allows: &[&str]) -> String {
    let mut tables = toml::Table::new();
    let levels = STRICT
        .iter()
        .map(|lint| (*lint, "deny"))
        .chain(allows.iter().map(|lint| (*lint, "allow")));
    for (lint, level) in levels {
        let (table, name) = lint
            .strip_prefix("clippy::")
            .map_or(("rust", lint), |name| ("clippy", name));
        let value: toml::Value = if GROUPS.contains(&lint) {
            let mut group = toml::Table::new();
            group.insert("level".into(), level.into());
            group.insert("priority".into(), (-1).into());
            group.into()
        } else {
            level.into()
        };
        tables
            .entry(table)
            .or_insert_with(|| toml::Table::new().into())
            .as_table_mut()
            .unwrap()
            .insert(name.into(), value);
    }
    let mut lints = toml::Table::new();
    lints.insert("lints".into(), tables.into());
    toml::to_string(&lints).unwrap()
}

fn workspace() -> PathBuf {
    Path::new(MANIFEST_DIR)
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn write_if_changed(path: &Path, content: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        std::fs::write(path, content).unwrap();
    }
}
