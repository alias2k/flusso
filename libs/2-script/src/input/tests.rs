#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;

use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use config::yaml::SchemaYaml;
use kernel::{IndexSchema, ParseFrom};

use super::*;

fn schema(fields: &str) -> IndexSchema {
    let yaml = format!("version: 1\ntable: events\nprimary_key: id\nfields:\n{fields}");
    IndexSchema::try_from(SchemaYaml::try_parse(&yaml).unwrap()).unwrap()
}

fn map(value: &GenericValue) -> &BTreeMap<String, GenericValue> {
    match value {
        GenericValue::Map(map) => map,
        other => panic!("{other:?}"),
    }
}

#[test]
fn input_json_is_typed_by_the_schema_like_the_builder() {
    let schema = schema(
        "  \
  - date: day\n    required: true\n  \
  - timestamp: at\n    required: true\n  \
  - timestamp: local\n    required: true\n  \
  - timestamp: clock\n    required: true\n  \
  - decimal: price\n    required: true\n  \
  - decimal: cost\n    required: true\n  \
  - date: holidays\n    required: true\n    array: true\n  \
  - keyword: label\n    required: true\n  \
  - map: due\n    values: date\n    required: true\n  \
  - map: rates\n    values: decimal\n    required: true\n  \
  - has_many: stays\n    table: stays\n    primary_key: id\n    foreign_key: event_id\n    fields:\n      \
    - date: from\n        required: true\n  \
  - max: lastStay\n    table: stays\n    foreign_key: event_id\n    column: from\n    value_type: date\n",
    );
    let json = serde_json::json!({
        "day": "2024-05-01",
        "at": "2024-05-01T12:00:00+02:00",
        "local": "2024-05-01T12:00:00",
        "clock": "10:15:00",
        "price": 12.50,
        "cost": "0.1",
        "holidays": ["2024-01-06", "not a date"],
        "label": "2024-05-01",
        "due": { "en": "2024-06-10" },
        "rates": { "vat": "0.22" },
        "stays": [{ "from": "2024-02-27" }],
        "lastStay": "2024-02-27",
        "extra": "2024-05-01",
    });
    let doc = document(&schema.fields, json);
    let doc = map(&doc);
    let day = NaiveDate::from_ymd_opt(2024, 5, 1).unwrap();
    assert_eq!(doc["day"], GenericValue::Date(day));
    assert_eq!(
        doc["at"],
        GenericValue::TimestampTz(Utc.with_ymd_and_hms(2024, 5, 1, 10, 0, 0).unwrap())
    );
    assert_eq!(
        doc["local"],
        GenericValue::Timestamp(day.and_hms_opt(12, 0, 0).unwrap())
    );
    assert_eq!(
        doc["clock"],
        GenericValue::Time(NaiveTime::from_hms_opt(10, 15, 0).unwrap())
    );
    assert_eq!(
        doc["price"],
        GenericValue::BigDecimal(kernel::BigDecimal::parse("12.5").unwrap())
    );
    assert_eq!(
        doc["cost"],
        GenericValue::BigDecimal(kernel::BigDecimal::parse("0.1").unwrap())
    );
    assert_eq!(
        doc["holidays"],
        GenericValue::Array(vec![
            GenericValue::Date(NaiveDate::from_ymd_opt(2024, 1, 6).unwrap()),
            GenericValue::String("not a date".to_owned()),
        ])
    );
    assert_eq!(doc["label"], GenericValue::String("2024-05-01".to_owned()));
    assert_eq!(
        map(&doc["due"])["en"],
        GenericValue::String("2024-06-10".to_owned()),
        "a map of dates stays text, as the builder leaves it"
    );
    assert_eq!(
        map(&doc["rates"])["vat"],
        GenericValue::BigDecimal(kernel::BigDecimal::parse("0.22").unwrap())
    );
    let GenericValue::Array(stays) = &doc["stays"] else {
        panic!()
    };
    assert_eq!(
        map(&stays[0])["from"],
        GenericValue::Date(NaiveDate::from_ymd_opt(2024, 2, 27).unwrap())
    );
    assert_eq!(
        doc["lastStay"],
        GenericValue::Date(NaiveDate::from_ymd_opt(2024, 2, 27).unwrap())
    );
    assert_eq!(doc["extra"], GenericValue::String("2024-05-01".to_owned()));
}

/// A number with no declared type reads as the Postgres builder reads it
/// (`kernel::untyped_json_number`): a script sees the same kind from
/// `--input` as from a build (issue #366).
#[test]
fn json_with_no_declared_type_reads_numbers_as_the_builder_does() {
    let json: serde_json::Value = serde_json::from_str(
        r#"{"n": 3, "x": 1.5, "s": "a", "big": 18446744073709551615, "tiny": 3.141592653589793e-14, "huge": 1.5e300}"#,
    )
    .unwrap();
    let doc = document(&[], json);
    let GenericValue::Map(doc) = doc else {
        panic!("{doc:?}")
    };
    assert_eq!(doc["n"], GenericValue::BigInt(3));
    assert_eq!(doc["s"], GenericValue::String("a".to_owned()));
    assert_eq!(doc["x"], GenericValue::Decimal("1.5".parse().unwrap()));
    assert_eq!(
        doc["big"],
        GenericValue::Decimal("18446744073709551615".parse().unwrap())
    );
    assert_eq!(
        doc["tiny"],
        kernel::untyped_number(3.141_592_653_589_793e-14)
    );
    assert!(matches!(doc["tiny"], GenericValue::BigDecimal(_)));
    assert!(matches!(doc["huge"], GenericValue::String(_)));
}

/// A declared `double` or `float` reads as that float, whole numbers included,
/// as the builder reads the column: a script multiplying it by a float literal
/// must run here as it runs in the pipeline.
#[test]
fn a_declared_float_reads_as_a_float_not_as_an_untyped_number() {
    let schema = schema(
        "  \
  - double: price\n    required: true\n  \
  - double: whole\n    required: true\n  \
  - float: ratio\n    required: true\n  \
  - double: series\n    required: true\n    array: true\n",
    );
    let json = serde_json::json!({
        "price": 9.5,
        "whole": 10,
        "ratio": 0.25,
        "series": [1, 2.5],
    });
    let doc = document(&schema.fields, json);
    let doc = map(&doc);
    assert_eq!(doc["price"], GenericValue::Double(9.5));
    assert_eq!(doc["whole"], GenericValue::Double(10.0));
    assert_eq!(doc["ratio"], GenericValue::Float(0.25));
    assert_eq!(
        doc["series"],
        GenericValue::Array(vec![GenericValue::Double(1.0), GenericValue::Double(2.5)])
    );
}
