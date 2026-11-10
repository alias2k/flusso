use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, DatabaseSchema, FieldName, FilterOp, FilterValue,
    FlussoType, GenericValue, Join, JoinKind, OrderBy, RawFilter, RawFilterValue, SoftDeleteColumn,
    Through, ValueOpFilter,
};

use super::*;

fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

fn columns(names: &[&str]) -> ColumnUse {
    ColumnUse::Columns(names.iter().map(|name| column(name)).collect())
}

fn field(name: &str) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: column(name),
            ty: FlussoType::Keyword,
            nullable: false,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
        }),
    }
}

fn relation_field(name: &str, relation: Relation) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(relation),
    }
}

fn value_filter(name: &str) -> Filter {
    Filter::ValueOp(ValueOpFilter {
        column: column(name),
        op: FilterOp::Eq,
        value: FilterValue::Single("x".into()),
    })
}

fn raw_filter() -> Filter {
    Filter::Raw(RawFilter {
        raw: RawFilterValue::try_new("status <> 'gone'").unwrap(),
    })
}

fn join(target: &str, kind: JoinKind, fields: Vec<Field>) -> Join {
    Join {
        table: table(target),
        kind,
        primary_key: column("id"),
        nullable: false,
        filters: None,
        order_by: None,
        limit: None,
        fields,
    }
}

fn has_many_orders() -> Join {
    join(
        "orders",
        JoinKind::HasMany {
            foreign_key: column("user_id"),
        },
        vec![
            field("total"),
            relation_field(
                "shop",
                Relation::Join(join(
                    "shops",
                    JoinKind::BelongsTo {
                        column: column("shop_id"),
                    },
                    vec![field("name")],
                )),
            ),
        ],
    )
}

fn through(filters: Option<Vec<Filter>>) -> Through {
    Through {
        table: table("user_tags"),
        left_key: column("user_id"),
        right_key: column("tag_id"),
        filters,
    }
}

fn users(fields: Vec<Field>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields,
    }
}

#[test]
fn a_child_reads_its_fields_belongs_to_columns_and_foreign_key() {
    let orders = Relation::Join(has_many_orders());
    assert_eq!(
        relation_columns(&orders, &table("orders")),
        columns(&["id", "shop_id", "total", "user_id"])
    );
}

#[test]
fn a_join_reads_its_filter_and_order_by_columns() {
    let mut orders = has_many_orders();
    orders.filters = Some(vec![value_filter("status")]);
    orders.order_by = Some(vec![OrderBy {
        column: column("placed_at"),
        direction: None,
    }]);
    assert_eq!(
        relation_columns(&Relation::Join(orders), &table("orders")),
        columns(&["id", "placed_at", "shop_id", "status", "total", "user_id"])
    );
}

#[test]
fn an_aggregate_reads_its_input_filters_and_link() {
    let sum = Relation::Aggregate(Aggregate {
        table: table("orders"),
        op: AggregateOp::Sum(column("amount")),
        key: AggregateKey::Direct(column("user_id")),
        value_type: None,
        filters: Some(vec![value_filter("status")]),
        distinct: false,
    });
    assert_eq!(
        relation_columns(&sum, &table("orders")),
        columns(&["amount", "status", "user_id"])
    );
}

#[test]
fn a_junction_reads_both_keys_and_its_filters_and_the_far_side_its_fields() {
    let tags = Relation::Join(join(
        "tags",
        JoinKind::ManyToMany {
            through: through(Some(vec![value_filter("active")])),
        },
        vec![field("label")],
    ));
    assert_eq!(
        relation_columns(&tags, &table("user_tags")),
        columns(&["active", "tag_id", "user_id"])
    );
    assert_eq!(
        relation_columns(&tags, &table("tags")),
        columns(&["id", "label"])
    );
}

#[test]
fn a_raw_filter_reads_every_column() {
    let mut orders = has_many_orders();
    orders.filters = Some(vec![raw_filter()]);
    assert_eq!(
        relation_columns(&Relation::Join(orders), &table("orders")),
        ColumnUse::All
    );
    let tags = Relation::Join(join(
        "tags",
        JoinKind::ManyToMany {
            through: through(Some(vec![raw_filter()])),
        },
        vec![field("label")],
    ));
    assert_eq!(relation_columns(&tags, &table("user_tags")), ColumnUse::All);
    assert_eq!(
        relation_columns(&tags, &table("tags")),
        columns(&["id", "label"])
    );
}

#[test]
fn the_root_reads_its_fields_filters_and_soft_delete_marker() {
    let mut schema = users(vec![
        field("email"),
        relation_field("orders", Relation::Join(has_many_orders())),
        relation_field(
            "team",
            Relation::Join(join(
                "teams",
                JoinKind::BelongsTo {
                    column: column("team_id"),
                },
                vec![field("name")],
            )),
        ),
    ]);
    schema.filters = Some(vec![value_filter("tenant")]);
    schema.soft_delete = Some(SoftDelete::Column(SoftDeleteColumn {
        column: column("deleted_at"),
        when: Some(vec![value_filter("kind")]),
    }));
    assert_eq!(
        root_columns(&schema),
        columns(&["deleted_at", "email", "id", "kind", "team_id", "tenant"])
    );

    schema.filters = Some(vec![raw_filter()]);
    assert_eq!(root_columns(&schema), ColumnUse::All);
}

#[test]
fn only_a_child_or_junction_holds_a_row_link() {
    let orders = Relation::Join(has_many_orders());
    assert_eq!(
        row_link(&orders),
        Some((&table("orders"), &column("user_id")))
    );
    let team = Relation::Join(join(
        "teams",
        JoinKind::BelongsTo {
            column: column("team_id"),
        },
        Vec::new(),
    ));
    assert_eq!(row_link(&team), None);
}

fn key() -> RowKey {
    RowKey(vec![(column("id"), GenericValue::Int(1))])
}

fn image(id: i32, name: &str, seen: i64) -> RowImage {
    RowImage(vec![
        (column("id"), GenericValue::Int(id)),
        (column("name"), GenericValue::String(name.into())),
        (column("seen"), GenericValue::BigInt(seen)),
    ])
}

#[test]
fn an_update_of_unused_columns_touches_nothing() {
    let use_ = columns(&["name"]);
    assert!(!use_.touched_by(&key(), Some(&image(1, "a", 1)), Some(&image(1, "a", 2))));
}

#[test]
fn an_update_of_a_used_column_touches() {
    let use_ = columns(&["name"]);
    assert!(use_.touched_by(&key(), Some(&image(1, "a", 1)), Some(&image(1, "b", 1))));
}

#[test]
fn a_primary_key_change_touches() {
    let use_ = columns(&["name"]);
    assert!(use_.touched_by(&key(), Some(&image(1, "a", 1)), Some(&image(2, "a", 1))));
}

#[test]
fn a_missing_image_or_column_touches() {
    let use_ = columns(&["name"]);
    assert!(use_.touched_by(&key(), None, Some(&image(1, "a", 1))));
    assert!(use_.touched_by(&key(), Some(&image(1, "a", 1)), None));
    let without_name = RowImage(vec![(column("id"), GenericValue::Int(1))]);
    assert!(use_.touched_by(&key(), Some(&without_name), Some(&without_name)));
}

#[test]
fn every_column_use_is_always_touched() {
    assert!(ColumnUse::All.touched_by(&key(), Some(&image(1, "a", 1)), Some(&image(1, "a", 1))));
}
