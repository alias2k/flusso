#![allow(clippy::unwrap_used)]
use super::*;
use schema_core::{Column, ColumnName, FieldName};
use serde_json::json;

fn column_field(name: &str, ty: FlussoType) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: ColumnName::try_new(name).unwrap(),
            ty,
            nullable: true,
            transforms: Vec::new(),
            default: None,
        }),
    }
}

fn body(doc: serde_json::Value, fields: &[Field]) -> BTreeMap<String, GenericValue> {
    match coerce_document(doc, fields) {
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
