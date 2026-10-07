use super::*;

fn columns(names: &[&str]) -> Vec<ColumnName> {
    names
        .iter()
        .map(|name| ColumnName::try_new(*name).unwrap())
        .collect()
}

fn carried(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn full_identity_carries_every_column() {
    assert!(uncarried(ReplicaIdentity::Full, &[], &columns(&["parent_id"])).is_empty());
}

#[test]
fn default_identity_carries_only_the_primary_key() {
    assert_eq!(
        uncarried(
            ReplicaIdentity::Default,
            &carried(&["id"]),
            &columns(&["parent_id"])
        ),
        columns(&["parent_id"])
    );
}

#[test]
fn a_composite_primary_key_holding_the_link_needs_nothing() {
    assert!(
        uncarried(
            ReplicaIdentity::Default,
            &carried(&["user_id", "tag_id"]),
            &columns(&["user_id"])
        )
        .is_empty()
    );
}

#[test]
fn an_identity_index_covering_the_link_is_enough() {
    assert!(
        uncarried(
            ReplicaIdentity::Index,
            &carried(&["id", "parent_id"]),
            &columns(&["parent_id"])
        )
        .is_empty()
    );
}

#[test]
fn nothing_identity_carries_nothing() {
    assert_eq!(
        uncarried(ReplicaIdentity::Nothing, &[], &columns(&["parent_id"])),
        columns(&["parent_id"])
    );
}

#[test]
fn the_remediation_quotes_the_table() {
    assert_eq!(
        alter_sql("public", "order_items"),
        "ALTER TABLE \"public\".\"order_items\" REPLICA IDENTITY FULL;"
    );
}
