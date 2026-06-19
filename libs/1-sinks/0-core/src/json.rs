//! Rendering a [`GenericValue`] as the JSON a sink (OpenSearch) ingests.
//!
//! `GenericValue`'s derived `Serialize` is externally tagged (`{"Int": 5}`),
//! which is not what a sink wants to emit. This is the **canonical → sink**
//! translation: numbers as numbers, strings as strings, maps as objects, and the
//! typed scalars as what OpenSearch expects — temporals as ISO strings (a `date`
//! field reads them), a UUID as its hyphenated string, and `bytea` as **base64**
//! (what an OpenSearch `binary` field wants). The base64 lives here, at the sink
//! boundary, not in core's value vocabulary.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use schema_core::GenericValue;
use serde_json::{Number, Value};

/// Convert a document value into the JSON a sink ingests.
pub fn to_json(value: &GenericValue) -> Value {
    match value {
        GenericValue::Null => Value::Null,
        GenericValue::Bool(b) => Value::Bool(*b),
        GenericValue::SmallInt(i) => Value::Number((*i).into()),
        GenericValue::Int(i) => Value::Number((*i).into()),
        GenericValue::BigInt(i) => Value::Number((*i).into()),
        GenericValue::Float(f) => float_to_json((*f).into()),
        GenericValue::Double(f) => float_to_json(*f),
        GenericValue::Decimal(d) => decimal_to_json(d),
        GenericValue::String(s) => Value::String(s.clone()),
        GenericValue::Uuid(u) => Value::String(u.to_string()),
        GenericValue::Date(d) => Value::String(d.to_string()),
        GenericValue::Time(t) => Value::String(t.to_string()),
        GenericValue::Timestamp(ts) => Value::String(ts.to_string()),
        GenericValue::TimestampTz(ts) => Value::String(ts.to_rfc3339()),
        GenericValue::Bytes(bytes) => Value::String(base64_encode(bytes)),
        GenericValue::Array(items) => Value::Array(items.iter().map(to_json).collect()),
        GenericValue::Map(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
    }
}

/// Floats become JSON numbers; a non-finite float (NaN/∞ — not valid JSON)
/// becomes null rather than failing the document.
fn float_to_json(value: f64) -> Value {
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// Standard base64 (no line breaks) — what an OpenSearch `binary` field expects.
/// Dependency-free; `bytea` values are rare on the document path.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // `% 64` masks to 6 bits and keeps the lookup in range, so no index is ever
    // out of bounds.
    let symbol =
        |six_bits: u32| char::from(*ALPHABET.get((six_bits % 64) as usize).unwrap_or(&b'A'));
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut it = chunk.iter();
        let b0 = u32::from(*it.next().unwrap_or(&0));
        let b1 = u32::from(*it.next().unwrap_or(&0));
        let b2 = u32::from(*it.next().unwrap_or(&0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(symbol(n >> 18));
        out.push(symbol(n >> 12));
        out.push(if chunk.len() > 1 { symbol(n >> 6) } else { '=' });
        out.push(if chunk.len() > 2 { symbol(n) } else { '=' });
    }
    out
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
mod tests;
