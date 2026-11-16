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
//! | `BigDecimal` (a declared `decimal`) | `Decimal`: the nearest to its text (about 28 significant digits, at most 28 after the point), or as read through `f64` when written with an exponent or zero; a string of its digits past `Decimal`'s range |
//! | `String`, `Uuid` | string |
//! | `Date` | [`date`](crate::dates::Date) |
//! | `Time` | [`time`](crate::dates::Time) |
//! | `Timestamp` | [`timestamp`](crate::dates::Timestamp) without a zone |
//! | `TimestampTz` | [`timestamp`](crate::dates::Timestamp) in UTC |
//! | `Bytes` | blob |
//! | `Array` / `Map` | array / object map |
//!
//! **Out** ([`from_dynamic`]): a script's result is checked against the field's declared type,
//! strictly. The only widening allowed: an integer is accepted for a `float`/`double`/`decimal`
//! field. A `date` field takes a `date`; a `timestamp` field a `timestamp` (a zoned one as its
//! UTC instant), a `time` or a `date` (each written as a column of that type under a
//! `timestamp` field is). A `decimal` result is a [`BigDecimal`](GenericValue::BigDecimal) of its exact text,
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

use chrono::{TimeZone, Utc};
use kernel::{FlussoType, GenericValue};
use rhai::{Array, Dynamic, Map};
use rust_decimal::Decimal;

use crate::dates::{Date, Duration, Time, Timestamp};

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
        GenericValue::Date(d) => Dynamic::from(Date(*d)),
        GenericValue::Time(t) => Dynamic::from(Time(*t)),
        GenericValue::Timestamp(t) => Dynamic::from(Timestamp::plain(*t)),
        GenericValue::TimestampTz(t) => Dynamic::from(Timestamp::utc(*t)),
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
            .map_err(|_| text_hint(found, mismatch())),
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
        FlussoType::Date => value
            .try_cast_result::<Date>()
            .map(|d| GenericValue::Date(d.0))
            .map_err(|value| {
                if value.is::<Timestamp>() {
                    format!("{}: take its day with .date()", mismatch())
                } else {
                    with_hint(&value, mismatch(), "parse_date")
                }
            }),
        FlussoType::Timestamp => match value.try_cast_result::<Timestamp>() {
            Ok(t) => Ok(timestamp(t)),
            Err(value) => match value.try_cast_result::<Time>() {
                Ok(t) => Ok(GenericValue::Time(t.0)),
                Err(value) => match value.try_cast_result::<Date>() {
                    Ok(d) => Ok(GenericValue::Date(d.0)),
                    Err(value) => Err(with_hint(&value, mismatch(), "parse_timestamp")),
                },
            },
        },
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

/// A declared decimal in Rhai. Written without an exponent, it is the nearest
/// `Decimal` (about 28 significant digits, at most 28 after the point; a next
/// digit of 5 or more rounds away from zero). Written with an exponent, or
/// zero, it reads as `main` read it before issue #327,
/// through `f64`, so a tiny value is the same zero. Past ±7.9e28 it is a string
/// of its exact digits.
// Cold and out of line: without it `to_dynamic` slows on documents with no
// declared decimal at all (measured: 2.6 µs vs 4.0 µs for 23 fields).
#[cold]
#[inline(never)]
fn declared_decimal(value: &kernel::BigDecimal) -> Dynamic {
    let text = value.as_str();
    let plain = !text.bytes().any(|b| b | 0x20 == b'e');
    if plain
        && let Ok(d) = Decimal::from_str(text)
        && !d.is_zero()
    {
        return Dynamic::from_decimal(d);
    }
    as_read_through_f64(text).map_or_else(|| Dynamic::from(text.to_owned()), Dynamic::from_decimal)
}

/// `text`, a JSON number, as the `Decimal` a `serde_json` number read gave:
/// its text when `Decimal` takes it, else its `f64`; `None` past either.
fn as_read_through_f64(text: &str) -> Option<Decimal> {
    let number: serde_json::Number = serde_json::from_str(text).ok()?;
    Decimal::from_str(&number.to_string())
        .ok()
        .or_else(|| number.as_f64().and_then(|f| Decimal::try_from(f).ok()))
}

/// A `decimal` script field's result, written with every digit it holds; a
/// zero without a sign.
fn exact(value: &Decimal) -> GenericValue {
    let value = &if value.is_zero() { value.abs() } else { *value };
    let mut text = DecimalText::default();
    let written = std::fmt::Write::write_fmt(&mut text, format_args!("{value}")).is_ok();
    written
        .then(|| kernel::BigDecimal::parse(text.as_str()))
        .flatten()
        .map_or(GenericValue::Decimal(*value), GenericValue::BigDecimal)
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

/// A timestamp result: one without a zone as it reads, a zoned one as its UTC instant.
fn timestamp(value: Timestamp) -> GenericValue {
    match value.instant() {
        Some(utc) => GenericValue::TimestampTz(Utc.from_utc_datetime(&utc)),
        None => GenericValue::Timestamp(value.wall()),
    }
}

/// A string returned where a date was expected: say which parse function reads it.
fn with_hint(value: &Dynamic, mismatch: String, parse: &str) -> String {
    if value.is_string() {
        format!("{mismatch}: parse it with {parse}(…)")
    } else {
        mismatch
    }
}

/// A date value returned to a text field: say how to write it as text.
fn text_hint(found: &str, mismatch: String) -> String {
    if is_date_type(found) {
        format!("{mismatch}: write it as text with .to_string() or .format(…)")
    } else {
        mismatch
    }
}

fn is_date_type(type_name: &str) -> bool {
    [
        std::any::type_name::<Date>(),
        std::any::type_name::<Time>(),
        std::any::type_name::<Timestamp>(),
        std::any::type_name::<Duration>(),
    ]
    .contains(&type_name)
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
        FlussoType::Date => "a date",
        FlussoType::Timestamp => "a timestamp",
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
        other if other == std::any::type_name::<Date>() => "date",
        other if other == std::any::type_name::<Time>() => "time",
        other if other == std::any::type_name::<Timestamp>() => "timestamp",
        other if other == std::any::type_name::<Duration>() => "duration",
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
