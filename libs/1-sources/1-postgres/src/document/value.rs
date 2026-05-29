//! Conversions between Postgres values and the schema's [`GenericValue`].
//!
//! Two directions: [`to_sea_value`] binds a key/filter scalar into a query, and
//! [`row_to_map`] decodes a fetched row into a name→value map the assembler
//! reads from.

use std::collections::HashMap;

use rust_decimal::Decimal;
use schema_core::GenericValue;
use sea_query::Value as SeaValue;
use sources_core::SourceError;
use sqlx::postgres::{PgColumn, PgRow};
use sqlx::{Column, Row, TypeInfo};

/// Bind a scalar as a query parameter. Only the types that occur in primary
/// keys and filters are accepted; null/array/map cannot be a key or operand.
pub(crate) fn to_sea_value(value: &GenericValue) -> Result<SeaValue, SourceError> {
    match value {
        GenericValue::Int(i) => Ok(SeaValue::BigInt(Some(*i))),
        GenericValue::Bool(b) => Ok(SeaValue::Bool(Some(*b))),
        GenericValue::Decimal(d) => Ok(SeaValue::Decimal(Some(Box::new(*d)))),
        GenericValue::String(s) => Ok(SeaValue::String(Some(Box::new(s.clone())))),
        GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => Err(
            SourceError::Query("cannot bind null, array, or map as a key or filter value".into()),
        ),
    }
}

/// Decode every column of a row into a name→value map.
pub(crate) fn row_to_map(row: &PgRow) -> HashMap<String, GenericValue> {
    let mut map = HashMap::with_capacity(row.columns().len());
    for col in row.columns() {
        map.insert(col.name().to_owned(), decode_column(row, col));
    }
    map
}

/// Decode a single column by its Postgres type. Unsupported types and decode
/// failures degrade to [`GenericValue::Null`] with a warning rather than
/// failing the whole document.
fn decode_column(row: &PgRow, col: &PgColumn) -> GenericValue {
    let idx = col.ordinal();
    let name = col.name();
    match col.type_info().name() {
        "INT2" => finish(row.try_get::<Option<i16>, _>(idx), |v| GenericValue::Int(v.into()), name),
        "INT4" => finish(row.try_get::<Option<i32>, _>(idx), |v| GenericValue::Int(v.into()), name),
        "INT8" => finish(row.try_get::<Option<i64>, _>(idx), GenericValue::Int, name),
        "BOOL" => finish(row.try_get::<Option<bool>, _>(idx), GenericValue::Bool, name),
        "FLOAT4" => finish(row.try_get::<Option<f32>, _>(idx), |v| float(v.into()), name),
        "FLOAT8" => finish(row.try_get::<Option<f64>, _>(idx), float, name),
        "NUMERIC" => finish(row.try_get::<Option<Decimal>, _>(idx), GenericValue::Decimal, name),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" | "CITEXT" => {
            finish(row.try_get::<Option<String>, _>(idx), GenericValue::String, name)
        }
        other => {
            tracing::warn!(column = %name, r#type = %other, "unsupported column type; treating as null");
            GenericValue::Null
        }
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
