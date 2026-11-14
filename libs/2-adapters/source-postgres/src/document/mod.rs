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
//!   either the far or junction table; tables reachable through multiple
//!   hops of nesting, chained back to the root; and self-joins, where a
//!   root-table row is also some other document's related row — it resolves to
//!   its own id plus every document that embeds it, to the declared depth.
//! - Assembly: column fields (transforms, defaults); belongs_to / has_one /
//!   has_many / many_to_many joins (filters, ordering, limit); joins nested
//!   inside joins; aggregates, including over a junction; boolean and timestamp
//!   soft-delete with optional `when` filters.
//!
//! Relation targets are matched on each table's real primary key, looked up
//! from the Postgres catalog and cached (see [`PgDocumentBuilder::table_primary_key`]).
//! The index's own root key comes from its declared `primary_key`.
//!
//! ## Deleted and re-parented children
//!
//! A child row that holds its parent's key (`has_one`/`has_many`, or a junction
//! row) is traced to its *old* parent through the change's pre-image — the
//! current row is gone or names the new parent. That needs the table's replica
//! identity to carry the link column; see [`resolve`].
//!
//! ## Updates that change nothing a document reads
//!
//! An update carries a pre- and post-image when the table's replica identity
//! is `FULL`. Resolution then compares them: the root row's own document is
//! emitted only if a column the index reads from the root changed, and a path
//! back from a related row is walked only if a column that relation reads
//! changed (fields, filters, `order_by`, aggregate input, link; see
//! [`source::relation_columns`]). Without both images every path is walked,
//! and an index with a raw filter anywhere walks every path. The column sets
//! are derived once, when the builder is built.
//!
//! ## Remaining limits
//!
//! Multi-hop reverse resolution issues one query per hop.

mod fields;
mod query;
mod reads;
mod resolve;
use resolve::RootLinks;
pub(crate) mod value;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use kernel::{
    ColumnName, DatabaseSchema, Filter, IndexMapping, IndexName, IndexSchema, SoftDelete,
    TableName, Transform,
};
use source::document::{BuildFailure, Document, DocumentBuilder, DocumentId, IndexScope};
use source::{
    Catalog, ColumnInfo, Result, RowDiff, RowImage, RowKey, SnapshotTable, SourceError, SourceSpec,
};
use sqlx::{PgPool, Row};

use fields::find_paths;
use reads::ReadColumns;

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
    /// A domain's type resolved through every domain; `None` for no domain.
    base_type: Option<String>,
    /// The `pg_type.typname` of the type, through every domain.
    base_name: String,
    /// Whether the column's collation compares by identity (always, for a
    /// type without one).
    deterministic: bool,
    nullable: bool,
}

impl ColumnMeta {
    /// Whether two values that compare equal always render the same text, so
    /// one key value has one document id. Not so for a `numeric` (`1.5` and
    /// `1.50`), a float (`0` and `-0`), a `citext` or a nondeterministic
    /// collation (`Ab` and `AB`).
    fn equal_is_identical(&self) -> bool {
        self.deterministic
            && !matches!(
                self.base_name.as_str(),
                "numeric" | "float4" | "float8" | "citext"
            )
    }
}

/// One root row's document as [`PgDocumentBuilder::document_by_id`] and
/// [`PgDocumentBuilder::sample_document`] build it: the body, or why it could
/// not be finished.
pub type Built = std::result::Result<kernel::GenericValue, BuildFailure>;

/// A built document's body or failure; `None` for a tombstone.
fn built(document: Document) -> Option<Built> {
    match document {
        Document::Upsert { body, .. } => Some(Ok(body)),
        Document::Failed { failure, .. } => Some(Err(failure)),
        Document::Delete { .. } => None,
    }
}

/// Builds index documents from a Postgres database, driven by a [`SourceSpec`] —
/// the enabled indexes and their schemas, translated from the top-level config
/// by the composition root. Cheap to clone — the pool, spec, and primary-key
/// cache are shared.
#[derive(Debug, Clone)]
pub struct PgDocumentBuilder {
    pool: PgPool,
    spec: Arc<SourceSpec>,
    /// The columns each index reads, per part, derived from `spec` once.
    reads: Arc<ReadColumns>,
    /// Cache of each `(schema, table)`'s single-column primary key.
    pk_cache: Arc<Mutex<HashMap<(String, String), ColumnName>>>,
    /// Cache of each `(schema, table, column)`'s SQL type, used to cast filter
    /// operands to the column's real type rather than comparing as text.
    col_type_cache: Arc<Mutex<ColTypeCache>>,
    /// Whether each `(link type, key type)` pair compares, probed once.
    comparison_cache: Arc<Mutex<HashMap<(String, String), bool>>>,
    /// Tables a change arrived for that no index reads, already warned about.
    unread_warned: Arc<Mutex<HashSet<TableName>>>,
    /// Root tables without a primary key already warned about for a key with
    /// a null part.
    keyless_warned: Arc<Mutex<HashSet<TableName>>>,
}

impl PgDocumentBuilder {
    /// Over a pool whose connections read `bytea` as hex text, as
    /// [`connect`](Self::connect)'s do.
    pub(crate) fn new(pool: PgPool, spec: Arc<SourceSpec>) -> Self {
        Self {
            pool,
            reads: Arc::new(ReadColumns::new(&spec)),
            spec,
            pk_cache: Arc::new(Mutex::new(HashMap::new())),
            col_type_cache: Arc::new(Mutex::new(HashMap::new())),
            comparison_cache: Arc::new(Mutex::new(HashMap::new())),
            unread_warned: Arc::new(Mutex::new(HashSet::new())),
            keyless_warned: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Warn, once per table, that a change arrived for a table no index reads —
    /// so a change streamed under an unexpected name (a partition's, say) isn't
    /// dropped silently.
    fn warn_unread(&self, table: &TableName) {
        let first = self
            .unread_warned
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(table.clone());
        if first {
            tracing::warn!(
                table = %table,
                "a change arrived for a table no index reads and was dropped; if it is a \
                 partition, publish its partitioned table via the root \
                 (publish_via_partition_root)",
            );
        }
    }

    /// Warn, once per table, that a root row keyed by something other than the
    /// index's `primary_key` has a null in its key. That is a table without a
    /// primary key: its changes are keyed by the replica identity's columns
    /// (every column under `FULL`), where a null is ordinary row data, and a
    /// key with a null addresses no document. A key that *is* the primary key
    /// with a null in it is a change feed that left it out, which the stream
    /// already warned about.
    fn warn_keyless_root(&self, schema: &IndexSchema, table: &TableName, key: &RowKey) {
        let keyed_by_primary_key = matches!(
            key.0.as_slice(),
            [(column, _)] if Some(column) == schema.primary_key.as_ref()
        );
        if keyed_by_primary_key {
            return;
        }
        let first = self
            .keyless_warned
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(table.clone());
        if first {
            tracing::warn!(
                table = %table,
                remediation = "give the table a primary key",
                "a change on a root table without a primary key has a NULL in a column that \
                 keys it, so it addresses no document; that row's document is not built or \
                 updated",
            );
        }
    }

    #[tracing::instrument(name = "pg.connect", skip_all, err)]
    pub async fn connect(connection_url: &str, spec: Arc<SourceSpec>) -> Result<Self> {
        // The document query renders a `bytea` as text inside its JSON, which
        // `value::postgres_hex` decodes. A database or role set to
        // `bytea_output = 'escape'` would render the escape text instead.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::Executor::execute(&mut *connection, "SET bytea_output = 'hex'").await?;
                    Ok(())
                })
            })
            .connect(connection_url)
            .await
            .map_err(|e| SourceError::Connection(e.to_string()))?;
        tracing::info!(indexes = spec.indexes().count(), "connected to Postgres");
        Ok(Self::new(pool, spec))
    }

    /// Build the document of the root row whose primary key reads as `id` (typed
    /// by [`RowKey::from_text`]), through the normal [`build`](DocumentBuilder::build)
    /// path. `Ok(None)` when the row is missing or soft-deleted, `Some(Err(_))`
    /// when the row's document fails to build; an error when the index has no
    /// single-column primary key.
    pub async fn document_by_id(&self, index: &IndexName, id: &str) -> Result<Option<Built>> {
        let primary_key = self
            .spec
            .schema(index)
            .and_then(|schema| schema.primary_key.clone())
            .ok_or_else(|| {
                SourceError::Query(format!("index `{index}` has no single-column primary key"))
            })?;
        let document = DocumentId {
            index: index.clone(),
            key: RowKey::from_text(primary_key, id),
        };
        Ok(built(self.build(&document).await?))
    }

    /// Build a real document for one arbitrary row of an index's root table —
    /// exactly what the sink would write, for previewing a schema against live
    /// data. Picks any row with a non-null primary key, then runs it through the
    /// normal [`build`](DocumentBuilder::build) path. Returns `Ok(None)` when the
    /// index has no single-column primary key or its root table is empty, and
    /// `Some(Err(_))` when the row's document fails to build.
    pub async fn sample_document(&self, index: &IndexName) -> Result<Option<Built>> {
        let schema = self
            .spec
            .schema(index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{index}`")))?;
        let Some(pk_column) = schema.primary_key.clone() else {
            return Ok(None);
        };
        let pk_base = self
            .base_name(&schema.db_schema, &schema.table, &pk_column)
            .await?;
        let sql = format!(
            "SELECT {key} FROM \"{db_schema}\".\"{table}\" WHERE \"{pk}\" IS NOT NULL LIMIT 1",
            key = value::key_select(&format!("\"{pk_column}\""), "key", &pk_base),
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
        let key_value = value::decode_key(&row, "key");
        if matches!(key_value, kernel::GenericValue::Null) {
            return Ok(None);
        }
        let id = DocumentId {
            index: index.clone(),
            key: RowKey(vec![(pk_column, key_value)]),
        };
        Ok(built(self.build(&id).await?))
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
        schema: &kernel::IndexSchema,
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
    /// The `pg_type.typname` of `column`'s type, through every domain.
    pub(super) async fn base_name(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        Ok(self.column_meta(schema, table, column).await?.base_name)
    }

    pub(super) async fn column_type(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        Ok(self.column_meta(schema, table, column).await?.sql_type)
    }

    /// The Postgres catalog's view of a column — its cast-ready SQL type and
    /// whether it admits null — cached. An unknown column (or table) is an
    /// error: a field or filter naming one that does not exist is a
    /// misconfiguration.
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
        // from the same catalog row as the type. `base_type` follows a domain
        // through `typbasetype` to the first type that is no domain, and
        // `base_name` names that type. `collisdeterministic` is read through
        // `to_jsonb` because Postgres before 12 has no such column (and only
        // deterministic collations).
        let sql = "SELECT format_type(a.atttypid, a.atttypmod) AS sql_type, \
                          b.base_type, b.base_name, \
                          coalesce((SELECT (to_jsonb(c) ->> 'collisdeterministic')::boolean \
                                    FROM pg_collation c WHERE c.oid = a.attcollation), \
                                   true) AS deterministic, \
                          a.attnotnull AS not_null \
                   FROM pg_attribute a, LATERAL ( \
                          WITH RECURSIVE d(oid, typmod) AS ( \
                               SELECT a.atttypid, a.atttypmod \
                             UNION ALL \
                               SELECT t.typbasetype, t.typtypmod \
                               FROM pg_type t JOIN d ON t.oid = d.oid WHERE t.typtype = 'd' \
                          ) SELECT format_type(d.oid, d.typmod) AS base_type, \
                                   t.typname::text AS base_name \
                            FROM d JOIN pg_type t ON t.oid = d.oid WHERE t.typtype <> 'd' \
                        ) b \
                   WHERE a.attrelid = to_regclass($1) AND a.attname = $2 \
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
                let base_type: Option<String> = row.try_get("base_type").map_err(query_err)?;
                let not_null: bool = row.try_get("not_null").map_err(query_err)?;
                ColumnMeta {
                    base_type: base_type.filter(|base| *base != sql_type),
                    base_name: row.try_get("base_name").map_err(query_err)?,
                    deterministic: row.try_get("deterministic").map_err(query_err)?,
                    sql_type,
                    nullable: !not_null,
                }
            }
            None if !self.table_exists(schema, table).await? => {
                return Err(SourceError::UnknownTable(format!("{schema}.{table}")));
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

    /// Whether `schema.table` names a relation, resolved the way a column
    /// lookup resolves it.
    async fn table_exists(&self, schema: &DatabaseSchema, table: &TableName) -> Result<bool> {
        sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("{schema}.{table}"))
            .fetch_one(&self.pool)
            .await
            .map_err(query_err)
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

impl PgDocumentBuilder {
    /// One change's documents. A path that ends on a link's value, which may
    /// spell the root key differently, adds it to `links` instead (see
    /// [`reads_root_from_a_link`](Self::reads_root_from_a_link)).
    #[tracing::instrument(
        name = "pg.resolve",
        level = "debug",
        skip_all,
        fields(table = table.as_ref()),
        err,
    )]
    async fn resolve_change(
        &self,
        table: &TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
        links: &mut RootLinks,
    ) -> Result<Vec<DocumentId>> {
        let mut ids = Vec::new();
        let mut read = false;
        let mut skipped = 0_usize;
        let mut unaddressed = 0_usize;
        let diff = RowDiff::new(key, before, after);
        for (name, schema) in self.spec.indexes() {
            let is_root = schema.table == *table;
            let mut seen = HashSet::new();
            if is_root {
                read = true;
                if key.has_null() {
                    unaddressed += 1;
                    self.warn_keyless_root(schema, table, key);
                } else if self.reads.root(name).touched_by(&diff) {
                    ids.push(DocumentId {
                        index: name.clone(),
                        key: key.clone(),
                    });
                    // A row that is its own ancestor must not re-emit this id.
                    if let (Some(pk_column), [(column, own)]) =
                        (&schema.primary_key, key.0.as_slice())
                        && column == pk_column
                    {
                        seen.insert(own.clone());
                    }
                } else {
                    skipped += 1;
                }
            }

            let mut paths = Vec::new();
            let mut prefix = Vec::new();
            find_paths(&schema.fields, table, &mut prefix, &mut paths);
            if paths.is_empty() {
                continue;
            }
            read = true;
            let Some(pk_column) = schema.primary_key.clone() else {
                tracing::warn!(
                    index = %name, table = %table,
                    "cannot reverse-resolve: index has no primary_key",
                );
                continue;
            };

            let mut roots = Vec::new();
            for (n, path) in paths.iter().enumerate() {
                if !self.reads.path(name, table, n).touched_by(&diff) {
                    skipped += 1;
                    continue;
                }
                let found = self.resolve_path(schema, table, key, before, path).await?;
                if let Some(link) = self
                    .reads_root_from_a_link(schema, &pk_column, path)
                    .await?
                {
                    links.add(name, link, found);
                } else {
                    roots.extend(found);
                }
            }
            for root in roots {
                if seen.insert(root.clone()) {
                    ids.push(DocumentId {
                        index: name.clone(),
                        key: RowKey(vec![(pk_column.clone(), root)]),
                    });
                }
            }
        }
        if skipped > 0 {
            tracing::trace!(skipped, "update changed no column these documents read");
        }
        if unaddressed > 0 {
            tracing::debug!(
                unaddressed,
                "a change whose key has a null part addresses no document of its own"
            );
        }
        if !read {
            self.warn_unread(table);
        }
        tracing::trace!(documents = ids.len(), "resolved affected documents");
        Ok(ids)
    }

    /// `ids` plus the documents `links` name, read back as the root rows' own
    /// keys, deduplicated.
    async fn with_root_keys(
        &self,
        ids: Vec<DocumentId>,
        links: RootLinks,
    ) -> Result<Vec<DocumentId>> {
        let mut seen = HashSet::new();
        let mut out: Vec<DocumentId> = ids
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        for id in self.read_root_links(links).await? {
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
        Ok(out)
    }
}

/// The Postgres source's view of its own catalog. The index mapping is derived
/// from the self-describing schema in [`kernel`]; this is the one
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
            base_type: meta.base_type,
            nullable: meta.nullable,
        })
    }

    async fn has_table(&self, schema: &DatabaseSchema, table: &TableName) -> Result<bool> {
        self.table_exists(schema, table).await
    }

    async fn primary_key(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
    ) -> Result<Option<Vec<ColumnName>>> {
        self.fetch_primary_key(schema, table).await.map(Some)
    }

    async fn reads_key(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<bool> {
        let meta = self.column_meta(schema, table, column).await?;
        Ok(value::KEY_TYPES.contains(&meta.base_name.as_str()))
    }

    async fn accepts_transforms(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
        transforms: &[Transform],
        element: bool,
    ) -> Result<bool> {
        let meta = self.column_meta(schema, table, column).await?;
        let sql_type = if element {
            element_type(meta.base_type.as_deref().unwrap_or(&meta.sql_type))
        } else {
            meta.sql_type
        };
        let probe = query::transform_probe(&sql_type, transforms);
        self.probe(probe, || {
            format!("could not check `{table}.{column}`'s transforms against its type")
        })
        .await
    }

    async fn compares(
        &self,
        schema: &DatabaseSchema,
        (link_table, link_column): (&TableName, &ColumnName),
        (key_table, key_column): (&TableName, &ColumnName),
    ) -> Result<bool> {
        let link = self.column_meta(schema, link_table, link_column).await?;
        let key = self.column_meta(schema, key_table, key_column).await?;
        let pair = (link.sql_type, key.sql_type);
        let cached = self
            .comparison_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&pair)
            .copied();
        if let Some(compares) = cached {
            return Ok(compares);
        }
        let probe = query::comparison_probe(&pair.0, &pair.1);
        let compares = self
            .probe(probe, || {
                format!(
                    "could not check `{link_table}.{link_column}` against \
                     `{key_table}.{key_column}`"
                )
            })
            .await?;
        self.comparison_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(pair, compares);
        Ok(compares)
    }
}

impl PgDocumentBuilder {
    /// Run a table-free `probe`, in one round trip (the simple protocol, no
    /// prepared statement): `false` when Postgres has no function or
    /// operator for it. The probe casts to a column's type, which needs USAGE
    /// on the type's schema; reading the column does not. So only "no such
    /// function" is a verdict, and any other refusal leaves it unknown
    /// (`true`, the check skipped, `what` logged).
    async fn probe(&self, probe: query::SqlString, what: impl FnOnce() -> String) -> Result<bool> {
        match sqlx::raw_sql(probe).execute(&self.pool).await {
            Ok(_) => Ok(true),
            Err(sqlx::Error::Database(error))
                if matches!(
                    error.code().as_deref(),
                    Some(UNDEFINED_FUNCTION | AMBIGUOUS_FUNCTION)
                ) =>
            {
                Ok(false)
            }
            Err(sqlx::Error::Database(error)) => {
                tracing::warn!(
                    error = %error.message(),
                    "{}; skipping the check", what()
                );
                Ok(true)
            }
            Err(error) => Err(query_err(error)),
        }
    }
}

/// SQLSTATE `42883`: no function takes the argument's type.
const UNDEFINED_FUNCTION: &str = "42883";
/// SQLSTATE `42725`: several functions could take it.
const AMBIGUOUS_FUNCTION: &str = "42725";

#[async_trait]
impl DocumentBuilder for PgDocumentBuilder {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> Result<Vec<DocumentId>> {
        let mut links = RootLinks::default();
        let ids = self
            .resolve_change(table, key, before, after, &mut links)
            .await?;
        self.with_root_keys(ids, links).await
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
                Ok(match value::coerce_document(document, &schema.fields) {
                    Ok(body) => Document::Upsert {
                        id: id.clone(),
                        body,
                    },
                    Err(failure) => Document::Failed {
                        id: id.clone(),
                        failure,
                    },
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
            let keyed: Option<Vec<(&kernel::GenericValue, &DocumentId)>> = group
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
            let pk_base = self
                .base_name(&schema.db_schema, &schema.table, &pk_column)
                .await?;

            for chunk in keyed.chunks(BUILD_CHUNK) {
                let keys: Vec<kernel::GenericValue> =
                    chunk.iter().map(|(value, _)| (*value).clone()).collect();
                let (sql, params) =
                    query::documents_query(schema, &pk_column, &pk_base, &keys, &pks, &col_types)?;

                let mut statement = sqlx::query(sql);
                for param in &params {
                    statement = query::bind_param(statement, param)?;
                }
                let rows = statement.fetch_all(&self.pool).await.map_err(query_err)?;

                // Map each returned root key to its assembled body. `doc_key` is
                // parsed as live-change keys are, so it matches the ids' key
                // values exactly.
                let mut bodies: HashMap<
                    kernel::GenericValue,
                    std::result::Result<kernel::GenericValue, source::document::BuildFailure>,
                > = HashMap::with_capacity(rows.len());
                for row in &rows {
                    let key = value::decode_key(row, query::DOC_KEY);
                    let document: serde_json::Value = row.try_get("document").map_err(query_err)?;
                    bodies.insert(key, value::coerce_document(document, &schema.fields));
                }

                // Every requested id yields an outcome: a body present in the
                // result is an upsert; an absent key means the root is gone or
                // soft-deleted (both fold into the query's WHERE) → a tombstone.
                for (value, id) in chunk {
                    let document = match bodies.remove(*value) {
                        Some(Ok(body)) => Document::Upsert {
                            id: (*id).clone(),
                            body,
                        },
                        Some(Err(failure)) => Document::Failed {
                            id: (*id).clone(),
                            failure,
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

/// The type of an array column's elements, as a transform probe casts to:
/// the `[]` column's base type, or `text` for a `json`/`jsonb` column, whose
/// string elements a document query transforms as text.
fn element_type(column_type: &str) -> String {
    match column_type.trim().strip_suffix("[]") {
        Some(base) => base.trim().to_owned(),
        None => "text".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
