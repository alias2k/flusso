use super::*;

#[test]
fn hash_is_deterministic_for_equal_values() {
    let a = ContentHash::of(&("users", 1u8, vec!["id", "email"]));
    let b = ContentHash::of(&("users", 1u8, vec!["id", "email"]));
    assert_eq!(a, b);
}

#[test]
fn hash_changes_when_structure_changes() {
    let before = ContentHash::of(&vec!["id", "email"]);
    let after = ContentHash::of(&vec!["id", "email", "name"]);
    assert_ne!(before, after);
}

#[test]
fn display_is_eight_hex_digits() {
    assert_eq!(format!("{}", ContentHash::new(0xABCD)), "0000abcd");
}
