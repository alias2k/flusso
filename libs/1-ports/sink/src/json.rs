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
//! [`Decimal`](GenericValue::Decimal) or [`BigDecimal`](GenericValue::BigDecimal)
//! is written as a JSON number holding its exact text, every digit kept (JSON
//! numbers have no precision limit). [`to_json`] builds a [`Value`] instead,
//! which holds a number as an `f64`: use it where a value tree is needed, never
//! to write a document.
//!
//! ```
//! use kernel::{BigDecimal, GenericValue};
//! use sink::Json;
//!
//! let big = GenericValue::BigDecimal(BigDecimal::parse("1000000000000000000000000000000").unwrap());
//! assert_eq!(serde_json::to_string(&Json(&big)).unwrap(), "1000000000000000000000000000000");
//!
//! let price = GenericValue::Decimal("12.50".parse().unwrap());
//! assert_eq!(serde_json::to_string(&Json(&price)).unwrap(), "12.50");
//! ```

use std::fmt::Write as _;

use kernel::GenericValue;
use rust_decimal::Decimal;
use serde::ser::{Error as _, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;

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
            GenericValue::Decimal(d) => decimal(d, serializer),
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

/// Convert a document value into a [`Value`] tree. A number is held as an
/// `f64` there, so a decimal past `f64`'s precision is rounded: write a
/// document with [`Json`].
pub fn to_json(value: &GenericValue) -> Value {
    serde_json::to_value(Json(value)).unwrap_or(Value::Null)
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

/// The longest text a [`Decimal`] displays as: a sign, 29 digits, a point and
/// up to 28 leading fractional zeros fit well within it.
const DECIMAL_TEXT: usize = 64;

/// A [`Decimal`]'s exact text, formatted on the stack.
fn decimal<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    let mut text = StackText::default();
    if write!(text, "{value}").is_ok() {
        number(text.as_str(), serializer)
    } else {
        number(&value.to_string(), serializer)
    }
}

/// `text`, a JSON number literal, written as is.
fn number<S: Serializer>(text: &str, serializer: S) -> Result<S::Ok, S::Error> {
    serde_json::from_str::<&RawValue>(text)
        .map_err(S::Error::custom)?
        .serialize(serializer)
}

struct StackText {
    bytes: [u8; DECIMAL_TEXT],
    len: usize,
}

impl Default for StackText {
    fn default() -> Self {
        Self {
            bytes: [0; DECIMAL_TEXT],
            len: 0,
        }
    }
}

impl StackText {
    fn as_str(&self) -> &str {
        self.bytes
            .get(..self.len)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or_default()
    }
}

impl std::fmt::Write for StackText {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.len + s.len();
        let slot = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        slot.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
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
