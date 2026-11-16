//! Rendering a [`GenericValue`] as the JSON a sink (OpenSearch) ingests.
//!
//! `GenericValue`'s derived `Serialize` is externally tagged (`{"Int": 5}`),
//! which is not what a sink wants to emit. This is the **canonical → sink**
//! translation: numbers as numbers, strings as strings, maps as objects, and the
//! typed scalars as what OpenSearch expects — temporals as ISO strings (a `date`
//! field reads them), a UUID as its hyphenated string, and `bytea` as **base64**
//! (what an OpenSearch `binary` field wants). The base64 lives here, at the sink
//! boundary, not in core's value vocabulary.
//!
//! [`Json`] is the rendering: serialize it with `serde_json`. A
//! [`BigDecimal`](GenericValue::BigDecimal), a declared `decimal`'s value, is
//! written as a JSON number holding its exact text, every digit kept (JSON
//! numbers have no precision limit). Every other number is written as before
//! issue #327: a [`Decimal`](GenericValue::Decimal) as its nearest `f64`.
//! [`to_json`] builds the same rendering as a [`Value`] tree, which holds a
//! number as an `f64`: use it where a value tree is needed, never to write a
//! document.
//!
//! ```
//! use kernel::{BigDecimal, GenericValue};
//! use sink::Json;
//!
//! let big = GenericValue::BigDecimal(BigDecimal::parse("1000000000000000000000000000000").unwrap());
//! assert_eq!(serde_json::to_string(&Json(&big)).unwrap(), "1000000000000000000000000000000");
//!
//! let price = GenericValue::BigDecimal(BigDecimal::parse("12.50").unwrap());
//! assert_eq!(serde_json::to_string(&Json(&price)).unwrap(), "12.50");
//!
//! let untyped = GenericValue::Decimal("12.50".parse().unwrap());
//! assert_eq!(serde_json::to_string(&Json(&untyped)).unwrap(), "12.5");
//! ```

use kernel::GenericValue;
use serde::ser::{Error as _, Serialize, Serializer};
use serde_json::value::RawValue;
use serde_json::{Number, Value};

/// A document value as the JSON a sink writes. Serialize it with `serde_json`:
/// a decimal is written through `serde_json`'s raw value, which another
/// serializer does not understand.
#[derive(Debug, Clone, Copy)]
pub struct Json<'a>(pub &'a GenericValue);

impl Serialize for Json<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            GenericValue::Null => serializer.serialize_unit(),
            GenericValue::Bool(b) => serializer.serialize_bool(*b),
            GenericValue::SmallInt(i) => serializer.serialize_i16(*i),
            GenericValue::Int(i) => serializer.serialize_i32(*i),
            GenericValue::BigInt(i) => serializer.serialize_i64(*i),
            GenericValue::Float(f) => float(f64::from(*f), serializer),
            GenericValue::Double(f) => float(*f, serializer),
            GenericValue::Decimal(d) => float(kernel::decimal_to_f64(*d), serializer),
            GenericValue::BigDecimal(d) => number(d.as_str(), serializer),
            GenericValue::String(s) => serializer.serialize_str(s),
            GenericValue::Uuid(u) => serializer.collect_str(u),
            GenericValue::Date(d) => serializer.collect_str(d),
            GenericValue::Time(t) => serializer.collect_str(t),
            // chrono's `Display` puts a space between date and time, which a
            // `date` field's default format refuses.
            GenericValue::Timestamp(ts) => {
                serializer.collect_str(&ts.format("%Y-%m-%dT%H:%M:%S%.f"))
            }
            GenericValue::TimestampTz(ts) => serializer.serialize_str(&ts.to_rfc3339()),
            GenericValue::Bytes(bytes) => serializer.serialize_str(&base64_encode(bytes)),
            GenericValue::Array(items) => serializer.collect_seq(items.iter().map(Json)),
            GenericValue::Map(fields) => {
                serializer.collect_map(fields.iter().map(|(k, v)| (k, Json(v))))
            }
        }
    }
}

/// Convert a document value into a [`Value`] tree: what [`Json`] writes, with
/// every number held as an `f64`. A [`BigDecimal`](GenericValue::BigDecimal)
/// is its nearest `f64`; one past `f64`'s range has none, so it is its exact
/// text as a JSON string. Total: every value converts.
pub fn to_json(value: &GenericValue) -> Value {
    match value {
        GenericValue::Null => Value::Null,
        GenericValue::Bool(b) => Value::Bool(*b),
        GenericValue::SmallInt(i) => Value::Number((*i).into()),
        GenericValue::Int(i) => Value::Number((*i).into()),
        GenericValue::BigInt(i) => Value::Number((*i).into()),
        GenericValue::Float(f) => float_to_json((*f).into()),
        GenericValue::Double(f) => float_to_json(*f),
        GenericValue::Decimal(d) => float_to_json(kernel::decimal_to_f64(*d)),
        GenericValue::BigDecimal(d) => match Number::from_f64(d.to_f64()) {
            Some(number) => Value::Number(number),
            None => Value::String(d.as_str().to_owned()),
        },
        GenericValue::String(s) => Value::String(s.clone()),
        GenericValue::Uuid(u) => Value::String(u.to_string()),
        GenericValue::Date(d) => Value::String(d.to_string()),
        GenericValue::Time(t) => Value::String(t.to_string()),
        // chrono's `Display` puts a space between date and time, which a
        // `date` field's default format refuses.
        GenericValue::Timestamp(ts) => Value::String(ts.format("%Y-%m-%dT%H:%M:%S%.f").to_string()),
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

/// A non-finite float (NaN/∞ — not valid JSON) is written as null rather than
/// failing the document.
fn float<S: Serializer>(value: f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_finite() {
        serializer.serialize_f64(value)
    } else {
        serializer.serialize_unit()
    }
}

/// `text`, a JSON number literal, written as is.
fn number<S: Serializer>(text: &str, serializer: S) -> Result<S::Ok, S::Error> {
    serde_json::from_str::<&RawValue>(text)
        .map_err(S::Error::custom)?
        .serialize(serializer)
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

#[cfg(test)]
mod tests;
