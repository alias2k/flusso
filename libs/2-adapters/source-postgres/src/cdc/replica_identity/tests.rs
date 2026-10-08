use super::*;

fn columns(names: &[&str]) -> Vec<ColumnName> {
    names
        .iter()
        .map(|name| ColumnName::try_new(*name).unwrap())
        .collect()
}

/// `uncarried` for an identity holding exactly `identity` columns.
fn missing(identity: &[&str], primary_key: &[&str], required: &[&str]) -> Vec<ColumnName> {
    uncarried(
        |column| identity.contains(&column),
        primary_key,
        &columns(required),
    )
}

#[test]
fn full_identity_carries_every_column() {
    assert!(uncarried(|_| true, &["id"], &columns(&["parent_id"])).is_empty());
}

#[test]
fn default_identity_carries_only_the_primary_key() {
    assert_eq!(
        missing(&["id"], &["id"], &["parent_id"]),
        columns(&["parent_id"])
    );
}

#[test]
fn a_composite_primary_key_holding_the_link_needs_nothing() {
    assert!(missing(&["user_id", "tag_id"], &["user_id", "tag_id"], &["user_id"]).is_empty());
}

#[test]
fn an_identity_index_covering_the_key_and_the_link_is_enough() {
    assert!(missing(&["id", "parent_id"], &["id"], &["parent_id"]).is_empty());
}

#[test]
fn an_identity_index_without_the_primary_key_delivers_nothing() {
    // `UNIQUE (parent_id)` as the identity: changes are keyed by `parent_id`
    // itself, so the pre-image adds nothing beyond the key and is dropped.
    assert_eq!(
        missing(&["parent_id"], &["id"], &["parent_id"]),
        columns(&["parent_id"])
    );
}

#[test]
fn nothing_identity_carries_nothing() {
    assert_eq!(
        missing(&[], &["id"], &["parent_id"]),
        columns(&["parent_id"])
    );
}

#[test]
fn a_table_without_a_primary_key_delivers_nothing() {
    assert_eq!(
        uncarried(|_| true, &[] as &[&str], &columns(&["parent_id"])),
        columns(&["parent_id"])
    );
}

#[test]
fn identity_codes_parse() {
    assert_eq!(ReplicaIdentity::from_code("d"), ReplicaIdentity::Default);
    assert_eq!(ReplicaIdentity::from_code("i"), ReplicaIdentity::Index);
    assert_eq!(ReplicaIdentity::from_code("f"), ReplicaIdentity::Full);
    assert_eq!(ReplicaIdentity::from_code("n"), ReplicaIdentity::Nothing);
    assert_eq!(ReplicaIdentity::from_code("?"), ReplicaIdentity::Nothing);
}

#[test]
fn the_remediation_quotes_the_table() {
    assert_eq!(
        alter_sql("public", "order_items"),
        "ALTER TABLE \"public\".\"order_items\" REPLICA IDENTITY FULL;"
    );
}
