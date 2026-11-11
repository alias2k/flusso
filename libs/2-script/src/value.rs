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
//! | `String`, `Uuid` | string |
//! | `Date` / `Time` / `Timestamp` / `TimestampTz` | RFC 3339 string |
//! | `Bytes` | blob |
//! | `Array` / `Map` | array / object map |
//!
//! **Out** ([`from_dynamic`]): a script's result is checked against the field's declared type,
//! strictly. The only widening allowed is the lossless one: an integer is accepted for a
//! `float`/`double`/`decimal` field.
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

use chrono::{DateTime, NaiveDate, NaiveDateTime, SecondsFormat};
use kernel::{FlussoType, GenericValue};
use rhai::{Array, Dynamic, Map};

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
                Ok(GenericValue::Decimal(d))
            } else {
                integer(&value, mismatch).map(|i| GenericValue::Decimal(i.into()))
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
