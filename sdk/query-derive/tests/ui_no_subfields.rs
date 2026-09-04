//! Compile-fail test for subfield gating (Tier 4): with `auto_subfields` off,
//! a `text`/`keyword` handle is `NoSubfields`, so its `.keyword()` accessor
//! doesn't exist.
//!
//! Lives in its own harness (not the main `ui` one) because it needs a
//! different `FLUSSO_CONFIG` — the `no_subfields.toml` fixture. Separate test
//! binaries run in separate processes, so the two harnesses' env vars don't
//! race.
#![allow(unsafe_code, unused_crate_dependencies)]

#[test]
fn ui_no_subfields() {
    // SAFETY: set once, before trybuild spawns the compiler; this binary runs
    // this single test.
    unsafe {
        std::env::set_var(
            "FLUSSO_CONFIG",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/no_subfields.toml"
            ),
        );
    }
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_no_subfields/subfields_gated.rs");
}
