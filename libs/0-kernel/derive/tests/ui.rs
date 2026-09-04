//! Compile-fail snapshot tests for the derive's error messages.
#![allow(unused_crate_dependencies)]

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
