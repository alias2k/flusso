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

/// A map value that is null is written as `null` under its key, not dropped:
/// what the OpenSearch sink indexes for a `map` field over `{"x": null}`.
#[test]
fn a_null_map_value_is_written_under_its_key() {
    let value = GenericValue::Map([("x".to_owned(), GenericValue::Null)].into_iter().collect());
    assert_eq!(to_json(&value), serde_json::json!({ "x": null }));
}

/// What each date kind is written as, and which chrono type reads it back:
/// each kind reads into its own type, except a naive `timestamp`, which only a
/// `String` reads;
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
    assert_eq!(timestamp, serde_json::json!("2024-01-02 03:04:05.123456"));
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
    // Written with a space, which no chrono type reads (nor OpenSearch's
    // default `strict_date_optional_time`): only a `String` does.
    assert_eq!(reads(&timestamp), [false, false, false, false, true]);
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
