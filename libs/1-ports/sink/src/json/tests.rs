use super::*;
use std::collections::BTreeMap;

#[test]
fn maps_to_plain_json() {
    let value = GenericValue::Map(BTreeMap::from([
        ("name".to_owned(), GenericValue::String("ada".to_owned())),
        ("age".to_owned(), GenericValue::Int(36)),
        ("admin".to_owned(), GenericValue::Bool(true)),
        (
            "tags".to_owned(),
            GenericValue::Array(vec![GenericValue::String("a".to_owned())]),
        ),
        ("missing".to_owned(), GenericValue::Null),
    ]));
    assert_eq!(
        to_json(&value),
        serde_json::json!({
            "name": "ada",
            "age": 36,
            "admin": true,
            "tags": ["a"],
            "missing": null,
        })
    );
}

#[test]
fn decimal_becomes_a_number() {
    let value = GenericValue::Decimal(Decimal::new(1234, 2)); // 12.34
    assert_eq!(to_json(&value), serde_json::json!(12.34));
}

/// What a document holds: the compact JSON [`Json`] writes.
fn written(value: &GenericValue) -> String {
    serde_json::to_string(&Json(value)).unwrap()
}

/// A decimal is written as a JSON number holding its exact text, trailing
/// zeros included (issue #327; before, it was the nearest `f64`, issue #306).
/// A reader without `serde_json`'s `arbitrary_precision` still reads it
/// through `f64`: the largest `Decimal` is then the `f64` past its range.
#[test]
fn a_decimal_is_written_as_its_exact_text() {
    assert_eq!(
        written(&GenericValue::Decimal(Decimal::MAX)),
        "79228162514264337593543950335"
    );
    assert_eq!(
        written(&GenericValue::Decimal(Decimal::MIN)),
        "-79228162514264337593543950335"
    );
    assert_eq!(
        written(&GenericValue::Decimal(
            "0.0000000000000000000000000001".parse().unwrap()
        )),
        "0.0000000000000000000000000001"
    );
    assert_eq!(
        written(&GenericValue::Decimal(Decimal::new(1250, 2))),
        "12.50"
    );
    let max = written(&GenericValue::Decimal(Decimal::MAX));
    assert_eq!(
        serde_json::from_str::<f64>(&max).unwrap(),
        7.922_816_251_426_434e28
    );
    assert!(serde_json::from_str::<Decimal>(&max).is_err());
    let price = written(&GenericValue::Decimal(Decimal::new(1250, 2)));
    assert_eq!(
        serde_json::from_str::<Decimal>(&price).unwrap(),
        Decimal::new(125, 1)
    );
}

/// A decimal past `Decimal`'s range is a JSON number with every digit, inside
/// an object or an array too; `f64` reads it, `Decimal` can't (issue #327).
#[test]
fn a_big_decimal_is_written_as_a_number_with_every_digit() {
    let big = |text: &str| GenericValue::BigDecimal(kernel::BigDecimal::parse(text).unwrap());
    let value = GenericValue::Map(BTreeMap::from([
        ("past".to_owned(), big("1000000000000000000000000000000")),
        (
            "list".to_owned(),
            GenericValue::Array(vec![big("-0.123456789012345678901234567890")]),
        ),
    ]));
    let json = written(&value);
    assert_eq!(
        json,
        r#"{"list":[-0.123456789012345678901234567890],"past":1000000000000000000000000000000}"#
    );
    let pretty = serde_json::to_string_pretty(&Json(&value)).unwrap();
    assert!(
        pretty.contains("\"past\": 1000000000000000000000000000000"),
        "{pretty}"
    );
    let past = written(&big("1000000000000000000000000000000"));
    assert_eq!(serde_json::from_str::<f64>(&past).unwrap(), 1e30);
    assert!(serde_json::from_str::<Decimal>(&past).is_err());
}

/// [`to_json`]'s value tree holds a number as an `f64`.
#[test]
fn to_json_holds_a_decimal_as_the_nearest_double() {
    let big = GenericValue::BigDecimal(
        kernel::BigDecimal::parse("1000000000000000000000000000000").unwrap(),
    );
    assert_eq!(to_json(&big), serde_json::json!(1e30));
    assert_eq!(
        to_json(&GenericValue::Decimal(Decimal::MAX)),
        serde_json::json!(7.922_816_251_426_434e28)
    );
}

/// A non-finite float is written as `null`.
#[test]
fn a_non_finite_float_is_written_as_null() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(written(&GenericValue::Double(value)), "null");
    }
    assert_eq!(written(&GenericValue::Float(f32::NAN)), "null");
}

/// A map value that is null is written as `null` under its key, not dropped:
/// what the OpenSearch sink indexes for a `map` field over `{"x": null}`.
#[test]
fn a_null_map_value_is_written_under_its_key() {
    let value = GenericValue::Map([("x".to_owned(), GenericValue::Null)].into_iter().collect());
    assert_eq!(to_json(&value), serde_json::json!({ "x": null }));
}

/// What each date kind is written as, and which chrono type reads it back:
/// each kind reads into its own type;
/// the table the query manual's "date kinds" trusted rule states (issue #294).
/// A `date` column is a `Date`; `time`, `timestamp` and `timestamptz` columns
/// are all a schema `timestamp`, so one field may hold any of the last three.
#[test]
fn each_date_kind_is_written_in_one_form_and_read_by_few_types() {
    use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
    let day = NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();
    let time = NaiveTime::from_hms_micro_opt(3, 4, 5, 123_456).unwrap();
    let naive = day.and_time(time);
    let written = |value: GenericValue| to_json(&value);
    let date = written(GenericValue::Date(day));
    let time_of_day = written(GenericValue::Time(time));
    let timestamp = written(GenericValue::Timestamp(naive));
    let timestamptz = written(GenericValue::TimestampTz(Utc.from_utc_datetime(&naive)));
    assert_eq!(date, serde_json::json!("2024-01-02"));
    assert_eq!(time_of_day, serde_json::json!("03:04:05.123456"));
    assert_eq!(timestamp, serde_json::json!("2024-01-02T03:04:05.123456"));
    assert_eq!(
        timestamptz,
        serde_json::json!("2024-01-02T03:04:05.123456+00:00")
    );

    let reads = |form: &serde_json::Value| {
        [
            serde_json::from_value::<NaiveDate>(form.clone()).is_ok(),
            serde_json::from_value::<NaiveTime>(form.clone()).is_ok(),
            serde_json::from_value::<NaiveDateTime>(form.clone()).is_ok(),
            serde_json::from_value::<DateTime<Utc>>(form.clone()).is_ok(),
            serde_json::from_value::<String>(form.clone()).is_ok(),
        ]
    };
    //                               NaiveDate NaiveTime NaiveDateTime DateTime<Utc> String
    assert_eq!(reads(&date), [true, false, false, false, true]);
    assert_eq!(reads(&time_of_day), [false, true, false, false, true]);
    assert_eq!(reads(&timestamp), [false, false, true, false, true]);
    assert_eq!(reads(&timestamptz), [false, false, false, true, true]);
}

#[test]
fn a_naive_timestamp_is_written_in_iso_8601_with_a_t() {
    let written = |text: &str| to_json(&GenericValue::Timestamp(text.parse().unwrap()));
    assert_eq!(written("2024-01-02T03:04:05"), "2024-01-02T03:04:05");
    assert_eq!(
        written("2024-01-02T03:04:05.123"),
        "2024-01-02T03:04:05.123"
    );
    assert_eq!(
        written("2024-01-02T03:04:05.123456"),
        "2024-01-02T03:04:05.123456"
    );
}

#[test]
fn the_other_temporals_keep_their_written_form() {
    assert_eq!(
        to_json(&GenericValue::Date("2024-01-02".parse().unwrap())),
        "2024-01-02"
    );
    assert_eq!(
        to_json(&GenericValue::Time("03:04:05.123456".parse().unwrap())),
        "03:04:05.123456"
    );
    assert_eq!(
        to_json(&GenericValue::TimestampTz(
            "2024-01-02T03:04:05.123456Z".parse().unwrap()
        )),
        "2024-01-02T03:04:05.123456+00:00"
    );
}
