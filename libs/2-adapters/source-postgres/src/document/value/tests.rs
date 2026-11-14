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
