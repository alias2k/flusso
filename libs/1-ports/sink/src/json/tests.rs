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
