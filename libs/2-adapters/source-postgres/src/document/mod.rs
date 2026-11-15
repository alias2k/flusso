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
mod matching;
mod query;
mod reads;
mod resolve;
use resolve::RootLinks;
pub(crate) mod value;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

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

use reads::ReadColumns;

/// Each `schema` → `table` → `column`'s catalog metadata, nested so a lookup
/// borrows the names instead of building a key.
type ColTypeCache = HashMap<String, HashMap<String, HashMap<String, Arc<ColumnMeta>>>>;

/// The document builder's cache of column types, which shapes every document
/// query it writes. Cheap to clone; clones share it.
///
/// A table's types can change while flusso runs (`ALTER COLUMN … TYPE`), so
/// entries are dropped per table: when a document query fails (its retry
/// looks the types up again), and when the WAL capture this is handed to
/// reads a `Relation` message (sent after a table's definition changes)
/// whose column types differ from the cached ones.
#[derive(Debug, Clone, Default)]
pub struct ColumnTypes(Arc<CachedTypes>);

#[derive(Debug, Default)]
struct CachedTypes {
    columns: Mutex<ColTypeCache>,
    /// Bumped every time a table's entries are dropped.
    generation: AtomicU64,
}

impl ColumnTypes {
    fn get(&self, schema: &str, table: &str, column: &str) -> Option<Arc<ColumnMeta>> {
        self.lock().get(schema)?.get(table)?.get(column).cloned()
    }

    fn insert(&self, schema: &str, table: &str, column: &str, meta: Arc<ColumnMeta>) {
        self.lock()
            .entry(schema.to_owned())
            .or_default()
            .entry(table.to_owned())
            .or_default()
            .insert(column.to_owned(), meta);
    }

    /// How many times cached entries were dropped so far. An answer derived
    /// from cached types is current while this has not moved since.
    fn generation(&self) -> u64 {
        self.0.generation.load(Ordering::Acquire)
    }

    /// Forget every cached column of `schema.table`.
    pub(crate) fn forget_table(&self, schema: &str, table: &str) {
        let mut columns = self.lock();
        if let Some(tables) = columns.get_mut(schema) {
            tables.remove(table);
        }
        self.0.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Forget `schema.table`'s cached columns when one of them has another
    /// type or size in `columns` (each name with its type OID and modifier,
    /// as a `Relation` lists them). A table whose types are unchanged keeps its entries, so a
    /// stream's first `Relation` costs no lookup. A cached column `columns`
    /// leaves out is no evidence: pgoutput omits stored generated columns
    /// and columns outside a publication's column list from every `Relation`.
    pub(crate) fn forget_changed<'a>(
        &self,
        schema: &str,
        table: &str,
        columns: impl Iterator<Item = (&'a str, u32, i32)> + Clone,
    ) {
        let changed = self
            .lock()
            .get(schema)
            .and_then(|tables| tables.get(table))
            .is_some_and(|cached| {
                columns.clone().any(|(name, type_oid, type_mod)| {
                    cached
                        .get(name)
                        .is_some_and(|meta| meta.type_oid != type_oid || meta.type_mod != type_mod)
                })
            });
        if changed {
            self.forget_table(schema, table);
        }
    }

    /// Cache an `integer` column of `schema.table` of type `type_oid`, for
    /// tests elsewhere in the crate and the `resolve` bench.
    #[cfg(any(test, feature = "bench"))]
    pub(crate) fn cache(&self, schema: &str, table: &str, column: &str, type_oid: u32) {
        self.insert(
            schema,
            table,
            column,
            Arc::new(ColumnMeta {
                sql_type: "integer".to_owned(),
                type_oid,
                type_mod: -1,
                key_type: "pg_catalog.int4".to_owned(),
                base_type: None,
                base_name: "int4".to_owned(),
                element_name: None,
                deterministic: true,
                nullable: true,
            }),
        );
    }

    /// Whether any column of `schema.table` is cached.
    #[cfg(test)]
    pub(crate) fn caches(&self, schema: &str, table: &str) -> bool {
        self.lock()
            .get(schema)
            .and_then(|tables| tables.get(table))
            .is_some_and(|columns| !columns.is_empty())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ColTypeCache> {
        self.0
            .columns
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Most keys per batched `build_many` query. The keys travel as one array
/// parameter per kind, so the statement's text no longer grows with them;
/// this bounds what one round-trip builds and holds. Larger id sets are split
/// across several.
const BUILD_CHUNK: usize = 512;

/// What the Postgres catalog says about a column: its cast-ready SQL type and
/// whether it admits null. Fetched once per column and cached — both the
/// document query (which needs the type to cast operands) and mapping resolution
/// (which needs the type and nullability) read from the same lookup.
#[derive(Debug, Clone)]
struct ColumnMeta {
    sql_type: String,
    /// `pg_attribute.atttypid`: the type OID a `Relation` message names the
    /// column by.
    type_oid: u32,
    /// `pg_attribute.atttypmod`: the size a `Relation` message names beside
    /// the type (`varchar(5)`, `numeric(10,2)`); `-1` for none.
    type_mod: i32,
    /// The type a key read from this column is cast to, schema-qualified and
    /// never with a modifier (see [`key_type`](PgDocumentBuilder::key_type)).
    key_type: String,
    /// A domain's type resolved through every domain; `None` for no domain.
    base_type: Option<String>,
    /// The `pg_type.typname` of the type, through every domain.
    base_name: String,
    /// For an array, the `pg_type.typname` of its element type, through every
    /// domain; `None` for any other type.
    element_name: Option<String>,
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

/// What a batched build reads from the catalog: relation keys, column types,
/// the columns read as bytes, whether the root key is unique, and the root
/// key column itself.
#[derive(Debug)]
struct BatchCatalog {
    pks: HashMap<String, ColumnName>,
    col_types: HashMap<(String, String), String>,
    bytes: query::ByteColumns,
    root_key: query::RootKey,
    key: Arc<ColumnMeta>,
    partitioned: bool,
}

impl BatchCatalog {
    /// Whether `chunk`'s keys pair with their rows by value
    /// ([`matching::pairs_by_key`]).
    fn pairs_by_key(&self, chunk: &[(&kernel::GenericValue, &DocumentId)]) -> bool {
        self.key.equal_is_identical()
            && matching::pairs_by_key(&self.key.base_name, chunk.iter().map(|(key, _)| *key))
    }
}

/// One chunk of a batched build: the index, its schema and root key column,
/// and the keys with the ids they were requested under.
#[derive(Debug)]
struct Chunk<'a> {
    index_name: &'a IndexName,
    schema: &'a IndexSchema,
    pk_column: &'a ColumnName,
    keys: &'a [(&'a kernel::GenericValue, &'a DocumentId)],
}

/// How a chunk showed the cached key type stale.
#[derive(Debug, Clone, Copy)]
enum Stale {
    /// A row equals no requested key once read back.
    RowUnclaimed,
    /// Postgres refused the prepared statement: its result type changed
    /// under it (a key column's type or collation changed).
    ResultType,
}

impl Stale {
    fn reason(self) -> &'static str {
        match self {
            Self::RowUnclaimed => "a row equals no requested key once read back",
            Self::ResultType => "the prepared statement's result type changed",
        }
    }
}

/// Whether Postgres refused a prepared statement because its result type
/// changed under it (`0A000`, "cached plan must not change result type").
fn changed_result_type(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|error| {
        error.code().as_deref() == Some(FEATURE_NOT_SUPPORTED)
            && error
                .message()
                .contains("cached plan must not change result type")
    })
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

type ComparisonCache = HashMap<(String, String), std::result::Result<bool, String>>;

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
    /// How each index's document is read, derived from `spec` once.
    plans: Arc<HashMap<IndexName, value::DocumentPlan>>,
    /// Cache of each `(schema, table)`'s single-column primary key.
    pk_cache: Arc<Mutex<HashMap<(String, String), ColumnName>>>,
    /// Cache of each root table's primary key and whether it is partitioned.
    root_key_cache: Arc<Mutex<RootKeys>>,
    /// Cache of each `(schema, table, column)`'s SQL type, used to cast filter
    /// operands to the column's real type rather than comparing as text.
    col_type_cache: ColumnTypes,
    /// Whether each `(link type, key type)` pair compares, probed once: the
    /// verdict, or the reason the database gave for not answering.
    comparison_cache: Arc<Mutex<ComparisonCache>>,
    /// Whether the role was refused `has_schema_privilege`, so the catalog
    /// lookup no longer probes with it.
    privilege_probe_refused: Arc<AtomicBool>,
    /// Tables a change arrived for that no index reads, already warned about.
    unread_warned: Arc<RwLock<HashSet<TableName>>>,
    /// Root tables without a primary key already warned about for a key with
    /// a null part.
    keyless_warned: Arc<Mutex<HashSet<TableName>>>,
}

/// Each root `(schema, table)`'s catalog facts.
type RootKeys = HashMap<(String, String), RootTable>;

/// What the catalog says of a root table that shapes its batched build.
#[derive(Debug, Clone)]
struct RootTable {
    /// Its primary key columns, any number of them.
    primary_key: Arc<[ColumnName]>,
    /// Whether it is partitioned, so a batch writes its keys where run-time
    /// pruning reads them ([`query::KeyForm`]).
    partitioned: bool,
}

/// A builder whose pool never connects, for the `resolve` bench.
#[cfg(feature = "bench")]
pub(crate) fn offline_builder(spec: Arc<SourceSpec>) -> PgDocumentBuilder {
    let options = sqlx::postgres::PgConnectOptions::new()
        .host("127.0.0.1")
        .port(1);
    let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy_with(options);
    PgDocumentBuilder::new(pool, spec)
}

impl PgDocumentBuilder {
    /// Over a pool whose connections read `bytea` as hex text, as
    /// [`connect`](Self::connect)'s do.
    pub(crate) fn new(pool: PgPool, spec: Arc<SourceSpec>) -> Self {
        Self {
            pool,
            reads: Arc::new(ReadColumns::new(&spec)),
            plans: Arc::new(
                spec.indexes()
                    .map(|(name, schema)| (name.clone(), value::DocumentPlan::new(&schema.fields)))
                    .collect(),
            ),
            spec,
            pk_cache: Arc::new(Mutex::new(HashMap::new())),
            root_key_cache: Arc::new(Mutex::new(HashMap::new())),
            col_type_cache: ColumnTypes::default(),
            comparison_cache: Arc::new(Mutex::new(HashMap::new())),
            privilege_probe_refused: Arc::new(AtomicBool::new(false)),
            unread_warned: Arc::new(RwLock::new(HashSet::new())),
            keyless_warned: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// How `index`'s document is read.
    fn plan(&self, index: &IndexName) -> Result<&value::DocumentPlan> {
        self.plans
            .get(index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{index}`")))
    }

    /// Warn, once per table, that a change arrived for a table no index reads —
    /// so a change streamed under an unexpected name (a partition's, say) isn't
    /// dropped silently.
    fn warn_unread(&self, table: &TableName) {
        let warned = self
            .unread_warned
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(table);
        if warned {
            return;
        }
        let first = self
            .unread_warned
            .write()
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
        // The document query renders the `bytea` it can reach itself (see
        // `byte_columns`). One nested in a composite, a range or a `json` it
        // builds is rendered by the session's `bytea_output`, which this sets
        // on every connection a direct link keeps. A transaction-mode pooler
        // may not.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::Executor::execute(&mut *connection, "SET bytea_output = 'hex'").await?;
                    without_jit(connection).await;
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
        let pk_meta = self
            .column_meta(&schema.db_schema, &schema.table, &pk_column)
            .await?;
        let sql = format!(
            "SELECT {key} FROM \"{db_schema}\".\"{table}\" WHERE \"{pk}\" IS NOT NULL LIMIT 1",
            key = value::key_select(
                &format!("\"{pk_column}\""),
                "key",
                &pk_meta.key_type,
                &pk_meta.base_name,
            ),
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

    /// What a batched build of `schema` needs from the catalog, read once per
    /// [`build_many`](DocumentBuilder::build_many) call (cached).
    async fn batch_catalog(
        &self,
        schema: &kernel::IndexSchema,
        pk_column: &ColumnName,
    ) -> Result<BatchCatalog> {
        let pks = self.relation_pks(schema).await?;
        let mut col_types = self.filter_column_types(schema).await?;
        self.add_key_column_types(schema, &[pk_column], &mut col_types)
            .await?;
        let bytes = self.byte_columns(schema, &pks).await?;
        let root_key = self.root_key(schema, &[pk_column]).await?;
        let key = self
            .column_meta(&schema.db_schema, &schema.table, pk_column)
            .await?;
        let partitioned = self.root_table(schema).await?.partitioned;
        Ok(BatchCatalog {
            pks,
            col_types,
            bytes,
            root_key,
            key,
            partitioned,
        })
    }

    /// Build one chunk of a batch onto `out`. `by_key`: pair rows by their
    /// key's value ([`query::documents_by_key_query`]). `Some` when the cached
    /// key type proved stale, with `out` maybe partly written, so the caller
    /// can undo it and rerun the chunk by position.
    async fn build_chunk(
        &self,
        Chunk {
            index_name,
            schema,
            pk_column,
            keys: chunk,
        }: &Chunk<'_>,
        catalog: &BatchCatalog,
        by_key: bool,
        out: &mut Vec<Document>,
    ) -> Result<Option<Stale>> {
        let keys: Vec<kernel::GenericValue> =
            chunk.iter().map(|(value, _)| (*value).clone()).collect();
        let BatchCatalog {
            pks,
            col_types,
            bytes,
            root_key,
            key,
            partitioned,
        } = catalog;
        let shape = query::Shape {
            root_key,
            form: query::KeyForm::for_batch(*partitioned, chunk.len()),
        };
        let (sql, params) = if by_key {
            query::documents_by_key_query(
                schema,
                pk_column,
                &keys,
                pks,
                col_types,
                bytes,
                (shape, &key.key_type, &key.base_name),
            )?
        } else {
            query::documents_query(schema, pk_column, &keys, pks, col_types, bytes, shape)?
        };

        let mut statement = sqlx::query(sql);
        for param in &params {
            statement = query::bind_param(statement, param)?;
        }
        let rows = match statement.fetch_all(&self.pool).await {
            Ok(rows) => rows,
            Err(error) if changed_result_type(&error) => return Ok(Some(Stale::ResultType)),
            Err(error) => return Err(self.failed_query(index_name, schema, error)),
        };

        let plan = self.plan(index_name)?;
        if by_key {
            let mut found = matching::ByKey::with_capacity(rows.len());
            for row in &rows {
                found.insert(
                    value::decode_key(row, query::DOC_KEY),
                    Ok(value::read_document(row, &schema.fields, plan)?),
                );
            }
            return Ok((!matching::match_by_key(found, chunk, out)).then_some(Stale::RowUnclaimed));
        }
        let mut found = Vec::with_capacity(rows.len());
        for row in &rows {
            // A built row the join paired with no position (never expected:
            // it was read by the same keys) answers nothing.
            let position: Option<i64> = row.try_get(query::DOC_POSITION).map_err(query_err)?;
            let Some(position) = position else {
                continue;
            };
            found.push((position, value::read_document(row, &schema.fields, plan)?));
        }
        matching::match_rows(found, chunk, out);
        Ok(None)
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

    /// What the catalog says of the root table: its primary key columns, any
    /// number of them, and whether it is partitioned (cached).
    async fn root_table(&self, schema: &kernel::IndexSchema) -> Result<RootTable> {
        let cache_key = (schema.db_schema.to_string(), schema.table.to_string());
        if let Some(root) = self
            .root_key_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&cache_key)
        {
            return Ok(root.clone());
        }
        let root = RootTable {
            primary_key: self
                .fetch_primary_key(&schema.db_schema, &schema.table)
                .await?
                .into(),
            partitioned: sqlx::query_scalar(
                "SELECT c.relkind = 'p' FROM pg_class c WHERE c.oid = $1::regclass",
            )
            .bind(format!("{}.{}", schema.db_schema, schema.table))
            .fetch_one(&self.pool)
            .await
            .map_err(query_err)?,
        };
        self.root_key_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(cache_key, root.clone());
        Ok(root)
    }

    /// Whether `key_columns` are the root table's whole primary key, so a key
    /// matches at most one row (the primary key is cached), and if not how
    /// the rows sharing a key are ordered ([`query::TieBreak`]).
    async fn root_key(
        &self,
        schema: &kernel::IndexSchema,
        key_columns: &[&ColumnName],
    ) -> Result<query::RootKey> {
        let primary_key = self.root_table(schema).await?.primary_key;
        if primary_key.iter().eq(key_columns.iter().copied()) {
            return Ok(query::RootKey::Unique);
        }
        Ok(query::RootKey::Shared(if primary_key.is_empty() {
            match source::root_columns(schema) {
                // The rows competing for a key all share the declared key's
                // value, so it orders nothing; and it may name no column (a
                // keyless root keyed by its whole row starts with a warning).
                source::ColumnUse::Columns(columns) => query::TieBreak::read_columns(
                    columns
                        .iter()
                        .filter(|column| Some(*column) != schema.primary_key.as_ref()),
                ),
                source::ColumnUse::All => query::TieBreak::whole_row(),
            }
        } else {
            query::TieBreak::primary_key(&primary_key)
        }))
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
    pub(super) async fn column_type(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        Ok(self
            .column_meta(schema, table, column)
            .await?
            .sql_type
            .clone())
    }

    /// The type a key read from a column is cast to (`$n::<bound>::<type>`,
    /// `expr::<type>`): the column's base type, schema-qualified, with no
    /// modifier. An explicit cast to a size truncates or rounds without an
    /// error (`'k12345678'::varchar(5)` is `k1234`, `1.2345::numeric(10,2)`
    /// is `1.23`), so a key cast to a size its column no longer has would
    /// match no row; `=` needs none. The qualified `typname` also keeps
    /// `bpchar` apart from `character`, which as a cast means `char(1)`.
    ///
    /// It is the base type, through every domain, when the role may use the
    /// base type's schema. When it may not (a domain `app.dcit` over
    /// `ext.citext`, no USAGE on `ext`), a cast to the base type would fail
    /// every query, so the key is cast to the column's declared type
    /// instead, as before the base type was used. That gives up one thing: a
    /// cast to a domain checks the domain's constraints, so a key that breaks
    /// them fails the query instead of matching nothing. The privilege is
    /// read by the same catalog query as the type, cached and refreshed with
    /// it.
    pub(super) async fn key_type(
        &self,
        schema: &DatabaseSchema,
        table: &TableName,
        column: &ColumnName,
    ) -> Result<String> {
        Ok(self
            .column_meta(schema, table, column)
            .await?
            .key_type
            .clone())
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
    ) -> Result<Arc<ColumnMeta>> {
        #[cfg(test)]
        tests::COLUMN_LOOKUPS.with(|count| count.set(count.get() + 1));
        if let Some(meta) =
            self.col_type_cache
                .get(schema.as_ref(), table.as_ref(), column.as_ref())
        {
            return Ok(meta);
        }
        let probe = !self.privilege_probe_refused.load(Ordering::Relaxed);
        let fetch = |sql: &'static str| {
            sqlx::query(sql)
                .bind(format!("{schema}.{table}"))
                .bind(column.as_ref().to_owned())
                .fetch_optional(&self.pool)
        };
        let row = match fetch(if probe {
            COLUMN_META_SQL
        } else {
            COLUMN_META_SQL_UNPROBED
        })
        .await
        {
            Err(error) if probe && refused_for_privilege(&error) => {
                self.privilege_probe_refused.store(true, Ordering::Relaxed);
                fetch(COLUMN_META_SQL_UNPROBED).await
            }
            other => other,
        }
        .map_err(query_err)?;
        let meta = Arc::new(match row {
            Some(row) => {
                let sql_type: String = row.try_get("sql_type").map_err(query_err)?;
                let type_oid: i64 = row.try_get("type_oid").map_err(query_err)?;
                let base_type: Option<String> = row.try_get("base_type").map_err(query_err)?;
                let not_null: bool = row.try_get("not_null").map_err(query_err)?;
                ColumnMeta {
                    base_type: base_type.filter(|base| *base != sql_type),
                    base_name: row.try_get("base_name").map_err(query_err)?,
                    element_name: row.try_get("element_name").map_err(query_err)?,
                    deterministic: row.try_get("deterministic").map_err(query_err)?,
                    sql_type,
                    type_oid: u32::try_from(type_oid).unwrap_or_default(),
                    type_mod: row.try_get("type_mod").map_err(query_err)?,
                    key_type: row.try_get("key_type").map_err(query_err)?,
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
        });
        self.col_type_cache.insert(
            schema.as_ref(),
            table.as_ref(),
            column.as_ref(),
            Arc::clone(&meta),
        );
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

    /// The cache of column types this builder writes its queries from, to
    /// hand to the WAL capture so it can drop a table's entries when the table
    /// changes.
    pub fn column_types(&self) -> ColumnTypes {
        self.col_type_cache.clone()
    }

    /// A document query of `index` failed: forget the column types of every
    /// table it reads, since a type that changed under the cache fails it the
    /// same way on every retry otherwise, then report the error
    /// ([`query_failed`]).
    #[cold]
    #[inline(never)]
    fn failed_query(
        &self,
        index: &IndexName,
        schema: &IndexSchema,
        error: sqlx::Error,
    ) -> SourceError {
        self.forget_tables_of(schema);
        query_failed(index, error)
    }

    /// A reverse lookup for `index` failed. A query error forgets the column
    /// types of every table the index reads, as a failed document query does,
    /// and is reported the same way ([`query_failed`]).
    #[cold]
    #[inline(never)]
    fn failed_lookup(
        &self,
        index: &IndexName,
        schema: &IndexSchema,
        error: resolve::LookupError,
    ) -> SourceError {
        match error {
            resolve::LookupError::Source(error) => error,
            resolve::LookupError::Query(error) => {
                self.forget_tables_of(schema);
                query_failed(index, *error)
            }
        }
    }

    fn forget_tables_of(&self, schema: &IndexSchema) {
        let mut tables = vec![schema.table.clone()];
        fields::collect_tables(&schema.fields, &mut tables);
        for table in tables {
            self.col_type_cache
                .forget_table(schema.db_schema.as_ref(), table.as_ref());
        }
    }

    /// The columns `schema`'s documents read as values whose base type is
    /// `bytea` or `bytea[]`, which the document query renders itself. Each
    /// type comes from the cached catalog lookup validation already made.
    async fn byte_columns(
        &self,
        schema: &IndexSchema,
        pks: &HashMap<String, ColumnName>,
    ) -> Result<query::ByteColumns> {
        let mut columns = Vec::new();
        fields::collect_value_columns(&schema.fields, &schema.table, pks, &mut columns);
        let mut bytes = query::ByteColumns::new();
        for (table, column) in columns {
            let meta = self.column_meta(&schema.db_schema, &table, &column).await?;
            let kind = if meta.base_name == "bytea" {
                query::Bytes::Scalar
            } else if meta.element_name.as_deref() == Some("bytea") {
                query::Bytes::Array
            } else {
                continue;
            };
            bytes.insert((table.to_string(), column.to_string()), kind);
        }
        Ok(bytes)
    }

    /// Set in `types` the [`key_type`](Self::key_type) of each root-table key
    /// column, so the keyed lookup can cast every `$n` to it. The keys come
    /// back from Postgres as values (a `uuid` as a string, say); without the
    /// cast the re-bound `$n` is `text` and `uuid = text` has no operator. It
    /// replaces a filter's entry for the column, which may carry a size.
    async fn add_key_column_types(
        &self,
        schema: &IndexSchema,
        columns: &[&ColumnName],
        types: &mut HashMap<(String, String), String>,
    ) -> Result<()> {
        for column in columns {
            let key_type = self
                .key_type(&schema.db_schema, &schema.table, column)
                .await?;
            types.insert((schema.table.to_string(), column.to_string()), key_type);
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
            let mut own = None;
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
                    if let (Some(pk_column), [(column, value)]) =
                        (&schema.primary_key, key.0.as_slice())
                        && column == pk_column
                    {
                        own = Some(value);
                    }
                } else {
                    skipped += 1;
                }
            }

            let paths = self.reads.paths(name, table);
            if paths.is_empty() {
                continue;
            }
            read = true;
            let Some(pk_column) = schema.primary_key.as_ref() else {
                tracing::warn!(
                    index = %name, table = %table,
                    "cannot reverse-resolve: index has no primary_key",
                );
                continue;
            };

            // A row that is its own ancestor must not re-emit its own id.
            let mut seen: HashSet<kernel::GenericValue> = own.into_iter().cloned().collect();
            let mut roots = Vec::new();
            for path in paths {
                if !path.columns.touched_by(&diff) {
                    skipped += 1;
                    continue;
                }
                let steps = path.address.steps(&schema.fields).ok_or_else(|| {
                    SourceError::Query("internal: a path's address is not in its index".into())
                })?;
                let found = match self.resolve_path(schema, table, key, before, &steps).await {
                    Ok(found) => found,
                    Err(error) => return Err(self.failed_lookup(name, schema, error)),
                };
                if let Some(link) = self
                    .reads_root_from_a_link(schema, pk_column, path, &steps)
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
    /// keys, deduplicated. Only for a change with links: `ids` alone are
    /// already unique, one set per index.
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
            sql_type: meta.sql_type.clone(),
            base_type: meta.base_type.clone(),
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
            meta.sql_type.clone()
        };
        self.probe(query::transform_probe(&sql_type, transforms))
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
        let pair = (link.sql_type.clone(), key.sql_type.clone());
        let cached = self
            .comparison_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&pair)
            .cloned();
        if let Some(answer) = cached {
            return answer.map_err(SourceError::Unchecked);
        }
        let probe = query::comparison_probe(&pair.0, &pair.1);
        let answer = match self.probe(probe).await {
            Ok(compares) => Ok(compares),
            Err(SourceError::Unchecked(reason)) => Err(reason),
            Err(other) => return Err(other),
        };
        self.comparison_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(pair, answer.clone());
        answer.map_err(SourceError::Unchecked)
    }
}

impl PgDocumentBuilder {
    /// Run a table-free `probe`, in one round trip (the simple protocol, no
    /// prepared statement): `false` when Postgres has no function or
    /// operator for it. The probe casts to a column's type, which needs USAGE
    /// on the type's schema; reading the column does not. So only "no such
    /// function" is a verdict, and any other refusal leaves it unknown:
    /// [`SourceError::Unchecked`] with Postgres's message, which validation
    /// reports as a skipped check.
    async fn probe(&self, probe: query::SqlString) -> Result<bool> {
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
                Err(SourceError::Unchecked(error.message().to_owned()))
            }
            Err(error) => Err(query_err(error)),
        }
    }
}

/// SQLSTATE `42883`: no function takes the argument's type.
const UNDEFINED_FUNCTION: &str = "42883";
/// SQLSTATE `42725`: several functions could take it.
const AMBIGUOUS_FUNCTION: &str = "42725";
/// SQLSTATE `42804`: an expression is of another type than its place takes.
const DATATYPE_MISMATCH: &str = "42804";
/// SQLSTATE `42703`: no such column.
const UNDEFINED_COLUMN: &str = "42703";
/// SQLSTATE `42P01`: no such table.
const UNDEFINED_TABLE: &str = "42P01";
/// SQLSTATE `42501`: the role lacks a privilege.
const INSUFFICIENT_PRIVILEGE: &str = "42501";
/// `feature_not_supported`, which a cached plan whose result type changed
/// raises.
const FEATURE_NOT_SUPPORTED: &str = "0A000";

/// Whether Postgres refused `error` for a privilege the role lacks.
pub(crate) fn refused_for_privilege(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|error| error.code().as_deref() == Some(INSUFFICIENT_PRIVILEGE))
}

/// The catalog lookup behind the column-type cache, with `$usable` saying
/// whether the role may use the base type's schema. `format_type` yields a
/// canonical, re-parseable type name, so it can be dropped straight into a
/// `$n::<type>` cast. `attnotnull` is the column's NOT NULL constraint.
/// `base_type` follows a domain through `typbasetype` to the first type that
/// is no domain, and `base_name` names that type. For an array,
/// `element_name` names its element type the same way (`typelem`, then every
/// domain), so an array of a domain over `bytea` reads as one of `bytea`.
/// `collisdeterministic` is read through `to_jsonb` because Postgres before
/// 12 has no such column (and only deterministic collations).
macro_rules! column_meta_sql {
    ($usable:literal) => {
        concat!(
            "SELECT format_type(a.atttypid, a.atttypmod) AS sql_type, \
                    a.atttypid::int8 AS type_oid, a.atttypmod AS type_mod, \
                    b.base_type, b.base_name, el.element_name, \
                    CASE WHEN b.base_usable THEN b.base_key ELSE ( \
                      SELECT quote_ident(dn.nspname) || '.' || quote_ident(dt.typname) \
                      FROM pg_type dt JOIN pg_namespace dn ON dn.oid = dt.typnamespace \
                      WHERE dt.oid = a.atttypid \
                    ) END AS key_type, \
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
                             t.typname::text AS base_name, t.oid AS base_oid, \
                             quote_ident(n.nspname) || '.' || quote_ident(t.typname) \
                               AS base_key, ",
            $usable,
            " AS base_usable \
                      FROM d JOIN pg_type t ON t.oid = d.oid \
                      JOIN pg_namespace n ON n.oid = t.typnamespace \
                      WHERE t.typtype <> 'd' \
                  ) b \
             LEFT JOIN LATERAL ( \
                    WITH RECURSIVE e(oid) AS ( \
                         SELECT a.typelem FROM pg_type a \
                         WHERE a.oid = b.base_oid AND a.typelem <> 0 AND a.typlen = -1 \
                       UNION ALL \
                         SELECT t.typbasetype FROM pg_type t JOIN e ON t.oid = e.oid \
                         WHERE t.typtype = 'd' \
                    ) SELECT t.typname::text AS element_name \
                      FROM e JOIN pg_type t ON t.oid = e.oid WHERE t.typtype <> 'd' \
                  ) el ON true \
             WHERE a.attrelid = to_regclass($1) AND a.attname = $2 \
               AND a.attnum > 0 AND NOT a.attisdropped"
        )
    };
}

/// [`column_meta_sql`] probing the base type's schema with
/// `has_schema_privilege`.
const COLUMN_META_SQL: &str = column_meta_sql!("has_schema_privilege(n.oid, 'USAGE')");

/// [`column_meta_sql`] for a role that may not run `has_schema_privilege`
/// (EXECUTE revoked): every key is cast to its declared type, as the cast
/// before the base type was used could be.
const COLUMN_META_SQL_UNPROBED: &str = column_meta_sql!("false");

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
        if links.is_empty() {
            return Ok(ids);
        }
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
        let bytes = self.byte_columns(schema, &pks).await?;
        let (mut sql, params) = query::document_query(schema, &id.key.0, &pks, &col_types, &bytes)?;
        if let query::RootKey::Shared(tie_break) = self.root_key(schema, &key_columns).await? {
            sql = query::first_row(sql, &tie_break);
        }

        let mut statement = sqlx::query(sql);
        for param in &params {
            statement = query::bind_param(statement, param)?;
        }
        let row = statement
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| self.failed_query(&id.index, schema, error))?;

        // No row means the root is absent or soft-deleted (both folded into the
        // query's WHERE) → the document should not exist.
        match row {
            None => Ok(Document::Delete { id: id.clone() }),
            Some(row) => Ok(
                match value::read_document(&row, &schema.fields, self.plan(&id.index)?)? {
                    Ok(body) => Document::Upsert {
                        id: id.clone(),
                        body,
                    },
                    Err(failure) => Document::Failed {
                        id: id.clone(),
                        failure,
                    },
                },
            ),
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

            // The batched query probes the root by one key column per requested
            // key, so it needs both a declared single-column primary key and ids
            // that carry exactly that one key column. Pair each id with its lone key
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

            let mut catalog = self.batch_catalog(schema, &pk_column).await?;
            for keys in keyed.chunks(BUILD_CHUNK) {
                let start = out.len();
                let by_key = catalog.pairs_by_key(keys);
                let chunk = Chunk {
                    index_name,
                    schema,
                    pk_column: &pk_column,
                    keys,
                };
                let Some(stale) = self.build_chunk(&chunk, &catalog, by_key, &mut out).await?
                else {
                    continue;
                };
                // The cached key type is stale (the change stream's
                // `Relation` message is still to come). Read the types afresh
                // and let Postgres pair the chunk by position, once.
                tracing::debug!(
                    index = %index_name,
                    reason = stale.reason(),
                    "rerunning a batch by position with the column types read again",
                );
                out.truncate(start);
                self.forget_tables_of(schema);
                catalog = self.batch_catalog(schema, &pk_column).await?;
                if let Some(stale) = self.build_chunk(&chunk, &catalog, false, &mut out).await? {
                    return Err(SourceError::Query(format!(
                        "index `{index_name}`: a batch failed again after its column types \
                         were read afresh: {}",
                        stale.reason()
                    )));
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

/// Turn JIT compilation off for a session the source opened. flusso's
/// queries are short index lookups and key reads: compiling one costs more
/// than running it, and a batch over a partitioned root or a wide document can
/// pass `jit_above_cost`. `jit` is user-settable, so this is expected to hold;
/// where it is refused (a server without the parameter), the session keeps the
/// server's setting and the refusal is a warning, never a failed connection.
/// Behind a transaction-mode pooler the setting may not stay with the session,
/// so the manual recommends `ALTER ROLE … SET jit = off` there.
pub(crate) async fn without_jit(connection: &mut sqlx::PgConnection) {
    if let Err(error) = sqlx::Executor::execute(&mut *connection, "SET jit = off").await {
        tracing::warn!(
            error = %error,
            "could not turn JIT compilation off for this session; it keeps the server's setting"
        );
    }
}

pub(super) fn query_err(error: sqlx::Error) -> SourceError {
    SourceError::Query(error.to_string())
}

/// A query `index` runs failed. One Postgres raises when the database no
/// longer has what the schema reads (a column, a table, an `=` between a link
/// and its key) names the index and points at `flusso check`: no retry fixes
/// it, a schema or database change does.
pub(super) fn query_failed(index: &IndexName, error: sqlx::Error) -> SourceError {
    let mismatch = error.as_database_error().is_some_and(|error| {
        matches!(
            error.code().as_deref(),
            Some(UNDEFINED_FUNCTION | DATATYPE_MISMATCH | UNDEFINED_COLUMN | UNDEFINED_TABLE)
        )
    });
    if mismatch {
        return SourceError::Query(format!(
            "{error}: the database no longer matches index `{index}`; `flusso check` names \
             the field"
        ));
    }
    query_err(error)
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
