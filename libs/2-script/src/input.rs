//! Typing text by its declared type, the way the Postgres builder does, for the two places a
//! script would otherwise see text where the pipeline has a typed value:
//!
//! - **`flusso script run --input`**: [`document`] reads a JSON document by the index schema,
//!   so a `date`, `timestamp` or `decimal` (and a `values: decimal` map) arrives as the
//!   builder would hand it over.
//! - **A `map` of dates**: its values are JSON text in the document; [`temporal`] types one
//!   for the script's view, leaving the document as it is (see [`crate::scripts`]).
//!
//! Text that doesn't parse as its type stays text, as the builder leaves it.
//!
//! ```
//! use kernel::{FlussoType, GenericValue};
//! use script::input::temporal;
//!
//! let day = temporal("2024-05-01", &FlussoType::Date);
//! assert!(matches!(day, Some(GenericValue::Date(_))));
//! assert_eq!(temporal("soon", &FlussoType::Date), None);
//! ```

use std::str::FromStr;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime};
use kernel::{AggregateOp, Field, FieldSource, FlussoType, GenericValue, Relation};

/// A JSON document, typed by the fields of its index (or of a level of it).
pub fn document(fields: &[Field], json: serde_json::Value) -> GenericValue {
    let serde_json::Value::Object(object) = json else {
        return untyped(json);
    };
    GenericValue::Map(
        object
            .into_iter()
            .map(|(key, value)| {
                let typed = match fields.iter().find(|f| f.field.as_ref() == key) {
                    Some(field) => by_field(field, value),
                    None => untyped(value),
                };
                (key, typed)
            })
            .collect(),
    )
}

fn by_field(field: &Field, json: serde_json::Value) -> GenericValue {
    match &field.source {
        FieldSource::Column(column) => scalar(&column.ty, json),
        FieldSource::Script(script) => scalar(&script.ty, json),
        FieldSource::Group(children) => document(children, json),
        FieldSource::Relation(Relation::Join(join)) => match json {
            serde_json::Value::Array(items) => GenericValue::Array(
                items
                    .into_iter()
                    .map(|item| document(&join.fields, item))
                    .collect(),
            ),
            other => document(&join.fields, other),
        },
        FieldSource::Relation(Relation::Aggregate(aggregate)) => match &aggregate.op {
            AggregateOp::Ids { element_type } => scalar(element_type, json),
            AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => {
                match &aggregate.value_type {
                    Some(ty) => scalar(ty, json),
                    None => untyped(json),
                }
            }
            AggregateOp::Count | AggregateOp::Avg(_) | AggregateOp::Exists => untyped(json),
        },
        FieldSource::Geo(_) | FieldSource::Constant(_) => untyped(json),
    }
}

/// One value of type `ty`; an array holds values of `ty` (an `array: true` column).
fn scalar(ty: &FlussoType, json: serde_json::Value) -> GenericValue {
    match (ty, json) {
        (_, serde_json::Value::Array(items)) => {
            GenericValue::Array(items.into_iter().map(|item| scalar(ty, item)).collect())
        }
        (FlussoType::Date | FlussoType::Timestamp, serde_json::Value::String(text)) => {
            temporal(&text, ty).unwrap_or(GenericValue::String(text))
        }
        (FlussoType::Double, serde_json::Value::Number(number)) => match number.as_f64() {
            Some(double) => GenericValue::Double(double),
            None => untyped(number.into()),
        },
        (FlussoType::Float, serde_json::Value::Number(number)) => {
            match number.to_string().parse::<f32>() {
                Ok(single) => GenericValue::Float(single),
                Err(_) => untyped(number.into()),
            }
        }
        (FlussoType::Decimal, serde_json::Value::Number(number)) => {
            decimal(&number.to_string()).unwrap_or_else(|| untyped(number.into()))
        }
        (FlussoType::Decimal, serde_json::Value::String(text)) => {
            decimal(&text).unwrap_or(GenericValue::String(text))
        }
        (FlussoType::Map { values }, serde_json::Value::Object(object))
            if **values == FlussoType::Decimal =>
        {
            GenericValue::Map(
                object
                    .into_iter()
                    .map(|(key, value)| (key, scalar(values, value)))
                    .collect(),
            )
        }
        (_, json) => untyped(json),
    }
}

fn decimal(text: &str) -> Option<GenericValue> {
    kernel::BigDecimal::parse(text).map(GenericValue::BigDecimal)
}

/// `text` as a value of the temporal type `ty`, read as the Postgres builder reads it: a
/// `date` as `YYYY-MM-DD`; a `timestamp` as an RFC 3339 instant (in UTC), a date and time
/// without an offset, a time of day, or a date.
pub fn temporal(text: &str, ty: &FlussoType) -> Option<GenericValue> {
    match ty {
        FlussoType::Date => NaiveDate::from_str(text).ok().map(GenericValue::Date),
        FlussoType::Timestamp => {
            if let Some(instant) = DateTime::parse_from_rfc3339(text)
                .ok()
                .or_else(|| crate::dates::with_short_offset(text))
            {
                return Some(GenericValue::TimestampTz(instant.to_utc()));
            }
            if let Ok(wall) = NaiveDateTime::from_str(text) {
                return Some(GenericValue::Timestamp(wall));
            }
            if let Ok(time) = NaiveTime::from_str(text) {
                return Some(GenericValue::Time(time));
            }
            NaiveDate::from_str(text).ok().map(GenericValue::Date)
        }
        _ => None,
    }
}

/// JSON with no declared type, as the Postgres builder reads it: a number by
/// `kernel::untyped_json_number`, everything else as it reads.
pub fn untyped(json: serde_json::Value) -> GenericValue {
    match json {
        serde_json::Value::Null => GenericValue::Null,
        serde_json::Value::Bool(b) => GenericValue::Bool(b),
        serde_json::Value::Number(n) => kernel::untyped_json_number(&n),
        serde_json::Value::String(s) => GenericValue::String(s),
        serde_json::Value::Array(items) => {
            GenericValue::Array(items.into_iter().map(untyped).collect())
        }
        serde_json::Value::Object(map) => {
            GenericValue::Map(map.into_iter().map(|(k, v)| (k, untyped(v))).collect())
        }
    }
}

#[cfg(test)]
mod tests;
