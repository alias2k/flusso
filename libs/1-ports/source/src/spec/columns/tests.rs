use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, DatabaseSchema, FilterOp, FilterValue,
    FlussoType, Join, JoinKind, OrderBy, RawFilter, RawFilterValue, SoftDeleteColumn,
    SoftDeleteField, Through, ValueOpFilter, ViaHop,
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
        emit: true,
        source: FieldSource::Column(Column {
            column: column(name),
            ty: FlussoType::Keyword,
            nullable: false,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

fn relation_field(name: &str, relation: Relation) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
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

fn raw_filter(sql: &str) -> Filter {
    Filter::Raw(RawFilter {
        raw: RawFilterValue::try_new(sql).unwrap(),
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
        indexed: true,
        fields,
        on: vec![],
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

/// What `relation` reads from `table`, as the one relation of a `users` index.
fn reads(relation: &Relation, table_name: &str) -> ColumnUse {
    let schema = users(vec![relation_field("rel", relation.clone())]);
    relation_columns(&schema, relation, &table(table_name))
}

fn tags(through_filters: Option<Vec<Filter>>, filters: Option<Vec<Filter>>) -> Relation {
    let mut tags = join(
        "tags",
        JoinKind::ManyToMany {
            through: through(through_filters),
        },
        vec![field("label")],
    );
    tags.filters = filters;
    Relation::Join(tags)
}

#[test]
fn a_child_reads_its_fields_belongs_to_columns_and_foreign_key() {
    let orders = Relation::Join(has_many_orders());
    assert_eq!(
        reads(&orders, "orders"),
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
        reads(&Relation::Join(orders), "orders"),
        columns(&["id", "placed_at", "shop_id", "status", "total", "user_id"])
    );
}

#[test]
fn an_aggregate_reads_its_input_filters_and_link() {
    let sum = Relation::Aggregate(Aggregate {
        table: table("orders"),
        op: AggregateOp::Sum(column("amount").into()),
        key: AggregateKey::Direct(column("user_id")),
        value_type: None,
        filters: Some(vec![value_filter("status")]),
        distinct: false,
        via: Vec::new(),
        on: vec![],
    });
    assert_eq!(
        reads(&sum, "orders"),
        columns(&["amount", "status", "user_id"])
    );
}

#[test]
fn an_aggregate_reads_every_column_of_its_expression() {
    for op in [
        AggregateOp::Sum,
        AggregateOp::Avg,
        AggregateOp::Min,
        AggregateOp::Max,
    ] {
        let value = Relation::Aggregate(Aggregate {
            table: table("order_lines"),
            op: op("quantity * (unit_price - discount) / 100".parse().unwrap()),
            key: AggregateKey::Direct(column("order_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: vec![],
        });
        assert_eq!(
            reads(&value, "order_lines"),
            columns(&["discount", "order_id", "quantity", "unit_price"])
        );
    }
}

#[test]
fn a_junction_reads_both_keys_and_its_filters_and_the_far_side_its_fields() {
    let tags = tags(Some(vec![value_filter("active")]), None);
    assert_eq!(
        reads(&tags, "user_tags"),
        columns(&["active", "tag_id", "user_id"])
    );
    assert_eq!(reads(&tags, "tags"), columns(&["id", "label"]));
}

#[test]
fn a_raw_filter_on_its_own_table_reads_every_column() {
    let mut orders = has_many_orders();
    orders.filters = Some(vec![raw_filter("status <> 'gone'")]);
    assert_eq!(reads(&Relation::Join(orders), "orders"), ColumnUse::All);
}

/// `placed_at >= root.trial_start` reads the root row from a child's filter.
#[test]
fn a_raw_filter_on_a_child_can_read_the_root_row() {
    let mut orders = has_many_orders();
    orders.filters = Some(vec![raw_filter("placed_at >= root.trial_start")]);
    let schema = users(vec![
        field("email"),
        relation_field("orders", Relation::Join(orders)),
    ]);
    assert_eq!(root_columns(&schema), ColumnUse::All);
}

/// A far-table filter can name a junction column and a junction filter a
/// far-table column: both sides read everything.
#[test]
fn a_raw_filter_on_either_side_of_a_junction_reads_both_sides() {
    for tags in [
        tags(None, Some(vec![raw_filter("user_tags.pinned")])),
        tags(Some(vec![raw_filter("tags.label <> ''")]), None),
    ] {
        assert_eq!(reads(&tags, "user_tags"), ColumnUse::All);
        assert_eq!(reads(&tags, "tags"), ColumnUse::All);
    }
}

/// A nested relation's filter can name an enclosing level's column unqualified.
#[test]
fn a_raw_filter_on_a_nested_relation_reads_its_parent_level() {
    let mut items = join(
        "items",
        JoinKind::HasMany {
            foreign_key: column("order_id"),
        },
        vec![field("sku")],
    );
    items.filters = Some(vec![raw_filter("qty > orders.min_qty")]);
    let mut orders = has_many_orders();
    orders
        .fields
        .push(relation_field("items", Relation::Join(items)));
    let orders = Relation::Join(orders);
    assert_eq!(reads(&orders, "orders"), ColumnUse::All);

    let schema = users(vec![
        relation_field("orders", orders.clone()),
        relation_field("team", tags(None, None)),
    ]);
    assert_eq!(
        relation_columns(&schema, &tags(None, None), &table("tags")),
        ColumnUse::All,
        "one raw filter anywhere turns the whole index to every column"
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

    schema.soft_delete = Some(SoftDelete::Column(SoftDeleteColumn {
        column: column("deleted_at"),
        when: Some(vec![raw_filter("kind = 'x'")]),
    }));
    assert_eq!(root_columns(&schema), ColumnUse::All);

    schema.soft_delete = None;
    schema.filters = Some(vec![raw_filter("tenant = 1")]);
    assert_eq!(root_columns(&schema), ColumnUse::All);
}

fn marked_by(field_name: &str, fields: Vec<Field>) -> IndexSchema {
    let mut schema = users(fields);
    schema.soft_delete = Some(SoftDelete::Field(SoftDeleteField {
        field: FieldName::try_new(field_name).unwrap(),
        when: None,
    }));
    schema
}

fn named(name: &str, column_name: &str) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        ..field(column_name)
    }
}

/// A `soft_delete` field projected only inside a relation names that column,
/// read on the root row, as the document query has always done.
#[test]
fn a_soft_delete_field_inside_a_relation_names_its_column_on_the_root() {
    let mut orders = has_many_orders();
    orders.fields.push(field("deleted_at"));
    let schema = marked_by(
        "deleted_at",
        vec![
            field("name"),
            relation_field("orders", Relation::Join(orders)),
        ],
    );
    assert_eq!(soft_delete_column(&schema), Some(&column("deleted_at")));
    assert_eq!(
        root_columns(&schema),
        columns(&["deleted_at", "id", "name"])
    );
}

/// The search is depth-first in declaration order: a relation listed before a
/// root-level field of the same name wins.
#[test]
fn a_soft_delete_field_takes_the_first_match_depth_first() {
    let mut orders = has_many_orders();
    orders.fields.push(named("deleted", "cancelled"));
    let schema = marked_by(
        "deleted",
        vec![
            relation_field("orders", Relation::Join(orders)),
            named("deleted", "archived"),
        ],
    );
    assert_eq!(soft_delete_column(&schema), Some(&column("cancelled")));
    assert_eq!(
        root_columns(&schema),
        columns(&["archived", "cancelled", "id"])
    );
}

/// No field of that name, or a first match that is not a column: no column,
/// so the root reads everything.
#[test]
fn a_soft_delete_field_without_a_column_resolves_to_none() {
    let schema = marked_by("deleted", vec![field("name")]);
    assert_eq!(soft_delete_column(&schema), None);
    assert_eq!(root_columns(&schema), ColumnUse::All);

    let schema = marked_by(
        "orders",
        vec![
            relation_field("orders", Relation::Join(has_many_orders())),
            field("orders"),
        ],
    );
    assert_eq!(soft_delete_column(&schema), None);
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

fn touched(use_: &ColumnUse, before: Option<&RowImage>, after: Option<&RowImage>) -> bool {
    use_.touched_by(&RowDiff::new(&key(), before, after))
}

#[test]
fn an_update_of_unused_columns_touches_nothing() {
    let use_ = columns(&["name"]);
    assert!(!touched(
        &use_,
        Some(&image(1, "a", 1)),
        Some(&image(1, "a", 2))
    ));
}

#[test]
fn an_update_of_a_used_column_touches() {
    let use_ = columns(&["name"]);
    assert!(touched(
        &use_,
        Some(&image(1, "a", 1)),
        Some(&image(1, "b", 1))
    ));
}

#[test]
fn a_primary_key_change_touches() {
    let use_ = columns(&["name"]);
    assert!(touched(
        &use_,
        Some(&image(1, "a", 1)),
        Some(&image(2, "a", 1))
    ));
}

#[test]
fn a_missing_image_or_column_touches() {
    let use_ = columns(&["name"]);
    assert!(touched(&use_, None, Some(&image(1, "a", 1))));
    assert!(touched(&use_, Some(&image(1, "a", 1)), None));
    let without_name = RowImage(vec![(column("id"), GenericValue::Int(1))]);
    assert!(touched(&use_, Some(&without_name), Some(&without_name)));
    let without_key = RowImage(vec![(column("name"), GenericValue::String("a".into()))]);
    assert!(touched(&use_, Some(&without_key), Some(&without_key)));
    assert!(use_.touched_by(&RowDiff::unknown()));
}

#[test]
fn every_column_use_is_always_touched() {
    assert!(touched(
        &ColumnUse::All,
        Some(&image(1, "a", 1)),
        Some(&image(1, "a", 1))
    ));
}

/// users → projects (archived?) → tasks: does the user have a completed task.
fn task_flag(project_filters: Option<Vec<Filter>>) -> Relation {
    Relation::Aggregate(Aggregate {
        table: table("tasks"),
        op: AggregateOp::Exists,
        key: AggregateKey::Direct(column("project_id")),
        value_type: None,
        filters: Some(vec![value_filter("status")]),
        distinct: false,
        via: vec![ViaHop {
            table: table("projects"),
            foreign_key: column("user_id"),
            filters: project_filters,
            on: vec![],
        }],
        on: vec![],
    })
}

#[test]
fn a_via_hop_reads_its_foreign_key_and_filters() {
    let flag = task_flag(Some(vec![value_filter("archived")]));
    let schema = users(vec![relation_field("flag", flag.clone())]);
    let hop = flag.via().first().unwrap();
    assert_eq!(
        via_columns(&schema, hop, &[]),
        columns(&["archived", "user_id"])
    );
    assert_eq!(reads(&flag, "tasks"), columns(&["project_id", "status"]));
}

#[test]
fn a_raw_filter_on_a_via_hop_reads_every_column_of_the_index() {
    let flag = task_flag(Some(vec![raw_filter("archived_at is null")]));
    let schema = users(vec![field("email"), relation_field("flag", flag.clone())]);
    assert_eq!(root_columns(&schema), ColumnUse::All);
    assert_eq!(
        via_columns(&schema, flag.via().first().unwrap(), &[]),
        ColumnUse::All
    );
}

fn pair(local: &str, related: &str) -> ColumnPair {
    ColumnPair {
        local: column(local),
        related: column(related),
    }
}

#[test]
fn an_on_pair_reads_related_on_the_child_and_local_on_the_parent() {
    let mut orders = has_many_orders();
    orders.on = vec![pair("tenant_id", "org_id")];
    let orders = Relation::Join(orders);
    let schema = users(vec![
        field("email"),
        relation_field("orders", orders.clone()),
    ]);
    assert_eq!(
        reads(&orders, "orders"),
        columns(&["id", "org_id", "shop_id", "total", "user_id"])
    );
    assert_eq!(
        root_columns(&schema),
        columns(&["email", "id", "tenant_id"])
    );
}

#[test]
fn a_belongs_to_pair_reads_local_on_the_enclosing_join() {
    let mut orders = has_many_orders();
    if let Some(FieldSource::Relation(Relation::Join(shop))) =
        orders.fields.get_mut(1).map(|field| &mut field.source)
    {
        shop.on = vec![pair("region", "region_code")];
    }
    let shop = match orders.fields.get(1).map(|field| &field.source) {
        Some(FieldSource::Relation(relation)) => relation.clone(),
        _ => unreachable!(),
    };
    let orders = Relation::Join(orders);
    assert_eq!(
        reads(&orders, "orders"),
        columns(&["id", "region", "shop_id", "total", "user_id"])
    );
    assert_eq!(
        reads(&shop, "shops"),
        columns(&["id", "name", "region_code"])
    );
}

#[test]
fn via_pairs_split_across_the_hops_they_join() {
    let mut flag = task_flag(None);
    if let Relation::Aggregate(aggregate) = &mut flag {
        aggregate.on = vec![pair("team_id", "team_id")];
        if let Some(hop) = aggregate.via.first_mut() {
            hop.on = vec![pair("tenant_id", "tenant_id")];
        }
    }
    let schema = users(vec![relation_field("flag", flag.clone())]);
    let (hop, next_on) = via_hops(&flag).next().unwrap();
    assert_eq!(
        via_columns(&schema, hop, next_on),
        columns(&["team_id", "tenant_id", "user_id"])
    );
    assert_eq!(
        reads(&flag, "tasks"),
        columns(&["project_id", "status", "team_id"])
    );
    assert_eq!(root_columns(&schema), columns(&["id", "tenant_id"]));
}
