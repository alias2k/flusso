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

/// The column a batched build paired by key selects beside each document:
/// the row's key, as [`key_select`](super::value::key_select) renders it.
pub(super) const DOC_KEY: &str = "doc_key";

/// The batched build's names one level out from the root: the built rows,
/// the rows paired with positions, the numbered list of requested keys, and
/// its columns. The document and its raw filters are built a level in, where
/// the root table is the only relation in scope, so no name here can meet one
/// of the user's.
const BUILT: &str = "flusso_built";
const PAIRED: &str = "flusso_paired";
const NUMBERED: &str = "flusso_numbered";
const KEYS: &str = "flusso_requested";
const KEY: &str = "flusso_key";
const POSITION: &str = "flusso_position";

/// Whether the declared root key is the root table's whole primary key, so a
/// key matches at most one row. A root without one (no primary key, a
/// composite one, a partitioned table keyed with its partition column, a
/// view) still starts, with a warning; a key several of its rows share builds
/// the row its [`TieBreak`] orders first, in a batched and a single build
/// alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RootKey {
    Unique,
    Shared(TieBreak),
}

/// How a batch writes its keys, decided per chunk from facts known when the
/// statement is built ([`KeyForm::for_batch`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyForm {
    /// Placeholders in an array constructor, padded to a power of two
    /// ([`key_constructor`]). Postgres sees every key: it knows the batch's
    /// length, so a generic plan of a small batch is cached, and it prunes
    /// a root partitioned by its key under a generic plan too.
    Constructor,
    /// One array parameter per kind ([`key_arrays`]): a short statement
    /// bound in one go, which costs a large batch less than placeholders do.
    /// Postgres takes such an array for ten elements and can't prune
    /// partitions by it at run time.
    Parameters,
}

/// Below this many keys a batch writes them as a constructor: Postgres takes
/// an array parameter for ten elements, so a smaller batch would look cheaper
/// planned afresh every call.
const SMALL_BATCH: usize = 16;

impl KeyForm {
    /// A constructor for a partitioned root (its run-time pruning reads keys
    /// one by one) and for fewer than [`SMALL_BATCH`] keys; parameters
    /// otherwise.
    pub(super) fn for_batch(partitioned: bool, keys: usize) -> Self {
        if partitioned || keys < SMALL_BATCH {
            Self::Constructor
        } else {
            Self::Parameters
        }
    }
}

/// What a batched statement is built for: whether its root key is unique,
/// and how it writes its keys.
#[derive(Debug, Clone, Copy)]
pub(super) struct Shape<'a> {
    pub(super) root_key: &'a RootKey,
    pub(super) form: KeyForm,
}

/// How the rows sharing a key are ordered to pick the one that builds: an
/// `ORDER BY` list over the root row. It reads no column the document
/// doesn't read, so a wide column the document leaves alone is never fetched:
///
/// - [`primary_key`](Self::primary_key): the table's own primary key, which is
///   unique, so the order is total.
/// - [`read_columns`](Self::read_columns), for a root without one (a keyless
///   table, a view): the text of every root column the document reads. Rows
///   that tie there build the same document.
/// - [`whole_row`](Self::whole_row): the whole row's text, only where the
///   columns read are unknown (a raw filter).
///
/// When there is nothing to order by (the document reads no root column but
/// the key, which every competing row shares), any row answers: no term is
/// rendered, so no caller can write an empty `ORDER BY`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TieBreak(Option<String>);

impl TieBreak {
    pub(super) fn primary_key(columns: &[ColumnName]) -> Self {
        Self::of(columns.iter().map(|column| qcol(ROOT, column)))
    }

    pub(super) fn read_columns<'a>(columns: impl IntoIterator<Item = &'a ColumnName>) -> Self {
        let mut columns: Vec<&ColumnName> = columns.into_iter().collect();
        columns.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
        Self::of(
            columns
                .into_iter()
                .map(|column| format!("{}::text", qcol(ROOT, column))),
        )
    }

    pub(super) fn whole_row() -> Self {
        Self(Some(format!("ROW({}.*)::text", qident(ROOT))))
    }

    /// The terms in order; none (the document reads no root column but the
    /// key) means every row sharing a key builds the same document, so any
    /// one answers and nothing is ordered.
    fn of(terms: impl Iterator<Item = String>) -> Self {
        let terms: Vec<String> = terms.collect();
        Self((!terms.is_empty()).then(|| terms.join(", ")))
    }

    /// `ORDER BY` terms after the key, each led by a comma; empty for any row.
    fn after_key(&self) -> String {
        self.0
            .as_ref()
            .map_or_else(String::new, |terms| format!(", {terms}"))
    }
}

/// A single build's query (from [`document_query`]) reading the row a shared
/// key picks: the one `tie_break` orders first, or as it is when any row
/// answers.
pub(super) fn first_row(query: SqlString, tie_break: &TieBreak) -> SqlString {
    match &tie_break.0 {
        Some(terms) => SqlString::new(format!("{} ORDER BY {terms} LIMIT 1", query.0)),
        None => query,
    }
}

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

/// The alias a backfill page selects each key under.
pub(super) const KEY_PAGE_KEY: &str = "key";

/// The alias of a page's "at or before the walk's bound" flag.
pub(super) const KEY_PAGE_WITHIN: &str = "within";

/// The operators a backfill walk orders and compares its keys by, as SQL:
/// the root key's btree operators written schema-qualified
/// (`OPERATOR(ext.<)`), or bare (`<`) when the role may not name them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WalkOperators {
    pub(super) less: String,
    pub(super) greater: String,
    pub(super) at_most: String,
}

impl WalkOperators {
    /// The operators the search path finds, for a key whose own the role
    /// may not name.
    pub(super) fn bare() -> Self {
        Self {
            less: "<".to_owned(),
            greater: ">".to_owned(),
            at_most: "<=".to_owned(),
        }
    }

    /// The btree operators of `schema` (quoted already): `<`, `>`, `<=`.
    pub(super) fn qualified(schema: &str) -> Self {
        let named = |op: &str| format!("OPERATOR({schema}.{op})");
        Self {
            less: named("<"),
            greater: named(">"),
            at_most: named("<="),
        }
    }
}

/// How a key is read and compared in a walk: its type to cast to, its base
/// type's name (for its text), and the walk's operators.
#[derive(Debug, Clone, Copy)]
pub(super) struct WalkKey<'a> {
    pub(super) column: &'a ColumnName,
    pub(super) key_type: &'a str,
    pub(super) base_name: &'a str,
    pub(super) operators: &'a WalkOperators,
}

/// A backfill's key page (see `keys.rs`): up to `limit` of the root's keys
/// after `after`, in key order, each with whether it is at or before
/// `through`. The bound is in the select list, not the `WHERE`: as a second
/// range condition a generic plan would take the range for a small one and
/// sort it, every page. Only identifiers, the key type (from the catalog),
/// the operators (from the catalog) and the limit are written into the
/// text; `after` and `through` are parameters cast by [`sql::key_cast`].
pub(super) fn key_page_query(
    schema: &IndexSchema,
    key: WalkKey<'_>,
    after: Option<&GenericValue>,
    through: &GenericValue,
    limit: usize,
) -> Result<SqlString> {
    let cast = |value: &GenericValue| {
        sql::key_cast(value, key.key_type).ok_or_else(|| {
            SourceError::Query(format!("a backfill page can't bind the key {value:?}"))
        })
    };
    let column = qident(key.column.as_ref());
    let select = super::value::key_select(&column, KEY_PAGE_KEY, key.key_type, key.base_name);
    let table = qtable(&schema.db_schema, &schema.table);
    let ops = key.operators;
    let (bound, through_param) = match after {
        None => (String::new(), 1),
        Some(after) => (
            format!(" WHERE {column} {} $1{}", ops.greater, cast(after)?),
            2,
        ),
    };
    Ok(SqlString::new(format!(
        "SELECT {select}, ({column} {} ${through_param}{}) AS \"{KEY_PAGE_WITHIN}\" \
         FROM {table}{bound} ORDER BY {column} USING {} LIMIT {limit}",
        ops.at_most,
        cast(through)?,
        ops.less,
    )))
}

/// The root's greatest key, by the walk's own order (see `keys.rs`).
pub(super) fn last_key_query(schema: &IndexSchema, key: WalkKey<'_>) -> SqlString {
    let column = qident(key.column.as_ref());
    let select = super::value::key_select(&column, KEY_PAGE_KEY, key.key_type, key.base_name);
    let table = qtable(&schema.db_schema, &schema.table);
    SqlString::new(format!(
        "SELECT {select} FROM {table} ORDER BY {column} USING {} LIMIT 1",
        key.operators.greater
    ))
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
        GenericValue::Array(items) => bind_array(query, items)?,
        GenericValue::Null | GenericValue::Map(_) => {
            return Err(SourceError::Query(
                "cannot bind null or map as a parameter".into(),
            ));
        }
    })
}

/// Bind an array of scalars of one kind (a batch's keys, [`key_arrays`]) as
/// the array of the type [`sql::bound_type`] names for them.
fn bind_array<'q>(query: PgQuery<'q>, items: &[GenericValue]) -> Result<PgQuery<'q>> {
    fn all<T>(items: &[GenericValue], take: impl Fn(&GenericValue) -> Option<T>) -> Result<Vec<T>> {
        items
            .iter()
            .map(|item| {
                take(item).ok_or_else(|| {
                    SourceError::Query("internal: an array parameter mixes kinds".into())
                })
            })
            .collect()
    }
    use GenericValue as V;
    Ok(match items.first() {
        None => {
            return Err(SourceError::Query(
                "internal: an empty array parameter".into(),
            ));
        }
        Some(V::Bool(_)) => query.bind(all(items, |v| match v {
            V::Bool(b) => Some(*b),
            _ => None,
        })?),
        Some(V::SmallInt(_)) => query.bind(all(items, |v| match v {
            V::SmallInt(i) => Some(*i),
            _ => None,
        })?),
        Some(V::Int(_)) => query.bind(all(items, |v| match v {
            V::Int(i) => Some(*i),
            _ => None,
        })?),
        Some(V::BigInt(_)) => query.bind(all(items, |v| match v {
            V::BigInt(i) => Some(*i),
            _ => None,
        })?),
        Some(V::Float(_)) => query.bind(all(items, |v| match v {
            V::Float(f) => Some(*f),
            _ => None,
        })?),
        Some(V::Double(_)) => query.bind(all(items, |v| match v {
            V::Double(f) => Some(*f),
            _ => None,
        })?),
        Some(V::Decimal(_)) => query.bind(all(items, |v| match v {
            V::Decimal(d) => Some(*d),
            _ => None,
        })?),
        Some(V::String(_) | V::BigDecimal(_)) => query.bind(all(items, |v| match v {
            V::String(s) => Some(s.clone()),
            V::BigDecimal(d) => Some(d.as_str().to_owned()),
            _ => None,
        })?),
        Some(V::Uuid(_)) => query.bind(all(items, |v| match v {
            V::Uuid(u) => Some(*u),
            _ => None,
        })?),
        Some(V::Date(_)) => query.bind(all(items, |v| match v {
            V::Date(d) => Some(*d),
            _ => None,
        })?),
        Some(V::Time(_)) => query.bind(all(items, |v| match v {
            V::Time(t) => Some(*t),
            _ => None,
        })?),
        Some(V::Timestamp(_)) => query.bind(all(items, |v| match v {
            V::Timestamp(t) => Some(*t),
            _ => None,
        })?),
        Some(V::TimestampTz(_)) => query.bind(all(items, |v| match v {
            V::TimestampTz(t) => Some(*t),
            _ => None,
        })?),
        Some(V::Bytes(_)) => query.bind(all(items, |v| match v {
            V::Bytes(b) => Some(b.clone()),
            _ => None,
        })?),
        Some(V::Null | V::Array(_) | V::Map(_)) => {
            return Err(SourceError::Query(
                "cannot bind null, array, or map in an array parameter".into(),
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
/// Two levels:
///
/// - **Inside**, the root rows are read as [`document_query`] reads one:
///   `root.pk = ANY(ARRAY[$n::<bound>::<key type>, …])` with soft delete and
///   root filters, one scan of the primary key index, rows in key order. The
///   document is built here, where the root table is the only relation in
///   scope, so a raw filter's unqualified name resolves as in a single build.
/// - **One level out**, the same keys, unnested `WITH ORDINALITY` for their
///   positions, are joined to the built rows on the same `=`. Postgres
///   decides which row answers which key, with no Rust mirror of its casts,
///   and a row answering several positions (the same key read as `int4` and
///   as `int8`) comes back once per position. The root is scanned once.
///
/// The join is a `FULL JOIN`, which Postgres runs only as a hash or merge
/// join. As an inner or left join the planner may pick a nested loop: over a
/// materialized batch of wide documents rescanned per requested key (it sizes
/// a document at a few bytes, so the loop looks free while it spills to disk
/// and reads the batch back per key), or over the key list rebuilt per row.
/// A requested key no row answers is dropped one level further out, whose
/// `OFFSET 0` keeps the planner from reading the `IS NOT NULL` back into the
/// join, which would make it a `LEFT JOIN` again.
///
/// [`RootKey::Shared`]: the inner level keeps one row per key, the one its
/// [`TieBreak`] orders first, as [`first_row`] does for a single build. A key
/// Postgres refuses to cast fails the statement, so the whole batch, as a
/// single build of it fails.
///
/// The pairing costs a light document about a quarter of its build, so a
/// batch whose keys pair by value (`matching::pairs_by_key`) runs the inner
/// level alone instead: [`documents_by_key_query`].
pub(super) fn documents_query(
    schema: &IndexSchema,
    pk_column: &ColumnName,
    keys: &[GenericValue],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
    bytes: &ByteColumns,
    shape: Shape<'_>,
) -> Result<(SqlString, Vec<GenericValue>)> {
    batched(
        schema,
        pk_column,
        keys,
        (pks, col_types, bytes),
        shape,
        None,
    )
}

/// The batched build of [`documents_query`] when every requested key pairs
/// with its row by value: the key's `=` is identity (`key_type`'s base
/// `base_name`, a deterministic collation) and every key is of the kind the
/// column reads back as. Main's own query: the inner level alone, selecting
/// the row's key under [`DOC_KEY`] beside its document, with no join.
pub(super) fn documents_by_key_query(
    schema: &IndexSchema,
    pk_column: &ColumnName,
    keys: &[GenericValue],
    pks: &HashMap<String, ColumnName>,
    col_types: &HashMap<(String, String), String>,
    bytes: &ByteColumns,
    (shape, key_type, base_name): (Shape<'_>, &str, &str),
) -> Result<(SqlString, Vec<GenericValue>)> {
    batched(
        schema,
        pk_column,
        keys,
        (pks, col_types, bytes),
        shape,
        Some((key_type, base_name)),
    )
}

/// What the catalog says that a document query needs: relation keys, column
/// types, and the columns read as bytes.
type Catalog<'a> = (
    &'a HashMap<String, ColumnName>,
    &'a HashMap<(String, String), String>,
    &'a ByteColumns,
);

/// [`documents_query`] (`by_key` `None`) or [`documents_by_key_query`].
fn batched(
    schema: &IndexSchema,
    pk_column: &ColumnName,
    keys: &[GenericValue],
    (pks, col_types, bytes): Catalog<'_>,
    Shape { root_key, form }: Shape<'_>,
    by_key: Option<(&str, &str)>,
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

    let with_positions = by_key.is_none();
    let requested = match form {
        KeyForm::Constructor => {
            key_constructor(&mut builder, keys, &schema.table, pk_column, with_positions)?
        }
        KeyForm::Parameters => {
            key_arrays(&mut builder, keys, &schema.table, pk_column, with_positions)?
        }
    };
    let root_pk = qcol(ROOT, pk_column);
    let mut predicate = format!("{root_pk} = ANY({})", requested.any);
    if let Some(soft_delete) = builder.soft_delete_predicate(schema)? {
        predicate = format!("{predicate} AND NOT ({soft_delete})");
    }
    // Root filters: a requested key outside the set comes back as no row,
    // which the caller reads as a tombstone.
    let root_filters = builder.filters(schema.filters.as_deref(), ROOT, &schema.table)?;
    predicate.push_str(&root_filters);

    let table = qtable(&schema.db_schema, &schema.table);
    let key_out = match by_key {
        Some((key_type, base_name)) => {
            super::value::key_select(&root_pk, DOC_KEY, key_type, base_name)
        }
        None => format!("{root_pk} AS \"{KEY}\""),
    };
    let built = match (root_key, by_key) {
        (RootKey::Unique, Some(_)) => format!(
            "SELECT {key_out}, {object} AS \"document\" FROM {table} AS \"{ROOT}\" WHERE {predicate}"
        ),
        // `OFFSET 0` keeps the level a scan of its own: pulled up into the
        // join, the planner may probe the root once per requested key.
        (RootKey::Unique, None) => format!(
            "SELECT {key_out}, {object} AS \"document\" \
             FROM {table} AS \"{ROOT}\" WHERE {predicate} OFFSET 0"
        ),
        (RootKey::Shared(tie_break), _) => format!(
            "SELECT DISTINCT ON ({root_pk}) {key_out}, {object} AS \"document\" \
             FROM {table} AS \"{ROOT}\" WHERE {predicate} ORDER BY {root_pk}{}",
            tie_break.after_key()
        ),
    };
    if by_key.is_some() {
        return Ok((SqlString::new(built), builder.params));
    }
    let paired = format!(
        "SELECT \"{KEYS}\".\"{POSITION}\" AS \"{DOC_POSITION}\", \"{BUILT}\".\"document\" \
         FROM ({built}) AS \"{BUILT}\" \
         FULL JOIN {rows} AS \"{KEYS}\"(\"{KEY}\", \"{POSITION}\") \
         ON \"{BUILT}\".\"{KEY}\" = \"{KEYS}\".\"{KEY}\" OFFSET 0",
        rows = requested
            .rows
            .ok_or_else(|| SourceError::Query("internal: positions not built".into()))?,
    );
    let sql = format!(
        "SELECT \"{DOC_POSITION}\", \"document\" FROM ({paired}) AS \"{PAIRED}\" \
         WHERE \"{PAIRED}\".\"document\" IS NOT NULL"
    );
    Ok((SqlString::new(sql), builder.params))
}

/// A batch's requested keys as SQL: `any`, the array `= ANY(…)` reads, and
/// `rows`, a `FROM` item of `(key, position)`.
#[derive(Debug)]
struct KeyList {
    any: String,
    rows: Option<String>,
}

/// The keys as an array constructor of placeholders, each cast from the type
/// it is bound as to the key column's type (`$n::<bound>::<key type>`), so
/// Postgres sees the list element by element: it knows how many keys a batch
/// holds, so a generic plan costs what a custom one does and is cached, and it
/// prunes the partitions of a root partitioned by its key under either plan.
///
/// The keys are grouped by the type they are bound as, in a fixed order of
/// those types, and each group is padded to the next power of two by
/// repeating its last key. A statement's text so depends on the kinds of keys
/// and on at most ten sizes per kind, never on the exact count, and every
/// placeholder keeps its type from one batch to the next (a prepared statement
/// keeps the types of its first use). A repeated key changes nothing an `= ANY`
/// reads. The position form unnests the same constructor beside one `bigint[]`
/// parameter of the position each key was requested at, `0` for a pad, and
/// drops the pads before the join.
fn key_constructor(
    builder: &mut Builder<'_>,
    keys: &[GenericValue],
    table: &TableName,
    column: &ColumnName,
    with_positions: bool,
) -> Result<KeyList> {
    let mut kinds: Vec<(&'static str, Vec<(GenericValue, i64)>)> = Vec::new();
    for (at, key) in keys.iter().enumerate() {
        let bound = sql::bound_type(key)
            .ok_or_else(|| SourceError::Query("cannot bind null, array, or map as a key".into()))?;
        let position = i64::try_from(at + 1)
            .map_err(|_| SourceError::Query("too many keys in one batch".into()))?;
        match kinds.iter_mut().find(|(kind, _)| *kind == bound) {
            Some((_, group)) => group.push((key.clone(), position)),
            None => kinds.push((bound, vec![(key.clone(), position)])),
        }
    }
    kinds.sort_by_key(|(kind, _)| *kind);
    let mut placeholders = Vec::new();
    let mut positions = Vec::new();
    for (_, group) in kinds {
        let padded = group.len().next_power_of_two();
        let last = group.last().map(|(key, _)| key.clone());
        let pads = std::iter::repeat_with(|| last.clone()).take(padded - group.len());
        for (key, position) in group
            .into_iter()
            .map(|(key, position)| (Some(key), position))
            .chain(pads.map(|key| (key, 0)))
        {
            let key =
                key.ok_or_else(|| SourceError::Query("internal: an empty key group".into()))?;
            placeholders.push(builder.key_placeholder(key, table, column)?);
            positions.push(position);
        }
    }
    let array = format!("ARRAY[{}]", placeholders.join(", "));
    if !with_positions {
        return Ok(KeyList {
            any: array,
            rows: None,
        });
    }
    let positions = builder.positions(positions);
    Ok(KeyList {
        rows: Some(format!(
            "(SELECT * FROM unnest({array}, {positions}) AS \"{NUMBERED}\"(\"{KEY}\", \"{POSITION}\") \
             WHERE \"{NUMBERED}\".\"{POSITION}\" > 0)"
        )),
        any: array,
    })
}

/// The keys as one array parameter per type they are bound as, each cast
/// element by element to the key column's type ([`Builder::key_array`]), in a
/// fixed order of those types: one statement text per kind, however many keys.
/// Keys of one kind keep their order, so `unnest … WITH ORDINALITY` numbers
/// them; keys of several kinds are unnested beside a `bigint[]` of their
/// positions, one `unnest` per kind.
fn key_arrays(
    builder: &mut Builder<'_>,
    keys: &[GenericValue],
    table: &TableName,
    column: &ColumnName,
    with_positions: bool,
) -> Result<KeyList> {
    let mut kinds: Vec<(&'static str, Vec<GenericValue>, Vec<i64>)> = Vec::new();
    for (at, key) in keys.iter().enumerate() {
        let bound = sql::bound_type(key)
            .ok_or_else(|| SourceError::Query("cannot bind null, array, or map as a key".into()))?;
        let position = i64::try_from(at + 1)
            .map_err(|_| SourceError::Query("too many keys in one batch".into()))?;
        match kinds.iter_mut().find(|(kind, _, _)| *kind == bound) {
            Some((_, values, positions)) => {
                values.push(key.clone());
                positions.push(position);
            }
            None => kinds.push((bound, vec![key.clone()], vec![position])),
        }
    }
    kinds.sort_by_key(|(kind, _, _)| *kind);
    if let [(bound, values, _)] = kinds.as_mut_slice() {
        let array = builder.key_array(std::mem::take(values), bound, table, column)?;
        return Ok(KeyList {
            rows: with_positions.then(|| format!("unnest({array}) WITH ORDINALITY")),
            any: array,
        });
    }
    let mut arrays = Vec::with_capacity(kinds.len());
    let mut unnests = Vec::with_capacity(kinds.len());
    for (bound, values, positions) in kinds {
        let array = builder.key_array(values, bound, table, column)?;
        if with_positions {
            let positions = builder.positions(positions);
            unnests.push(format!("SELECT * FROM unnest({array}, {positions})"));
        }
        arrays.push(array);
    }
    Ok(KeyList {
        any: arrays.join(" || "),
        rows: with_positions.then(|| format!("({})", unnests.join(" UNION ALL "))),
    })
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
