use super::*;

#[test]
fn alias_actions_skip_when_already_on_target() {
    let holders = vec!["users_abc123".to_owned()];
    assert!(plan_alias_actions("users", "users_abc123", &holders).is_none());
}

#[test]
fn alias_actions_add_when_alias_is_absent() {
    let actions = plan_alias_actions("users", "users_abc123", &[]).unwrap();
    assert_eq!(
        actions,
        json!({ "actions": [
            { "add": { "index": "users_abc123", "alias": "users" } },
        ]})
    );
}

#[test]
fn alias_actions_move_off_stale_indexes_atomically() {
    // A schema change left the alias on the old physical index (plus a
    // hypothetical second straggler): one call removes both and adds the
    // current target.
    let holders = vec!["users_old111".to_owned(), "users_old222".to_owned()];
    let actions = plan_alias_actions("users", "users_new333", &holders).unwrap();
    assert_eq!(
        actions,
        json!({ "actions": [
            { "remove": { "index": "users_old111", "alias": "users" } },
            { "remove": { "index": "users_old222", "alias": "users" } },
            { "add": { "index": "users_new333", "alias": "users" } },
        ]})
    );
}

#[test]
fn alias_actions_keep_target_while_dropping_stragglers() {
    // Target already holds the alias but a stale index does too: no remove
    // for the target, just the straggler, and the (idempotent) add.
    let holders = vec!["users_new333".to_owned(), "users_old111".to_owned()];
    let actions = plan_alias_actions("users", "users_new333", &holders).unwrap();
    assert_eq!(
        actions,
        json!({ "actions": [
            { "remove": { "index": "users_old111", "alias": "users" } },
            { "add": { "index": "users_new333", "alias": "users" } },
        ]})
    );
}

// ── generation naming (alias-over-generations reindex) ───────────────────

#[test]
fn generation_name_appends_the_number() {
    assert_eq!(generation_name("users_ab12", 3), "users_ab12_3");
}

#[test]
fn parse_generation_reads_a_numeric_suffix_only() {
    assert_eq!(parse_generation("users_ab12", "users_ab12_3"), Some(3));
    // A legacy concrete index named exactly the hash alias is not a generation.
    assert_eq!(parse_generation("users_ab12", "users_ab12"), None);
    // Non-numeric suffix.
    assert_eq!(parse_generation("users_ab12", "users_ab12_x"), None);
    // A different hash that merely shares a prefix (no `_` after the alias).
    assert_eq!(parse_generation("users_ab12", "users_ab12x_1"), None);
    // A shorter alias that prefixes a longer hash.
    assert_eq!(parse_generation("users_ab", "users_ab12_3"), None);
}

#[test]
fn hash_alias_of_strips_the_generation_suffix() {
    assert_eq!(hash_alias_of("users_ab12_3").as_deref(), Some("users_ab12"));
    // A logical name with underscores: only the trailing `_{n}` is stripped.
    assert_eq!(
        hash_alias_of("user_events_ab12_10").as_deref(),
        Some("user_events_ab12")
    );
    // No numeric suffix → not a generation name.
    assert_eq!(hash_alias_of("users"), None);
    assert_eq!(hash_alias_of("users_abcd"), None);
}

#[test]
fn naming_round_trips_under_an_index_prefix() {
    // A prefixed hash alias is just a longer string to the pure naming
    // functions: generation naming and its inverses still line up, even when
    // the prefix itself contains underscores or trailing digits.
    for prefix in ["dev_", "staging-", "env1_"] {
        let hash_alias = format!("{prefix}users_ab12");
        let generation = generation_name(&hash_alias, 3);
        assert_eq!(generation, format!("{prefix}users_ab12_3"));
        assert_eq!(parse_generation(&hash_alias, &generation), Some(3));
        assert_eq!(
            hash_alias_of(&generation).as_deref(),
            Some(hash_alias.as_str())
        );
        assert_eq!(next_generation(&[generation], &hash_alias), 4);
    }
}

#[test]
fn next_generation_is_one_past_the_highest_existing() {
    assert_eq!(next_generation(&[], "users_ab12"), 1);
    assert_eq!(
        next_generation(
            &["users_ab12_1".to_owned(), "users_ab12_2".to_owned()],
            "users_ab12"
        ),
        3
    );
    // A leftover from a crashed reindex: go past it, never reuse.
    assert_eq!(
        next_generation(&["users_ab12_5".to_owned()], "users_ab12"),
        6
    );
    // Unrelated indexes are ignored.
    assert_eq!(
        next_generation(
            &["other_9".to_owned(), "users_ab12_2".to_owned()],
            "users_ab12"
        ),
        3
    );
}
