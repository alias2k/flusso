//! Decoding Postgres results into the schema's [`GenericValue`]: single named
//! columns (for reverse-resolution lookups) and JSON documents (assembled
//! server-side).

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use schema_core::{Aggregate, AggregateOp, Field, FieldSource, FlussoType, GenericValue, Relation};
use sqlx::postgres::{PgColumn, PgRow};
use sqlx::{Column, Row, TypeInfo};
use uuid::Uuid;

/// Decode one named column of a row into a [`GenericValue`], or
/// [`GenericValue::Null`] if the row has no such column.
pub(super) fn decode_named_column(row: &PgRow, name: &str) -> GenericValue {
    match row.columns().iter().find(|col| col.name() == name) {
        Some(col) => decode_column(row, col),
        None => GenericValue::Null,
    }
}

/// Decode a single column by its Postgres type. Unsupported types and decode
/// failures degrade to [`GenericValue::Null`] with a warning rather than
/// failing the whole document.
fn decode_column(row: &PgRow, col: &PgColumn) -> GenericValue {
    let idx = col.ordinal();
    let name = col.name();
    match col.type_info().name() {
        "INT2" => finish(
            row.try_get::<Option<i16>, _>(idx),
            GenericValue::SmallInt,
            name,
        ),
        "INT4" => finish(row.try_get::<Option<i32>, _>(idx), GenericValue::Int, name),
        "INT8" => finish(
            row.try_get::<Option<i64>, _>(idx),
            GenericValue::BigInt,
            name,
        ),
        "BOOL" => finish(
            row.try_get::<Option<bool>, _>(idx),
            GenericValue::Bool,
            name,
        ),
        "FLOAT4" => finish(
            row.try_get::<Option<f32>, _>(idx),
            GenericValue::Float,
            name,
        ),
        "FLOAT8" => finish(
            row.try_get::<Option<f64>, _>(idx),
            GenericValue::Double,
            name,
        ),
        "NUMERIC" => finish(
            row.try_get::<Option<Decimal>, _>(idx),
            GenericValue::Decimal,
            name,
        ),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" | "CITEXT" => finish(
            row.try_get::<Option<String>, _>(idx),
            GenericValue::String,
            name,
        ),
        "TIMESTAMP" => finish(
            row.try_get::<Option<chrono::NaiveDateTime>, _>(idx),
            GenericValue::Timestamp,
            name,
        ),
        "TIMESTAMPTZ" => finish(
            row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(idx),
            GenericValue::TimestampTz,
            name,
        ),
        "DATE" => finish(
            row.try_get::<Option<chrono::NaiveDate>, _>(idx),
            GenericValue::Date,
            name,
        ),
        "TIME" => finish(
            row.try_get::<Option<chrono::NaiveTime>, _>(idx),
            GenericValue::Time,
            name,
        ),
        "UUID" => finish(
            row.try_get::<Option<uuid::Uuid>, _>(idx),
            GenericValue::Uuid,
            name,
        ),
        "BYTEA" => finish(
            row.try_get::<Option<Vec<u8>>, _>(idx),
            GenericValue::Bytes,
            name,
        ),
        "JSON" | "JSONB" => finish(
            row.try_get::<Option<serde_json::Value>, _>(idx),
            json_to_generic,
            name,
        ),
        other => {
            tracing::warn!(column = %name, r#type = %other, "unsupported column type; treating as null");
            GenericValue::Null
        }
    }
}

/// Decode the first column of a single-column row into a [`GenericValue`],
/// using the same per-type decoding as the rest of the read path. The initial
/// backfill selects one primary-key column per row and turns it into a
/// [`RowKey`](sources_core::RowKey) value this way, so a snapshotted key matches
/// what a live change would produce.
pub(crate) fn first_column_to_generic(row: &PgRow) -> GenericValue {
    match row.columns().first() {
        Some(col) => decode_column(row, col),
        None => GenericValue::Null,
    }
}

/// Convert a JSON value (e.g. a server-assembled document) into the value tree.
pub(super) fn json_to_generic(value: serde_json::Value) -> GenericValue {
    match value {
        serde_json::Value::Null => GenericValue::Null,
        serde_json::Value::Bool(b) => GenericValue::Bool(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => GenericValue::BigInt(i),
            None => n
                .as_f64()
                .map_or_else(|| GenericValue::String(n.to_string()), float),
        },
        serde_json::Value::String(s) => GenericValue::String(s),
        serde_json::Value::Array(items) => {
            GenericValue::Array(items.into_iter().map(json_to_generic).collect())
        }
        serde_json::Value::Object(fields) => GenericValue::Map(
            fields
                .into_iter()
                .map(|(k, v)| (k, json_to_generic(v)))
                .collect(),
        ),
    }
}

/// Coerce a server-assembled JSON document into the **typed** value tree, using
/// each field's declared type so the values a sink sees are canonical — a `date`
/// field becomes a [`Date`](GenericValue::Date), a `uuid` a [`Uuid`](GenericValue::Uuid),
/// not opaque strings. The document is built server-side as one JSON blob (so the
/// types are erased on the wire); this is where we put them back, guided by the
/// schema. Anything that fails to parse falls back to the untyped JSON value
/// rather than failing the document.
pub(crate) fn coerce_document(value: serde_json::Value, fields: &[Field]) -> GenericValue {
    let serde_json::Value::Object(mut object) = value else {
        // A document is always a JSON object; anything else can only be decoded
        // untyped.
        return json_to_generic(value);
    };
    let mut out = BTreeMap::new();
    for field in fields {
        let name = field.field.as_ref();
        if let Some(json) = object.remove(name) {
            out.insert(name.to_owned(), coerce_field(json, &field.source));
        }
    }
    GenericValue::Map(out)
}

/// Coerce one field's JSON value by where it comes from.
fn coerce_field(json: serde_json::Value, source: &FieldSource) -> GenericValue {
    match source {
        FieldSource::Column(column) => coerce_scalar(json, &column.ty),
        FieldSource::Group(fields) => coerce_document(json, fields),
        FieldSource::Relation(Relation::Join(join)) => coerce_relation(json, &join.fields),
        FieldSource::Relation(Relation::Aggregate(aggregate)) => coerce_aggregate(json, aggregate),
        // A geo point ({lat,lon} / [lon,lat] / "lat,lon") and a bare constant
        // carry no scalar type to coerce to; keep their natural JSON shape.
        FieldSource::Geo(_) | FieldSource::Constant(_) => json_to_generic(json),
    }
}

/// A joined relation: an array of sub-documents (`has_many` / `many_to_many`), a
/// single sub-document (`belongs_to` / `has_one`), or null.
fn coerce_relation(json: serde_json::Value, fields: &[Field]) -> GenericValue {
    match json {
        serde_json::Value::Array(items) => GenericValue::Array(
            items
                .into_iter()
                .map(|item| coerce_document(item, fields))
                .collect(),
        ),
        object @ serde_json::Value::Object(_) => coerce_document(object, fields),
        other => json_to_generic(other),
    }
}

/// An aggregate's result, typed by its op: a count is a [`BigInt`](GenericValue::BigInt),
/// an average a [`Double`](GenericValue::Double), `sum`/`min`/`max` follow the
/// declared `value_type`, and `ids` is an array of the element type.
fn coerce_aggregate(json: serde_json::Value, aggregate: &Aggregate) -> GenericValue {
    match &aggregate.op {
        AggregateOp::Count => coerce_scalar(json, &FlussoType::Long),
        AggregateOp::Avg(_) => coerce_scalar(json, &FlussoType::Double),
        AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => {
            match &aggregate.value_type {
                Some(ty) => coerce_scalar(json, ty),
                None => json_to_generic(json),
            }
        }
        AggregateOp::Ids { element_type } => match json {
            serde_json::Value::Array(items) => GenericValue::Array(
                items
                    .into_iter()
                    .map(|item| coerce_scalar(item, element_type))
                    .collect(),
            ),
            other => json_to_generic(other),
        },
    }
}

/// Coerce a JSON scalar to the canonical variant its declared [`FlussoType`]
/// implies. A null stays null; a value that doesn't fit the declared type falls
/// back to its untyped JSON shape (never an error).
fn coerce_scalar(json: serde_json::Value, ty: &FlussoType) -> GenericValue {
    if json.is_null() {
        return GenericValue::Null;
    }
    match ty {
        FlussoType::Boolean => match json.as_bool() {
            Some(b) => GenericValue::Bool(b),
            None => json_to_generic(json),
        },
        FlussoType::Short => match json.as_i64().and_then(|i| i16::try_from(i).ok()) {
            Some(i) => GenericValue::SmallInt(i),
            None => json_to_generic(json),
        },
        FlussoType::Integer => match json.as_i64().and_then(|i| i32::try_from(i).ok()) {
            Some(i) => GenericValue::Int(i),
            None => json_to_generic(json),
        },
        FlussoType::Long => match json.as_i64() {
            Some(i) => GenericValue::BigInt(i),
            None => json_to_generic(json),
        },
        FlussoType::Float => match json.as_f64() {
            Some(f) => GenericValue::Float(f as f32),
            None => json_to_generic(json),
        },
        FlussoType::Double => match json.as_f64() {
            Some(f) => GenericValue::Double(f),
            None => json_to_generic(json),
        },
        FlussoType::Decimal => coerce_decimal(json),
        FlussoType::Uuid => match json.as_str().and_then(|s| Uuid::parse_str(s).ok()) {
            Some(u) => GenericValue::Uuid(u),
            None => json_to_generic(json),
        },
        FlussoType::Date => match json.as_str().and_then(|s| s.parse::<NaiveDate>().ok()) {
            Some(d) => GenericValue::Date(d),
            None => json_to_generic(json),
        },
        // `timestamp` / `timestamptz` / `time` all declare `Timestamp`; recover
        // the precise variant from the value's shape.
        FlussoType::Timestamp => json
            .as_str()
            .and_then(parse_temporal)
            .unwrap_or_else(|| json_to_generic(json)),
        // Text-family and the structured / escape-hatch types keep their natural
        // JSON shape.
        FlussoType::Text
        | FlussoType::Identifier
        | FlussoType::Keyword
        | FlussoType::Enum
        | FlussoType::Binary
        | FlussoType::Json
        | FlussoType::GeoPoint
        | FlussoType::Custom { .. } => json_to_generic(json),
    }
}

/// Parse a `Timestamp`-declared string into the tightest temporal variant: an
/// offset instant → [`TimestampTz`](GenericValue::TimestampTz), else a naive
/// datetime, a time, or a date. `None` if it's none of those.
fn parse_temporal(s: &str) -> Option<GenericValue> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(s) {
        return Some(GenericValue::TimestampTz(instant.with_timezone(&Utc)));
    }
    if let Ok(naive) = s.parse::<NaiveDateTime>() {
        return Some(GenericValue::Timestamp(naive));
    }
    if let Ok(time) = s.parse::<NaiveTime>() {
        return Some(GenericValue::Time(time));
    }
    s.parse::<NaiveDate>().ok().map(GenericValue::Date)
}

/// A `numeric`-declared value, kept exact: parse from the JSON number's text (so
/// precision isn't lost through `f64`) or a string, else fall back untyped.
fn coerce_decimal(json: serde_json::Value) -> GenericValue {
    let parsed = match &json {
        serde_json::Value::Number(n) => Decimal::from_str(&n.to_string()).ok(),
        serde_json::Value::String(s) => Decimal::from_str(s).ok(),
        _ => None,
    };
    parsed.map_or_else(|| json_to_generic(json), GenericValue::Decimal)
}

/// Resolve a decoded column: SQL `NULL` → [`GenericValue::Null`], a value runs
/// through `f`, a decode error warns and falls back to null.
fn finish<T>(
    decoded: Result<Option<T>, sqlx::Error>,
    f: impl FnOnce(T) -> GenericValue,
    column: &str,
) -> GenericValue {
    match decoded {
        Ok(Some(value)) => f(value),
        Ok(None) => GenericValue::Null,
        Err(e) => {
            tracing::warn!(column = %column, error = %e, "failed to decode column; treating as null");
            GenericValue::Null
        }
    }
}

/// Floats have no dedicated [`GenericValue`]; keep precision as a decimal where
/// possible, otherwise fall back to the text form.
fn float(v: f64) -> GenericValue {
    match Decimal::try_from(v) {
        Ok(d) => GenericValue::Decimal(d),
        Err(_) => GenericValue::String(v.to_string()),
    }
}

#[cfg(test)]
mod tests;
