//! Moving values across the Rhai boundary.
//!
//! **In** ([`to_dynamic`]): the document is converted once per level, losslessly where Rhai has a
//! type and as a string where it doesn't.
//!
//! | `GenericValue` | Rhai |
//! | --- | --- |
//! | `Null` | `()` |
//! | `Bool` | `bool` |
//! | `SmallInt` / `Int` / `BigInt` | `i64` |
//! | `Float` / `Double` | `f64` |
//! | `Decimal` | `Decimal` |
//! | `BigDecimal` (a declared `decimal`) | `Decimal`: exact when it holds the text, else as read through `f64`; a string of its digits past `Decimal`'s range |
//! | `String`, `Uuid` | string |
//! | `Date` / `Time` / `Timestamp` / `TimestampTz` | RFC 3339 string |
//! | `Bytes` | blob |
//! | `Array` / `Map` | array / object map |
//!
//! **Out** ([`from_dynamic`]): a script's result is checked against the field's declared type,
//! strictly. The only widening allowed: an integer is accepted for a `float`/`double`/`decimal`
//! field. A `decimal` result is a [`BigDecimal`](GenericValue::BigDecimal) of its exact text,
//! written with every digit.
//!
//! ```
//! use kernel::{FlussoType, GenericValue};
//! use script::value::{from_dynamic, to_dynamic, Output};
//!
//! let rhai = to_dynamic(&GenericValue::Int(5));
//! assert_eq!(rhai.as_int(), Ok(5));
//!
//! let out = Output { ty: &FlussoType::Long, nullable: false, array: false, variants: &[] };
//! assert_eq!(from_dynamic(rhai, &out), Ok(GenericValue::BigInt(5)));
//! ```

use std::str::FromStr;

use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat};
use kernel::{FlussoType, GenericValue};
use rhai::{Array, Dynamic, Map};
use rust_decimal::Decimal;

/// Convert a document value into a Rhai value.
pub fn to_dynamic(value: &GenericValue) -> Dynamic {
    match value {
        GenericValue::Null => Dynamic::UNIT,
        GenericValue::Bool(b) => Dynamic::from_bool(*b),
        GenericValue::SmallInt(i) => Dynamic::from_int(i64::from(*i)),
        GenericValue::Int(i) => Dynamic::from_int(i64::from(*i)),
        GenericValue::BigInt(i) => Dynamic::from_int(*i),
        GenericValue::Float(f) => Dynamic::from_float(f64::from(*f)),
        GenericValue::Double(f) => Dynamic::from_float(*f),
        GenericValue::Decimal(d) => Dynamic::from_decimal(*d),
        GenericValue::BigDecimal(d) => declared_decimal(d),
        GenericValue::String(s) => Dynamic::from(s.clone()),
        GenericValue::Uuid(u) => Dynamic::from(u.to_string()),
        GenericValue::Date(d) => Dynamic::from(d.format("%Y-%m-%d").to_string()),
        GenericValue::Time(t) => Dynamic::from(t.format("%H:%M:%S%.f").to_string()),
        GenericValue::Timestamp(t) => Dynamic::from(t.format("%Y-%m-%dT%H:%M:%S%.f").to_string()),
        GenericValue::TimestampTz(t) => {
            Dynamic::from(t.to_rfc3339_opts(SecondsFormat::AutoSi, true))
        }
        GenericValue::Bytes(b) => Dynamic::from_blob(b.clone()),
        GenericValue::Array(items) => Dynamic::from_array(items.iter().map(to_dynamic).collect()),
        GenericValue::Map(map) => Dynamic::from_map(
            map.iter()
                .map(|(key, value)| (key.as_str().into(), to_dynamic(value)))
                .collect::<Map>(),
        ),
    }
}

/// What a script field must return.
#[derive(Debug, Clone, Copy)]
pub struct Output<'a> {
    pub ty: &'a FlussoType,
    pub nullable: bool,
    pub array: bool,
    /// An `enum` field's declared variants; empty accepts any string.
    pub variants: &'a [String],
}

/// Check a script's result against its declared type and convert it. The error
/// is one line, naming what was returned and what was expected.
pub fn from_dynamic(value: Dynamic, out: &Output<'_>) -> Result<GenericValue, String> {
    if value.is_unit() {
        return if out.nullable {
            Ok(GenericValue::Null)
        } else {
            Err("returned `()` but the field is `required: true`".to_owned())
        };
    }
    if !out.array {
        return leaf(value, out);
    }
    let found = value.type_name();
    let items: Array = value
        .into_array()
        .map_err(|_| format!("returned {}, expected an array", article(found)))?;
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            if item.is_unit() {
                return Err(format!("element {index} is `()`; an array holds no nulls"));
            }
            leaf(item, out).map_err(|e| format!("element {index}: {e}"))
        })
        .collect::<Result<_, _>>()
        .map(GenericValue::Array)
}

fn leaf(value: Dynamic, out: &Output<'_>) -> Result<GenericValue, String> {
    let found = value.type_name();
    let mismatch = || format!("returned {}, expected {}", article(found), expected(out.ty));
    match out.ty {
        FlussoType::Text | FlussoType::Identifier | FlussoType::Keyword => value
            .into_string()
            .map(GenericValue::String)
            .map_err(|_| mismatch()),
        FlussoType::Enum => {
            let variant = value.into_string().map_err(|_| mismatch())?;
            if out.variants.is_empty() || out.variants.contains(&variant) {
                Ok(GenericValue::String(variant))
            } else {
                Err(format!(
                    "returned `{variant}`, which is not a declared variant ({})",
                    out.variants.join(", ")
                ))
            }
        }
        FlussoType::Uuid => {
            let text = value.into_string().map_err(|_| mismatch())?;
            uuid::Uuid::parse_str(&text)
                .map(GenericValue::Uuid)
                .map_err(|e| format!("returned `{text}`, which is not a uuid: {e}"))
        }
        FlussoType::Boolean => value
            .as_bool()
            .map(GenericValue::Bool)
            .map_err(|_| mismatch()),
        FlussoType::Short => integer(&value, mismatch).and_then(|i| {
            i16::try_from(i)
                .map(GenericValue::SmallInt)
                .map_err(|_| format!("returned {i}, out of range for a short"))
        }),
        FlussoType::Integer => integer(&value, mismatch).and_then(|i| {
            i32::try_from(i)
                .map(GenericValue::Int)
                .map_err(|_| format!("returned {i}, out of range for an integer"))
        }),
        FlussoType::Long => integer(&value, mismatch).map(GenericValue::BigInt),
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        FlussoType::Float => number(&value, mismatch).map(|f| GenericValue::Float(f as f32)),
        FlussoType::Double => number(&value, mismatch).map(GenericValue::Double),
        FlussoType::Decimal => {
            if let Ok(d) = value.as_decimal() {
                Ok(exact(&d))
            } else {
                integer(&value, mismatch).map(|i| exact(&Decimal::from(i)))
            }
        }
        FlussoType::Date => {
            let text = value.into_string().map_err(|_| mismatch())?;
            NaiveDate::parse_from_str(&text, "%Y-%m-%d")
                .map(GenericValue::Date)
                .map_err(|_| format!("returned `{text}`, expected a `YYYY-MM-DD` date"))
        }
        FlussoType::Timestamp => {
            let text = value.into_string().map_err(|_| mismatch())?;
            timestamp(&text).ok_or_else(|| {
                format!(
                    "returned `{text}`, expected an RFC 3339 timestamp \
                     (`2024-05-01T12:00:00Z`, or without the offset)"
                )
            })
        }
        FlussoType::Binary
        | FlussoType::Json
        | FlussoType::Map { .. }
        | FlussoType::GeoPoint
        | FlussoType::Custom { .. } => Err(format!(
            "a script cannot return {}; the schema should have rejected it",
            expected(out.ty)
        )),
    }
}

/// A declared decimal in Rhai. A value `Decimal` holds exactly is that
/// decimal, every digit kept. Any other (written with an exponent, more than
/// 28 fractional digits, zero, past the range) reads as `main` read it before
/// issue #327, through `f64`, so a tiny value is the same zero and a value past
/// ±7.9e28 is a string, here of its exact digits.
// Out of line: the common arms of `to_dynamic` stay small and fast.
#[inline(never)]
fn declared_decimal(value: &kernel::BigDecimal) -> Dynamic {
    let text = value.as_str();
    match Decimal::from_str_exact(text) {
        Ok(d) if !d.is_zero() => Dynamic::from_decimal(d),
        _ => as_read_through_f64(text)
            .map_or_else(|| Dynamic::from(text.to_owned()), Dynamic::from_decimal),
    }
}

/// `text`, a JSON number, as the `Decimal` a `serde_json` number read gave:
/// its text when `Decimal` takes it, else its `f64`; `None` past either.
fn as_read_through_f64(text: &str) -> Option<Decimal> {
    let number: serde_json::Number = serde_json::from_str(text).ok()?;
    Decimal::from_str(&number.to_string())
        .ok()
        .or_else(|| number.as_f64().and_then(|f| Decimal::try_from(f).ok()))
}

/// A `decimal` script field's result, written with every digit it holds.
fn exact(value: &Decimal) -> GenericValue {
    let mut text = DecimalText::default();
    let written = std::fmt::Write::write_fmt(&mut text, format_args!("{value}")).is_ok();
    written
        .then(|| kernel::BigDecimal::parse(text.as_str()))
        .flatten()
        .map_or(GenericValue::Decimal(*value), GenericValue::BigDecimal)
}

/// A decimal's `to_float()` in a script: the `f64` nearest its exact value,
/// parsed from its text (`Decimal::to_f64` can land one unit in the last place
/// off for a long value); `0.0` for a zero of either sign.
pub(crate) fn decimal_to_float(value: Decimal) -> f64 {
    if value.is_zero() {
        return 0.0;
    }
    let mut text = DecimalText::default();
    match std::fmt::Write::write_fmt(&mut text, format_args!("{value}")) {
        Ok(()) => text.as_str().parse().unwrap_or(f64::NAN),
        Err(_) => value.to_string().parse().unwrap_or(f64::NAN),
    }
}

/// A `Decimal`'s text on the stack: a sign, 29 digits, a point and up to 28
/// leading fractional zeros fit.
struct DecimalText {
    bytes: [u8; 64],
    len: usize,
}

impl Default for DecimalText {
    fn default() -> Self {
        Self {
            bytes: [0; 64],
            len: 0,
        }
    }
}

impl DecimalText {
    fn as_str(&self) -> &str {
        self.bytes
            .get(..self.len)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or_default()
    }
}

impl std::fmt::Write for DecimalText {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.len + s.len();
        let slot = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        slot.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn integer(value: &Dynamic, mismatch: impl Fn() -> String) -> Result<i64, String> {
    value.as_int().map_err(|_| mismatch())
}

#[allow(clippy::cast_precision_loss)]
fn number(value: &Dynamic, mismatch: impl Fn() -> String) -> Result<f64, String> {
    value
        .as_float()
        .or_else(|_| value.as_int().map(|i| i as f64))
        .map_err(|_| mismatch())
}

fn timestamp(text: &str) -> Option<GenericValue> {
    if let Ok(t) = DateTime::parse_from_rfc3339(text) {
        return Some(GenericValue::TimestampTz(t.to_utc()));
    }
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(GenericValue::Timestamp)
}

/// The Rhai type a declared type expects, for messages.
fn expected(ty: &FlussoType) -> &'static str {
    match ty {
        FlussoType::Text | FlussoType::Identifier | FlussoType::Keyword | FlussoType::Enum => {
            "a string"
        }
        FlussoType::Uuid => "a uuid string",
        FlussoType::Boolean => "a bool",
        FlussoType::Short | FlussoType::Integer | FlussoType::Long => "an integer",
        FlussoType::Float | FlussoType::Double => "a number",
        FlussoType::Decimal => "a decimal (or an integer)",
        FlussoType::Date => "a date string",
        FlussoType::Timestamp => "a timestamp string",
        FlussoType::Binary => "a blob",
        FlussoType::Json | FlussoType::Map { .. } => "an object",
        FlussoType::GeoPoint => "a geo point",
        FlussoType::Custom { .. } => "a custom value",
    }
}

fn article(type_name: &str) -> String {
    let name = match type_name {
        "()" => return "`()`".to_owned(),
        "i64" => "integer",
        "f64" => "float",
        "map" => "object",
        "string" | "&str" | "ImmutableString" => "string",
        other => other,
    };
    let a = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{a} {name}")
}

#[cfg(test)]
mod tests;
