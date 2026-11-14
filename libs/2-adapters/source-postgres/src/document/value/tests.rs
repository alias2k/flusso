#![allow(clippy::unwrap_used)]
use super::*;
use kernel::{Column, ColumnName, FieldName};
use serde_json::json;

fn column_field(name: &str, ty: FlussoType) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: ColumnName::try_new(name).unwrap(),
            ty,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

fn body(doc: serde_json::Value, fields: &[Field]) -> BTreeMap<String, GenericValue> {
    match coerce_document(doc, fields).unwrap() {
        GenericValue::Map(map) => map,
        other => panic!("expected a map document, got {other:?}"),
    }
}

#[test]
fn coerces_each_leaf_to_its_declared_type() {
    let fields = vec![
        column_field("uid", FlussoType::Uuid),
        column_field("born", FlussoType::Date),
        column_field("seen", FlussoType::Timestamp),
        column_field("clock", FlussoType::Timestamp),
        column_field("at", FlussoType::Timestamp),
        column_field("small", FlussoType::Short),
        column_field("n", FlussoType::Integer),
        column_field("big", FlussoType::Long),
        column_field("ratio", FlussoType::Double),
        column_field("name", FlussoType::Keyword),
    ];
    let map = body(
        json!({
            "uid": "11111111-1111-1111-1111-111111111111",
            "born": "2024-01-02",
            "seen": "2024-01-02T03:04:05+00:00",
            "clock": "2024-01-02T03:04:05",
            "at": "03:04:05",
            "small": 7,
            "n": 70,
            "big": 9_000_000_000_i64,
            "ratio": 1.5,
            "name": "ada",
        }),
        &fields,
    );
    assert!(matches!(map.get("uid"), Some(GenericValue::Uuid(_))));
    assert!(matches!(map.get("born"), Some(GenericValue::Date(_))));
    assert!(matches!(
        map.get("seen"),
        Some(GenericValue::TimestampTz(_))
    ));
    assert!(matches!(map.get("clock"), Some(GenericValue::Timestamp(_))));
    assert!(matches!(map.get("at"), Some(GenericValue::Time(_))));
    assert_eq!(map.get("small"), Some(&GenericValue::SmallInt(7)));
    assert_eq!(map.get("n"), Some(&GenericValue::Int(70)));
    assert_eq!(map.get("big"), Some(&GenericValue::BigInt(9_000_000_000)));
    assert_eq!(map.get("ratio"), Some(&GenericValue::Double(1.5)));
    assert_eq!(
        map.get("name"),
        Some(&GenericValue::String("ada".to_owned()))
    );
}

#[test]
fn an_unparseable_value_falls_back_untyped_rather_than_failing() {
    let fields = vec![column_field("uid", FlussoType::Uuid)];
    let map = body(json!({ "uid": "not-a-uuid" }), &fields);
    assert_eq!(
        map.get("uid"),
        Some(&GenericValue::String("not-a-uuid".to_owned()))
    );
}

#[test]
fn a_null_leaf_stays_null_for_any_type() {
    let fields = vec![column_field("born", FlussoType::Date)];
    let map = body(json!({ "born": null }), &fields);
    assert_eq!(map.get("born"), Some(&GenericValue::Null));
}

fn array_field(name: &str, ty: FlussoType, default: Option<GenericValue>) -> Field {
    let mut field = column_field(name, ty);
    if let FieldSource::Column(column) = &mut field.source {
        column.array = true;
        column.default = default;
    }
    field
}

fn strings(items: &[&str]) -> GenericValue {
    GenericValue::Array(
        items
            .iter()
            .map(|item| GenericValue::String((*item).to_owned()))
            .collect(),
    )
}

#[test]
fn an_array_column_drops_null_elements_and_wraps_a_lone_value() {
    let fields = vec![
        array_field("tags", FlussoType::Keyword, None),
        array_field("solo", FlussoType::Keyword, None),
        array_field("none", FlussoType::Keyword, None),
        array_field("empty", FlussoType::Keyword, None),
        array_field("days", FlussoType::Date, None),
    ];
    let map = body(
        json!({
            "tags": ["urgent", null, "fragile"],
            "solo": "urgent",
            "none": null,
            "empty": [],
            "days": ["2024-01-02", null],
        }),
        &fields,
    );
    assert_eq!(map["tags"], strings(&["urgent", "fragile"]));
    assert_eq!(map["solo"], strings(&["urgent"]));
    assert_eq!(map["none"], GenericValue::Null);
    assert_eq!(map["empty"], GenericValue::Array(Vec::new()));
    assert_eq!(
        map["days"],
        GenericValue::Array(vec![GenericValue::Date(
            NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
        )])
    );
}

#[test]
fn a_null_array_column_takes_its_default_list_coerced_to_the_element_type() {
    let fields = vec![
        array_field("tags", FlussoType::Keyword, Some(strings(&["general"]))),
        array_field(
            "scores",
            FlussoType::Integer,
            Some(GenericValue::Array(vec![GenericValue::BigInt(1)])),
        ),
    ];
    let map = body(json!({ "tags": null, "scores": null }), &fields);
    assert_eq!(map["tags"], strings(&["general"]));
    assert_eq!(
        map["scores"],
        GenericValue::Array(vec![GenericValue::Int(1)])
    );

    let map = body(json!({ "tags": ["vip"], "scores": [] }), &fields);
    assert_eq!(map["tags"], strings(&["vip"]));
    assert_eq!(map["scores"], GenericValue::Array(Vec::new()));
}

#[test]
fn a_json_array_column_keeps_structured_elements_and_drops_only_top_level_nulls() {
    let fields = vec![
        array_field("notes", FlussoType::Json, None),
        array_field("one", FlussoType::Json, None),
    ];
    let map = body(
        json!({ "notes": [{ "a": null }, null, [1, null]], "one": { "a": 1 } }),
        &fields,
    );
    assert_eq!(
        map["notes"],
        GenericValue::Array(vec![
            GenericValue::Map(BTreeMap::from([("a".to_owned(), GenericValue::Null)])),
            GenericValue::Array(vec![GenericValue::BigInt(1), GenericValue::Null]),
        ])
    );
    assert_eq!(
        map["one"],
        GenericValue::Array(vec![GenericValue::Map(BTreeMap::from([(
            "a".to_owned(),
            GenericValue::BigInt(1)
        )]))])
    );
}

#[test]
fn a_structured_element_in_a_scalar_array_fails_the_document_at_its_path() {
    let fields = vec![array_field("tags", FlussoType::Keyword, None)];
    let failure = coerce_document(json!({ "tags": ["a", { "b": 1 }] }), &fields).unwrap_err();
    assert_eq!(failure.field, "tags");
    assert!(
        failure.message.contains("an object at [1]"),
        "{}",
        failure.message
    );

    let failure = coerce_document(json!({ "tags": [["a"]] }), &fields).unwrap_err();
    assert!(
        failure.message.contains("an array at [0]"),
        "{}",
        failure.message
    );

    let failure = coerce_document(json!({ "tags": { "b": 1 } }), &fields).unwrap_err();
    assert!(
        failure.message.starts_with("the column holds an object,"),
        "{}",
        failure.message
    );

    let joined = vec![Field {
        field: FieldName::try_new("items").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(kernel::Join {
            table: kernel::TableName::try_new("items").unwrap(),
            kind: kernel::JoinKind::HasMany {
                foreign_key: ColumnName::try_new("order_id").unwrap(),
            },
            primary_key: ColumnName::try_new("id").unwrap(),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: fields.clone(),
            indexed: true,
            on: Vec::new(),
        })),
    }];
    let failure = coerce_document(
        json!({ "items": [{ "tags": ["a"] }, { "tags": [{}] }] }),
        &joined,
    )
    .unwrap_err();
    assert_eq!(failure.field, "items[1].tags");
}

#[test]
fn a_binary_value_reads_back_as_the_bytes_postgres_rendered_in_hex() {
    let fields = vec![
        column_field("blob", FlussoType::Binary),
        column_field("empty", FlussoType::Binary),
        column_field("upper", FlussoType::Binary),
        array_field("blobs", FlussoType::Binary, None),
    ];
    let map = body(
        json!({
            "blob": "\\x00ff7f80",
            "empty": "\\x",
            "upper": "\\xABCD",
            "blobs": ["\\x01", null, "\\x0203"],
        }),
        &fields,
    );
    assert_eq!(
        map.get("blob"),
        Some(&GenericValue::Bytes(vec![0x00, 0xff, 0x7f, 0x80]))
    );
    assert_eq!(map.get("empty"), Some(&GenericValue::Bytes(Vec::new())));
    assert_eq!(
        map.get("upper"),
        Some(&GenericValue::Bytes(vec![0xab, 0xcd]))
    );
    assert_eq!(
        map.get("blobs"),
        Some(&GenericValue::Array(vec![
            GenericValue::Bytes(vec![0x01]),
            GenericValue::Bytes(vec![0x02, 0x03]),
        ]))
    );
}

#[test]
fn a_binary_value_that_is_not_postgres_hex_stays_as_it_reads() {
    let fields = vec![
        column_field("odd", FlussoType::Binary),
        column_field("bare", FlussoType::Binary),
        column_field("junk", FlussoType::Binary),
    ];
    let map = body(
        json!({ "odd": "\\x012", "bare": "0102", "junk": "\\x+1" }),
        &fields,
    );
    assert_eq!(map.get("odd"), Some(&GenericValue::String("\\x012".into())));
    assert_eq!(map.get("bare"), Some(&GenericValue::String("0102".into())));
    assert_eq!(map.get("junk"), Some(&GenericValue::String("\\x+1".into())));
}

#[test]
fn a_binary_ids_aggregate_reads_back_as_bytes() {
    let aggregate = Aggregate {
        table: kernel::TableName::try_new("files").unwrap(),
        op: AggregateOp::Ids {
            element_type: FlussoType::Binary,
        },
        key: kernel::AggregateKey::Direct(ColumnName::try_new("owner_id").unwrap()),
        value_type: None,
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: Vec::new(),
    };
    assert_eq!(
        coerce_aggregate(json!(["\\x0a", "\\x0b"]), &aggregate),
        GenericValue::Array(vec![
            GenericValue::Bytes(vec![0x0a]),
            GenericValue::Bytes(vec![0x0b]),
        ])
    );
}

fn constant_field(name: &str, value: GenericValue) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Constant(value),
    }
}

#[test]
fn a_constant_builds_the_value_the_schema_holds_whatever_the_query_read() {
    let object = GenericValue::Map(BTreeMap::from([
        ("a".to_owned(), GenericValue::BigInt(1)),
        (
            "b".to_owned(),
            GenericValue::Array(vec![GenericValue::String("x".to_owned())]),
        ),
    ]));
    let constants = [
        ("text", GenericValue::String("crm".to_owned())),
        ("number", GenericValue::BigInt(5)),
        ("fraction", GenericValue::Decimal("1.5".parse().unwrap())),
        ("flag", GenericValue::Bool(true)),
        ("object", object),
        (
            "list",
            GenericValue::Array(vec![
                GenericValue::String("a".to_owned()),
                GenericValue::String("b".to_owned()),
            ]),
        ),
        ("nothing", GenericValue::Null),
    ];
    let fields: Vec<Field> = constants
        .iter()
        .map(|(name, value)| constant_field(name, value.clone()))
        .collect();
    let map = body(
        json!({
            "text": "crm",
            "number": "5",
            "fraction": "1.5",
            "flag": "true",
            "object": null,
            "list": null,
            "nothing": null,
        }),
        &fields,
    );
    for (name, value) in &constants {
        assert_eq!(map.get(*name), Some(value), "{name}");
    }
}

/// A `map` over a `jsonb` column keeps a key whose value is JSON `null`: the
/// document carries `{"x": null}`, so a typed map value must read `null`
/// (`HashMap<String, Option<String>>`) to read every document (issue #294).
#[test]
fn a_map_keeps_a_key_whose_value_is_null() {
    let fields = [column_field(
        "labels",
        FlussoType::Map {
            values: Box::new(FlussoType::Keyword),
        },
    )];
    let doc = body(json!({ "labels": { "x": null, "y": "a" } }), &fields);
    let GenericValue::Map(labels) = &doc["labels"] else {
        panic!("expected a map, got {:?}", doc["labels"]);
    };
    assert_eq!(labels.get("x"), Some(&GenericValue::Null));
    assert_eq!(labels.get("y"), Some(&GenericValue::String("a".into())));
}

/// A `timetz` column under a schema `timestamp` reads as none of the typed
/// date variants, so it is kept as the text Postgres wrote: the query
/// manual's date table lists it as read by `String` only (issue #294).
#[test]
fn a_time_with_a_zone_stays_the_text_postgres_wrote() {
    let fields = [column_field("at", FlussoType::Timestamp)];
    let doc = body(json!({ "at": "03:04:05.123456+02" }), &fields);
    assert_eq!(doc["at"], GenericValue::String("03:04:05.123456+02".into()));
}

/// A `default` list never holds a timestamp when it comes from a schema file,
/// but a hand-written `flusso.lock` (`Timestamp = "…"`) or a designer request
/// can carry one. It reaches the document in the ISO 8601 form `sink::to_json`
/// writes a timestamp in: as the timestamp itself under a `timestamp` field,
/// as that text under a `keyword` one.
#[test]
fn a_timestamp_in_a_default_list_reads_as_the_iso_8601_form() {
    let stamp = GenericValue::Timestamp("2024-01-02T03:04:05.123".parse().unwrap());
    let fields = vec![
        array_field(
            "seen",
            FlussoType::Timestamp,
            Some(GenericValue::Array(vec![stamp.clone()])),
        ),
        array_field(
            "labels",
            FlussoType::Keyword,
            Some(GenericValue::Array(vec![stamp.clone()])),
        ),
    ];
    let map = body(json!({ "seen": null, "labels": null }), &fields);
    assert_eq!(map["seen"], GenericValue::Array(vec![stamp]));
    assert_eq!(map["labels"], strings(&["2024-01-02T03:04:05.123"]));
}

/// `doc` as the query returns it: JSON text, parsed as the builder parses it,
/// so a number keeps the digits Postgres wrote.
fn body_text(doc: &str, fields: &[Field]) -> BTreeMap<String, GenericValue> {
    match build(doc, fields).unwrap() {
        GenericValue::Map(map) => map,
        other => panic!("expected a map document, got {other:?}"),
    }
}

/// Build the document `doc` (JSON text) for `fields`.
fn build(doc: &str, fields: &[Field]) -> Result<GenericValue, BuildFailure> {
    coerce_document(serde_json::from_str(doc).unwrap(), fields)
}

/// The exact decimal text `value` carries, `None` when it is not an exact
/// decimal number.
fn exact_decimal(value: Option<&GenericValue>) -> Option<String> {
    match value? {
        GenericValue::Decimal(d) => Some(d.to_string()),
        _ => None,
    }
}

fn aggregate_field(name: &str, op: AggregateOp, value_type: Option<FlussoType>) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: kernel::TableName::try_new("parts").unwrap(),
            op,
            key: kernel::AggregateKey::Direct(ColumnName::try_new("owner_id").unwrap()),
            value_type,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: Vec::new(),
        })),
    }
}

/// Every digit of a `numeric` reaches a `decimal` field, inside
/// `rust_decimal`'s range or past it (issue #327): a value past ±7.9e28 or with
/// more than 28 fractional digits is still a number, never a string, and a
/// value within it keeps the digits `f64` would round away, trailing zeros
/// included.
#[test]
fn a_decimal_keeps_every_digit_postgres_wrote() {
    let cases = [
        ("past", "1000000000000000000000000000000"),
        ("negative", "-1000000000000000000000000000000.5"),
        ("fraction", "0.123456789012345678901234567890"),
        ("wide", "1234567890.123456789"),
        ("scale", "12.50"),
        ("small", "0.5"),
        ("whole", "42"),
    ];
    let fields: Vec<Field> = cases
        .iter()
        .map(|(name, _)| column_field(name, FlussoType::Decimal))
        .collect();
    let doc = format!(
        "{{{}}}",
        cases
            .iter()
            .map(|(name, text)| format!("\"{name}\": {text}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let map = body_text(&doc, &fields);
    for (name, text) in cases {
        assert_eq!(
            exact_decimal(map.get(name)).as_deref(),
            Some(text),
            "{name}: {:?}",
            map.get(name)
        );
    }
}

/// The elements of a `decimal` array column, `sum`/`min`/`max` with a
/// `decimal` result, and `ids` of `decimal` keys keep every digit too.
#[test]
fn a_decimal_array_and_aggregates_keep_every_digit() {
    let big = "1000000000000000000000000000000";
    let fraction = "0.123456789012345678901234567890";
    let fields = vec![
        array_field("amounts", FlussoType::Decimal, None),
        aggregate_field(
            "total",
            AggregateOp::Sum(ColumnName::try_new("v").unwrap().into()),
            Some(FlussoType::Decimal),
        ),
        aggregate_field(
            "least",
            AggregateOp::Min(ColumnName::try_new("v").unwrap().into()),
            Some(FlussoType::Decimal),
        ),
        aggregate_field(
            "most",
            AggregateOp::Max(ColumnName::try_new("v").unwrap().into()),
            Some(FlussoType::Decimal),
        ),
        aggregate_field(
            "keys",
            AggregateOp::Ids {
                element_type: FlussoType::Decimal,
            },
            None,
        ),
    ];
    let map = body_text(
        &format!(
            r#"{{"amounts": [{big}, null, {fraction}], "total": {big}, "least": {fraction},
                "most": {big}, "keys": [{big}, {fraction}]}}"#
        ),
        &fields,
    );
    for name in ["amounts", "keys"] {
        let Some(GenericValue::Array(items)) = map.get(name) else {
            panic!("{name}: expected an array, got {:?}", map.get(name));
        };
        let texts: Vec<Option<String>> = items.iter().map(|i| exact_decimal(Some(i))).collect();
        assert_eq!(
            texts,
            [Some(big.to_owned()), Some(fraction.to_owned())],
            "{name}"
        );
    }
    for (name, text) in [("total", big), ("least", fraction), ("most", big)] {
        assert_eq!(
            exact_decimal(map.get(name)).as_deref(),
            Some(text),
            "{name}: {:?}",
            map.get(name)
        );
    }
}

/// A `double` field over a `numeric` past `rust_decimal`'s range is the
/// nearest `f64`, as for any other `double`.
#[test]
fn a_double_past_the_decimal_range_is_the_nearest_double() {
    let fields = vec![column_field("ratio", FlussoType::Double)];
    let map = body_text(r#"{"ratio": 1000000000000000000000000000000}"#, &fields);
    assert_eq!(map.get("ratio"), Some(&GenericValue::Double(1e30)));
}

/// A number past `rust_decimal`'s range inside a `json` field stays a number.
#[test]
fn a_json_number_past_the_decimal_range_stays_a_number() {
    let fields = vec![column_field("payload", FlussoType::Json)];
    let map = body_text(r#"{"payload": {"n": 1e30, "m": [-1e30]}}"#, &fields);
    let want = GenericValue::Map(BTreeMap::from([
        ("n".to_owned(), GenericValue::Double(1e30)),
        (
            "m".to_owned(),
            GenericValue::Array(vec![GenericValue::Double(-1e30)]),
        ),
    ]));
    assert_eq!(map.get("payload"), Some(&want));
}

/// `NaN`, `Infinity` and `-Infinity` have no JSON number: Postgres writes them
/// as strings. Under a `decimal`, `double` or `float` field (a scalar, an array
/// element or an aggregate) the document fails at the field's path, for
/// `on_error` to decide, instead of holding a string where its other
/// documents hold a number.
#[test]
fn a_non_finite_number_fails_the_document_at_its_field() {
    for text in ["NaN", "Infinity", "-Infinity"] {
        for ty in [FlussoType::Decimal, FlussoType::Double, FlussoType::Float] {
            let fields = vec![column_field("n", ty.clone())];
            let doc = json!({ "n": text }).to_string();
            let failure = build(&doc, &fields).expect_err(&format!("{text} as {ty:?}"));
            assert_eq!(failure.field, "n", "{text} as {ty:?}");
            assert!(
                failure.message.contains(text),
                "{text} as {ty:?}: {}",
                failure.message
            );
        }
        let fields = vec![
            column_field("id", FlussoType::Long),
            array_field("amounts", FlussoType::Decimal, None),
        ];
        let doc = json!({ "id": 1, "amounts": [1, text] }).to_string();
        let failure = build(&doc, &fields).expect_err(text);
        assert_eq!(failure.field, "amounts[1]", "{text}");

        let fields = vec![
            aggregate_field(
                "total",
                AggregateOp::Sum(ColumnName::try_new("v").unwrap().into()),
                Some(FlussoType::Decimal),
            ),
            aggregate_field(
                "mean",
                AggregateOp::Avg(ColumnName::try_new("v").unwrap().into()),
                None,
            ),
        ];
        for name in ["total", "mean"] {
            let doc = json!({ name: text }).to_string();
            let failure = build(&doc, &fields).expect_err(&format!("{name}: {text}"));
            assert_eq!(failure.field, name, "{text}");
        }
    }
}

/// Text that is no number under a `decimal` field keeps its fallback: the
/// string it is.
#[test]
fn text_that_is_no_number_under_a_decimal_stays_a_string() {
    let fields = vec![column_field("n", FlussoType::Decimal)];
    let map = body_text(r#"{"n": "n/a"}"#, &fields);
    assert_eq!(map.get("n"), Some(&GenericValue::String("n/a".into())));
}
