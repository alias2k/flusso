//! SQL generation.
//!
//! The document query assembles a whole nested document **server-side** in one
//! round-trip: `json_build_object` for each level, relations as correlated
//! subqueries (`json_agg` for to-many, a scalar subquery for to-one and
//! aggregates), so nested relations never trigger extra queries. Existence and
//! soft-delete fold into the `WHERE`. Reverse-resolution queries (one selected
//! column, filtered by a key) live here too.
//!
//! Identifiers come from `nutype`-validated schema types (so quoting them is
//! injection-safe); every data value is a bound `$n` parameter.
//!
//! This module is split into:
//!
//! - this file — the [`SqlString`] safety wrapper, [`bind_param`], and the entry
//!   queries ([`document_query`], [`documents_query`], [`reverse_query`]).
//! - [`builder`] — the [`Builder`] that assembles the nested
//!   `json_build_object` for a document's field tree.
//! - [`sql`] — the pure SQL-string fragments (quoting, value expressions,
//!   `ORDER BY`/`LIMIT`/aggregate helpers).

use std::collections::HashMap;

use schema_core::{ColumnName, DatabaseSchema, GenericValue, IndexSchema, TableName};
use sources_core::{Result, SourceError};
use sqlx::Postgres;
use sqlx::postgres::PgArguments;

use builder::Builder;
use sql::{qcol, qident, qtable};

mod builder;
mod sql;

#[cfg(test)]
mod proptests;
#[cfg(test)]
mod tests;

type PgQuery<'q> = sqlx::query::Query<'q, Postgres, PgArguments>;

const ROOT: &str = "root";

/// SQL assembled by this module's query builder, ready to hand to
/// [`sqlx::query`](fn@sqlx::query).
///
/// Since sqlx 0.9, [`sqlx::query`](fn@sqlx::query) only accepts strings that implement
/// [`SqlSafeStr`](sqlx::SqlSafeStr) — natively just `&'static str` — to stop
/// dynamic data being interpolated into SQL. Everything we build here is
/// dynamic, so wrapping it in this type is the single audit point: a value of
/// `SqlString` asserts that the SQL was assembled the safe way — identifiers
/// come from `nutype`-validated schema types (so quoting them is
/// injection-safe) and every data value is a bound `$n` parameter, never
/// formatted into the string. Construct it only from query-builder output.
#[derive(Debug, Clone)]
pub(super) struct SqlString(String);

impl SqlString {
    fn new(sql: String) -> Self {
        Self(sql)
    }

    #[cfg(test)]
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

impl sqlx::SqlSafeStr for SqlString {
    fn into_sql_str(self) -> sqlx::SqlStr {
        // Safe by construction — see the type's documentation.
        sqlx::AssertSqlSafe(self.0).into_sql_str()
    }
}

/// Bind a scalar parameter onto a query, in `params` order.
pub(super) fn bind_param<'q>(query: PgQuery<'q>, value: &GenericValue) -> Result<PgQuery<'q>> {
    Ok(match value {
        GenericValue::Int(i) => query.bind(*i),
        GenericValue::Bool(b) => query.bind(*b),
        GenericValue::Decimal(d) => query.bind(*d),
        GenericValue::String(s) => query.bind(s.clone()),
        GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => {
            return Err(SourceError::Query(
                "cannot bind null, array, or map as a parameter".into(),
            ));
        }
    })
}

/// Build the single query that assembles one document, given its key. Returns
/// the SQL (selecting one `json` column named `document`) and its bound params.
pub(super) fn document_query(
    schema: &IndexSchema,
    key: &[(ColumnName, GenericValue)],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut builder = Builder {
        db: &schema.db_schema,
        pks,
        col_types,
        params: Vec::new(),
        seq: 0,
    };

    let object = builder.object(&schema.fields, ROOT, schema.primary_key.as_ref())?;

    let mut conditions = Vec::new();
    for (column, value) in key {
        let placeholder = builder.typed_placeholder(value.clone(), &schema.table, column)?;
        conditions.push(format!("{} = {placeholder}", qcol(ROOT, column)));
    }
    if let Some(predicate) = builder.soft_delete_predicate(schema)? {
        conditions.push(format!("NOT ({predicate})"));
    }
    if conditions.is_empty() {
        conditions.push("true".to_owned());
    }
    // Root filters scope which rows are documents at all; a row outside the
    // set returns nothing → a tombstone, exactly like soft-delete.
    let root_filters = builder.filters(schema.filters.as_deref(), ROOT, &schema.table)?;

    let sql = format!(
        "SELECT {object} AS \"document\" FROM {} AS \"{ROOT}\" WHERE {}{root_filters}",
        qtable(&schema.db_schema, &schema.table),
        conditions.join(" AND "),
    );
    Ok((SqlString::new(sql), builder.params))
}

/// Build a single query that assembles every document whose root key is in
/// `keys`, for an index with a single-column root key (`pk_column`). Selects
/// the root key as the first column (`doc_key`) beside the assembled document,
/// so the caller can match each row back to its id; a key with no matching
/// row simply doesn't come back, which the caller reads as a tombstone.
///
/// The document is assembled exactly as in [`document_query`] — same nested
/// `json_build_object` / `json_agg` — differing only in selecting the key and
/// matching the root with `IN (…)` instead of a single equality. The key is
/// wrapped in `to_json` so it decodes through the same path as the document.
pub(super) fn documents_query(
    schema: &IndexSchema,
    pk_column: &ColumnName,
    keys: &[GenericValue],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut builder = Builder {
        db: &schema.db_schema,
        pks,
        col_types,
        params: Vec::new(),
        seq: 0,
    };

    // Build the object first: its filters push the leading `$n` params, exactly
    // as `document_query` does, so the key placeholders that follow come after.
    let object = builder.object(&schema.fields, ROOT, schema.primary_key.as_ref())?;

    let mut placeholders = Vec::with_capacity(keys.len());
    for key in keys {
        placeholders.push(builder.typed_placeholder(key.clone(), &schema.table, pk_column)?);
    }
    let mut predicate = format!("{} IN ({})", qcol(ROOT, pk_column), placeholders.join(", "),);
    if let Some(soft_delete) = builder.soft_delete_predicate(schema)? {
        predicate = format!("{predicate} AND NOT ({soft_delete})");
    }
    // Root filters: a requested key outside the set comes back as no row,
    // which the caller reads as a tombstone.
    let root_filters = builder.filters(schema.filters.as_deref(), ROOT, &schema.table)?;
    predicate.push_str(&root_filters);

    let sql = format!(
        "SELECT to_json({key}) AS \"doc_key\", {object} AS \"document\" \
         FROM {} AS \"{ROOT}\" WHERE {predicate}",
        qtable(&schema.db_schema, &schema.table),
        key = qcol(ROOT, pk_column),
    );
    Ok((SqlString::new(sql), builder.params))
}

/// Build a reverse-resolution query: one column from a table, filtered by a key.
///
/// Each key column is matched with its operand cast to the column's catalog SQL
/// type (`= $n::<type>`), so a `uuid`/`date`/… foreign key compares against its
/// own type rather than `text`. `col_types` must carry every key column's type.
pub(super) fn reverse_query(
    db: &DatabaseSchema,
    table: &TableName,
    select_column: &ColumnName,
    key: &[(ColumnName, GenericValue)],
    col_types: &HashMap<(String, String), String>,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut params = Vec::new();
    let mut conditions = Vec::new();
    for (column, value) in key {
        if !value.is_bindable_scalar() {
            return Err(SourceError::Query(
                "cannot bind null, array, or map as a key".into(),
            ));
        }
        let sql_type = col_types
            .get(&(table.to_string(), column.to_string()))
            .ok_or_else(|| {
                SourceError::Query(format!("internal: missing type for `{table}.{column}`"))
            })?;
        params.push(value.clone());
        conditions.push(format!(
            "{} = ${}::{sql_type}",
            qident(column.as_ref()),
            params.len()
        ));
    }
    if conditions.is_empty() {
        conditions.push("true".to_owned());
    }
    let sql = format!(
        "SELECT {} FROM {} WHERE {}",
        qident(select_column.as_ref()),
        qtable(db, table),
        conditions.join(" AND "),
    );
    Ok((SqlString::new(sql), params))
}
