//! [`PgDocumentBuilder`] — the read half of the Postgres source.
//!
//! Resolves which documents a changed row affects and assembles them from the
//! schema with sea-query + sqlx. The work is split across this module:
//!
//! - [`fields`] — pure traversal of the index's field tree.
//! - [`resolve`] — reverse resolution: changed row → affected document keys.
//! - [`assemble`] — building a document body from a root row and its relations.
//! - [`soft_delete`] — deciding when a row counts as deleted.
//! - [`sql`] / [`value`] — query building and Postgres ↔ value conversion.
//!
//! ## Coverage
//!
//! - Resolution: root table; direct foreign-key relations; many-to-many
//!   (`through`) relations on either the far or junction table; and tables
//!   reachable through *multiple* hops of nesting, chained back to the root.
//! - Assembly: column fields (transforms, defaults); one-to-one / one-to-many /
//!   many-to-many joins (filters, ordering, limit); joins nested inside joins;
//!   aggregates, including over a junction; boolean and timestamp soft-delete
//!   with optional `when` filters; tombstones for missing rows.
//! - Decoding covers the common scalar types plus timestamps, dates, UUIDs, and
//!   JSON (carried as text / value trees).
//!
//! Relation targets are matched on each table's real primary key, looked up
//! from the Postgres catalog and cached (see [`PgDocumentBuilder::table_primary_key`]).
//! The index's own root key comes from its declared `primary_key`.
//!
//! ## Remaining limits
//!
//! - A child-row *delete* on a related table can't be reverse-resolved from a
//!   key-only change (the row is already gone); this follows from the thin-event
//!   CDC design.
//! - Nested joins and multi-hop resolution issue one query per parent row / hop
//!   (N+1). `Raw` soft-delete `when` filters are not evaluated (logged and
//!   skipped); `LIKE`/`ILIKE` in `when` filters are matched approximately.

mod assemble;
mod fields;
mod resolve;
mod soft_delete;
mod sql;
mod value;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use schema_core::{ColumnName, Config, DatabaseSchema, GenericValue, TableName};
use sea_query::PostgresQueryBuilder;
use sea_query_binder::SqlxBinder;
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_core::{Result, RowKey, SourceError};
use sqlx::{PgPool, Row};

use fields::{find_paths, root_columns};
use soft_delete::is_soft_deleted;

/// Builds index documents from a Postgres database, driven by the loaded
/// [`Config`]. Cheap to clone — the pool, config, and primary-key cache are
/// shared.
#[derive(Debug, Clone)]
pub struct PgDocumentBuilder {
    pool: PgPool,
    config: Arc<Config>,
    /// Cache of each `(schema, table)`'s single-column primary key.
    pk_cache: Arc<Mutex<HashMap<(String, String), ColumnName>>>,
}

impl PgDocumentBuilder {
    /// Create a builder over a connection pool and the resolved config.
    pub fn new(pool: PgPool, config: Arc<Config>) -> Self {
        Self {
            pool,
            config,
            pk_cache: Arc::new(Mutex::new(HashMap::new())),
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
        let qualified = format!("{schema}.{table}");
        let sql = "SELECT a.attname AS name \
                   FROM pg_index i \
                   JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
                   WHERE i.indrelid = $1::regclass AND i.indisprimary";
        let rows = sqlx::query(sql)
            .bind(qualified)
            .fetch_all(&self.pool)
            .await
            .map_err(query_err)?;

        let mut columns = Vec::with_capacity(rows.len());
        for row in &rows {
            let name: String = row.try_get("name").map_err(query_err)?;
            columns.push(
                ColumnName::try_new(name)
                    .map_err(|e| SourceError::Query(format!("invalid primary key column: {e}")))?,
            );
        }
        Ok(columns)
    }
}

#[async_trait]
impl DocumentBuilder for PgDocumentBuilder {
    async fn resolve(&self, table: &TableName, key: &RowKey) -> Result<Vec<DocumentId>> {
        let mut ids = Vec::new();
        for (name, index) in &self.config.indexes {
            if !index.enabled {
                continue;
            }
            let schema = &index.schema;

            // Change on the document's own root table: the key is the id.
            if schema.table == *table {
                ids.push(DocumentId {
                    index: name.clone(),
                    key: key.clone(),
                });
                continue;
            }

            // Change on a related table: resolve every path back to the root.
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
        Ok(ids)
    }

    async fn build(&self, id: &DocumentId) -> Result<Document> {
        let index = self
            .config
            .indexes
            .get(&id.index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{}`", id.index)))?;
        let schema = &index.schema;

        let mut columns = root_columns(schema);
        for (column, _) in &id.key.0 {
            push_unique(&mut columns, column);
        }

        let query = sql::root_select(&schema.db_schema, &schema.table, &columns, &id.key.0)?;
        let (statement, values) = query.build_sqlx(PostgresQueryBuilder);
        let row = sqlx::query_with(&statement, values)
            .fetch_optional(&self.pool)
            .await
            .map_err(query_err)?;

        // No root row, or it is soft-deleted → the document should not exist.
        let Some(row) = row else {
            return Ok(Document::Delete { id: id.clone() });
        };
        let root = value::row_to_map(&row);
        if is_soft_deleted(schema, &root) {
            return Ok(Document::Delete { id: id.clone() });
        }

        // The root's primary-key value keys any top-level relations.
        let root_pk = match &schema.primary_key {
            Some(pk) => primary_key_value(&id.key, pk)?.clone(),
            None => GenericValue::Null,
        };
        let body = self.assemble_row(&schema.db_schema, &schema.fields, &root, &root_pk).await?;
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(body),
        })
    }
}

/// The value of the index's primary-key column within a document key. Supports
/// composite keys: relations match on the declared primary key.
fn primary_key_value<'a>(key: &'a RowKey, primary_key: &ColumnName) -> Result<&'a GenericValue> {
    key.0
        .iter()
        .find(|(column, _)| column == primary_key)
        .map(|(_, value)| value)
        .ok_or_else(|| {
            SourceError::Query(format!("document key is missing primary key column `{primary_key}`"))
        })
}

/// Push a column only if it isn't already present (preserves order).
pub(super) fn push_unique(columns: &mut Vec<ColumnName>, column: &ColumnName) {
    if !columns.iter().any(|c| c == column) {
        columns.push(column.clone());
    }
}

pub(super) fn query_err(error: sqlx::Error) -> SourceError {
    SourceError::Query(error.to_string())
}
