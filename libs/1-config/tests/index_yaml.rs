#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

//! Parsing and conversion tests for the type-as-key field format.

use config::yaml::{ConversionError, ParseError, SchemaYaml};
use kernel::{
    AggregateKey, AggregateOp, Column, Field, FieldSource, Filter, FilterValue, FlussoType, Geo,
    IndexSchema, JoinKind, ParseFrom, Relation,
};

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

// ── enum declared order ──────────────────────────────────────────────────────

#[test]
fn enum_variants_land_on_the_column_in_order() {
    let schema = convert(
        "version: 1\ntable: t\nfields:\n  - enum: status\n    required: true\n    variants: [low, medium, high]",
    )
    .unwrap();
    match &field(&schema, "status").source {
        FieldSource::Column(Column { ty, enum_order, .. }) => {
            assert_eq!(*ty, FlussoType::Enum);
            assert_eq!(enum_order, &["low", "medium", "high"]);
        }
        other => panic!("expected a column, got {other:?}"),
    }
}

#[test]
fn bare_enum_has_no_declared_order() {
    let schema =
        convert("version: 1\ntable: t\nfields:\n  - enum: status\n    required: true").unwrap();
    match &field(&schema, "status").source {
        FieldSource::Column(Column { ty, enum_order, .. }) => {
            assert_eq!(*ty, FlussoType::Enum);
            assert!(enum_order.is_empty(), "a bare enum declares no order");
        }
        other => panic!("expected a column, got {other:?}"),
    }
}

#[test]
fn enum_rejects_duplicate_variants() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - enum: status\n    required: true\n    variants: [low, low]",
    )
    .unwrap_err();
    assert!(
        matches!(err, ConversionError::DuplicateEnumVariant { .. }),
        "got {err:?}"
    );
}

#[test]
fn enum_rejects_variant_with_mapping_arrow() {
    let err = convert(
        "version: 1\ntable: t\nfields:\n  - enum: status\n    required: true\n    variants: [\"a => b\"]",
    )
    .unwrap_err();
    assert!(
        matches!(err, ConversionError::InvalidEnumVariant { .. }),
        "got {err:?}"
    );
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
fn scalar_default_is_kept() {
    let schema = convert(
        "version: 1\ntable: t\nfields:\n  - integer: n\n    required: false\n    default: 0",
    )
    .unwrap();
    match &field(&schema, "n").source {
        FieldSource::Column(Column { default, .. }) => {
            assert!(default.is_some(), "a scalar default is preserved");
        }
        other => panic!("expected a column, got {other:?}"),
    }
}

#[test]
fn non_scalar_default_is_rejected() {
    for (kind, yaml) in [
        (
            "array",
            "version: 1\ntable: t\nfields:\n  - json: data\n    required: false\n    default: [1, 2]",
        ),
        (
            "object",
            "version: 1\ntable: t\nfields:\n  - json: data\n    required: false\n    default: { a: 1 }",
        ),
    ] {
        let err = convert(yaml).unwrap_err();
        assert!(
            matches!(err, ConversionError::NonScalarDefault { got } if got == kind),
            "{kind} default should be rejected, got {err:?}"
        );
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
        Some(&kernel::GenericValue::Bool(true)),
    );
}

#[test]
fn map_resolves_value_kind_onto_the_mapping() {
    use kernel::{IndexName, MappingType};
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
    assert_eq!(
        codes
            .mapping
            .map_values
            .as_ref()
            .map(|values| &values.mapping_type),
        Some(&MappingType::Keyword)
    );
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
            assert!(matches!(&a.op, AggregateOp::Sum(c) if c.to_string() == "total"));
            assert_eq!(a.value_type, Some(FlussoType::Decimal));
        }
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

fn aggregate_input(column: &str) -> Result<String, String> {
    let yaml = format!(
        "version: 1\ntable: orders\nfields:\n  - sum: linesValue\n    table: order_lines\n    foreign_key: order_id\n    column: \"{column}\"\n    value_type: double"
    );
    let parsed = parse(&yaml).map_err(|e| e.to_string())?;
    let schema = IndexSchema::try_from(parsed).map_err(|e| e.to_string())?;
    match &field(&schema, "linesValue").source {
        FieldSource::Relation(Relation::Aggregate(a)) => match &a.op {
            AggregateOp::Sum(input) => Ok(input.to_string()),
            other => panic!("expected a sum, got {other:?}"),
        },
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

#[test]
fn an_aggregate_column_may_be_arithmetic_over_the_table_s_columns() {
    assert_eq!(
        aggregate_input("quantity * unit_price").unwrap(),
        "quantity * unit_price"
    );
    assert_eq!(
        aggregate_input(" Quantity*( unit_price-discount )/100 ").unwrap(),
        "quantity * (unit_price - discount) / 100"
    );
    assert_eq!(aggregate_input("amount").unwrap(), "amount");
    for (op, value_type) in [
        ("avg", ""),
        ("min", "\n    value_type: double"),
        ("max", "\n    value_type: double"),
    ] {
        let schema = convert(&format!(
            "version: 1\ntable: orders\nfields:\n  - {op}: x\n    table: order_lines\n    foreign_key: order_id\n    column: -quantity * 2.5{value_type}"
        ))
        .unwrap();
        match &field(&schema, "x").source {
            FieldSource::Relation(Relation::Aggregate(a)) => {
                assert_eq!(a.op.input().unwrap().to_string(), "-quantity * 2.5", "{op}");
            }
            other => panic!("expected an aggregate, got {other:?}"),
        }
    }
}

#[test]
fn a_malformed_aggregate_expression_names_the_field_and_the_position() {
    let err = aggregate_input("quantity *").unwrap_err();
    assert!(err.contains("`sum` field `linesValue`"), "{err}");
    assert!(
        err.contains("invalid expression `quantity *`: at position 11: expected a column"),
        "{err}"
    );
    let err = aggregate_input("sum(quantity)").unwrap_err();
    assert!(err.contains("at position 4"), "{err}");
    let err = aggregate_input("quantity * price %").unwrap_err();
    assert!(
        err.contains("at position 18: unexpected character `%`"),
        "{err}"
    );
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
    assert!(matches!(filters.first(), Some(kernel::Filter::ValueOp(_))));
    assert!(matches!(filters.get(1), Some(kernel::Filter::NullCheck(_))));
}

fn filter_value(filter: &kernel::Filter) -> Option<&FilterValue> {
    match filter {
        kernel::Filter::ValueOp(v) => Some(&v.value),
        _ => None,
    }
}

// ── errors ───────────────────────────────────────────────────────────────────

#[test]
fn missing_type_tag_is_an_error() {
    let err = parse("version: 1\ntable: t\nfields:\n  - required: true").unwrap_err();
    assert!(matches!(err, ParseError::Syntax { .. }));
}

#[test]
fn unknown_sibling_is_rejected() {
    let err =
        parse("version: 1\ntable: t\nfields:\n  - keyword: x\n    required: true\n    bogus: 1")
            .unwrap_err();
    assert!(matches!(err, ParseError::Syntax { .. }));
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
    // …suggests the near-miss the typo probably meant…
    assert!(msg.contains("did you mean `required`?"), "{msg}");
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

const ORDER_WEIGHT: &str = "version: 1\ntable: orders\nfields:\n  - sum: totalWeight\n    table: products\n    column: weight\n    value_type: double\n    through:\n      table: order_items\n      left_key: order_id\n      right_key: product_id\n";

#[test]
fn aggregate_over_through_defaults_to_one_value_per_junction_row() {
    let schema = convert(ORDER_WEIGHT).unwrap();
    match &field(&schema, "totalWeight").source {
        FieldSource::Relation(Relation::Aggregate(a)) => {
            assert!(!a.distinct);
            assert!(matches!(&a.key, AggregateKey::Through(t) if t.filters.is_none()));
        }
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

#[test]
fn aggregate_over_through_takes_distinct_and_junction_filters() {
    let schema = convert(&format!(
        "{ORDER_WEIGHT}      filters:\n        - {{ column: status, op: neq, value: cancelled }}\n    distinct: true\n    filters:\n      - {{ column: active, op: eq, value: 'true' }}\n"
    ))
    .unwrap();
    match &field(&schema, "totalWeight").source {
        FieldSource::Relation(Relation::Aggregate(a)) => {
            assert!(a.distinct);
            let AggregateKey::Through(through) = &a.key else {
                panic!("expected a through key");
            };
            let junction = through.filters.as_deref().unwrap();
            assert!(matches!(
                junction,
                [Filter::ValueOp(f)] if f.column.as_ref() == "status"
            ));
            assert!(matches!(
                a.filters.as_deref().unwrap(),
                [Filter::ValueOp(f)] if f.column.as_ref() == "active"
            ));
        }
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

#[test]
fn distinct_is_rejected_on_a_foreign_key_aggregate() {
    let err = convert(
        "version: 1\ntable: users\nfields:\n  - count: orderCount\n    table: orders\n    foreign_key: user_id\n    distinct: true",
    )
    .unwrap_err();
    assert!(matches!(err, ConversionError::DistinctWithoutThrough));
}

#[test]
fn many_to_many_join_takes_junction_filters() {
    let schema = convert(
        "version: 1\ntable: posts\nfields:\n  - many_to_many: tags\n    table: tags\n    primary_key: id\n    through:\n      table: post_tags\n      left_key: post_id\n      right_key: tag_id\n      filters:\n        - { column: removed_at, op: is_null }\n    fields:\n      - keyword: name\n        required: true",
    )
    .unwrap();
    match &field(&schema, "tags").source {
        FieldSource::Relation(Relation::Join(j)) => {
            let JoinKind::ManyToMany { through } = &j.kind else {
                panic!("expected many_to_many");
            };
            assert!(matches!(
                through.filters.as_deref(),
                Some([Filter::NullCheck(f)]) if f.column.as_ref() == "removed_at"
            ));
        }
        other => panic!("expected a join, got {other:?}"),
    }
}

// ── via / exists ─────────────────────────────────────────────────────────────

const TASK_FLAG: &str = "version: 1\ntable: users\nprimary_key: id\nfields:\n  - exists: hasCompletedTask\n    table: tasks\n    foreign_key: project_id\n    filters:\n      - { column: status, op: eq, value: completed }\n    via:\n      - table: teams\n        foreign_key: owner_id\n      - table: projects\n        foreign_key: team_id\n        filters:\n          - { column: archived, op: eq, value: 'false' }\n";

fn aggregate<'a>(schema: &'a IndexSchema, name: &str) -> &'a kernel::Aggregate {
    match &field(schema, name).source {
        FieldSource::Relation(Relation::Aggregate(aggregate)) => aggregate,
        other => panic!("expected an aggregate, got {other:?}"),
    }
}

#[test]
fn an_aggregate_reaches_its_table_via_hops_in_order_from_the_parent() {
    let schema = convert(TASK_FLAG).unwrap();
    let a = aggregate(&schema, "hasCompletedTask");
    assert!(matches!(a.op, AggregateOp::Exists));
    assert!(matches!(&a.key, AggregateKey::Direct(fk) if fk.as_ref() == "project_id"));
    let hops: Vec<(&str, &str)> = a
        .via
        .iter()
        .map(|hop| (hop.table.as_ref(), hop.foreign_key.as_ref()))
        .collect();
    assert_eq!(hops, [("teams", "owner_id"), ("projects", "team_id")]);
    assert!(a.via[0].filters.is_none());
    assert!(matches!(
        a.via[1].filters.as_deref(),
        Some([Filter::ValueOp(f)]) if f.column.as_ref() == "archived"
    ));
}

#[test]
fn exists_maps_to_a_non_null_boolean() {
    let schema = convert(TASK_FLAG).unwrap();
    let mapping = schema.resolve(kernel::IndexName::try_new("users").unwrap());
    let flag = &mapping.fields[0];
    assert!(matches!(
        flag.mapping.mapping_type,
        kernel::MappingType::Boolean
    ));
    assert!(!flag.nullable);
}

#[test]
fn an_aggregate_without_via_has_no_hops() {
    let schema = convert(ORDER_WEIGHT).unwrap();
    assert!(aggregate(&schema, "totalWeight").via.is_empty());
}

#[test]
fn via_ends_in_a_junction_through_the_aggregate_s_own_through() {
    let schema = convert(&format!(
        "{ORDER_WEIGHT}    via:\n      - table: orders_archive\n        foreign_key: customer_id\n"
    ))
    .unwrap();
    let a = aggregate(&schema, "totalWeight");
    assert!(matches!(a.key, AggregateKey::Through(_)));
    assert_eq!(a.via.len(), 1);
}

#[test]
fn distinct_stays_rejected_on_a_foreign_key_chain() {
    let err = convert(&format!("{TASK_FLAG}    distinct: true\n")).unwrap_err();
    assert!(matches!(err, ConversionError::DistinctWithoutThrough));
}

#[test]
fn a_via_hop_needs_a_foreign_key() {
    let err = convert(
        "version: 1\ntable: users\nfields:\n  - count: tasks\n    table: tasks\n    foreign_key: project_id\n    via:\n      - table: projects\n",
    )
    .unwrap_err();
    assert!(
        matches!(err, ConversionError::ViaMissingForeignKey { ref field, ref table } if field == "tasks" && table == "projects")
    );
    assert!(
        err.to_string()
            .starts_with("aggregate `tasks`: `via` entry `projects`"),
        "{err}"
    );
}

#[test]
fn a_belongs_to_direction_via_hop_is_rejected() {
    let err = convert(
        "version: 1\ntable: tasks\nfields:\n  - count: siblings\n    table: tasks\n    foreign_key: project_id\n    via:\n      - table: projects\n        column: project_id\n",
    )
    .unwrap_err();
    assert!(matches!(err, ConversionError::ViaBelongsTo { ref field, .. } if field == "siblings"));
    assert!(err.to_string().contains("belongs_to"), "{err}");
    assert!(err.to_string().contains("aggregate `siblings`"), "{err}");
}

#[test]
fn a_junction_in_the_middle_of_via_is_rejected() {
    let err = convert(
        "version: 1\ntable: users\nfields:\n  - count: tasks\n    table: tasks\n    foreign_key: project_id\n    via:\n      - table: projects\n        through: { table: user_projects, left_key: user_id, right_key: project_id }\n",
    )
    .unwrap_err();
    assert!(matches!(err, ConversionError::ViaJunction { ref field, .. } if field == "tasks"));
    assert!(err.to_string().contains("last hop"), "{err}");
    assert!(err.to_string().contains("aggregate `tasks`"), "{err}");
}

#[test]
fn exists_rejects_column_and_value_type() {
    let base = "version: 1\ntable: users\nfields:\n  - exists: hasOrder\n    table: orders\n    foreign_key: user_id\n";
    let err = convert(&format!("{base}    column: total\n")).unwrap_err();
    assert!(matches!(
        err,
        ConversionError::UnexpectedExistsSibling { sibling: "column" }
    ));
    let err = convert(&format!("{base}    value_type: boolean\n")).unwrap_err();
    assert!(matches!(
        err,
        ConversionError::UnexpectedExistsSibling {
            sibling: "value_type"
        }
    ));
}

#[test]
fn an_unknown_key_in_a_via_hop_is_a_parse_error() {
    let err = parse(
        "version: 1\ntable: users\nfields:\n  - count: tasks\n    table: tasks\n    foreign_key: project_id\n    via:\n      - table: projects\n        foreign_key: user_id\n        order_by: [{ column: id }]\n",
    )
    .unwrap_err();
    assert!(err.to_string().contains("order_by"), "{err}");
}

// ── subfields / indexed opt-outs ─────────────────────────────────────────────

fn column<'a>(schema: &'a IndexSchema, name: &str) -> &'a Column {
    match &field(schema, name).source {
        FieldSource::Column(column) => column,
        other => panic!("expected a column, got {other:?}"),
    }
}

fn join<'a>(schema: &'a IndexSchema, name: &str) -> &'a kernel::Join {
    match &field(schema, name).source {
        FieldSource::Relation(Relation::Join(join)) => join,
        other => panic!("expected a join, got {other:?}"),
    }
}

#[test]
fn subfields_false_is_accepted_on_text_keyword_and_uuid() {
    let schema = convert(
        "version: 1\ntable: notes\nfields:\n  \
         - text: body\n    required: true\n    subfields: false\n  \
         - keyword: code\n    required: true\n    subfields: false\n  \
         - uuid: ref\n    required: true\n    subfields: false\n  \
         - text: title\n    required: true\n    subfields: true\n  \
         - keyword: plain\n    required: true",
    )
    .unwrap();
    for name in ["body", "code", "ref"] {
        assert!(!column(&schema, name).subfields, "`{name}` opted out");
    }
    assert!(column(&schema, "title").subfields, "`true` is the default");
    assert!(column(&schema, "plain").subfields, "omitted → subfields on");
}

#[test]
fn subfields_is_rejected_on_any_other_type() {
    for tag in ["integer", "json", "identifier", "date"] {
        let err = convert(&format!(
            "version: 1\ntable: notes\nfields:\n  - {tag}: x\n    required: true\n    subfields: false"
        ))
        .unwrap_err();
        assert!(
            matches!(&err, ConversionError::SubfieldsNotAllowed { tag: t, field } if t == tag && field == "x"),
            "{tag}: got {err:?}"
        );
        assert!(
            err.to_string()
                .contains("only on a `text`, `keyword` or `uuid` field")
        );
    }
    for body in [
        "  - enum: status\n    required: true\n    subfields: false",
        "  - map: labels\n    values: text\n    required: true\n    subfields: false",
        "  - has_many: orders\n    table: orders\n    primary_key: id\n    foreign_key: user_id\n    subfields: false\n    fields: []",
    ] {
        let err = parse(&format!("version: 1\ntable: users\nfields:\n{body}")).unwrap_err();
        assert!(err.to_string().contains("subfields"), "got {err}");
    }
}

#[test]
fn indexed_false_is_accepted_on_json_and_every_join_verb() {
    let schema = convert(
        "version: 1\ntable: users\nfields:\n  \
         - json: snapshot\n    required: false\n    indexed: false\n  \
         - json: attrs\n    required: false\n  \
         - has_many: orders\n    table: orders\n    primary_key: id\n    foreign_key: user_id\n    indexed: false\n    fields:\n      - keyword: status\n        required: true\n        options: { ignore_above: 10 }\n  \
         - has_one: profile\n    table: profiles\n    primary_key: id\n    foreign_key: user_id\n    indexed: false\n    fields: []\n  \
         - belongs_to: org\n    table: orgs\n    primary_key: id\n    indexed: false\n    fields: []\n  \
         - many_to_many: tags\n    table: tags\n    primary_key: id\n    through: { table: user_tags, left_key: user_id, right_key: tag_id }\n    indexed: false\n    fields: []\n  \
         - has_many: visits\n    table: visits\n    primary_key: id\n    foreign_key: user_id\n    indexed: true\n    fields: []",
    )
    .unwrap();
    assert!(!column(&schema, "snapshot").indexed);
    assert!(column(&schema, "attrs").indexed, "omitted → indexed");
    for name in ["orders", "profile", "org", "tags"] {
        assert!(!join(&schema, name).indexed, "`{name}` opted out");
    }
    assert!(join(&schema, "visits").indexed);
}

#[test]
fn indexed_is_rejected_on_any_other_type() {
    for tag in ["text", "keyword", "integer"] {
        let err = convert(&format!(
            "version: 1\ntable: notes\nfields:\n  - {tag}: x\n    required: true\n    indexed: false"
        ))
        .unwrap_err();
        assert!(
            matches!(&err, ConversionError::IndexedNotAllowed { tag: t, .. } if t == tag),
            "{tag}: got {err:?}"
        );
        assert!(err.to_string().contains("only a `json` field or a join"));
    }
    for body in [
        "  - object: address\n    indexed: false\n    fields: []",
        "  - map: labels\n    values: text\n    required: true\n    indexed: false",
        "  - count: orderCount\n    table: orders\n    foreign_key: user_id\n    indexed: false",
    ] {
        let err = parse(&format!("version: 1\ntable: users\nfields:\n{body}")).unwrap_err();
        assert!(err.to_string().contains("indexed"), "got {err}");
    }
}

#[test]
fn options_on_an_unindexed_field_are_rejected() {
    for body in [
        "  - json: snapshot\n    required: false\n    indexed: false\n    options: { dynamic: true }",
        "  - has_many: orders\n    table: orders\n    primary_key: id\n    foreign_key: user_id\n    indexed: false\n    options: { include_in_parent: true }\n    fields: []",
    ] {
        let err = convert(&format!("version: 1\ntable: users\nfields:\n{body}")).unwrap_err();
        assert!(
            matches!(err, ConversionError::OptionsOnUnindexed { .. }),
            "got {err:?}"
        );
    }
}

// ── on ───────────────────────────────────────────────────────────────────────

const TENANT_HEAD: &str = "version: 1\ntable: orders\nprimary_key: id\nfields:\n";

fn on_pairs(pairs: &[kernel::ColumnPair]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .map(|pair| (pair.local.as_ref(), pair.related.as_ref()))
        .collect()
}

#[test]
fn every_to_one_and_has_many_join_takes_on_pairs() {
    let schema = convert(&format!(
        "{TENANT_HEAD}  - has_many: lines\n    table: order_lines\n    primary_key: id\n    foreign_key: order_id\n    on:\n      - {{ local: tenant_id, related: tenant_id }}\n      - {{ local: region, related: region_code }}\n    fields:\n      - long: id\n        required: true\n  - has_one: invoice\n    table: invoices\n    primary_key: id\n    foreign_key: order_id\n    on: [{{ local: tenant_id, related: tenant_id }}]\n    fields:\n      - long: id\n        required: true\n  - belongs_to: customer\n    table: customers\n    primary_key: id\n    column: customer_id\n    on: [{{ local: tenant_id, related: tenant_id }}]\n    fields:\n      - long: id\n        required: true\n"
    ))
    .unwrap();
    assert_eq!(
        on_pairs(&join(&schema, "lines").on),
        [("tenant_id", "tenant_id"), ("region", "region_code")]
    );
    assert_eq!(
        on_pairs(&join(&schema, "invoice").on),
        [("tenant_id", "tenant_id")]
    );
    assert_eq!(
        on_pairs(&join(&schema, "customer").on),
        [("tenant_id", "tenant_id")]
    );
}

#[test]
fn an_empty_on_list_is_no_pairs() {
    let schema = convert(&format!(
        "{TENANT_HEAD}  - has_many: lines\n    table: order_lines\n    primary_key: id\n    foreign_key: order_id\n    on: []\n    fields:\n      - long: id\n        required: true\n"
    ))
    .unwrap();
    assert!(join(&schema, "lines").on.is_empty());
}

#[test]
fn an_aggregate_and_its_via_hops_take_on_pairs() {
    let schema = convert(&format!(
        "{TENANT_HEAD}  - count: shipped\n    table: parcels\n    foreign_key: shipment_id\n    on: [{{ local: tenant_id, related: tenant_id }}]\n    via:\n      - table: shipments\n        foreign_key: order_id\n        on: [{{ local: tenant_id, related: tenant_id }}]\n"
    ))
    .unwrap();
    let a = aggregate(&schema, "shipped");
    assert_eq!(on_pairs(&a.on), [("tenant_id", "tenant_id")]);
    assert_eq!(on_pairs(&a.via[0].on), [("tenant_id", "tenant_id")]);
}

#[test]
fn on_is_rejected_on_many_to_many() {
    let err = convert(&format!(
        "{TENANT_HEAD}  - many_to_many: tags\n    table: tags\n    primary_key: id\n    through: {{ table: order_tags, left_key: order_id, right_key: tag_id }}\n    on: [{{ local: tenant_id, related: tenant_id }}]\n    fields:\n      - long: id\n        required: true\n"
    ))
    .unwrap_err();
    assert!(matches!(err, ConversionError::OnNotAllowed { .. }));
    assert!(
        err.to_string()
            .starts_with("many_to_many `tags`: `on` is not allowed"),
        "{err}"
    );
}

#[test]
fn on_is_rejected_on_an_aggregate_over_through() {
    let err = convert(&format!(
        "{TENANT_HEAD}  - count: tags\n    table: tags\n    through: {{ table: order_tags, left_key: order_id, right_key: tag_id }}\n    on: [{{ local: tenant_id, related: tenant_id }}]\n"
    ))
    .unwrap_err();
    assert!(matches!(err, ConversionError::OnNotAllowed { .. }));
    assert!(err.to_string().contains("aggregate `tags`"), "{err}");
    assert!(err.to_string().contains("over `through`"), "{err}");
}

#[test]
fn a_half_written_on_pair_is_rejected() {
    let err = convert(&format!(
        "{TENANT_HEAD}  - has_many: lines\n    table: order_lines\n    primary_key: id\n    foreign_key: order_id\n    on: [{{ local: tenant_id }}]\n    fields:\n      - long: id\n        required: true\n"
    ))
    .unwrap_err();
    assert!(matches!(err, ConversionError::OnIncompletePair { .. }));
    assert!(err.to_string().starts_with("has_many `lines`"), "{err}");

    let err = convert(&format!(
        "{TENANT_HEAD}  - count: shipped\n    table: parcels\n    foreign_key: shipment_id\n    via:\n      - table: shipments\n        foreign_key: order_id\n        on: [{{}}]\n"
    ))
    .unwrap_err();
    assert!(matches!(err, ConversionError::OnIncompletePair { .. }));
    assert!(
        err.to_string()
            .starts_with("aggregate `shipped`, `via` entry `shipments`"),
        "{err}"
    );
}

#[test]
fn a_duplicate_on_pair_is_rejected() {
    let err = convert(&format!(
        "{TENANT_HEAD}  - count: lines\n    table: order_lines\n    foreign_key: order_id\n    on:\n      - {{ local: tenant_id, related: tenant_id }}\n      - {{ local: tenant_id, related: tenant_id }}\n"
    ))
    .unwrap_err();
    assert!(
        matches!(err, ConversionError::OnDuplicatePair { ref pair, .. } if pair == "`tenant_id` = `tenant_id`")
    );
    assert!(err.to_string().starts_with("aggregate `lines`"), "{err}");
}

#[test]
fn an_unknown_key_in_an_on_pair_is_a_parse_error() {
    assert!(
        parse(&format!(
            "{TENANT_HEAD}  - count: lines\n    table: order_lines\n    foreign_key: order_id\n    on: [{{ local: a, related: b, other: c }}]\n"
        ))
        .is_err()
    );
}
