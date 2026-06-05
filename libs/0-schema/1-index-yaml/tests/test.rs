#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use schema_core::{Filter, FilterValue, IndexSchema, JoinKey, ParseFrom, Relation};
use schema_index_yaml::{ConversionError, ParseError, SchemaYaml};

fn parse(yaml: &str) -> Result<SchemaYaml, ParseError> {
    SchemaYaml::try_parse(yaml)
}

fn convert(yaml: &str) -> Result<IndexSchema, ConversionError> {
    let schema = SchemaYaml::try_parse(yaml).expect("yaml should be valid for a conversion test");
    IndexSchema::try_from(schema)
}

// ── parse: valid ─────────────────────────────────────────────────────────────

#[test]
fn parse_fixture() {
    parse(include_str!("user.schema.yml")).unwrap();
}

#[test]
fn parse_minimal_schema() {
    parse("version: 1\ntable: users\nfields:\n  - id").unwrap();
}

#[test]
fn parse_with_optional_fields() {
    parse(
        r#"
version: 1
table: users
schema: public
primary_key: id
doc_id: id
fields:
  - id
  - email
"#,
    )
    .unwrap();
}

#[test]
fn parse_with_join_foreign_key() {
    parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      primary_key: id
      fields: [id, total]
"#,
    )
    .unwrap();
}

#[test]
fn parse_with_join_through() {
    parse(
        r#"
version: 1
table: users
fields:
  - field: tags
    join:
      table: tags
      type: many_to_many
      through:
        table: user_tags
        left_key: user_id
        right_key: tag_id
      primary_key: id
      fields: [name]
"#,
    )
    .unwrap();
}

#[test]
fn parse_with_aggregate_count() {
    parse(
        r#"
version: 1
table: users
fields:
  - field: order_count
    aggregate:
      table: orders
      op: count
      foreign_key: user_id
"#,
    )
    .unwrap();
}

#[test]
fn parse_with_aggregate_sum() {
    parse(
        r#"
version: 1
table: users
fields:
  - field: total_spent
    aggregate:
      table: orders
      op: sum
      column: total
      foreign_key: user_id
"#,
    )
    .unwrap();
}

#[test]
fn parse_with_value_filters() {
    parse(
        r#"
version: 1
table: users
fields:
  - field: active_orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: status, op: eq, value: "active" }
        - { column: total, op: between, value: [10, 1000] }
        - { column: tag, op: in, value: [a, b, c] }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
}

#[test]
fn parse_with_null_check_filter() {
    parse(
        r#"
version: 1
table: users
fields:
  - field: active_orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: deleted_at, op: is_null }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
}

#[test]
fn parse_with_raw_filter() {
    parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - raw: "status != 'cancelled'"
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
}

#[test]
fn parse_soft_delete_field_form() {
    parse(
        r#"
version: 1
table: users
soft_delete:
  field: archived
  when:
    - { column: archived_at, op: is_not_null }
fields:
  - id
"#,
    )
    .unwrap();
}

#[test]
fn parse_soft_delete_column_form() {
    parse(
        r#"
version: 1
table: users
soft_delete:
  column: deleted_at
fields:
  - id
"#,
    )
    .unwrap();
}

// ── parse: invalid ───────────────────────────────────────────────────────────

#[test]
fn parse_unsupported_version_fails() {
    let err = parse("version: 99\ntable: users\nfields:\n  - id").unwrap_err();
    assert!(matches!(
        err,
        ParseError::UnsupportedVersion { got: 99, .. }
    ));
}

#[test]
fn parse_missing_version_fails() {
    assert!(parse("table: users\nfields:\n  - id").is_err());
}

#[test]
fn parse_missing_table_fails() {
    assert!(parse("version: 1\nfields:\n  - id").is_err());
}

#[test]
fn parse_missing_fields_fails() {
    assert!(parse("version: 1\ntable: users").is_err());
}

#[test]
fn parse_unknown_field_fails() {
    assert!(
        parse(
            r#"
version: 1
table: users
unknown_field: oops
fields:
  - id
"#
        )
        .is_err()
    );
}

// ── conversion: valid ────────────────────────────────────────────────────────

#[test]
fn convert_fixture() {
    convert(include_str!("user.schema.yml")).unwrap();
}

#[test]
fn convert_table_name() {
    let schema = convert("version: 1\ntable: orders\nfields:\n  - id").unwrap();
    assert_eq!(schema.table.as_ref(), "orders");
}

#[test]
fn convert_default_db_schema_is_public() {
    let schema = convert("version: 1\ntable: users\nfields:\n  - id").unwrap();
    assert_eq!(schema.db_schema.as_ref(), "public");
}

#[test]
fn convert_explicit_db_schema() {
    let schema = convert(
        r#"
version: 1
table: users
schema: analytics
fields:
  - id
"#,
    )
    .unwrap();
    assert_eq!(schema.db_schema.as_ref(), "analytics");
}

#[test]
fn convert_join_foreign_key_becomes_direct() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();

    let field = &schema.fields[0];
    match field.relation().unwrap() {
        Relation::Join(join) => match &join.key {
            JoinKey::Direct(col) => assert_eq!(col.as_ref(), "user_id"),
            JoinKey::Through(_) => panic!("expected direct key"),
        },
        _ => panic!("expected join relation"),
    }
}

#[test]
fn convert_join_through_becomes_through() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - field: tags
    join:
      table: tags
      type: many_to_many
      through:
        table: user_tags
        left_key: user_id
        right_key: tag_id
      primary_key: id
      fields: [name]
"#,
    )
    .unwrap();

    let field = &schema.fields[0];
    match field.relation().unwrap() {
        Relation::Join(join) => match &join.key {
            JoinKey::Through(t) => {
                assert_eq!(t.table.as_ref(), "user_tags");
                assert_eq!(t.left_key.as_ref(), "user_id");
                assert_eq!(t.right_key.as_ref(), "tag_id");
            }
            JoinKey::Direct(_) => panic!("expected through key"),
        },
        _ => panic!("expected join relation"),
    }
}

#[test]
fn convert_aggregate_count_no_column_needed() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - field: order_count
    aggregate:
      table: orders
      op: count
      foreign_key: user_id
"#,
    )
    .unwrap();

    let field = &schema.fields[0];
    match field.relation().unwrap() {
        Relation::Aggregate(a) => {
            assert!(matches!(a.op, schema_core::AggregateOp::Count))
        }
        _ => panic!("expected aggregate relation"),
    }
}

#[test]
fn convert_aggregate_sum_with_column() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - field: total_spent
    type: double
    aggregate:
      table: orders
      op: sum
      column: amount
      foreign_key: user_id
"#,
    )
    .unwrap();

    let field = &schema.fields[0];
    match field.relation().unwrap() {
        Relation::Aggregate(a) => match &a.op {
            schema_core::AggregateOp::Sum(col) => assert_eq!(col.as_ref(), "amount"),
            _ => panic!("expected sum op"),
        },
        _ => panic!("expected aggregate relation"),
    }
}

#[test]
fn convert_filter_in_becomes_list() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: status, op: in, value: [active, pending] }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();

    let field = &schema.fields[0];
    let join = match field.relation().unwrap() {
        Relation::Join(join) => join,
        _ => panic!("expected join"),
    };
    let filter = &join.filters.as_ref().unwrap()[0];
    match filter {
        Filter::ValueOp(v) => {
            assert!(matches!(&v.value, FilterValue::List(items) if items.len() == 2))
        }
        _ => panic!("expected value op filter"),
    }
}

#[test]
fn convert_filter_between_becomes_range() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: total, op: between, value: [10, 500] }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();

    let field = &schema.fields[0];
    let join = match field.relation().unwrap() {
        Relation::Join(join) => join,
        _ => panic!("expected join"),
    };
    let filter = &join.filters.as_ref().unwrap()[0];
    match filter {
        Filter::ValueOp(v) => {
            assert!(matches!(&v.value, FilterValue::Range(lo, hi) if lo == "10" && hi == "500"))
        }
        _ => panic!("expected value op filter"),
    }
}

#[test]
fn convert_filter_eq_becomes_single() {
    let schema = convert(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: status, op: eq, value: "active" }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();

    let field = &schema.fields[0];
    let join = match field.relation().unwrap() {
        Relation::Join(join) => join,
        _ => panic!("expected join"),
    };
    let filter = &join.filters.as_ref().unwrap()[0];
    match filter {
        Filter::ValueOp(v) => {
            assert!(matches!(&v.value, FilterValue::Single(s) if s == "active"))
        }
        _ => panic!("expected value op filter"),
    }
}

// ── conversion: invalid ──────────────────────────────────────────────────────

#[test]
fn convert_invalid_table_name_fails() {
    let schema = parse("version: 1\ntable: \"123 bad name\"\nfields:\n  - id").unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(err, ConversionError::TableName(_)));
}

#[test]
fn convert_join_both_keys_fails() {
    let schema = parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      through:
        table: pivot
        left_key: user_id
        right_key: order_id
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(err, ConversionError::InvalidJoinKey));
}

#[test]
fn convert_join_no_key_fails() {
    let schema = parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(err, ConversionError::InvalidJoinKey));
}

#[test]
fn convert_aggregate_sum_no_column_fails() {
    let schema = parse(
        r#"
version: 1
table: users
fields:
  - field: total
    aggregate:
      table: orders
      op: sum
      foreign_key: user_id
"#,
    )
    .unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(
        err,
        ConversionError::MissingAggregateColumn { op: "sum" }
    ));
}

#[test]
fn convert_filter_in_non_sequence_fails() {
    let schema = parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: status, op: in, value: "active" }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(
        err,
        ConversionError::ExpectedListValue { op: "in" }
    ));
}

#[test]
fn convert_filter_between_wrong_arity_fails() {
    let schema = parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: total, op: between, value: [10] }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(
        err,
        ConversionError::InvalidBetweenArity { got: 1 }
    ));
}

#[test]
fn convert_filter_eq_missing_value_fails() {
    let schema = parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      filters:
        - { column: status, op: eq }
      primary_key: id
      fields: [id]
"#,
    )
    .unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(
        err,
        ConversionError::MissingFilterValue { op: "eq" }
    ));
}

#[test]
fn convert_field_conflicting_relation_fails() {
    let schema = parse(
        r#"
version: 1
table: users
fields:
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      primary_key: id
      fields: [id]
    aggregate:
      table: orders
      op: count
      foreign_key: user_id
"#,
    )
    .unwrap();
    let err = IndexSchema::try_from(schema).unwrap_err();
    assert!(matches!(err, ConversionError::ConflictingRelation));
}

// ── kind sugar ───────────────────────────────────────────────────────────────

use schema_core::{Column, FieldSource, FlussoType};

fn column_of<'a>(schema: &'a IndexSchema, name: &str) -> &'a Column {
    match &schema
        .fields
        .iter()
        .find(|f| f.field.as_ref() == name)
        .expect("field present")
        .source
    {
        FieldSource::Column(c) => c,
        other => panic!("expected a column field, got {other:?}"),
    }
}

fn analyzer_of(schema: &IndexSchema, name: &str) -> String {
    let field = schema
        .fields
        .iter()
        .find(|f| f.field.as_ref() == name)
        .expect("field present");
    match field.options.get("analyzer") {
        Some(schema_core::GenericValue::String(s)) => s.clone(),
        other => panic!("expected analyzer string, got {other:?}"),
    }
}

#[test]
fn kind_prose_implies_text_with_prose_analyzer() {
    let schema =
        convert("version: 1\ntable: users\nfields:\n  - id\n  - field: bio\n    kind: prose")
            .unwrap();
    assert_eq!(column_of(&schema, "bio").ty, FlussoType::Text);
    assert_eq!(analyzer_of(&schema, "bio"), "flusso_text");
}

#[test]
fn kind_code_implies_text_with_code_analyzer() {
    let schema =
        convert("version: 1\ntable: users\nfields:\n  - id\n  - field: sku\n    kind: code")
            .unwrap();
    assert_eq!(column_of(&schema, "sku").ty, FlussoType::Text);
    assert_eq!(analyzer_of(&schema, "sku"), "flusso_code");
}

#[test]
fn explicit_analyzer_beats_kind() {
    let schema = convert(
        "version: 1\ntable: users\nfields:\n  - id\n  - field: bio\n    kind: prose\n    options: { analyzer: english }",
    )
    .unwrap();
    assert_eq!(analyzer_of(&schema, "bio"), "english");
}

#[test]
fn kind_on_non_text_type_errors() {
    let yaml = "version: 1\ntable: users\nfields:\n  - id\n  - field: tags\n    kind: code\n    type: keyword";
    let err = IndexSchema::try_from(SchemaYaml::try_parse(yaml).unwrap()).unwrap_err();
    assert!(matches!(err, ConversionError::KindRequiresTextType { .. }));
}

#[test]
fn declared_type_sets_column_type() {
    let schema =
        convert("version: 1\ntable: users\nfields:\n  - field: age\n    type: integer").unwrap();
    assert_eq!(column_of(&schema, "age").ty, FlussoType::Integer);
}

#[test]
fn shorthand_defaults_to_keyword_nullable() {
    let schema = convert("version: 1\ntable: users\nfields:\n  - id").unwrap();
    let col = column_of(&schema, "id");
    assert_eq!(col.ty, FlussoType::Keyword);
    assert!(col.nullable);
}

#[test]
fn required_makes_column_non_null() {
    let schema =
        convert("version: 1\ntable: users\nfields:\n  - field: id\n    required: true").unwrap();
    assert!(!column_of(&schema, "id").nullable);
}

#[test]
fn type_on_join_field_errors() {
    let yaml = "version: 1\ntable: users\nfields:\n  - id\n  - field: orders\n    type: integer\n    join:\n      table: orders\n      type: one_to_many\n      foreign_key: user_id\n      primary_key: id\n      fields: [id]";
    let err = IndexSchema::try_from(SchemaYaml::try_parse(yaml).unwrap()).unwrap_err();
    assert!(matches!(err, ConversionError::TypeOnNonScalarField { .. }));
}

#[test]
fn aggregate_sum_without_type_errors() {
    let yaml = "version: 1\ntable: users\nfields:\n  - field: total\n    aggregate:\n      table: orders\n      op: sum\n      column: amount\n      foreign_key: user_id";
    let err = IndexSchema::try_from(SchemaYaml::try_parse(yaml).unwrap()).unwrap_err();
    assert!(matches!(
        err,
        ConversionError::MissingAggregateType { op: "sum" }
    ));
}

#[test]
fn kind_on_join_field_errors() {
    let yaml = "version: 1\ntable: users\nfields:\n  - id\n  - field: orders\n    kind: prose\n    join:\n      table: orders\n      type: one_to_many\n      foreign_key: user_id\n      primary_key: id\n      fields: [id]";
    let err = IndexSchema::try_from(SchemaYaml::try_parse(yaml).unwrap()).unwrap_err();
    assert!(matches!(err, ConversionError::KindOnNonScalarField));
}

#[test]
fn unknown_kind_fails_to_parse() {
    // An invalid `kind` value is rejected at parse time, like any other enum.
    let yaml = "version: 1\ntable: users\nfields:\n  - id\n  - field: bio\n    kind: bogus";
    assert!(parse(yaml).is_err());
}
