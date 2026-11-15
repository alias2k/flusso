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

use kernel::{ColumnName, DatabaseSchema, GenericValue, IndexSchema, TableName, Transform};
use source::{Result, SourceError};
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

/// The column the batched build selects, beside each document, the 1-based
/// position (a `bigint`) of the requested key the row matched.
pub(super) const DOC_POSITION: &str = "doc_position";

/// The alias of the batched build's list of requested keys, and its two
/// columns: the key and its position. A raw filter's unqualified name that its
/// own table lacks resolves outward through the root row to this list, so the
/// names are ones no user table is expected to carry.
const KEYS: &str = "flusso_requested";
const KEY: &str = "flusso_key";
const POSITION: &str = "flusso_position";

/// The columns a document reads whose base type is `bytea` or `bytea[]`, by
/// `(table, column)`. The document query renders them as hex text itself, so
/// what a document holds never depends on the session's `bytea_output`.
pub(super) type ByteColumns = HashMap<(String, String), Bytes>;

/// Which `bytea` a [`ByteColumns`] column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Bytes {
    Scalar,
    Array,
}

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
        GenericValue::Bool(b) => query.bind(*b),
        GenericValue::SmallInt(i) => query.bind(*i),
        GenericValue::Int(i) => query.bind(*i),
        GenericValue::BigInt(i) => query.bind(*i),
        GenericValue::Float(f) => query.bind(*f),
        GenericValue::Double(f) => query.bind(*f),
        GenericValue::Decimal(d) => query.bind(*d),
        GenericValue::BigDecimal(d) => query.bind(d.as_str().to_owned()),
        GenericValue::String(s) => query.bind(s.clone()),
        GenericValue::Uuid(u) => query.bind(*u),
        GenericValue::Date(d) => query.bind(*d),
        GenericValue::Time(t) => query.bind(*t),
        GenericValue::Timestamp(ts) => query.bind(*ts),
        GenericValue::TimestampTz(ts) => query.bind(*ts),
        GenericValue::Bytes(bytes) => query.bind(bytes.clone()),
        GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => {
            return Err(SourceError::Query(
                "cannot bind null, array, or map as a parameter".into(),
            ));
        }
    })
}

/// A query that applies `transforms` to a `NULL` of `sql_type` (a column's
/// `format_type`) exactly as a document query applies them to the column.
/// Postgres resolves the functions the same way, so it fails where a transform
/// does not take the type. It reads no table, so it takes no lock.
pub(super) fn transform_probe(sql_type: &str, transforms: &[Transform]) -> SqlString {
    SqlString::new(format!(
        "SELECT {}",
        sql::transformed(format!("NULL::{sql_type}"), transforms)
    ))
}

/// A query that compares a `NULL` of `link_type` with a `NULL` of
/// `key_type` (each a column's `format_type`) exactly as a document query
/// joins a link column to its key. Postgres resolves the `=` the same way, so
/// it fails where the two types don't compare. It reads no table, so it takes
/// no lock.
pub(super) fn comparison_probe(link_type: &str, key_type: &str) -> SqlString {
    SqlString::new(format!("SELECT NULL::{link_type} = NULL::{key_type}"))
}

/// Build the single query that assembles one document, given its key. Returns
/// the SQL (selecting one `json` column named `document`) and its bound params.
pub(super) fn document_query(
    schema: &IndexSchema,
    key: &[(ColumnName, GenericValue)],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
    bytes: &ByteColumns,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut builder = Builder {
        db: &schema.db_schema,
        pks,
        col_types,
        bytes,
        params: Vec::new(),
        seq: 0,
    };

    let object = builder.object(
        &schema.fields,
        &schema.table,
        ROOT,
        schema.primary_key.as_ref(),
    )?;

    let mut conditions = Vec::new();
    for (column, value) in key {
        let placeholder = builder.key_placeholder(value.clone(), &schema.table, column)?;
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

/// Build a single query that assembles every document whose root key is one
/// of `keys`, for an index with a single-column root key (`pk_column`). Each
/// row carries, under [`DOC_POSITION`], the 1-based position in `keys` of the
/// key it matched; a key no root row matches simply doesn't come back, which
/// the caller reads as a tombstone.
///
/// The keys are an array of `$n::<bound>::<key type>`, unnested `WITH
/// ORDINALITY` for their positions, and each position looks its root row up
/// through a `LATERAL` subquery, `root.pk = flusso_requested.flusso_key`: the very comparison
/// [`document_query`] makes for one key, so Postgres decides which row answers
/// which key in both, with no Rust mirror of its casts. The subquery's
/// `LIMIT 1` does two things:
///
/// - it keeps the subquery a nested loop of primary-key index probes at any
///   batch size (as a plain join the planner may turn a large list into a hash
///   join over a scan of the root);
/// - it tells the planner a probe returns at most one row, which the primary
///   key already guarantees. Over a partitioned root the probe is planned
///   across every partition and pruned to one at run time; without the limit
///   the planner counts a row per partition per key, costs every relation
///   subquery that many times and passes `jit_above_cost` from about 130 keys
///   on 16 partitions, compiling each execution.
///
/// A row matched by several positions comes back once per position. The
/// document is assembled exactly as in [`document_query`]. A key Postgres
/// refuses to cast fails the statement, so the whole batch, as a single build
/// of it fails.
pub(super) fn documents_query(
    schema: &IndexSchema,
    pk_column: &ColumnName,
    keys: &[GenericValue],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
    bytes: &ByteColumns,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut builder = Builder {
        db: &schema.db_schema,
        pks,
        col_types,
        bytes,
        params: Vec::new(),
        seq: 0,
    };

    // Build the object first: its filters push the leading `$n` params, exactly
    // as `document_query` does, so the key placeholders that follow come after.
    let object = builder.object(
        &schema.fields,
        &schema.table,
        ROOT,
        schema.primary_key.as_ref(),
    )?;

    let mut values = Vec::with_capacity(keys.len());
    for key in keys {
        values.push(builder.key_placeholder(key.clone(), &schema.table, pk_column)?);
    }
    let mut predicate = format!("{} = \"{KEYS}\".\"{KEY}\"", qcol(ROOT, pk_column));
    if let Some(soft_delete) = builder.soft_delete_predicate(schema)? {
        predicate = format!("{predicate} AND NOT ({soft_delete})");
    }
    // Root filters: a requested key outside the set comes back as no row,
    // which the caller reads as a tombstone.
    let root_filters = builder.filters(schema.filters.as_deref(), ROOT, &schema.table)?;
    predicate.push_str(&root_filters);

    let sql = format!(
        "SELECT \"{KEYS}\".\"{POSITION}\" AS \"{DOC_POSITION}\", {object} AS \"document\" \
         FROM unnest(ARRAY[{values}]) WITH ORDINALITY AS \"{KEYS}\"(\"{KEY}\", \"{POSITION}\") \
         CROSS JOIN LATERAL (SELECT * FROM {table} AS \"{ROOT}\" WHERE {predicate} LIMIT 1) \
         AS \"{ROOT}\"",
        values = values.join(", "),
        table = qtable(&schema.db_schema, &schema.table),
    );
    Ok((SqlString::new(sql), builder.params))
}

/// Build the query that reads back the root rows whose key equals one of
/// `links`, selecting each row's own key: the document ids those links name.
///
/// Each link is cast from the type it is bound as to its link column's,
/// `sql_type` (`$n::<bound>::<sql_type>`), so the key compares with it as the
/// document query's join compares the two columns, and the statement text
/// names every parameter's type: links bound as different types never share a
/// prepared statement.
pub(super) fn root_keys_query(
    db: &DatabaseSchema,
    table: &TableName,
    (pk, pk_type, pk_base): (&ColumnName, &str, &str),
    links: &[GenericValue],
    sql_type: &str,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut placeholders = Vec::with_capacity(links.len());
    for (n, link) in links.iter().enumerate() {
        let bound = sql::bound_type(link)
            .ok_or_else(|| SourceError::Query("cannot bind null, array, or map as a key".into()))?;
        placeholders.push(format!("${}::{bound}::{sql_type}", n + 1));
    }
    let column = qident(pk.as_ref());
    let sql = format!(
        "SELECT {} FROM {} WHERE {column} IN ({})",
        super::value::key_select(&column, pk.as_ref(), pk_type, pk_base),
        qtable(db, table),
        placeholders.join(", "),
    );
    Ok((SqlString::new(sql), links.to_vec()))
}

/// Build a reverse-resolution query: one column from a table, filtered by a key.
///
/// Each key column is matched with its operand cast to the column's catalog SQL
/// type (`= $n::<type>`), so a `uuid`/`date`/… foreign key compares against its
/// own type rather than `text`. `col_types` must carry every key column's type.
pub(super) fn reverse_query(
    db: &DatabaseSchema,
    table: &TableName,
    (select_column, select_type, select_base): (&ColumnName, &str, &str),
    key: &[(ColumnName, GenericValue)],
    col_types: &HashMap<(String, String), String>,
) -> Result<(SqlString, Vec<GenericValue>)> {
    let mut params = Vec::new();
    let mut conditions = Vec::new();
    for (column, value) in key {
        let unbindable = || SourceError::Query("cannot bind null, array, or map as a key".into());
        if !value.is_bindable_scalar() {
            return Err(unbindable());
        }
        let sql_type = col_types
            .get(&(table.to_string(), column.to_string()))
            .ok_or_else(|| {
                SourceError::Query(format!("internal: missing type for `{table}.{column}`"))
            })?;
        params.push(value.clone());
        conditions.push(format!(
            "{} = ${}{}",
            qident(column.as_ref()),
            params.len(),
            sql::key_cast(value, sql_type).ok_or_else(unbindable)?,
        ));
    }
    if conditions.is_empty() {
        conditions.push("true".to_owned());
    }
    let sql = format!(
        "SELECT {} FROM {} WHERE {}",
        super::value::key_select(
            &qident(select_column.as_ref()),
            select_column.as_ref(),
            select_type,
            select_base,
        ),
        qtable(db, table),
        conditions.join(" AND "),
    );
    Ok((SqlString::new(sql), params))
}
