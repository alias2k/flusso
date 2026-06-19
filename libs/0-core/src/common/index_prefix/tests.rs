use super::validate_index_prefix;

#[test]
fn accepts_empty_and_common_separators() {
    for ok in ["", "dev_", "staging-", "nightly_", "env1_", "a"] {
        assert!(validate_index_prefix(ok).is_ok(), "{ok:?} should be valid");
    }
}

#[test]
fn rejects_illegal_prefixes() {
    for bad in [
        "Dev_", "_dev", "-dev", "+dev", "de v", "dev,", "dev/", "dev*",
    ] {
        assert!(
            validate_index_prefix(bad).is_err(),
            "{bad:?} should be rejected"
        );
    }
}
