//! The golden lock: pins the exact text `flusso.lock` serializes to.
//!
//! `tests/fixtures/golden/` holds a maximal config — every field type tag,
//! join kind, aggregate op, and config knob — plus the committed lock it
//! compiles to. Any change to the serialized shape of `Config` shows up here
//! as a failing byte diff, so a format break is a conscious decision instead
//! of an accident. Re-bless with `FLUSSO_BLESS=1` after reviewing the diff —
//! and remember the compatibility rule: within the major, additive only.

#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden")
}

#[test]
fn golden_lock_matches_the_committed_bytes() {
    let compiled = schema::compile(golden_dir().join("flusso.toml")).unwrap();
    let actual = schema::to_bytes(&compiled).unwrap();

    let golden_path = golden_dir().join("flusso.lock");
    let expected = std::fs::read(&golden_path).unwrap_or_default();

    if actual != expected {
        if std::env::var_os("FLUSSO_BLESS").is_some() {
            std::fs::write(&golden_path, &actual).unwrap();
            panic!(
                "golden lock re-blessed at {} — review the diff before committing; \
                 a shape change must stay readable by every earlier release of the major",
                golden_path.display()
            );
        }
        panic!(
            "the serialized lock no longer matches the golden fixture at {} — \
             this is a lock format change. If it is intentional (and additive!), \
             re-run with FLUSSO_BLESS=1 and review the diff",
            golden_path.display()
        );
    }
}

#[test]
fn serialization_is_deterministic_and_roundtrips() {
    let compiled = schema::compile(golden_dir().join("flusso.toml")).unwrap();

    let first = schema::to_bytes(&compiled).unwrap();
    let second = schema::to_bytes(&compiled).unwrap();
    assert_eq!(first, second, "same envelope must yield identical bytes");

    // Full identity through the codec: decode, re-wrap, re-encode, compare.
    let config = schema::from_bytes(&first).unwrap();
    let rewrapped = schema::Compiled {
        format_version: schema::FORMAT_VERSION,
        config,
    };
    let third = schema::to_bytes(&rewrapped).unwrap();
    assert_eq!(first, third, "decode → encode must be the identity");
}

#[test]
fn legacy_binary_lock_is_diagnosed() {
    // A MessagePack map header — the first bytes of every pre-freeze lock.
    let legacy = [0x83u8, 0xae, 0x66, 0x6f, 0x72, 0xff];
    let err = schema::from_bytes(&legacy).unwrap_err();
    assert!(matches!(err, schema::CompileError::LegacyFormat));
    assert!(err.to_string().contains("older flusso"));
}

#[test]
fn future_format_version_is_rejected() {
    let golden = std::fs::read_to_string(golden_dir().join("flusso.lock")).unwrap();
    let text = golden.replace("format_version = 2", "format_version = 200");
    let err = schema::from_bytes(text.as_bytes()).unwrap_err();
    match err {
        schema::CompileError::VersionMismatch { got, expected } => {
            assert_eq!(got, 200);
            assert_eq!(expected, schema::FORMAT_VERSION);
        }
        other => panic!("expected VersionMismatch, got {other:?}"),
    }
}
