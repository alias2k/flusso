#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use crate::common::{ColumnName, FieldName, IndexName, TableName};
use crate::config::{
    Aggregate, AggregateKey, AggregateOp, DatabaseSchema, Field, FieldSource, FlussoType,
    IndexSchema, Join, JoinKind, MappingType, Relation,
};

fn ids_schema(element_type: FlussoType) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("users").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new("orderIds").unwrap(),
            options: Default::default(),
            source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                table: TableName::try_new("orders").unwrap(),
                op: AggregateOp::Ids { element_type },
                key: AggregateKey::Direct(crate::common::ColumnName::try_new("user_id").unwrap()),
                value_type: None,
                filters: None,
                distinct: false,
                via: vec![],
            })),
        }],
    }
}

fn belongs_to_schema(nullable: bool) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("tickets").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new("author").unwrap(),
            options: Default::default(),
            source: FieldSource::Relation(Relation::Join(Join {
                table: TableName::try_new("users").unwrap(),
                kind: JoinKind::BelongsTo {
                    column: ColumnName::try_new("author_id").unwrap(),
                },
                primary_key: ColumnName::try_new("id").unwrap(),
                nullable,
                filters: None,
                order_by: None,
                limit: None,
                fields: vec![],
                indexed: true,
            })),
        }],
    }
}

#[test]
fn to_one_join_nullability_flows_into_the_mapping() {
    let mapping = belongs_to_schema(true).resolve(IndexName::try_new("tickets").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Object);
    assert!(field.nullable, "nullable to-one join → nullable object");

    let mapping = belongs_to_schema(false).resolve(IndexName::try_new("tickets").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Object);
    assert!(!field.nullable, "required to-one join → non-null object");
}

fn column_schema(name: &str, ty: FlussoType) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("accounts").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new(name).unwrap(),
            options: Default::default(),
            source: FieldSource::Column(crate::config::Column {
                column: ColumnName::try_new(name).unwrap(),
                ty,
                nullable: false,
                transforms: vec![],
                default: None,
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
            }),
        }],
    }
}

#[test]
fn decimal_maps_to_double_but_flags_decimal() {
    let m = column_schema("creditLimit", FlussoType::Decimal)
        .resolve(IndexName::try_new("accounts").unwrap());
    let field = &m.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Double);
    assert!(field.mapping.decimal, "a `decimal` column flags `decimal`");

    // A true `double` is the same OS type but must NOT be flagged.
    let m =
        column_schema("ratio", FlussoType::Double).resolve(IndexName::try_new("accounts").unwrap());
    let field = &m.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Double);
    assert!(!field.mapping.decimal, "a `double` column is not a decimal");
}

fn enum_schema(name: &str, order: Vec<String>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("accounts").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new(name).unwrap(),
            options: Default::default(),
            source: FieldSource::Column(crate::config::Column {
                column: ColumnName::try_new(name).unwrap(),
                ty: FlussoType::Enum,
                nullable: false,
                transforms: vec![],
                default: None,
                enum_order: order,
                subfields: true,
                indexed: true,
            }),
        }],
    }
}

#[test]
fn enum_order_projects_onto_the_mapping_but_type_stays_keyword() {
    let order = vec!["low".to_owned(), "medium".to_owned(), "high".to_owned()];
    let m = enum_schema("status", order.clone()).resolve(IndexName::try_new("accounts").unwrap());
    let field = &m.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Keyword);
    assert_eq!(field.mapping.enum_order.as_deref(), Some(order.as_slice()));

    // A bare enum (no declared order) carries no metadata — a plain keyword.
    let m = enum_schema("kind", Vec::new()).resolve(IndexName::try_new("accounts").unwrap());
    assert_eq!(m.fields[0].mapping.enum_order, None);
}

#[test]
fn ids_projects_to_a_non_null_element_typed_array() {
    let mapping = ids_schema(FlussoType::Long).resolve(IndexName::try_new("users").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Long);
    assert!(field.array);
    assert!(!field.nullable);

    let mapping = ids_schema(FlussoType::Keyword).resolve(IndexName::try_new("users").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Keyword);
    assert!(field.array);
    assert!(!field.nullable);
}

fn through() -> crate::config::Through {
    crate::config::Through {
        table: TableName::try_new("order_items").unwrap(),
        left_key: ColumnName::try_new("order_id").unwrap(),
        right_key: ColumnName::try_new("product_id").unwrap(),
        filters: None,
    }
}

fn through_schema() -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("orders").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            Field {
                field: FieldName::try_new("totalWeight").unwrap(),
                options: Default::default(),
                source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                    table: TableName::try_new("products").unwrap(),
                    op: AggregateOp::Sum(ColumnName::try_new("weight").unwrap()),
                    key: AggregateKey::Through(through()),
                    value_type: Some(FlussoType::Double),
                    filters: None,
                    distinct: false,
                    via: vec![],
                })),
            },
            Field {
                field: FieldName::try_new("products").unwrap(),
                options: Default::default(),
                source: FieldSource::Relation(Relation::Join(Join {
                    table: TableName::try_new("products").unwrap(),
                    kind: JoinKind::ManyToMany { through: through() },
                    primary_key: ColumnName::try_new("id").unwrap(),
                    nullable: false,
                    filters: None,
                    order_by: None,
                    limit: None,
                    fields: vec![],
                    indexed: true,
                })),
            },
        ],
    }
}

#[test]
fn a_through_schema_keeps_its_content_hash() {
    let mapping = through_schema().resolve(IndexName::try_new("orders").unwrap());
    assert_eq!(mapping.hash.to_string(), "af5b97ca");
}

#[test]
fn an_empty_junction_filter_list_keeps_the_content_hash() {
    let base = through_schema()
        .resolve(IndexName::try_new("orders").unwrap())
        .hash;
    let mut empty = through_schema();
    if let FieldSource::Relation(Relation::Aggregate(aggregate)) = &mut empty.fields[0].source
        && let AggregateKey::Through(through) = &mut aggregate.key
    {
        through.filters = Some(vec![]);
    }
    assert_eq!(
        base,
        empty.resolve(IndexName::try_new("orders").unwrap()).hash
    );
}

#[test]
fn opting_into_distinct_or_junction_filters_changes_the_content_hash() {
    let base = through_schema()
        .resolve(IndexName::try_new("orders").unwrap())
        .hash;

    let mut distinct = through_schema();
    if let FieldSource::Relation(Relation::Aggregate(aggregate)) = &mut distinct.fields[0].source {
        aggregate.distinct = true;
    }
    let distinct = distinct.resolve(IndexName::try_new("orders").unwrap()).hash;

    let mut filtered = through_schema();
    if let FieldSource::Relation(Relation::Aggregate(aggregate)) = &mut filtered.fields[0].source
        && let AggregateKey::Through(through) = &mut aggregate.key
    {
        through.filters = Some(vec![crate::config::Filter::NullCheck(
            crate::config::NullCheckFilter {
                column: ColumnName::try_new("cancelled_at").unwrap(),
                op: crate::config::NullOp::IsNull,
            },
        )]);
    }
    let filtered = filtered.resolve(IndexName::try_new("orders").unwrap()).hash;

    assert_ne!(base, distinct);
    assert_ne!(base, filtered);
    assert_ne!(distinct, filtered);
}

fn aggregate_field(name: &str, op: AggregateOp, value_type: Option<FlussoType>) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: TableName::try_new("tasks").unwrap(),
            op,
            key: AggregateKey::Direct(ColumnName::try_new("user_id").unwrap()),
            value_type,
            filters: None,
            distinct: false,
            via: vec![],
        })),
    }
}

fn direct_aggregates_schema() -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("users").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            aggregate_field("taskCount", AggregateOp::Count, None),
            aggregate_field(
                "effort",
                AggregateOp::Sum(ColumnName::try_new("effort").unwrap()),
                Some(FlussoType::Long),
            ),
            aggregate_field(
                "taskIds",
                AggregateOp::Ids {
                    element_type: FlussoType::Long,
                },
                None,
            ),
        ],
    }
}

fn via_hop(table: &str, foreign_key: &str) -> crate::config::ViaHop {
    crate::config::ViaHop {
        table: TableName::try_new(table).unwrap(),
        foreign_key: ColumnName::try_new(foreign_key).unwrap(),
        filters: None,
    }
}

#[test]
fn a_schema_without_via_keeps_its_content_hash() {
    let mapping = direct_aggregates_schema().resolve(IndexName::try_new("users").unwrap());
    assert_eq!(mapping.hash.to_string(), "e25bb2e9");
}

#[test]
fn an_aggregate_via_hops_changes_the_content_hash() {
    let base = direct_aggregates_schema()
        .resolve(IndexName::try_new("users").unwrap())
        .hash;
    let with_via = |hops: Vec<crate::config::ViaHop>| {
        let mut schema = direct_aggregates_schema();
        if let FieldSource::Relation(Relation::Aggregate(aggregate)) = &mut schema.fields[0].source
        {
            aggregate.via = hops;
        }
        schema.resolve(IndexName::try_new("users").unwrap()).hash
    };
    let one = with_via(vec![via_hop("projects", "user_id")]);
    let two = with_via(vec![
        via_hop("teams", "user_id"),
        via_hop("projects", "team_id"),
    ]);
    assert_eq!(with_via(vec![]), base);
    assert_ne!(base, one);
    assert_ne!(one, two);
}

#[test]
fn exists_maps_to_a_non_null_boolean() {
    let mut schema = direct_aggregates_schema();
    schema.fields = vec![aggregate_field("hasTask", AggregateOp::Exists, None)];
    let mapping = schema.resolve(IndexName::try_new("users").unwrap());
    let field = &mapping.fields[0];
    assert!(matches!(field.mapping.mapping_type, MappingType::Boolean));
    assert!(!field.nullable);
    assert_ne!(
        mapping.hash,
        direct_aggregates_schema()
            .resolve(IndexName::try_new("users").unwrap())
            .hash
    );
}

fn column_field(name: &str, ty: FlussoType) -> Field {
    let mut options = ty.default_options();
    if matches!(ty, FlussoType::Map { .. }) {
        options.insert(
            "dynamic".to_owned(),
            crate::common::GenericValue::Bool(true),
        );
    }
    Field {
        field: FieldName::try_new(name).unwrap(),
        options,
        source: FieldSource::Column(crate::config::Column {
            column: ColumnName::try_new(name).unwrap(),
            ty,
            nullable: true,
            transforms: vec![],
            default: None,
            enum_order: vec![],
            subfields: true,
            indexed: true,
        }),
    }
}

fn columns_schema(fields: Vec<Field>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("products").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields,
    }
}

fn map_of(values: FlussoType) -> FlussoType {
    FlussoType::Map {
        values: Box::new(values),
    }
}

/// Every column type but `map`, including the `json` a map reads from.
fn mapless_schema() -> IndexSchema {
    let mut status = column_field("status", FlussoType::Enum);
    if let FieldSource::Column(column) = &mut status.source {
        column.enum_order = vec!["open".to_owned(), "closed".to_owned()];
    }
    columns_schema(vec![
        column_field("name", FlussoType::Text),
        column_field("sku", FlussoType::Identifier),
        column_field("code", FlussoType::Keyword),
        status,
        column_field("price", FlussoType::Decimal),
        column_field("stock", FlussoType::Integer),
        column_field("created", FlussoType::Timestamp),
        column_field("attrs", FlussoType::Json),
        column_field(
            "rank",
            FlussoType::Custom {
                postgres: vec!["numeric".to_owned()],
                opensearch: "scaled_float".to_owned(),
            },
        ),
    ])
}

#[test]
fn a_schema_without_a_map_keeps_its_content_hash() {
    let mapping = mapless_schema().resolve(IndexName::try_new("products").unwrap());
    assert_eq!(mapping.hash.to_string(), "9156f06f");
}

#[test]
fn a_map_schema_moves_off_its_dynamically_mapped_hash() {
    let pre_typed_keys = ["ab9831b4", "c99e8742"];
    for (values, old) in [FlussoType::Text, FlussoType::Double]
        .into_iter()
        .zip(pre_typed_keys)
    {
        let schema = columns_schema(vec![
            column_field("name", FlussoType::Text),
            column_field("title", map_of(values)),
        ]);
        let hash = schema.resolve(IndexName::try_new("products").unwrap()).hash;
        assert_ne!(hash.to_string(), old);
    }
}

#[test]
fn a_map_key_maps_like_a_plain_field_of_its_values_type() {
    for values in [
        FlussoType::Text,
        FlussoType::Identifier,
        FlussoType::Keyword,
        FlussoType::Decimal,
        FlussoType::Long,
        FlussoType::Timestamp,
    ] {
        let plain = columns_schema(vec![column_field("title", values.clone())])
            .resolve(IndexName::try_new("products").unwrap());
        let map = columns_schema(vec![column_field("title", map_of(values))])
            .resolve(IndexName::try_new("products").unwrap());
        let plain = &plain.fields[0].mapping;
        let map = &map.fields[0].mapping;
        assert_eq!(map.mapping_type, MappingType::Object);
        let key = map.map_values.as_deref().unwrap();
        assert_eq!(key.mapping_type, plain.mapping_type);
        assert_eq!(key.extra, plain.extra);
        assert_eq!(key.decimal, plain.decimal);
        assert!(key.map_values.is_none());
    }
}

#[test]
fn a_map_serializes_its_key_mapping_as_values() {
    let mapping = columns_schema(vec![column_field("title", map_of(FlussoType::Identifier))])
        .resolve(IndexName::try_new("products").unwrap());
    let json = serde_json::to_value(&mapping.fields[0].mapping).unwrap();
    assert_eq!(json["type"], "object");
    assert_eq!(json["values"]["type"], "text");
    assert!(json["values"].get("analyzer").is_some());
}

/// The two pinned hashes above already cover `text`/`keyword`/`json` columns
/// and a join; pinned again here so the reason they must not move is named.
#[test]
fn a_schema_that_never_opts_out_keeps_its_content_hash() {
    let columns = mapless_schema().resolve(IndexName::try_new("products").unwrap());
    let joined = through_schema().resolve(IndexName::try_new("orders").unwrap());
    assert_eq!(columns.hash.to_string(), "9156f06f");
    assert_eq!(joined.hash.to_string(), "af5b97ca");
}

fn set_column(schema: &mut IndexSchema, name: &str, edit: impl FnOnce(&mut crate::config::Column)) {
    let field = schema
        .fields
        .iter_mut()
        .find(|field| field.field.as_ref() == name)
        .unwrap();
    if let FieldSource::Column(column) = &mut field.source {
        edit(column);
    }
}

#[test]
fn opting_out_of_subfields_or_indexing_changes_the_content_hash() {
    let products = IndexName::try_new("products").unwrap();
    let base = mapless_schema().resolve(products.clone()).hash;

    let mut bare = mapless_schema();
    set_column(&mut bare, "name", |column| column.subfields = false);
    let bare = bare.resolve(products.clone()).hash;

    let mut stored = mapless_schema();
    set_column(&mut stored, "attrs", |column| column.indexed = false);
    let stored = stored.resolve(products).hash;

    let mut unindexed_join = through_schema();
    if let FieldSource::Relation(Relation::Join(join)) = &mut unindexed_join.fields[1].source {
        join.indexed = false;
    }
    let orders = IndexName::try_new("orders").unwrap();
    let unindexed_join = unindexed_join.resolve(orders.clone()).hash;

    assert_ne!(base, bare);
    assert_ne!(base, stored);
    assert_ne!(bare, stored);
    assert_ne!(through_schema().resolve(orders).hash, unindexed_join);
}

#[test]
fn opt_outs_flow_into_the_mapping_and_serialize_only_when_off() {
    let mut schema = mapless_schema();
    set_column(&mut schema, "name", |column| column.subfields = false);
    set_column(&mut schema, "attrs", |column| column.indexed = false);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let by_name = |name: &str| {
        &mapping
            .fields
            .iter()
            .find(|field| field.name.as_ref() == name)
            .unwrap()
            .mapping
    };

    assert!(!by_name("name").subfields);
    assert!(by_name("name").indexed);
    assert!(!by_name("attrs").indexed);
    assert!(by_name("code").subfields && by_name("code").indexed);

    let name = serde_json::to_value(by_name("name")).unwrap();
    assert_eq!(name["subfields"], false);
    assert!(name.get("indexed").is_none());
    let attrs = serde_json::to_value(by_name("attrs")).unwrap();
    assert_eq!(attrs["indexed"], false);
    let code = serde_json::to_value(by_name("code")).unwrap();
    assert!(code.get("subfields").is_none() && code.get("indexed").is_none());
}

#[test]
fn an_unindexed_join_keeps_its_shape_and_children() {
    let mut schema = through_schema();
    if let FieldSource::Relation(Relation::Join(join)) = &mut schema.fields[1].source {
        join.indexed = false;
        join.fields = vec![column_field("name", FlussoType::Text)];
    }
    let mapping = schema.resolve(IndexName::try_new("orders").unwrap());
    let products = &mapping.fields[1];
    assert_eq!(products.mapping.mapping_type, MappingType::Nested);
    assert!(!products.mapping.indexed);
    assert_eq!(products.children.len(), 1);
}
