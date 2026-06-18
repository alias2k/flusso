//! Decoding Postgres results into the schema's [`GenericValue`]: single named
//! columns (for reverse-resolution lookups) and JSON documents (assembled
//! server-side).

use rust_decimal::Decimal;
use schema_core::GenericValue;
use sqlx::postgres::{PgColumn, PgRow};
use sqlx::{Column, Row, TypeInfo};

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
            |v| GenericValue::Int(v.into()),
            name,
        ),
        "INT4" => finish(
            row.try_get::<Option<i32>, _>(idx),
            |v| GenericValue::Int(v.into()),
            name,
        ),
        "INT8" => finish(row.try_get::<Option<i64>, _>(idx), GenericValue::Int, name),
        "BOOL" => finish(
            row.try_get::<Option<bool>, _>(idx),
            GenericValue::Bool,
            name,
        ),
        "FLOAT4" => finish(
            row.try_get::<Option<f32>, _>(idx),
            |v| float(v.into()),
            name,
        ),
        "FLOAT8" => finish(row.try_get::<Option<f64>, _>(idx), float, name),
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
            |v| GenericValue::String(v.to_string()),
            name,
        ),
        "TIMESTAMPTZ" => finish(
            row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(idx),
            |v| GenericValue::String(v.to_rfc3339()),
            name,
        ),
        "DATE" => finish(
            row.try_get::<Option<chrono::NaiveDate>, _>(idx),
            |v| GenericValue::String(v.to_string()),
            name,
        ),
        "TIME" => finish(
            row.try_get::<Option<chrono::NaiveTime>, _>(idx),
            |v| GenericValue::String(v.to_string()),
            name,
        ),
        "UUID" => finish(
            row.try_get::<Option<uuid::Uuid>, _>(idx),
            |v| GenericValue::String(v.to_string()),
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
            Some(i) => GenericValue::Int(i),
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
