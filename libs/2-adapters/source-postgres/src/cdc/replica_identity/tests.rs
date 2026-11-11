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

fn member(name: &str, identity: ReplicaIdentity, owned: bool) -> Member {
    Member {
        schema: "public".to_owned(),
        name: name.to_owned(),
        identity,
        partitioned: false,
        carried: vec!["id".to_owned()],
        primary_key: vec!["id".to_owned()],
        owned,
    }
}

fn orders() -> QualifiedTable {
    QualifiedTable::new(
        kernel::DatabaseSchema::try_new("public").unwrap(),
        kernel::TableName::try_new("orders").unwrap(),
    )
}

#[test]
fn a_table_full_throughout_falls_short_of_nothing() {
    let members = [
        member("orders", ReplicaIdentity::Full, true),
        member("orders_2026", ReplicaIdentity::Full, false),
    ];
    assert_eq!(shortfall(&orders(), &columns(&["user_id"]), &members), None);
}

#[test]
fn a_missing_link_is_a_gap_and_the_rest_of_the_row_is_partial() {
    let members = [member("orders", ReplicaIdentity::Default, true)];
    let gap = shortfall(&orders(), &columns(&["user_id"]), &members).unwrap();
    assert_eq!(gap.missing, columns(&["user_id"]));
    assert!(gap.manageable);
    assert_eq!(
        gap.remediation,
        "ALTER TABLE \"public\".\"orders\" REPLICA IDENTITY FULL;"
    );

    let partial = shortfall(&orders(), &[], &members).unwrap();
    assert!(partial.missing.is_empty(), "no link to miss");
    assert!(partial.manageable);
    assert_eq!(partial.remediation, gap.remediation);
}

#[test]
fn every_member_short_of_full_is_listed_and_unowned_ones_block() {
    let members = [
        member("orders", ReplicaIdentity::Full, true),
        member("orders_2025", ReplicaIdentity::Default, false),
        member("orders_2026", ReplicaIdentity::Index, true),
    ];
    let gap = shortfall(&orders(), &[], &members).unwrap();
    assert!(!gap.manageable);
    assert_eq!(gap.blockers, ["role does not own table public.orders_2025"]);
    assert_eq!(
        gap.remediation,
        "ALTER TABLE \"public\".\"orders_2025\" REPLICA IDENTITY FULL; \
         ALTER TABLE \"public\".\"orders_2026\" REPLICA IDENTITY FULL;"
    );
}

#[test]
fn a_partitioned_table_without_a_link_is_left_alone() {
    let members = [
        Member {
            partitioned: true,
            ..member("orders", ReplicaIdentity::Default, true)
        },
        member("orders_2026", ReplicaIdentity::Default, true),
    ];
    assert_eq!(
        shortfall(&orders(), &[], &members),
        None,
        "its updates always rebuild"
    );
    let gap = shortfall(&orders(), &columns(&["user_id"]), &members).unwrap();
    assert_eq!(gap.missing, columns(&["user_id"]));
    assert_eq!(
        gap.remediation,
        "ALTER TABLE \"public\".\"orders\" REPLICA IDENTITY FULL; \
         ALTER TABLE \"public\".\"orders_2026\" REPLICA IDENTITY FULL;"
    );
}
