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
            emit: true,
            source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                table: TableName::try_new("orders").unwrap(),
                op: AggregateOp::Ids { element_type },
                key: AggregateKey::Direct(crate::common::ColumnName::try_new("user_id").unwrap()),
                value_type: None,
                filters: None,
                distinct: false,
                via: vec![],
                on: vec![],
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
            emit: true,
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
                on: vec![],
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
            emit: true,
            source: FieldSource::Column(crate::config::Column {
                column: ColumnName::try_new(name).unwrap(),
                ty,
                nullable: false,
                transforms: vec![],
                default: None,
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
                array: false,
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
            emit: true,
            source: FieldSource::Column(crate::config::Column {
                column: ColumnName::try_new(name).unwrap(),
                ty: FlussoType::Enum,
                nullable: false,
                transforms: vec![],
                default: None,
                enum_order: order,
                subfields: true,
                indexed: true,
                array: false,
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
                emit: true,
                source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                    table: TableName::try_new("products").unwrap(),
                    op: AggregateOp::Sum(ColumnName::try_new("weight").unwrap().into()),
                    key: AggregateKey::Through(through()),
                    value_type: Some(FlussoType::Double),
                    filters: None,
                    distinct: false,
                    via: vec![],
                    on: vec![],
                })),
            },
            Field {
                field: FieldName::try_new("products").unwrap(),
                options: Default::default(),
                emit: true,
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
                    on: vec![],
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
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: TableName::try_new("tasks").unwrap(),
            op,
            key: AggregateKey::Direct(ColumnName::try_new("user_id").unwrap()),
            value_type,
            filters: None,
            distinct: false,
            via: vec![],
            on: vec![],
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
                AggregateOp::Sum(ColumnName::try_new("effort").unwrap().into()),
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
        on: vec![],
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
        emit: true,
        source: FieldSource::Column(crate::config::Column {
            column: ColumnName::try_new(name).unwrap(),
            ty,
            nullable: true,
            transforms: vec![],
            default: None,
            enum_order: vec![],
            subfields: true,
            indexed: true,
            array: false,
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

/// Where map schemas live at `MAP_KEY_MAPPING_REVISION` 2: moving these moves
/// every map index to a new physical index.
#[test]
fn a_map_schema_keeps_its_revision_two_content_hash() {
    let hashes: Vec<String> = [FlussoType::Text, FlussoType::Keyword, FlussoType::Double]
        .into_iter()
        .map(|values| {
            columns_schema(vec![
                column_field("name", FlussoType::Text),
                column_field("title", map_of(values)),
            ])
            .resolve(IndexName::try_new("products").unwrap())
            .hash
            .to_string()
        })
        .collect();
    assert_eq!(hashes, ["d6964182", "e1ee0108", "a48ee9c0"]);
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

/// Every place `on` can sit: a `belongs_to`, a `has_many`, an aggregate and its
/// `via` hop — none of them with pairs.
fn relations_without_on_schema() -> IndexSchema {
    let mut schema = direct_aggregates_schema();
    if let FieldSource::Relation(Relation::Aggregate(aggregate)) = &mut schema.fields[0].source {
        aggregate.via = vec![via_hop("projects", "user_id")];
    }
    let join = |name: &str, kind: JoinKind| Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("orders").unwrap(),
            kind,
            primary_key: ColumnName::try_new("id").unwrap(),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![],
            indexed: true,
            on: vec![],
        })),
    };
    schema.fields.push(join(
        "lastOrder",
        JoinKind::BelongsTo {
            column: ColumnName::try_new("last_order_id").unwrap(),
        },
    ));
    schema.fields.push(join(
        "orders",
        JoinKind::HasMany {
            foreign_key: ColumnName::try_new("user_id").unwrap(),
        },
    ));
    schema
}

#[test]
fn a_schema_without_on_keeps_its_content_hash() {
    let mapping = relations_without_on_schema().resolve(IndexName::try_new("users").unwrap());
    assert_eq!(mapping.hash.to_string(), "1feea849");
}

#[test]
fn on_pairs_change_the_content_hash() {
    let pair = || crate::config::ColumnPair {
        local: ColumnName::try_new("tenant_id").unwrap(),
        related: ColumnName::try_new("tenant_id").unwrap(),
    };
    let hash = |schema: IndexSchema| schema.resolve(IndexName::try_new("users").unwrap()).hash;
    let base = hash(relations_without_on_schema());

    let mut on_aggregate = relations_without_on_schema();
    let mut on_hop = relations_without_on_schema();
    if let FieldSource::Relation(Relation::Aggregate(aggregate)) =
        &mut on_aggregate.fields[0].source
    {
        aggregate.on = vec![pair()];
    }
    if let FieldSource::Relation(Relation::Aggregate(aggregate)) = &mut on_hop.fields[0].source {
        aggregate.via[0].on = vec![pair()];
    }
    let mut on_belongs_to = relations_without_on_schema();
    let mut on_has_many = relations_without_on_schema();
    for (schema, at) in [(&mut on_belongs_to, 3), (&mut on_has_many, 4)] {
        if let FieldSource::Relation(Relation::Join(join)) = &mut schema.fields[at].source {
            join.on = vec![pair()];
        }
    }
    let hashes = [
        base,
        hash(on_aggregate),
        hash(on_hop),
        hash(on_belongs_to),
        hash(on_has_many),
    ];
    for (i, a) in hashes.iter().enumerate() {
        for b in hashes.iter().skip(i + 1) {
            assert_ne!(a, b);
        }
    }
}

fn script_field(name: &str, ty: FlussoType, array: bool) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Script(crate::config::Script {
            source: "this.name".to_owned(),
            ty,
            nullable: false,
            array,
            enum_order: vec![],
            subfields: true,
        }),
    }
}

#[test]
fn a_script_resolves_to_its_declared_leaf_type() {
    let mut fields = vec![column_field("name", FlussoType::Text)];
    fields.push(script_field("label", FlussoType::Keyword, false));
    fields.push(script_field("prices", FlussoType::Decimal, true));
    let mapping = columns_schema(fields).resolve(IndexName::try_new("products").unwrap());

    let label = &mapping.fields[1];
    assert_eq!(label.mapping.mapping_type, MappingType::Keyword);
    assert!(!label.nullable);
    assert!(!label.array);

    let prices = &mapping.fields[2];
    assert_eq!(prices.mapping.mapping_type, MappingType::Double);
    assert!(prices.mapping.decimal);
    assert!(prices.array);
}

#[test]
fn an_input_only_field_has_no_mapping() {
    let mut hidden = column_field("cost", FlussoType::Double);
    hidden.emit = false;
    let schema = columns_schema(vec![column_field("name", FlussoType::Text), hidden]);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let names: Vec<&str> = mapping.fields.iter().map(|f| f.name.as_ref()).collect();
    assert_eq!(names, ["name"]);
}

#[test]
fn scripts_and_input_only_fields_change_the_content_hash() {
    let products = IndexName::try_new("products").unwrap();
    let base = mapless_schema().resolve(products.clone()).hash;

    let mut hidden = mapless_schema();
    hidden.fields[0].emit = false;
    let hidden = hidden.resolve(products.clone()).hash;

    let mut scripted = mapless_schema();
    scripted
        .fields
        .push(script_field("label", FlussoType::Keyword, false));
    let scripted_hash = scripted.resolve(products.clone()).hash;

    let mut edited = scripted.clone();
    if let FieldSource::Script(script) = &mut edited.fields.last_mut().unwrap().source {
        script.source = "this.code".to_owned();
    }
    let edited = edited.resolve(products).hash;

    assert_ne!(base, hidden);
    assert_ne!(base, scripted_hash);
    assert_ne!(scripted_hash, edited);
}

#[test]
fn emit_serializes_only_when_off() {
    let mut field = column_field("cost", FlussoType::Double);
    let on = serde_json::to_value(&field).unwrap();
    assert!(on.get("emit").is_none());
    field.emit = false;
    let off = serde_json::to_value(&field).unwrap();
    assert_eq!(off["emit"], serde_json::Value::Bool(false));
}

fn plain_column_aggregates_schema() -> IndexSchema {
    let column = || crate::config::Expression::Column(ColumnName::try_new("amount").unwrap());
    let mut schema = direct_aggregates_schema();
    schema.fields = vec![
        aggregate_field(
            "total",
            AggregateOp::Sum(column()),
            Some(FlussoType::Double),
        ),
        aggregate_field("mean", AggregateOp::Avg(column()), None),
        aggregate_field(
            "least",
            AggregateOp::Min(column()),
            Some(FlussoType::Double),
        ),
        aggregate_field("most", AggregateOp::Max(column()), Some(FlussoType::Double)),
    ];
    schema
}

#[test]
fn a_plain_column_aggregate_keeps_its_content_hash() {
    let mapping = plain_column_aggregates_schema().resolve(IndexName::try_new("orders").unwrap());
    assert_eq!(mapping.hash.to_string(), "6fafe945");
}

#[test]
fn an_expression_input_changes_the_content_hash() {
    let base = plain_column_aggregates_schema()
        .resolve(IndexName::try_new("orders").unwrap())
        .hash;
    let mut schema = plain_column_aggregates_schema();
    schema.fields[0] = aggregate_field(
        "total",
        AggregateOp::Sum("amount * 2".parse().unwrap()),
        Some(FlussoType::Double),
    );
    assert_ne!(
        base,
        schema.resolve(IndexName::try_new("orders").unwrap()).hash
    );
}

#[test]
fn two_spellings_of_one_expression_hash_the_same() {
    let hash = |input: &str| {
        let mut schema = plain_column_aggregates_schema();
        schema.fields[0] = aggregate_field(
            "total",
            AggregateOp::Sum(input.parse().unwrap()),
            Some(FlussoType::Double),
        );
        schema.resolve(IndexName::try_new("orders").unwrap()).hash
    };
    assert_eq!(
        hash("quantity*unit_price"),
        hash(" (quantity) *  unit_price ")
    );
}

#[test]
fn an_array_column_changes_the_content_hash_only_while_it_is_set() {
    let products = IndexName::try_new("products").unwrap();
    let base = mapless_schema().resolve(products.clone()).hash;

    let mut array = mapless_schema();
    set_column(&mut array, "name", |column| column.array = true);
    let array = array.resolve(products.clone()).hash;

    let mut unset = mapless_schema();
    set_column(&mut unset, "name", |column| column.array = false);

    assert_ne!(base, array);
    assert_eq!(base, unset.resolve(products).hash);
}

#[test]
fn an_array_column_projects_to_an_array_of_its_element() {
    let mut schema = mapless_schema();
    set_column(&mut schema, "name", |column| column.array = true);
    set_column(&mut schema, "attrs", |column| column.array = true);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let by_name = |name: &str| {
        mapping
            .fields
            .iter()
            .find(|field| field.name.as_ref() == name)
            .unwrap()
    };

    let name = by_name("name");
    assert!(name.array);
    assert_eq!(name.mapping.mapping_type, MappingType::Text);
    assert!(by_name("attrs").array);
    assert_eq!(by_name("attrs").mapping.mapping_type, MappingType::Object);
    assert!(!by_name("code").array);
}

fn constant_field(name: &str, value: crate::common::GenericValue) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Constant(value),
    }
}

fn hash_of(schema: &IndexSchema) -> String {
    schema
        .resolve(IndexName::try_new("products").unwrap())
        .hash
        .to_string()
}

/// A schema with a `binary` column, or an `ids` of `binary`, moves off the
/// hash it had while the column was indexed as Postgres hex text, so the index
/// is rebuilt once with the bytes as base64. The pins are the hashes before.
#[test]
fn a_binary_schema_moves_off_its_hex_text_hash() {
    let binary = columns_schema(vec![
        column_field("name", FlussoType::Text),
        column_field("thumbnail", FlussoType::Binary),
    ]);
    assert_ne!(hash_of(&binary), "28a742cb");
    assert_ne!(hash_of(&ids_schema(FlussoType::Binary)), "7690cadc");
}

#[test]
fn a_schema_without_binary_or_an_object_constant_keeps_its_content_hash() {
    use crate::common::GenericValue;
    let scalars = columns_schema(vec![
        column_field("name", FlussoType::Text),
        constant_field("source", GenericValue::String("crm".to_owned())),
        constant_field("weight", GenericValue::BigInt(5)),
        constant_field(
            "tags",
            GenericValue::Array(vec![GenericValue::String("a".to_owned())]),
        ),
        constant_field("nothing", GenericValue::Null),
    ]);
    assert_eq!(hash_of(&scalars), "894b0c70");
    assert_eq!(hash_of(&ids_schema(FlussoType::Long)), "4d3e4e36");
}

/// A constant holding an object is built as written and stored unindexed: the
/// strict root refuses keys in an `object` with no children. Its schema moves
/// off the hash it had while it built `null`; a scalar constant stays indexed.
#[test]
fn a_constant_holding_an_object_is_stored_unindexed_and_moves_its_hash() {
    use crate::common::GenericValue;
    let object = GenericValue::Map(
        [("a".to_owned(), GenericValue::BigInt(1))]
            .into_iter()
            .collect(),
    );
    let schema = columns_schema(vec![
        constant_field("meta", object.clone()),
        constant_field("rows", GenericValue::Array(vec![object])),
        constant_field("source", GenericValue::String("crm".to_owned())),
    ]);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let indexed: Vec<bool> = mapping.fields.iter().map(|f| f.mapping.indexed).collect();
    assert_eq!(indexed, [false, false, true]);
    assert_eq!(mapping.fields[0].mapping.mapping_type, MappingType::Object);
    assert_ne!(mapping.hash.to_string(), "56ebe48b");
}

fn list_constant_schema(items: Vec<crate::common::GenericValue>) -> IndexSchema {
    columns_schema(vec![constant_field(
        "codes",
        crate::common::GenericValue::Array(items),
    )])
}

/// A list whose scalars are one kind (integers and floats are both numbers,
/// nulls aside, nested lists flattened; a whole number after a fractional
/// first one is fine, a `double` takes it) stays indexed and keeps the hash it
/// had before constants were built as written. The pins are those hashes.
#[test]
fn a_constant_list_of_one_kind_stays_indexed_and_keeps_its_hash() {
    use crate::common::GenericValue as V;
    for (items, pin) in [
        (
            vec![
                V::Double(2.5),
                V::BigInt(1),
                V::Null,
                V::Array(vec![V::BigInt(3)]),
            ],
            "fdf39dda",
        ),
        (vec![V::BigInt(1), V::BigInt(2)], "c3c3eed1"),
        (
            vec![
                V::Null,
                V::String("a".into()),
                V::Array(vec![V::String("b".into())]),
            ],
            "61fc25ce",
        ),
    ] {
        let schema = list_constant_schema(items);
        assert_eq!(hash_of(&schema), pin);
        let mapping = schema.resolve(IndexName::try_new("products").unwrap());
        assert!(mapping.fields[0].mapping.indexed);
    }
}

fn mixed_constant_lists() -> Vec<(Vec<crate::common::GenericValue>, &'static str)> {
    use crate::common::GenericValue as V;
    vec![
        (vec![V::BigInt(1), V::String("n/a".into())], "bc077fc0"),
        (vec![V::Bool(true), V::BigInt(1)], "aa845af3"),
        (
            vec![
                V::BigInt(1),
                V::Double(2.5),
                V::Null,
                V::Array(vec![V::BigInt(3)]),
            ],
            "39e3f15a",
        ),
        (vec![V::BigInt(1), V::Double(1e20)], "92bce669"),
        (
            vec![
                V::Array(vec![V::BigInt(1)]),
                V::Array(vec![V::String("a".into())]),
            ],
            "2b4a291f",
        ),
    ]
}

/// A list mixing scalar kinds moves off the hash it had while it built
/// `null`, so it lands in a new physical index. The pins are those hashes.
#[test]
fn a_constant_list_mixing_kinds_moves_its_hash() {
    for (items, pin) in mixed_constant_lists() {
        assert_ne!(hash_of(&list_constant_schema(items)), pin);
    }
}

/// One OpenSearch field takes one kind, so a list mixing kinds is stored
/// unindexed as an opaque object, like an object constant.
#[test]
fn a_constant_list_mixing_kinds_is_stored_unindexed() {
    for (items, _) in mixed_constant_lists() {
        let mapping = list_constant_schema(items).resolve(IndexName::try_new("products").unwrap());
        let field = &mapping.fields[0];
        assert!(!field.mapping.indexed);
        assert_eq!(field.mapping.mapping_type, MappingType::Object);
        assert!(!field.array);
    }
}

fn with_format(mut field: Field, format: &str) -> Field {
    field.options.insert(
        "format".to_owned(),
        crate::common::GenericValue::String(format.to_owned()),
    );
    field
}

fn format_of(mapping: &crate::config::Mapping) -> Option<&str> {
    match mapping.extra.get("format") {
        Some(crate::common::GenericValue::String(format)) => Some(format),
        _ => None,
    }
}

const SPACE_ONLY: &str = "yyyy-MM-dd HH:mm:ss";

/// The append rule, pattern by pattern (`with_iso`), by what flusso writes
/// into the field: a date field holds `2024-01-02`; a timestamp field
/// `2024-01-02T03:04:05`, with a fraction, and with an offset. A format with
/// a pattern reading every form it holds is left alone, as is one whose every
/// pattern reads a time of day only (`time_of_day_as_date`'s business); any
/// other gets `||strict_date_optional_time` last, the user's patterns first.
#[test]
fn a_date_format_that_cannot_read_what_flusso_writes_gets_the_iso_pattern_last() {
    use crate::config::DateValues::{Both, Dates, Timestamps};
    let appended = |format: &str| Some(format!("{format}||strict_date_optional_time"));
    for (format, values, expected) in [
        ("yyyy-MM-dd", Dates, None),
        ("uuuu-MM-dd", Dates, None),
        ("date", Dates, None),
        ("strict_date", Dates, None),
        ("year_month_day", Dates, None),
        ("strictYearMonthDay", Dates, None),
        ("iso8601", Dates, None),
        ("strict_date_optional_time", Dates, None),
        ("yyyy/MM/dd", Dates, appended("yyyy/MM/dd")),
        ("epoch_millis", Dates, appended("epoch_millis")),
        ("basic_date", Dates, appended("basic_date")),
        (
            "date_hour_minute_second",
            Dates,
            appended("date_hour_minute_second"),
        ),
        ("yyyy-MM-dd", Timestamps, appended("yyyy-MM-dd")),
        ("date", Timestamps, appended("date")),
        ("iso8601", Timestamps, None),
        ("strict_date_optional_time", Timestamps, None),
        ("date_optional_time||epoch_millis", Timestamps, None),
        ("strictDateOptionalTime", Timestamps, None),
        (" strict_date_optional_time_nanos ", Timestamps, None),
        (
            "yyyy-MM-dd HH:mm:ss||strict_date_optional_time",
            Timestamps,
            None,
        ),
        ("date_time", Timestamps, appended("date_time")),
        (
            "strict_date_time_no_millis",
            Timestamps,
            appended("strict_date_time_no_millis"),
        ),
        (
            "date_hour_minute_second_fraction",
            Timestamps,
            appended("date_hour_minute_second_fraction"),
        ),
        ("epoch_millis", Timestamps, appended("epoch_millis")),
        (SPACE_ONLY, Timestamps, appended(SPACE_ONLY)),
        (
            "yyyy-MM-dd HH:mm:ss.SSSSSS||yyyy-MM-dd HH:mm:ss||epoch_second",
            Timestamps,
            appended("yyyy-MM-dd HH:mm:ss.SSSSSS||yyyy-MM-dd HH:mm:ss||epoch_second"),
        ),
        (
            "yyyy-MM-dd'T'HH:mm:ss",
            Timestamps,
            appended("yyyy-MM-dd'T'HH:mm:ss"),
        ),
        ("HH:mm:ss", Timestamps, None),
        ("HH:mm:ss.SSS||hour_minute_second", Timestamps, None),
        ("hour_minute_second", Dates, None),
        ("strict_t_time", Timestamps, None),
        (
            "HH:mm:ss||yyyy-MM-dd HH:mm",
            Timestamps,
            appended("HH:mm:ss||yyyy-MM-dd HH:mm"),
        ),
        ("yyyy-MM-dd", Both, appended("yyyy-MM-dd")),
        ("iso8601", Both, None),
        ("", Timestamps, None),
        ("  ", Dates, None),
    ] {
        assert_eq!(
            crate::config::with_iso(format, values),
            expected,
            "{format:?} over {values:?}"
        );
    }
}

fn custom(name: &str, postgres: &[&str], opensearch: &str) -> Field {
    column_field(
        name,
        FlussoType::Custom {
            postgres: postgres.iter().map(|ty| (*ty).to_owned()).collect(),
            opensearch: opensearch.to_owned(),
        },
    )
}

/// A field whose format already reads what flusso writes there, or whose
/// value flusso doesn't write as a date, keeps its format and the schema its
/// content hash (`main`'s, pinned): a `date` column read by `yyyy-MM-dd`, a
/// `custom` date over a number or text column (its own value), a
/// `timestamp` over a time of day read by `HH:mm:ss`, `iso8601`, an
/// input-only field, a field under an `indexed: false` join.
#[test]
fn a_date_field_whose_format_reads_what_flusso_writes_there_keeps_it_and_its_hash() {
    let mut hidden = with_format(column_field("hidden", FlussoType::Timestamp), SPACE_ONLY);
    hidden.emit = false;
    let mut archive = belongs_to_schema(true).fields.remove(0);
    if let FieldSource::Relation(Relation::Join(join)) = &mut archive.source {
        join.indexed = false;
        join.fields = vec![with_format(
            column_field("placed", FlussoType::Timestamp),
            SPACE_ONLY,
        )];
    }
    let schema = columns_schema(vec![
        with_format(column_field("born", FlussoType::Date), "yyyy-MM-dd"),
        with_format(custom("millis", &["bigint"], "date"), "epoch_millis"),
        with_format(custom("written", &["text"], "date"), "dd/MM/yyyy"),
        with_format(column_field("opens", FlussoType::Timestamp), "HH:mm:ss"),
        with_format(column_field("seen", FlussoType::Timestamp), "iso8601"),
        hidden,
        archive,
    ]);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let formats: Vec<(&str, Option<&str>)> = mapping
        .fields
        .iter()
        .map(|field| (field.name.as_ref(), format_of(&field.mapping)))
        .collect();
    assert_eq!(
        formats,
        [
            ("born", Some("yyyy-MM-dd")),
            ("millis", Some("epoch_millis")),
            ("written", Some("dd/MM/yyyy")),
            ("opens", Some("HH:mm:ss")),
            ("seen", Some("iso8601")),
            ("author", None),
        ]
    );
    assert_eq!(
        format_of(&mapping.fields[5].children[0].mapping),
        Some(SPACE_ONLY),
        "an unindexed join's children are stored, not mapped"
    );
    assert!(schema.completed_date_formats().is_empty());
    assert_eq!(mapping.hash.to_string(), "ad3ab48b");
}

/// A `custom` field mapped as `date` or `date_nanos` over a date or timestamp
/// column takes the rule by what that column holds.
#[test]
fn a_custom_date_over_a_date_or_timestamp_column_takes_the_rule() {
    let schema = columns_schema(vec![
        with_format(custom("day", &["date"], "date"), "yyyy/MM/dd"),
        with_format(custom("kept", &["date"], "date"), "yyyy-MM-dd"),
        with_format(
            custom("nanos", &["timestamp without time zone"], "date_nanos"),
            SPACE_ONLY,
        ),
        with_format(
            custom("both", &["date", "timestamptz"], "date"),
            "yyyy-MM-dd",
        ),
    ]);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let formats: Vec<Option<&str>> = mapping
        .fields
        .iter()
        .map(|field| format_of(&field.mapping))
        .collect();
    assert_eq!(
        formats,
        [
            Some("yyyy/MM/dd||strict_date_optional_time"),
            Some("yyyy-MM-dd"),
            Some("yyyy-MM-dd HH:mm:ss||strict_date_optional_time"),
            Some("yyyy-MM-dd||strict_date_optional_time"),
        ]
    );
    assert_eq!(
        schema.completed_date_formats(),
        ["day".to_owned(), "nanos".to_owned(), "both".to_owned()]
    );
}

/// Every date-typed field takes the rule: a `timestamp` or `date` column, an
/// array of them, a `min`/`max` of a date type, a timestamp script; any other
/// type keeps its options as written, and a field with no `format` is left
/// alone.
#[test]
fn every_date_mapping_with_a_refusing_format_resolves_with_the_iso_pattern() {
    let appended = "yyyy-MM-dd HH:mm:ss||strict_date_optional_time";
    let mut array = column_field("days", FlussoType::Date);
    if let FieldSource::Column(column) = &mut array.source {
        column.array = true;
    }
    let latest = Field {
        field: FieldName::try_new("latest").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: TableName::try_new("orders").unwrap(),
            op: AggregateOp::Max(crate::config::Expression::Column(
                ColumnName::try_new("placed_at").unwrap(),
            )),
            key: AggregateKey::Direct(ColumnName::try_new("user_id").unwrap()),
            value_type: Some(FlussoType::Timestamp),
            filters: None,
            distinct: false,
            via: vec![],
            on: vec![],
        })),
    };
    let script = Field {
        field: FieldName::try_new("computed").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Script(crate::config::Script {
            source: "this.placed".to_owned(),
            ty: FlussoType::Timestamp,
            nullable: true,
            array: false,
            enum_order: vec![],
            subfields: true,
        }),
    };
    let schema = columns_schema(vec![
        with_format(column_field("placed", FlussoType::Timestamp), SPACE_ONLY),
        with_format(column_field("born", FlussoType::Date), SPACE_ONLY),
        with_format(array, SPACE_ONLY),
        with_format(latest, SPACE_ONLY),
        with_format(script, SPACE_ONLY),
        with_format(
            column_field("kept", FlussoType::Timestamp),
            "strict_date_optional_time",
        ),
        column_field("plain", FlussoType::Timestamp),
        with_format(column_field("name", FlussoType::Keyword), SPACE_ONLY),
    ]);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let formats: Vec<(&str, Option<&str>)> = mapping
        .fields
        .iter()
        .map(|field| (field.name.as_ref(), format_of(&field.mapping)))
        .collect();
    assert_eq!(
        formats,
        [
            ("placed", Some(appended)),
            ("born", Some(appended)),
            ("days", Some(appended)),
            ("latest", Some(appended)),
            ("computed", Some(appended)),
            ("kept", Some("strict_date_optional_time")),
            ("plain", None),
            ("name", Some(SPACE_ONLY)),
        ]
    );
}

/// A date field inside an object or a join resolves with the rule too, and a
/// `map` of timestamps (whose keys take no `format`) keeps its key mapping.
#[test]
fn a_nested_date_field_takes_the_iso_pattern_and_a_map_of_dates_is_untouched() {
    let placed = with_format(column_field("placed", FlussoType::Timestamp), SPACE_ONLY);
    let group = Field {
        field: FieldName::try_new("meta").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Group(vec![placed.clone()]),
    };
    let mut join = belongs_to_schema(true).fields.remove(0);
    if let FieldSource::Relation(Relation::Join(join)) = &mut join.source {
        join.fields = vec![placed];
    }
    let schema = columns_schema(vec![
        group,
        join,
        column_field("stamps", map_of(FlussoType::Timestamp)),
    ]);
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    for field in &mapping.fields[..2] {
        assert_eq!(
            format_of(&field.children[0].mapping),
            Some("yyyy-MM-dd HH:mm:ss||strict_date_optional_time"),
            "{}",
            field.name
        );
    }
    let keys = mapping.fields[2].mapping.map_values.as_ref().unwrap();
    assert_eq!(keys.mapping_type, MappingType::Date);
    assert_eq!(format_of(keys), None);
}

/// Every schema the rule doesn't touch keeps its content hash, so its
/// physical index: a date field with no format or one reading ISO, any other
/// field with a `format` option.
#[test]
fn a_schema_whose_date_formats_read_what_flusso_writes_keeps_its_content_hash() {
    let schema = columns_schema(vec![
        column_field("plain", FlussoType::Timestamp),
        with_format(
            column_field("kept", FlussoType::Timestamp),
            "yyyy-MM-dd HH:mm:ss||strict_date_optional_time",
        ),
        with_format(column_field("name", FlussoType::Keyword), SPACE_ONLY),
    ]);
    let hash = schema.resolve(IndexName::try_new("products").unwrap()).hash;
    assert_eq!(hash.to_string(), "f20b4f60");
}

/// A schema whose date format gets the pattern moves to a new physical index
/// once: its old mapping refused every document holding a date.
#[test]
fn a_schema_whose_date_format_gets_the_iso_pattern_moves_off_its_hash() {
    let refusing = columns_schema(vec![with_format(
        column_field("placed", FlussoType::Timestamp),
        SPACE_ONLY,
    )]);
    let hash = refusing
        .resolve(IndexName::try_new("products").unwrap())
        .hash;
    assert_ne!(
        hash.to_string(),
        "41d45db2",
        "the hash it had before the pattern was appended"
    );
}
