//! [`PgDocumentBuilder`] — the read half of the Postgres source.
//!
//! Resolves which documents a changed row affects and assembles them from the
//! schema. The work is split across this module:
//!
//! - [`fields`] — pure traversal of the index's field tree.
//! - [`resolve`] — reverse resolution: changed row → affected document keys.
//! - [`query`] — SQL generation (the server-side document query, reverse
//!   queries, parameter binding).
//! - [`value`] — decoding Postgres results into the value tree.
//!
//! ## Assembly happens in Postgres
//!
//! [`build`](PgDocumentBuilder::build) issues **one** query per document: the
//! whole nested document is assembled server-side with `json_build_object` /
//! `json_agg` and correlated subqueries (see [`query`]). Nested relations don't
//! trigger extra round-trips, so there is no N+1. Existence and soft-delete
//! fold into the query's `WHERE`, so a missing or deleted row simply returns no
//! row → a tombstone.
//!
//! ## Coverage
//!
//! - Resolution: root table; direct foreign-key relations (`has_one`/
//!   `has_many`); parent-side-key relations (`belongs_to`, resolved against the
//!   parent table — so a change to, or deletion of, the *target* row re-emits
//!   every document pointing at it); many-to-many (`through`) relations on
//!   either the far or junction table; and tables reachable through multiple
//!   hops of nesting, chained back to the root.
//! - Assembly: column fields (transforms, defaults); belongs_to / has_one /
//!   has_many / many_to_many joins (filters, ordering, limit); joins nested
//!   inside joins; aggregates, including over a junction; boolean and timestamp
//!   soft-delete with optional `when` filters.
//!
//! Relation targets are matched on each table's real primary key, looked up
//! from the Postgres catalog and cached (see [`PgDocumentBuilder::table_primary_key`]).
//! The index's own root key comes from its declared `primary_key`.
//!
//! ## Remaining limits
//!
//! A child-row *delete* on a related table can't be reverse-resolved from a
//! key-only change (the row is already gone); this follows from the thin-event
//! CDC design. Multi-hop reverse resolution issues one query per hop.

mod fields;
mod query;
mod resolve;
pub(crate) mod value;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use schema_core::{
    ColumnName, DatabaseSchema, Filter, IndexMapping, IndexName, IndexSchema, SoftDelete, TableName,
};
use sources_core::document::{Document, DocumentBuilder, DocumentId, IndexScope};
use sources_core::{Catalog, ColumnInfo, Result, RowKey, SnapshotTable, SourceError, SourceSpec};
use sqlx::{PgPool, Row};

use fields::find_paths;

/// Cache of each `(schema, table, column)`'s catalog metadata.
type ColTypeCache = HashMap<(String, String, String), ColumnMeta>;

/// Most keys per batched `build_many` query. Bounds the SQL length and the
/// prepared-statement cache churn from the `IN (…)` list growing with key
/// count; larger id sets are split across several round-trips.
const BUILD_CHUNK: usize = 512;

/// What the Postgres catalog says about a column: its cast-ready SQL type and
/// whether it admits null. Fetched once per column and cached — both the
/// document query (which needs the type to cast operands) and mapping resolution
/// (which needs the type and nullability) read from the same lookup.
#[derive(Debug, Clone)]
struct ColumnMeta {
    sql_type: String,
    nullable: bool,
}

/// Builds index documents from a Postgres database, driven by a [`SourceSpec`] —
/// the enabled indexes and their schemas, translated from the top-level config
/// by the composition root. Cheap to clone — the pool, spec, and primary-key
/// cache are shared.
#[derive(Debug, Clone)]
pub struct PgDocumentBuilder {
    pool: PgPool,
    spec: Arc<SourceSpec>,
    /// Cache of each `(schema, table)`'s single-column primary key.
    pk_cache: Arc<Mutex<HashMap<(String, String), ColumnName>>>,
    /// Cache of each `(schema, table, column)`'s SQL type, used to cast filter
    /// operands to the column's real type rather than comparing as text.
    col_type_cache: Arc<Mutex<ColTypeCache>>,
}

impl PgDocumentBuilder {
    pub fn new(pool: PgPool, spec: Arc<SourceSpec>) -> Self {
        Self {
            pool,
            spec,
            pk_cache: Arc::new(Mutex::new(HashMap::new())),
            col_type_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[tracing::instrument(name = "pg.connect", skip_all, err)]
    pub async fn connect(connection_url: &str, spec: Arc<SourceSpec>) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect(connection_url)
            .await
            .map_err(|e| SourceError::Connection(e.to_string()))?;
        tracing::info!(indexes = spec.indexes().count(), "connected to Postgres");
        Ok(Self::new(pool, spec))
    }

    /// Build a real document for one arbitrary row of an index's root table —
    /// exactly what the sink would write, for previewing a schema against live
    /// data. Picks any row with a non-null primary key, then runs it through the
    /// normal [`build`](DocumentBuilder::build) path. Returns `Ok(None)` when the
    /// index has no single-column primary key or its root table is empty.
    pub async fn sample_document(
        &self,
        index: &IndexName,
    ) -> Result<Option<schema_core::GenericValue>> {
        let schema = self
            .spec
            .schema(index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{index}`")))?;
        let Some(pk_column) = schema.primary_key.clone() else {
            return Ok(None);
        };
        let sql = format!(
            "SELECT \"{pk}\" FROM \"{db_schema}\".\"{table}\" WHERE \"{pk}\" IS NOT NULL LIMIT 1",
            pk = pk_column,
            db_schema = schema.db_schema,
            table = schema.table,
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_optional(&self.pool)
            .await
            .map_err(query_err)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let key_value = value::first_column_to_generic(&row);
        if matches!(key_value, schema_core::GenericValue::Null) {
            return Ok(None);
        }
        let id = DocumentId {
            index: index.clone(),
            key: RowKey(vec![(pk_column, key_value)]),
        };
        match self.build(&id).await? {
            Document::Upsert { body, .. } => Ok(Some(body)),
            Document::Delete { .. } => Ok(None),
        }
    }

    /// The single-column primary key of a table, from the Postgres catalog
    /// (cached). Relations match against this, so a composite or missing
    /// primary key is an error.
    pub(super) async fn table_primary_key(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
    ) -> Result<ColumnName> {
        let cache_key = (schema.to_string(), table.to_string());
        {
            let cache = self.pk_cache.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(column) = cache.get(&cache_key) {
                return Ok(column.clone());
            }
        }
        let column = match self.fetch_primary_key(schema, table).await?.as_slice() {
            [single] => single.clone(),
            [] => {
                return Err(SourceError::Query(format!(
                    "table `{schema}.{table}` has no primary key"
                )));
            }
            _ => {
                return Err(SourceError::Unsupported(format!(
                    "table `{schema}.{table}` has a composite primary key; relations require a single-column key"
                )));
            }
        };
        self.pk_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cache_key, column.clone());
        Ok(column)
    }

    async fn fetch_primary_key(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
    ) -> Result<Vec<ColumnName>> {
        let names = primary_key_column_names(&self.pool, format!("{schema}.{table}")).await?;
        names
            .into_iter()
            .map(|name| {
                ColumnName::try_new(name)
                    .map_err(|e| SourceError::Query(format!("invalid primary key column: {e}")))
            })
            .collect()
    }

    /// Resolve every relation table's primary key up front (cached), so the
    /// document query can correlate and join through them.
    async fn relation_pks(
        &self,
        schema: &schema_core::IndexSchema,
    ) -> Result<HashMap<String, ColumnName>> {
        let mut tables = Vec::new();
        fields::collect_relation_tables(&schema.fields, &mut tables);
        let unique: HashSet<&TableName> = tables.iter().collect();
        let mut pks = HashMap::new();
        for table in unique {
            pks.insert(
                table.to_string(),
                self.table_primary_key(&schema.db_schema, table).await?,
            );
        }
        Ok(pks)
    }

    /// The SQL type of a column, as a cast-ready name from the Postgres catalog
    /// (e.g. `numeric`, `integer`, `timestamp with time zone`). A thin view over
    /// [`column_meta`](Self::column_meta) for callers that only need the type to
    /// cast a query operand.
    pub(super) async fn column_type(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        Ok(self.column_meta(schema, table, column).await?.sql_type)
    }

    /// The Postgres catalog's view of a column — its cast-ready SQL type and
    /// whether it admits null — cached. An unknown column is an error: a field or
    /// filter naming a column that does not exist is a misconfiguration.
    async fn column_meta(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnMeta> {
        let cache_key = (schema.to_string(), table.to_string(), column.to_string());
        {
            let cache = self
                .col_type_cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some(meta) = cache.get(&cache_key) {
                return Ok(meta.clone());
            }
        }
        // `format_type` yields a canonical, re-parseable type name, so it can be
        // dropped straight into a `$n::<type>` cast. `attnotnull` is the column's
        // NOT NULL constraint — the nullability mapping resolution needs, read
        // from the same catalog row as the type.
        let sql = "SELECT format_type(a.atttypid, a.atttypmod) AS sql_type, a.attnotnull AS not_null \
                   FROM pg_attribute a \
                   WHERE a.attrelid = $1::regclass AND a.attname = $2 \
                     AND a.attnum > 0 AND NOT a.attisdropped";
        let row = sqlx::query(sql)
            .bind(format!("{schema}.{table}"))
            .bind(column.as_ref().to_owned())
            .fetch_optional(&self.pool)
            .await
            .map_err(query_err)?;
        let meta = match row {
            Some(row) => {
                let sql_type: String = row.try_get("sql_type").map_err(query_err)?;
                let not_null: bool = row.try_get("not_null").map_err(query_err)?;
                ColumnMeta {
                    sql_type,
                    nullable: !not_null,
                }
            }
            None => {
                return Err(SourceError::UnknownColumn(format!(
                    "{schema}.{table}.{column}"
                )));
            }
        };
        self.col_type_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cache_key, meta.clone());
        Ok(meta)
    }

    /// Resolve the SQL type of every column a value filter compares against,
    /// keyed by `(table, column)`, so the document query can cast each operand
    /// to its column's type. Covers relation filters at any depth and the
    /// root-table columns named by a soft-delete `when`.
    async fn filter_column_types(
        &self,
        schema: &IndexSchema,
    ) -> Result<HashMap<(String, String), String>> {
        let mut columns = Vec::new();
        fields::collect_filter_columns(&schema.fields, &mut columns);

        // Soft-delete `when` filters and root filters run against the root table.
        let when = match &schema.soft_delete {
            Some(SoftDelete::Column(c)) => c.when.as_deref(),
            Some(SoftDelete::Field(f)) => f.when.as_deref(),
            None => None,
        };
        let root_filters = schema.filters.as_deref().unwrap_or_default();
        for filter in when.unwrap_or_default().iter().chain(root_filters) {
            if let Filter::ValueOp(value_op) = filter {
                columns.push((&schema.table, &value_op.column));
            }
        }

        let mut types = HashMap::new();
        for (table, column) in columns {
            let key = (table.to_string(), column.to_string());
            if types.contains_key(&key) {
                continue;
            }
            let sql_type = self.column_type(&schema.db_schema, table, column).await?;
            types.insert(key, sql_type);
        }
        Ok(types)
    }

    /// Ensure `types` carries the catalog SQL type of each root-table key column,
    /// so the keyed lookup can cast every `$n` to it. The keys come back from
    /// Postgres as values (a `uuid` as a string, say); without the cast the
    /// re-bound `$n` is `text` and `uuid = text` has no operator.
    async fn add_key_column_types(
        &self,
        schema: &IndexSchema,
        columns: &[&ColumnName],
        types: &mut HashMap<(String, String), String>,
    ) -> Result<()> {
        for column in columns {
            let key = (schema.table.to_string(), column.to_string());
            if types.contains_key(&key) {
                continue;
            }
            let sql_type = self
                .column_type(&schema.db_schema, &schema.table, column)
                .await?;
            types.insert(key, sql_type);
        }
        Ok(())
    }
}

/// The Postgres source's view of its own catalog. The index mapping is derived
/// from the self-describing schema in [`schema_core`]; this is the one
/// store-specific piece used for *validation* — how Postgres types and
/// constrains a column — so a declared schema can be checked against the live
/// database.
#[async_trait]
impl Catalog for PgDocumentBuilder {
    async fn column(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<ColumnInfo> {
        let meta = self.column_meta(schema, table, column).await?;
        Ok(ColumnInfo {
            sql_type: meta.sql_type,
            nullable: meta.nullable,
        })
    }
}

#[async_trait]
impl DocumentBuilder for PgDocumentBuilder {
    #[tracing::instrument(
        name = "pg.resolve",
        level = "debug",
        skip_all,
        fields(table = table.as_ref()),
        err,
    )]
    async fn resolve(&self, table: &TableName, key: &RowKey) -> Result<Vec<DocumentId>> {
        let mut ids = Vec::new();
        for (name, schema) in self.spec.indexes() {
            if schema.table == *table {
                ids.push(DocumentId {
                    index: name.clone(),
                    key: key.clone(),
                });
                continue;
            }

            let mut paths = Vec::new();
            let mut prefix = Vec::new();
            find_paths(&schema.fields, table, &mut prefix, &mut paths);
            if paths.is_empty() {
                continue;
            }
            let Some(pk_column) = schema.primary_key.clone() else {
                tracing::warn!(
                    index = %name, table = %table,
                    "cannot reverse-resolve: index has no primary_key",
                );
                continue;
            };

            let mut seen = HashSet::new();
            for path in &paths {
                for root in self.resolve_path(schema, table, key, path).await? {
                    if seen.insert(root.clone()) {
                        ids.push(DocumentId {
                            index: name.clone(),
                            key: RowKey(vec![(pk_column.clone(), root)]),
                        });
                    }
                }
            }
        }
        tracing::trace!(documents = ids.len(), "resolved affected documents");
        Ok(ids)
    }

    #[tracing::instrument(
        name = "pg.build",
        level = "debug",
        skip_all,
        fields(index = id.index.as_ref()),
        err,
    )]
    async fn build(&self, id: &DocumentId) -> Result<Document> {
        let schema = self
            .spec
            .schema(&id.index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{}`", id.index)))?;

        let pks = self.relation_pks(schema).await?;
        let mut col_types = self.filter_column_types(schema).await?;
        let key_columns: Vec<&ColumnName> = id.key.0.iter().map(|(column, _)| column).collect();
        self.add_key_column_types(schema, &key_columns, &mut col_types)
            .await?;
        let (sql, params) = query::document_query(schema, &id.key.0, &pks, &col_types)?;

        let mut statement = sqlx::query(sql);
        for param in &params {
            statement = query::bind_param(statement, param)?;
        }
        let row = statement
            .fetch_optional(&self.pool)
            .await
            .map_err(query_err)?;

        // No row means the root is absent or soft-deleted (both folded into the
        // query's WHERE) → the document should not exist.
        match row {
            None => Ok(Document::Delete { id: id.clone() }),
            Some(row) => {
                let document: serde_json::Value = row.try_get("document").map_err(query_err)?;
                Ok(Document::Upsert {
                    id: id.clone(),
                    body: value::coerce_document(document, &schema.fields),
                })
            }
        }
    }

    #[tracing::instrument(name = "pg.build_many", level = "debug", skip_all, fields(ids = ids.len()))]
    async fn build_many(&self, ids: &[DocumentId]) -> Result<Vec<Document>> {
        let mut by_index: HashMap<&IndexName, Vec<&DocumentId>> = HashMap::new();
        for id in ids {
            by_index.entry(&id.index).or_default().push(id);
        }

        let mut out = Vec::with_capacity(ids.len());
        for (index_name, group) in by_index {
            let schema = self
                .spec
                .schema(index_name)
                .ok_or_else(|| SourceError::Query(format!("unknown index `{index_name}`")))?;

            // The batched query keys the root with `IN (…)` on a single column,
            // so it needs both a declared single-column primary key and ids that
            // carry exactly that one key column. Pair each id with its lone key
            // value; if any id is composite (or the index has no `primary_key`),
            // fall back to per-document assembly for this group — correct, just
            // not batched.
            let keyed: Option<Vec<(&schema_core::GenericValue, &DocumentId)>> = group
                .iter()
                .map(|id| match id.key.0.as_slice() {
                    [(_, value)] => Some((value, *id)),
                    _ => None,
                })
                .collect();
            let (Some(pk_column), Some(keyed)) = (schema.primary_key.clone(), keyed) else {
                for id in group {
                    out.push(self.build(id).await?);
                }
                continue;
            };

            let pks = self.relation_pks(schema).await?;
            let mut col_types = self.filter_column_types(schema).await?;
            self.add_key_column_types(schema, &[&pk_column], &mut col_types)
                .await?;

            for chunk in keyed.chunks(BUILD_CHUNK) {
                let keys: Vec<schema_core::GenericValue> =
                    chunk.iter().map(|(value, _)| (*value).clone()).collect();
                let (sql, params) =
                    query::documents_query(schema, &pk_column, &keys, &pks, &col_types)?;

                let mut statement = sqlx::query(sql);
                for param in &params {
                    statement = query::bind_param(statement, param)?;
                }
                let rows = statement.fetch_all(&self.pool).await.map_err(query_err)?;

                // Map each returned root key to its assembled body. `doc_key` is
                // the first column, decoded through the same path live-change
                // keys take, so it matches the ids' key values exactly.
                let mut bodies: HashMap<schema_core::GenericValue, schema_core::GenericValue> =
                    HashMap::with_capacity(rows.len());
                for row in &rows {
                    let key = value::first_column_to_generic(row);
                    let document: serde_json::Value = row.try_get("document").map_err(query_err)?;
                    bodies.insert(key, value::coerce_document(document, &schema.fields));
                }

                // Every requested id yields an outcome: a body present in the
                // result is an upsert; an absent key means the root is gone or
                // soft-deleted (both fold into the query's WHERE) → a tombstone.
                for (value, id) in chunk {
                    let document = match bodies.remove(*value) {
                        Some(body) => Document::Upsert {
                            id: (*id).clone(),
                            body,
                        },
                        None => Document::Delete { id: (*id).clone() },
                    };
                    out.push(document);
                }
            }
        }
        Ok(out)
    }

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        // A document is keyed by its root row, so the root table alone seeds the
        // whole index — `build` assembles the joins and aggregates per root row.
        self.spec
            .indexes()
            .map(|(name, schema)| IndexScope {
                index: name.clone(),
                root: SnapshotTable {
                    db_schema: schema.db_schema.clone(),
                    table: schema.table.clone(),
                },
            })
            .collect()
    }

    async fn index_mappings(&self) -> Result<Vec<IndexMapping>> {
        // The schema is self-describing, so the mapping is projected from it
        // without touching the database.
        Ok(self.spec.index_mappings())
    }
}

pub(super) fn query_err(error: sqlx::Error) -> SourceError {
    SourceError::Query(error.to_string())
}

/// Primary-key column names of a table, in index order. `$1` binds the
/// qualified `schema.table` (cast to `regclass`).
pub(crate) const PRIMARY_KEY_SQL: &str = "SELECT a.attname AS name \
     FROM pg_index i \
     JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
     WHERE i.indrelid = $1::regclass AND i.indisprimary";

/// Fetch the raw primary-key column-name strings for the table `qualified`
/// names (e.g. `"public"."users"` or `public.users`). Callers apply their own
/// policy for an invalid name, a missing key, or a composite key.
pub(crate) async fn primary_key_column_names(
    pool: &PgPool,
    qualified: String,
) -> Result<Vec<String>> {
    let rows = sqlx::query(PRIMARY_KEY_SQL)
        .bind(qualified)
        .fetch_all(pool)
        .await
        .map_err(query_err)?;
    let mut names = Vec::with_capacity(rows.len());
    for row in &rows {
        names.push(row.try_get::<String, _>("name").map_err(query_err)?);
    }
    Ok(names)
}
