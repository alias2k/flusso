use kernel::{Aggregate, AggregateKey, AggregateOp, Through};

use super::*;

fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

fn count(on: &str, key: AggregateKey) -> Relation {
    Relation::Aggregate(Aggregate {
        table: table(on),
        op: AggregateOp::Count,
        key,
        value_type: None,
        filters: None,
        distinct: false,
    })
}

fn through() -> AggregateKey {
    AggregateKey::Through(Through {
        table: table("user_tags"),
        left_key: column("user_id"),
        right_key: column("tag_id"),
        filters: None,
    })
}

fn image(parent_id: GenericValue) -> RowImage {
    RowImage(vec![
        (column("id"), GenericValue::Int(7)),
        (column("parent_id"), parent_id),
    ])
}

#[test]
fn a_child_links_to_its_parent_through_its_foreign_key() {
    let relation = count("child", AggregateKey::Direct(column("parent_id")));
    assert_eq!(
        link_on_row(&relation, &table("child")),
        Some(&column("parent_id"))
    );
}

#[test]
fn a_changed_junction_links_through_left_key_but_the_far_table_does_not() {
    let relation = count("tags", through());
    assert_eq!(
        link_on_row(&relation, &table("user_tags")),
        Some(&column("user_id"))
    );
    assert_eq!(link_on_row(&relation, &table("tags")), None);
}

#[test]
fn a_deleted_child_resolves_to_its_old_parent() {
    let parents = with_old_parent(
        Vec::new(),
        &image(GenericValue::Int(1)),
        &column("parent_id"),
    );
    assert_eq!(parents, vec![GenericValue::Int(1)]);
}

#[test]
fn a_reparented_child_resolves_to_both_parents_once() {
    let moved = with_old_parent(
        vec![GenericValue::Int(3)],
        &image(GenericValue::Int(1)),
        &column("parent_id"),
    );
    assert_eq!(moved, vec![GenericValue::Int(3), GenericValue::Int(1)]);

    let stayed = with_old_parent(
        vec![GenericValue::Int(1)],
        &image(GenericValue::Int(1)),
        &column("parent_id"),
    );
    assert_eq!(stayed, vec![GenericValue::Int(1)]);
}

#[test]
fn a_null_or_missing_old_link_adds_nothing() {
    let null = with_old_parent(Vec::new(), &image(GenericValue::Null), &column("parent_id"));
    assert!(null.is_empty());
    let missing = with_old_parent(
        Vec::new(),
        &RowImage(vec![(column("id"), GenericValue::Int(7))]),
        &column("parent_id"),
    );
    assert!(missing.is_empty());
}
