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
//! - Resolution: root table; direct foreign-key relations; many-to-many
//!   (`through`) relations on either the far or junction table; and tables
//!   reachable through multiple hops of nesting, chained back to the root.
//! - Assembly: column fields (transforms, defaults); one-to-one / one-to-many /
//!   many-to-many joins (filters, ordering, limit); joins nested inside joins;
//!   aggregates, including over a junction; boolean and timestamp soft-delete
//!   with optional `when` filters.
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
mod value;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use schema_core::{ColumnName, Config, DatabaseSchema, TableName};
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_core::{Result, RowKey, SourceError};
use sqlx::{PgPool, Row};

use fields::find_paths;

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

        let pks = self.relation_pks(schema).await?;
        let (sql, params) = query::document_query(schema, &id.key.0, &pks)?;

        let mut statement = sqlx::query(&sql);
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
                    body: value::json_to_generic(document),
                })
            }
        }
    }
}

pub(super) fn query_err(error: sqlx::Error) -> SourceError {
    SourceError::Query(error.to_string())
}
