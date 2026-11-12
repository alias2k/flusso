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
        member("orders_2026", ReplicaIdentity::Nothing, true),
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

/// A lone table with `identity`, its identity index (or primary key) holding
/// `carried`.
fn lone(identity: ReplicaIdentity, carried: &[&str]) -> [Member; 1] {
    [Member {
        carried: carried.iter().map(|c| (*c).to_owned()).collect(),
        ..member("orders", identity, true)
    }]
}

/// What flusso does with a lone table: `None` when it is `FULL`, else whether
/// the link is a gap and whether the table is kept as is.
fn decide(identity: ReplicaIdentity, carried: &[&str], required: &[&str]) -> Option<(bool, bool)> {
    shortfall(&orders(), &columns(required), &lone(identity, carried))
        .map(|gap| (!gap.missing.is_empty(), gap.kept.is_some()))
}

#[test]
fn only_an_identity_index_carrying_every_link_is_kept() {
    use ReplicaIdentity::{Default, Full, Index, Nothing};
    let link = &["user_id"][..];
    let none: &[&str] = &[];
    // (identity, carried, required) → (gap, kept)
    let cases = [
        (Default, &["id"][..], link, Some((true, false))),
        (Default, &["id"][..], none, Some((false, false))),
        (Nothing, &[][..], link, Some((true, false))),
        (Nothing, &[][..], none, Some((false, false))),
        (Index, &["id", "user_id"][..], link, Some((false, true))),
        (Index, &["id"][..], link, Some((true, false))),
        (Index, &["user_id"][..], link, Some((true, false))),
        (Index, &["id"][..], none, Some((false, true))),
        (Index, &["id", "slug"][..], none, Some((false, true))),
        (Index, &["tenant", "slug"][..], none, Some((false, false))),
        (Index, &[][..], none, Some((false, false))),
        (Index, &[][..], link, Some((true, false))),
        (Full, &["id"][..], link, None),
        (Full, &["id"][..], none, None),
    ];
    for (identity, carried, required, expected) in cases {
        assert_eq!(
            decide(identity, carried, required),
            expected,
            "{identity:?} carrying {carried:?}, links {required:?}"
        );
    }
}

#[test]
fn a_kept_table_still_names_the_statement_and_is_never_managed() {
    let gap = shortfall(
        &orders(),
        &columns(&["user_id"]),
        &lone(ReplicaIdentity::Index, &["id", "user_id"]),
    )
    .unwrap();
    assert!(gap.missing.is_empty());
    assert!(gap.manageable);
    assert!(!gap.will_manage(true));
    assert_eq!(
        gap.remediation,
        "ALTER TABLE \"public\".\"orders\" REPLICA IDENTITY FULL;"
    );
}

#[test]
fn a_partition_keeps_its_identity_index_while_a_sibling_missing_the_link_is_set() {
    let members = [
        Member {
            partitioned: true,
            carried: vec!["id".to_owned(), "user_id".to_owned()],
            ..member("orders", ReplicaIdentity::Index, true)
        },
        Member {
            carried: vec!["id".to_owned(), "user_id".to_owned()],
            ..member("orders_2025", ReplicaIdentity::Index, true)
        },
        member("orders_2026", ReplicaIdentity::Default, true),
    ];
    let gap = shortfall(&orders(), &columns(&["user_id"]), &members).unwrap();
    assert_eq!(gap.missing, columns(&["user_id"]));
    assert_eq!(gap.kept, None);
    assert_eq!(
        gap.remediation, "ALTER TABLE \"public\".\"orders_2026\" REPLICA IDENTITY FULL;",
        "the parent and the partition on a carrying index are kept"
    );
    assert_eq!(
        shortfall(&orders(), &columns(&["user_id"]), &members[..2]),
        None,
        "a partitioned table with no gap is left alone"
    );
}

/// Whether a lone table with `identity` on an index holding `carried`, keyed
/// by `primary_key`, is kept when it has no link to carry.
fn kept_without_links(identity: ReplicaIdentity, carried: &[&str], primary_key: &[&str]) -> bool {
    let table = [Member {
        carried: carried.iter().map(|c| (*c).to_owned()).collect(),
        primary_key: primary_key.iter().map(|c| (*c).to_owned()).collect(),
        ..member("orders", identity, true)
    }];
    shortfall(&orders(), &[], &table).unwrap().kept.is_some()
}

#[test]
fn a_table_with_no_link_is_kept_only_on_an_index_carrying_its_whole_primary_key() {
    use ReplicaIdentity::Index;
    assert!(kept_without_links(Index, &["id"], &["id"]));
    assert!(kept_without_links(
        Index,
        &["tenant", "id", "slug"],
        &["tenant", "id"]
    ));
    assert!(
        !kept_without_links(Index, &["tenant", "slug"], &["id"]),
        "changes would be keyed by (tenant, slug)"
    );
    assert!(
        !kept_without_links(Index, &["tenant", "slug"], &["tenant", "id"]),
        "a composite key only partly carried"
    );
    assert!(
        !kept_without_links(Index, &[], &["id"]),
        "a dropped identity index logs nothing"
    );
    assert!(
        !kept_without_links(Index, &["tenant", "slug"], &[]),
        "a table with no primary key"
    );
}

#[test]
fn a_root_on_an_identity_index_without_its_primary_key_is_set_to_full() {
    let gap = shortfall(
        &orders(),
        &[],
        &lone(ReplicaIdentity::Index, &["tenant", "slug"]),
    )
    .unwrap();
    assert!(gap.missing.is_empty(), "no link to miss");
    assert_eq!(
        gap.missing_key,
        columns(&["id"]),
        "a gap all the same: changes are keyed by (tenant, slug)"
    );
    assert_eq!(gap.kept, None);
    assert!(gap.will_manage(true));
    assert_eq!(
        gap.remediation,
        "ALTER TABLE \"public\".\"orders\" REPLICA IDENTITY FULL;"
    );
}

/// A partitioned `orders(id PK)` whose parent has `parent` and one partition
/// `partition`, each identity index (or primary key) holding `carried`.
fn tree(parent: (ReplicaIdentity, &[&str]), partition: (ReplicaIdentity, &[&str])) -> [Member; 3] {
    let carrying = |(identity, carried): (ReplicaIdentity, &[&str]), name: &str| Member {
        carried: carried.iter().map(|c| (*c).to_owned()).collect(),
        ..member(name, identity, true)
    };
    [
        Member {
            partitioned: true,
            ..carrying(parent, "orders")
        },
        carrying(partition, "orders_2025"),
        member("orders_2026", ReplicaIdentity::Default, true),
    ]
}

#[test]
fn a_partitioned_table_is_left_alone_only_while_every_member_logs_the_primary_key() {
    use ReplicaIdentity::{Default, Full, Index, Nothing};
    let pk = &["id"][..];
    // (parent, partition) → the members set, `None` when left alone.
    let cases: [(
        (ReplicaIdentity, &[&str]),
        (ReplicaIdentity, &[&str]),
        Option<&[&str]>,
    ); 9] = [
        ((Default, pk), (Default, pk), None),
        ((Default, pk), (Full, pk), None),
        ((Default, pk), (Index, &["id", "code"]), None),
        ((Full, pk), (Index, &["code"]), Some(&["orders_2025"])),
        ((Default, pk), (Index, &["code"]), Some(&["orders_2025"])),
        ((Default, pk), (Index, &[]), Some(&["orders_2025"])),
        ((Default, pk), (Nothing, &[]), Some(&["orders_2025"])),
        ((Index, &["code", "id"]), (Default, pk), None),
        ((Index, &["code"]), (Default, pk), Some(&["orders"])),
    ];
    for (parent, partition, expected) in cases {
        let members = tree(parent, partition);
        let gap = shortfall(&orders(), &[], &members);
        let set = gap.as_ref().map(|gap| {
            assert!(gap.missing.is_empty());
            assert_eq!(gap.missing_key, columns(&["id"]));
            assert_eq!(gap.kept, None);
            assert!(gap.will_manage(true));
            members
                .iter()
                .filter(|m| gap.remediation.contains(&format!("\"{}\"", m.name)))
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            set.as_deref(),
            expected,
            "parent {parent:?}, partition {partition:?}"
        );
        let planned = plan(&[], &members).map(|plan| {
            plan.altered
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
        });
        assert_eq!(planned.as_deref(), expected, "apply sets what is reported");
    }
}

#[test]
fn a_partition_without_the_primary_key_and_a_missing_link_sets_every_member_short_of_both() {
    let members = tree(
        (ReplicaIdentity::Default, &["id"]),
        (ReplicaIdentity::Index, &["code"]),
    );
    let gap = shortfall(&orders(), &columns(&["user_id"]), &members).unwrap();
    assert_eq!(gap.missing, columns(&["user_id"]));
    assert_eq!(gap.missing_key, columns(&["id"]));
    assert_eq!(
        gap.remediation,
        "ALTER TABLE \"public\".\"orders\" REPLICA IDENTITY FULL; \
         ALTER TABLE \"public\".\"orders_2025\" REPLICA IDENTITY FULL; \
         ALTER TABLE \"public\".\"orders_2026\" REPLICA IDENTITY FULL;"
    );
}

#[test]
fn an_unowned_partition_without_the_primary_key_blocks_and_is_still_a_gap() {
    let mut members = tree(
        (ReplicaIdentity::Default, &["id"]),
        (ReplicaIdentity::Index, &["code"]),
    );
    if let Some(partition) = members.get_mut(1) {
        partition.owned = false;
    }
    let gap = shortfall(&orders(), &[], &members).unwrap();
    assert_eq!(gap.missing_key, columns(&["id"]));
    assert!(!gap.manageable);
    assert_eq!(gap.blockers, ["role does not own table public.orders_2025"]);
}

#[test]
fn a_plain_table_short_of_its_primary_key_is_a_key_gap_and_set_as_before() {
    use ReplicaIdentity::{Default, Index, Nothing};
    // (identity, carried) → missing_key; every one is set to FULL, as #223 does.
    let cases = [
        (Nothing, &[][..], &["id"][..]),
        (Index, &[][..], &["id"][..]),
        (Index, &["tenant", "slug"][..], &["id"][..]),
        (Default, &["id"][..], &[][..]),
    ];
    for (identity, carried, missing_key) in cases {
        let gap = shortfall(&orders(), &[], &lone(identity, carried)).unwrap();
        assert_eq!(
            gap.missing_key,
            columns(missing_key),
            "{identity:?} {carried:?}"
        );
        assert_eq!(gap.kept, None);
        assert_eq!(
            gap.remediation,
            "ALTER TABLE \"public\".\"orders\" REPLICA IDENTITY FULL;"
        );
    }
    let no_key = [Member {
        primary_key: Vec::new(),
        ..member("orders", Index, true)
    }];
    assert!(
        shortfall(&orders(), &[], &no_key)
            .unwrap()
            .missing_key
            .is_empty(),
        "a table without a primary key has none to miss"
    );
}
