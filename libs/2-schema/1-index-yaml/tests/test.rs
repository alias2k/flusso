#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

//! Parsing and conversion tests for the type-as-key field format.

use schema_core::{
    AggregateKey, AggregateOp, Column, Field, FieldSource, FilterValue, FlussoType, Geo,
    IndexSchema, JoinKind, ParseFrom, Relation,
};
use schema_index_yaml::{ConversionError, ParseError, SchemaYaml};

fn parse(yaml: &str) -> Result<SchemaYaml, ParseError> {
    SchemaYaml::try_parse(yaml)
}

fn convert(yaml: &str) -> Result<IndexSchema, ConversionError> {
    let schema = SchemaYaml::try_parse(yaml).expect("yaml should parse for a conversion test");
    IndexSchema::try_from(schema)
}

/// Find a top-level field by its document key.
fn field<'a>(schema: &'a IndexSchema, name: &str) -> &'a Field {
    schema
        .fields
        .iter()
        .find(|f| f.field.as_ref() == name)
        .unwrap_or_else(|| panic!("field `{name}` should be present"))
}

// ── parse / convert the fixture ──────────────────────────────────────────────

#[test]
fn parse_fixture() {
    parse(include_str!("user.schema.yml")).unwrap();
}

#[test]
fn convert_fixture() {
    convert(include_str!("user.schema.yml")).unwrap();
}

#[test]
fn doc_id_is_rejected_until_supported() {
    let err = convert(
        "version: 1\ntable: users\ndoc_id: slug\nfields:\n  - keyword: slug\n    required: true",
    )
    .unwrap_err();
    assert!(
        matches!(err, ConversionError::DocIdUnsupported),
        "got {err:?}"
    );
}

#[test]
fn minimal_schema() {
    let schema =
        convert("version: 1\ntable: users\nfields:\n  - integer: id\n    required: false").unwrap();
    assert!(matches!(
        field(&schema, "id").source,
        FieldSource::Column(_)
    ));
}

// ── each field kind converts to the right source ─────────────────────────────

#[test]
fn scalar_carries_type_and_nullability() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    match &field(&schema, "email").source {
        FieldSource::Column(Column {
            ty,
            nullable,
            transforms,
            ..
        }) => {
            assert_eq!(*ty, FlussoType::Keyword);
            assert!(!nullable, "required → non-null");
            assert_eq!(transforms.len(), 2);
        }
        other => panic!("expected a column, got {other:?}"),
    }
}

#[test]
fn column_defaults_to_field_name_else_explicit() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    // `text: fullName` with `column: full_name`.
    assert_eq!(
        field(&schema, "fullName").column().map(|c| c.as_ref()),
        Some("full_name")
    );
    // `keyword: email` with no column → defaults to `email`.
    assert_eq!(
        field(&schema, "email").column().map(|c| c.as_ref()),
        Some("email")
    );
}

#[test]
fn geo_two_columns_is_a_geo_source() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    match &field(&schema, "location").source {
        FieldSource::Geo(Geo { lat, lon, nullable }) => {
            assert_eq!(lat.as_ref(), "latitude");
            assert_eq!(lon.as_ref(), "longitude");
            assert!(nullable);
        }
        other => panic!("expected a geo source, got {other:?}"),
    }
}

#[test]
fn geo_single_column_is_a_geo_point_column() {
    let schema = convert(
        "version: 1\ntable: places\nfields:\n  - geo: location\n    column: loc_json\n    required: true",
    )
    .unwrap();
    match &field(&schema, "location").source {
        FieldSource::Column(Column { column, ty, .. }) => {
            assert_eq!(column.as_ref(), "loc_json");
            assert_eq!(*ty, FlussoType::GeoPoint);
        }
        other => panic!("expected a geo_point column, got {other:?}"),
    }
}

#[test]
fn custom_scalar_carries_its_pair() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    match &field(&schema, "price").source {
        FieldSource::Column(Column { ty, .. }) => match ty {
            FlussoType::Custom {
                postgres,
                opensearch,
            } => {
                assert_eq!(postgres, &["numeric".to_owned()]);
                assert_eq!(opensearch, "scaled_float");
            }
            other => panic!("expected a custom type, got {other:?}"),
        },
        other => panic!("expected a column, got {other:?}"),
    }
}

#[test]
fn object_becomes_a_group() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    match &field(&schema, "account").source {
        FieldSource::Group(fields) => assert_eq!(fields.len(), 2),
        other => panic!("expected a group, got {other:?}"),
    }
}

#[test]
fn map_is_a_dynamic_object_column() {
    let schema = convert(
        "version: 1\ntable: products\nfields:\n  - map: title\n    values: text\n    required: true",
    )
    .unwrap();
    match &field(&schema, "title").source {
        FieldSource::Column(Column {
            column,
            ty,
            nullable,
            ..
        }) => {
            assert_eq!(column.as_ref(), "title", "defaults to the field name");
            assert_eq!(
                *ty,
                FlussoType::Map {
                    values: Box::new(FlussoType::Text)
                }
            );
            assert!(!nullable, "required → non-null");
        }
        other => panic!("expected a map column, got {other:?}"),
    }
    // `dynamic: true` is injected so the dynamic keys stay searchable.
    assert_eq!(
        field(&schema, "title").options.get("dynamic"),
        Some(&schema_core::GenericValue::Bool(true)),
    );
}

#[test]
fn map_resolves_value_kind_onto_the_mapping() {
    use schema_core::{IndexName, MappingType};
    let schema = convert(
        "version: 1\ntable: products\nfields:\n  - map: codes\n    values: keyword\n    required: false",
    )
    .unwrap();
    let mapping = schema.resolve(IndexName::try_new("products").unwrap());
    let codes = mapping
        .fields
        .iter()
        .find(|f| f.name.as_ref() == "codes")
        .unwrap();
    assert_eq!(codes.mapping.mapping_type, MappingType::Object);
    assert_eq!(codes.mapping.map_values, Some(MappingType::Keyword));
    assert!(codes.nullable, "not required → nullable");
}

#[test]
fn map_rejects_a_non_leaf_value_type() {
    let err = convert(
        "version: 1\ntable: products\nfields:\n  - map: blob\n    values: json\n    required: true",
    )
    .unwrap_err();
    assert!(
        matches!(err, ConversionError::InvalidMapValueType { got: "json" }),
        "got {err:?}"
    );
}

#[test]
fn join_verb_comes_from_the_tag() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    assert!(matches!(
        &field(&schema, "profile").source,
        FieldSource::Relation(Relation::Join(j)) if matches!(j.kind, JoinKind::HasOne { .. })
    ));
    assert!(matches!(
        &field(&schema, "organization").source,
        FieldSource::Relation(Relation::Join(j))
            if matches!(&j.kind, JoinKind::BelongsTo { column } if column.as_ref() == "organization_id")
    ));
    match &field(&schema, "orders").source {
        FieldSource::Relation(Relation::Join(j)) => {
            assert!(matches!(j.kind, JoinKind::HasMany { .. }));
            assert!(j.filters.as_ref().is_some_and(|f| f.len() == 1));
            assert!(j.order_by.as_ref().is_some_and(|o| o.len() == 1));
        }
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn belongs_to_column_defaults_to_the_field_name() {
    let schema = convert(
        "version: 1\ntable: tickets\nfields:\n  - belongs_to: created_by\n    table: users\n    primary_key: id\n    fields:\n      - keyword: email\n        required: true",
    )
    .unwrap();
    match &field(&schema, "created_by").source {
        FieldSource::Relation(Relation::Join(j)) => {
            assert!(
                matches!(&j.kind, JoinKind::BelongsTo { column } if column.as_ref() == "created_by")
            );
        }
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn to_one_join_defaults_to_nullable() {
    let schema = convert(
        "version: 1\ntable: tickets\nfields:\n  - belongs_to: created_by\n    table: users\n    primary_key: id\n    fields:\n      - keyword: email\n        required: true",
    )
    .unwrap();
    match &field(&schema, "created_by").source {
        FieldSource::Relation(Relation::Join(j)) => assert!(j.nullable, "no `required` → nullable"),
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn to_one_join_required_is_non_null() {
    let schema = convert(
        "version: 1\ntable: tickets\nfields:\n  - belongs_to: created_by\n    table: users\n    primary_key: id\n    required: true\n    fields:\n      - keyword: email\n        required: true",
    )
    .unwrap();
    match &field(&schema, "created_by").source {
        FieldSource::Relation(Relation::Join(j)) => {
            assert!(!j.nullable, "`required: true` → non-null")
        }
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn required_on_a_to_many_join_is_rejected() {
    let err = convert(
        "version: 1\ntable: users\nfields:\n  - has_many: orders\n    table: orders\n    primary_key: id\n    foreign_key: user_id\n    required: true\n    fields:\n      - keyword: status\n        required: true",
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("required"),
        "to-many `required` should be rejected, got: {err}"
    );
}

#[test]
fn aggregates_come_from_the_op_tag() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    match &field(&schema, "orderCount").source {
        FieldSource::Relation(Relation::Aggregate(a)) => {
            assert!(matches!(a.op, AggregateOp::Count));
            assert!(a.value_type.is_none());
        }
        other => panic!("expected an aggregate, got {other:?}"),
    }
    match &field(&schema, "lifetimeValue").source {
        FieldSource::Relation(Relation::Aggregate(a)) => {
            assert!(matches!(&a.op, AggregateOp::Sum(c) if c.as_ref() == "total"));
            assert_eq!(a.value_type, Some(FlussoType::Decimal));
        }
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

#[test]
fn ids_direct_collects_related_primary_keys() {
    let schema = convert(
        "version: 1\ntable: users\nfields:\n  - ids: orderIds\n    table: orders\n    foreign_key: user_id\n    element_type: long",
    )
    .unwrap();
    match &field(&schema, "orderIds").source {
        FieldSource::Relation(Relation::Aggregate(a)) => {
            assert!(
                matches!(&a.op, AggregateOp::Ids { element_type } if *element_type == FlussoType::Long)
            );
            assert!(matches!(&a.key, AggregateKey::Direct(fk) if fk.as_ref() == "user_id"));
            assert!(a.value_type.is_none());
        }
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

#[test]
fn ids_through_uses_a_junction() {
    let schema = convert(
        "version: 1\ntable: posts\nfields:\n  - ids: tagIds\n    table: tags\n    through:\n      table: post_tags\n      left_key: post_id\n      right_key: tag_id\n    element_type: long",
    )
    .unwrap();
    match &field(&schema, "tagIds").source {
        FieldSource::Relation(Relation::Aggregate(a)) => {
            assert!(matches!(&a.op, AggregateOp::Ids { .. }));
            assert!(matches!(
                &a.key,
                AggregateKey::Through(t)
                    if t.table.as_ref() == "post_tags"
                        && t.left_key.as_ref() == "post_id"
                        && t.right_key.as_ref() == "tag_id"
            ));
        }
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

#[test]
fn ids_requires_element_type() {
    let err = convert(
        "version: 1\ntable: users\nfields:\n  - ids: orderIds\n    table: orders\n    foreign_key: user_id",
    )
    .unwrap_err();
    assert!(matches!(err, ConversionError::MissingElementType));
}

#[test]
fn ids_rejects_a_column_sibling() {
    let err = convert(
        "version: 1\ntable: users\nfields:\n  - ids: orderIds\n    table: orders\n    foreign_key: user_id\n    element_type: long\n    column: total",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::UnexpectedIdsSibling { sibling: "column" }
    ));
}

#[test]
fn element_type_rejected_on_non_ids_aggregate() {
    let err = convert(
        "version: 1\ntable: users\nfields:\n  - count: orderCount\n    table: orders\n    foreign_key: user_id\n    element_type: long",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::UnexpectedAggregateSibling {
            sibling: "element_type"
        }
    ));
}

#[test]
fn constant_field() {
    let schema = convert(include_str!("user.schema.yml")).unwrap();
    assert!(matches!(
        &field(&schema, "source").source,
        FieldSource::Constant(_)
    ));
}

// ── filters convert by shape ─────────────────────────────────────────────────

#[test]
fn between_and_in_filter_values() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - has_many: orders
    table: orders
    foreign_key: user_id
    primary_key: id
    filters:
      - { column: total, op: between, value: [1, 100] }
      - { column: status, op: in, value: [paid, shipped] }
    fields:
      - keyword: status
        required: true
"#,
    )
    .unwrap();
    let FieldSource::Relation(Relation::Join(join)) = &field(&schema, "orders").source else {
        panic!("expected a join");
    };
    let filters = join.filters.as_ref().unwrap();
    assert!(matches!(
        filter_value(&filters[0]),
        Some(FilterValue::Range(_, _))
    ));
    assert!(matches!(
        filter_value(&filters[1]),
        Some(FilterValue::List(items)) if items.len() == 2
    ));
}

#[test]
fn root_filters_parse_and_convert() {
    let schema = convert(
        "version: 1\ntable: users\nfilters:\n  - { column: status, op: eq, value: active }\n  - { column: deleted_at, op: is_null }\nfields:\n  - integer: id\n    required: false",
    )
    .unwrap();
    let filters = schema.filters.as_deref().unwrap();
    assert_eq!(filters.len(), 2);
    assert!(matches!(
        filters.first(),
        Some(schema_core::Filter::ValueOp(_))
    ));
    assert!(matches!(
        filters.get(1),
        Some(schema_core::Filter::NullCheck(_))
    ));
}

fn filter_value(filter: &schema_core::Filter) -> Option<&FilterValue> {
    match filter {
        schema_core::Filter::ValueOp(v) => Some(&v.value),
        _ => None,
    }
}

// ── errors ───────────────────────────────────────────────────────────────────

#[test]
fn missing_type_tag_is_an_error() {
    let err = parse("version: 1\ntable: t\nfields:\n  - required: true").unwrap_err();
    assert!(matches!(err, ParseError::Syntax(_)));
}

#[test]
fn unknown_sibling_is_rejected() {
    let err =
        parse("version: 1\ntable: t\nfields:\n  - keyword: x\n    required: true\n    bogus: 1")
            .unwrap_err();
    assert!(matches!(err, ParseError::Syntax(_)));
}

#[test]
fn sum_without_value_type_is_an_error() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - sum: s\n    table: orders\n    column: total\n    foreign_key: t_id",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::MissingAggregateType { op: "sum" }
    ));
}

#[test]
fn sum_without_column_is_an_error() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - sum: s\n    table: orders\n    value_type: decimal\n    foreign_key: t_id",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::MissingAggregateColumn { op: "sum" }
    ));
}

#[test]
fn aggregate_value_type_rejects_geo_point() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - max: m\n    table: orders\n    column: loc\n    value_type: geo_point\n    foreign_key: t_id",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::InvalidAggregateType { op: "max" }
    ));
}

#[test]
fn join_without_its_key_is_an_error() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - has_many: o\n    table: orders\n    primary_key: id\n    fields:\n      - keyword: x\n        required: true",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::MissingJoinKey {
            verb: "has_many",
            ..
        }
    ));
}

#[test]
fn join_with_the_wrong_key_sibling_is_an_error() {
    // `belongs_to` reads its key from this table's `column`, not `foreign_key`.
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - belongs_to: owner\n    table: users\n    foreign_key: owner_id\n    primary_key: id\n    fields:\n      - keyword: email\n        required: true",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::UnexpectedJoinKey {
            verb: "belongs_to",
            sibling: "foreign_key",
            ..
        }
    ));
}

#[test]
fn belongs_to_rejects_order_by_and_limit() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - belongs_to: owner\n    table: users\n    primary_key: id\n    order_by:\n      - { column: id }\n    fields:\n      - keyword: email\n        required: true",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::UnexpectedJoinSibling {
            verb: "belongs_to",
            sibling: "order_by",
        }
    ));

    let err = convert(
        "version: 1\ntable: t\nfields:\n  - has_one: profile\n    table: profiles\n    foreign_key: user_id\n    primary_key: id\n    limit: 3\n    fields:\n      - keyword: bio\n        required: false",
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ConversionError::UnexpectedJoinSibling {
            verb: "has_one",
            sibling: "limit",
        }
    ));
}

// ── error messages are clear: location snippet + field-aware phrasing ────────

#[test]
fn top_level_error_renders_a_source_snippet() {
    // A top-level (non-field) error locates accurately, so it gets a caret snippet.
    let err = parse("version: 1\ntable: t\nbogus: 9\nfields: []").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("--> line 3"), "missing location header: {msg}");
    assert!(msg.contains("bogus: 9"), "missing source line: {msg}");
    assert!(msg.contains('^'), "missing caret: {msg}");
}

#[test]
fn unknown_sibling_message_names_the_field_and_hides_internal_key() {
    let err =
        parse("version: 1\ntable: t\nfields:\n  - keyword: email\n    requierd: true").unwrap_err();
    let msg = err.to_string();
    // Names the field by tag and document key, in our phrasing…
    assert!(msg.contains("`keyword` field `email`"), "{msg}");
    assert!(msg.contains("unknown key `requierd`"), "{msg}");
    // …never leaks the internal `field` key we inject while parsing…
    assert!(!msg.contains("`field`"), "internal key leaked: {msg}");
    // …and omits the snippet, whose location would point at the wrong field.
    assert!(!msg.contains("-->"), "misleading snippet rendered: {msg}");
}

#[test]
fn missing_type_tag_lists_the_fields_keys_to_locate_it() {
    let err =
        parse("version: 1\ntable: t\nfields:\n  - column: c\n    required: true").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("missing a type tag"), "{msg}");
    // With no tag there's no name, so the present keys are listed instead.
    assert!(msg.contains("column") && msg.contains("required"), "{msg}");
}

#[test]
fn missing_required_key_uses_plain_phrasing() {
    let err = parse("version: 1\ntable: t\nfields:\n  - keyword: email").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("`keyword` field `email`: missing key `required`"),
        "{msg}"
    );
    assert!(!msg.contains("missing field"), "serde jargon leaked: {msg}");
}

#[test]
fn geo_with_only_one_coordinate_is_an_error() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - geo: location\n    lat: latitude\n    required: false",
    )
    .unwrap_err();
    assert!(matches!(err, ConversionError::InvalidGeoSource));
}
