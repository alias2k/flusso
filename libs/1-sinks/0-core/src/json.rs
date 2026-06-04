//! Rendering a [`GenericValue`] as natural JSON.
//!
//! `GenericValue`'s derived `Serialize` is externally tagged (`{"Int": 5}`),
//! which is not what a sink wants to emit. This maps it to plain JSON instead —
//! numbers as numbers, strings as strings, maps as objects.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use schema_core::GenericValue;
use serde_json::{Number, Value};

/// Convert a document value into natural JSON.
pub fn to_json(value: &GenericValue) -> Value {
    match value {
        GenericValue::Null => Value::Null,
        GenericValue::Bool(b) => Value::Bool(*b),
        GenericValue::Int(i) => Value::Number((*i).into()),
        GenericValue::Decimal(d) => decimal_to_json(d),
        GenericValue::String(s) => Value::String(s.clone()),
        GenericValue::Array(items) => Value::Array(items.iter().map(to_json).collect()),
        GenericValue::Map(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
    }
}

/// Decimals become JSON numbers when they fit a float, else a string (so large
/// or high-precision values are preserved rather than silently mangled).
fn decimal_to_json(value: &Decimal) -> Value {
    match value.to_f64().and_then(Number::from_f64) {
        Some(number) => Value::Number(number),
        None => Value::String(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
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
}
