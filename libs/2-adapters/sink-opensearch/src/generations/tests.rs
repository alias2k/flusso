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
fn naming_round_trips_under_an_index_prefix() {
    // A prefixed hash alias is just a longer string to the pure naming
    // functions: generation naming and its inverse still line up, even when
    // the prefix itself contains underscores or trailing digits.
    for prefix in ["dev_", "staging-", "env1_"] {
        let hash_alias = format!("{prefix}users_ab12");
        let generation = generation_name(&hash_alias, 3);
        assert_eq!(generation, format!("{prefix}users_ab12_3"));
        assert_eq!(parse_generation(&hash_alias, &generation), Some(3));
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

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_first_seed_with_its_aliases_in_place_needs_no_alias_call() {
    let target = "users_1_1";
    let held = names(&[target]);
    let none = std::collections::BTreeSet::new();
    assert_eq!(
        plan_seed_actions(target, &[("users_1", &held), ("users", &held)], &none),
        None
    );
}

#[test]
fn a_rebuild_moves_both_aliases_and_removes_the_old_generation_in_one_call() {
    let target = "users_1_2";
    let old = names(&["users_1_1"]);
    let stale = std::collections::BTreeSet::from(["users_1_1".to_owned()]);
    let actions = plan_seed_actions(target, &[("users_1", &old), ("users", &old)], &stale).unwrap();
    assert_eq!(
        actions,
        json!({ "actions": [
            { "add": { "index": "users_1_2", "alias": "users_1" } },
            { "add": { "index": "users_1_2", "alias": "users" } },
            { "remove_index": { "index": "users_1_1" } },
        ]}),
        "the removed generation's aliases go with it"
    );
}

#[test]
fn an_alias_held_by_an_index_that_stays_is_removed_from_it() {
    let target = "users_2_1";
    let hash = names(&[target]);
    let other_schema = names(&["users_1_1"]);
    let none = std::collections::BTreeSet::new();
    let actions = plan_seed_actions(
        target,
        &[("users_2", &hash), ("users", &other_schema)],
        &none,
    )
    .unwrap();
    assert_eq!(
        actions,
        json!({ "actions": [
            { "remove": { "index": "users_1_1", "alias": "users" } },
            { "add": { "index": "users_2_1", "alias": "users" } },
        ]})
    );
}

#[test]
fn an_orphan_generation_holding_no_alias_is_removed_too() {
    let target = "users_1_3";
    let held = names(&[target]);
    let stale = std::collections::BTreeSet::from(["users_1_2".to_owned()]);
    let actions = plan_seed_actions(target, &[("users_1", &held)], &stale).unwrap();
    assert_eq!(
        actions,
        json!({ "actions": [{ "remove_index": { "index": "users_1_2" } }] })
    );
}

#[test]
fn an_alias_lookup_lists_every_asked_alias_even_a_missing_one() {
    let body = json!({
        "users_1_1": { "aliases": { "users_1": {}, "users": {} } },
        "orders_1_1": { "aliases": { "orders": {} } },
        "error": "alias [missing] missing",
        "status": 404,
    });
    let holders = holders_of(&["users_1", "users", "missing"], &body);
    assert_eq!(holders["users_1"], names(&["users_1_1"]));
    assert_eq!(holders["users"], names(&["users_1_1"]));
    assert!(holders["missing"].is_empty());
    assert!(!holders.contains_key("orders"));
}

/// `walked` is read only from an unseeded doc, and only when it names the
/// active generation: anything else is no checkpoint, never an error.
#[test]
fn a_backfill_checkpoint_is_read_only_for_the_unseeded_active_generation() {
    let read = |source: serde_json::Value| Meta::read(&source).map(|meta| meta.walked);
    assert_eq!(
        read(
            json!({ "active_generation": 2, "seeded": false, "walked": { "mark": "k", "generation": 2 } })
        ),
        Some(Some("k".to_owned()))
    );
    assert_eq!(
        read(
            json!({ "active_generation": 3, "seeded": false, "walked": { "mark": "k", "generation": 2 } })
        ),
        Some(None),
        "a record of a generation since replaced"
    );
    assert_eq!(
        read(
            json!({ "active_generation": 2, "seeded": true, "walked": { "mark": "k", "generation": 2 } })
        ),
        Some(None),
        "a stray record on a seeded doc"
    );
    assert_eq!(
        read(json!({ "active_generation": 2, "seeded": false })),
        Some(None)
    );
    assert_eq!(
        read(json!({ "active_generation": 2, "seeded": false, "walked": null })),
        Some(None),
        "forgotten"
    );
    assert_eq!(
        read(
            json!({ "active_generation": 2, "seeded": false, "walked": { "mark": 7, "generation": 2 } })
        ),
        Some(None),
        "malformed"
    );
    assert_eq!(
        read(json!({ "active_generation": 2, "seeded": false, "walked": "k" })),
        Some(None),
        "malformed"
    );
    assert_eq!(
        read(json!({ "seeded": false })),
        None,
        "a legacy doc with no generation"
    );
}

/// #479: a seed's `_aliases` refusal naming an old generation's block is the
/// typed `RetireBlocked`, under either read-only block and whatever status
/// carried it (OpenSearch 2's answers, CI run 45715512518). A
/// `read_only_allow_delete` block lets the old generation be deleted.
#[test]
fn a_seed_refused_by_an_old_generations_block_is_retire_blocked() {
    let old = ["users_00000001_1".to_owned()];
    let old: Vec<&String> = old.iter().collect();
    let refusal = |reason: &str| {
        json!({ "error": { "type": "cluster_block_exception", "reason": reason } }).to_string()
    };
    let read_only =
        refusal("index [users_00000001_1] blocked by: [FORBIDDEN/5/index read-only (api)];");
    match super::retire_blocked(&read_only, "users_00000001_2", &old) {
        Some(sink::SinkError::RetireBlocked {
            new,
            old,
            block,
            deletable,
        }) => {
            assert_eq!(new, "users_00000001_2");
            assert_eq!(old, "users_00000001_1");
            assert!(
                block.contains("FORBIDDEN/5/index read-only (api)"),
                "{block}"
            );
            assert!(!deletable, "a read-only index can't be deleted");
        }
        other => panic!("{other:?}"),
    }
    let flood = refusal(
        "index [users_00000001_1] blocked by: [TOO_MANY_REQUESTS/12/disk usage exceeded \
         flood-stage watermark, index has read-only-allow-delete block];",
    );
    assert!(matches!(
        super::retire_blocked(&flood, "users_00000001_2", &old),
        Some(sink::SinkError::RetireBlocked {
            deletable: true,
            ..
        })
    ));
    let by_hand = refusal(
        "index [users_00000001_1] blocked by: [FORBIDDEN/12/index read-only / allow delete (api)];",
    );
    assert!(matches!(
        super::retire_blocked(&by_hand, "users_00000001_2", &old),
        Some(sink::SinkError::RetireBlocked {
            deletable: true,
            ..
        })
    ));
}

/// Anything else stays a plain refusal: a block on another index (the new
/// generation, a cluster block), another error, a body that isn't JSON.
#[test]
fn any_other_alias_refusal_is_not_retire_blocked() {
    let old = ["users_00000001_1".to_owned()];
    let old: Vec<&String> = old.iter().collect();
    for text in [
        json!({ "error": { "type": "cluster_block_exception",
            "reason": "index [users_00000001_2] blocked by: [FORBIDDEN/5/index read-only (api)];" } })
        .to_string(),
        json!({ "error": { "type": "cluster_block_exception",
            "reason": "blocked by: [FORBIDDEN/6/cluster read-only (api)];" } })
        .to_string(),
        json!({ "error": { "type": "index_not_found_exception", "reason": "no such index [users_00000001_1]" } })
            .to_string(),
        "<html>busy</html>".to_owned(),
    ] {
        assert!(super::retire_blocked(&text, "users_00000001_2", &old).is_none(), "{text}");
    }
}
