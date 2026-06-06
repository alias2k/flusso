//! Compile-fail snapshot tests for the derive's error messages (bon-style).
//!
//! trybuild compiles each `tests/ui/*.rs` in a temp dir, so we point the derive
//! at the fixture config via `FLUSSO_CONFIG` (absolute) rather than a relative
//! `config = "…"` attribute.
#![allow(unsafe_code, unused_crate_dependencies)]

#[test]
fn ui() {
    // SAFETY: set once, before trybuild spawns the compiler; the test body is
    // single-threaded here.
    unsafe {
        std::env::set_var(
            "FLUSSO_CONFIG",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/flusso.toml"),
        );
    }
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
