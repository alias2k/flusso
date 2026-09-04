//! The frozen compatibility corpus: every snapshot under `tests/compat/` must
//! keep loading on the current code, for the whole major.
//!
//! Each `v*` directory is an immutable copy of user-owned files as some
//! release wrote them — see `tests/compat/README.md` for the rules. A failure
//! here means the change under review breaks a file that was valid at the
//! start of the major; fix the change, never the snapshot.

#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

fn snapshots() -> Vec<PathBuf> {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compat");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(corpus)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "the compat corpus must not be empty");
    dirs
}

#[test]
fn every_frozen_config_still_loads_and_converts() {
    for snapshot in snapshots() {
        let config = config::load(snapshot.join("flusso.toml"))
            .unwrap_or_else(|err| panic!("{} no longer loads: {err}", snapshot.display()));
        // Conversion ran inside `load`; mappings must still resolve too.
        assert!(
            !config.resolve_mappings().is_empty(),
            "{} resolves no mappings",
            snapshot.display()
        );
    }
}

/// Snapshots frozen before lock format 3 (ADR 0005) keep only their
/// user-authored files; the lock guarantee starts with the first snapshot that
/// carries a `flusso.lock`.
#[test]
fn every_frozen_lock_still_decodes() {
    let with_lock = snapshots()
        .into_iter()
        .filter(|snapshot| snapshot.join("flusso.lock").exists());
    for snapshot in with_lock {
        let from_lock = config::load_compiled(snapshot.join("flusso.lock"))
            .unwrap_or_else(|err| panic!("{} lock no longer decodes: {err}", snapshot.display()));
        let from_config = config::load(snapshot.join("flusso.toml")).unwrap();
        assert_eq!(
            from_lock.indexes.keys().collect::<Vec<_>>(),
            from_config.indexes.keys().collect::<Vec<_>>(),
            "{}: the lock and the config disagree on the index set",
            snapshot.display()
        );
    }
}
