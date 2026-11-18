//! A backfill's key pages: an index's root keys, `limit` at a time, in the
//! order the root's primary key index sorts them, up to the greatest key the
//! walk found when it started (issue #386 stage 3).
//!
//! Two statements, on the builder's pool, outside any transaction. At the
//! start of the walk, its bound:
//!
//! ```text
//! SELECT <key text> AS "key", "id"::<key type> AS "key:typed"
//! FROM "public"."users" ORDER BY "id" USING OPERATOR(pg_catalog.>) LIMIT 1
//! ```
//!
//! Then one statement per page:
//!
//! ```text
//! SELECT <key text> AS "key", "id"::<key type> AS "key:typed",
//!        ("id" OPERATOR(pg_catalog.<=) $2::<bound>::<key type>) AS "within"
//! FROM "public"."users"
//! WHERE "id" OPERATOR(pg_catalog.>) $1::<bound>::<key type>   -- every page but the first
//! ORDER BY "id" USING OPERATOR(pg_catalog.<)
//! LIMIT 1024
//! ```
//!
//! - **No snapshot across pages.** The slot exists before any backfill
//!   (#120), so a row that gets a key behind the walk is in the WAL and built
//!   live. A page holds no lock and no `xmin` past its own statement.
//! - **The walk is bounded.** It ends at the greatest key it read when it
//!   started. A row whose key is past the bound got that key after the walk
//!   started, which is after the slot existed, so the live stream delivers
//!   it: an insert, or a key changed to above the bound. A row at or below it
//!   is paged or, changed later, delivered live too. The bound is compared by
//!   value, so a walk ends when the bound's row is deleted mid-walk. Without
//!   a bound, writers adding greater keys keep the walk going, and every row
//!   they add is built twice.
//! - **The bound is in the select list, not the `WHERE`.** A page past the
//!   bound drops those rows and is short, which ends the walk. As a second
//!   range condition, a generic plan would take `key > $1 AND key <= $2` for
//!   a small range and sort it on every page.
//! - **The cursor is the last key, read back and bound again.** It is read
//!   with [`value::key_select`] and bound with `sql::key_cast`, as every
//!   key a build binds: the bound value is `=` to the stored key, so `>` is
//!   "after that row". The bound is read and bound the same way.
//! - **The key index's own operators.** `<`, `>` and `<=` are the primary key
//!   index's btree operators, read from its opclass and written
//!   schema-qualified, so the index serves the order whatever the search
//!   path (a `citext` in a schema off it compared as `text` would skip
//!   rows). A role that may not use the operators' schema gets the bare
//!   names, resolved through the search path, `USING <` keeping order and
//!   comparison one operator: correct, but every page then sorts the rest
//!   of the table, which is warned once per index.
//! - **Which roots.** Only a root whose single-column catalog primary key is
//!   the declared key (or no key is declared): the pages then partition the
//!   key space and each document is one row. Any other root answers no
//!   bound, with a warning, so its walk ends at once (#391).
//! - **A type changed under a page** (`0A000`) forgets the root's column
//!   types and operators and reads the page once more, as a batched build
//!   does.

use kernel::{ColumnName, GenericValue, IndexName, IndexSchema};
use source::{Result, RowKey, SourceError};
use sqlx::Row;

use super::query::{WalkKey, WalkOperators};
use super::{PgDocumentBuilder, changed_result_type, query, refused_for_privilege, value};

impl PgDocumentBuilder {
    /// `index`'s greatest root key, by its walk's order; `None` when it has
    /// none or can't be walked.
    pub(super) async fn greatest_key(&self, index: &IndexName) -> Result<Option<RowKey>> {
        let schema = self.schema_of(index)?;
        let Some(key) = self.walked_key(index, schema).await? else {
            return Ok(None);
        };
        let read = |key: ColumnName| async move {
            let meta = self
                .column_meta(&schema.db_schema, &schema.table, &key)
                .await?;
            let operators = self.walk_operators(index, schema, &key).await?;
            let walk = WalkKey {
                column: &key,
                key_type: &meta.key_type,
                base_name: &meta.base_name,
                operators: &operators,
            };
            let row = sqlx::query(query::last_key_query(schema, walk))
                .fetch_optional(&self.pool)
                .await;
            Ok::<_, SourceError>(row.map(|row| {
                row.map(|row| {
                    RowKey(vec![(
                        key.clone(),
                        value::decode_key(&row, query::KEY_PAGE_KEY),
                    )])
                })
            }))
        };
        match read(key.clone()).await? {
            Err(error) if changed_result_type(&error) => {
                self.forget_walk(schema);
                read(key)
                    .await?
                    .map_err(|error| self.failed_walk(index, schema, error))
            }
            Err(error) => Err(self.failed_walk(index, schema, error)),
            Ok(last) => Ok(last),
        }
    }

    /// Up to `limit` of `index`'s root keys after `after`, at most `through`,
    /// in key order.
    pub(super) async fn key_page(
        &self,
        index: &IndexName,
        after: Option<&RowKey>,
        through: &RowKey,
        limit: usize,
    ) -> Result<Vec<RowKey>> {
        let schema = self.schema_of(index)?;
        let Some(key) = self.walked_key(index, schema).await? else {
            return Ok(Vec::new());
        };
        let after = match after {
            None => None,
            Some(after) => Some(walked_value(after, &key)?),
        };
        let through = walked_value(through, &key)?;
        match self
            .read_page(index, schema, &key, after, through, limit)
            .await?
        {
            Err(error) if changed_result_type(&error) => {
                tracing::debug!(%index, "reading a key page again with the column types read afresh");
                self.forget_walk(schema);
                self.read_page(index, schema, &key, after, through, limit)
                    .await?
                    .map_err(|error| self.failed_walk(index, schema, error))
            }
            Err(error) => Err(self.failed_walk(index, schema, error)),
            Ok(page) => Ok(page),
        }
    }

    /// One page: an outer error is the builder's own, an inner one the
    /// statement's, which the caller may retry.
    async fn read_page(
        &self,
        index: &IndexName,
        schema: &IndexSchema,
        key: &ColumnName,
        after: Option<&GenericValue>,
        through: &GenericValue,
        limit: usize,
    ) -> Result<std::result::Result<Vec<RowKey>, sqlx::Error>> {
        let meta = self
            .column_meta(&schema.db_schema, &schema.table, key)
            .await?;
        let operators = self.walk_operators(index, schema, key).await?;
        let walk = WalkKey {
            column: key,
            key_type: &meta.key_type,
            base_name: &meta.base_name,
            operators: &operators,
        };
        let text = query::key_page_query(schema, walk, after, through, limit)?;
        let mut statement = sqlx::query(text);
        if let Some(after) = after {
            statement = query::bind_param(statement, after)?;
        }
        statement = query::bind_param(statement, through)?;
        let rows = match statement.fetch_all(&self.pool).await {
            Ok(rows) => rows,
            Err(error) => return Ok(Err(error)),
        };
        let mut page = Vec::with_capacity(rows.len());
        for row in &rows {
            let within: bool = row
                .try_get(query::KEY_PAGE_WITHIN)
                .map_err(super::query_err)?;
            if !within {
                break;
            }
            page.push(RowKey(vec![(
                key.clone(),
                value::decode_key(row, query::KEY_PAGE_KEY),
            )]));
        }
        Ok(Ok(page))
    }

    fn schema_of(&self, index: &IndexName) -> Result<&IndexSchema> {
        self.spec
            .schema(index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{index}`")))
    }

    /// The operators `index`'s walk orders and compares by (cached per root
    /// table): its primary key index's btree `<`, `>` and `<=`, written
    /// schema-qualified when the role may use their schema, else bare.
    async fn walk_operators(
        &self,
        index: &IndexName,
        schema: &IndexSchema,
        key: &ColumnName,
    ) -> Result<WalkOperators> {
        let cache_key = (schema.db_schema.to_string(), schema.table.to_string());
        if let Some(found) = self.lock_walk_operators().get(&cache_key) {
            return Ok(found.clone());
        }
        let table = format!("{}.{}", schema.db_schema, schema.table);
        let fetch = |sql: &'static str| {
            sqlx::query(sql)
                .bind(table.clone())
                .bind(key.as_ref().to_owned())
                .fetch_optional(&self.pool)
        };
        let row = match fetch(WALK_OPERATORS_SQL).await {
            Err(error) if refused_for_privilege(&error) => fetch(WALK_OPERATORS_SQL_UNPROBED).await,
            other => other,
        }
        .map_err(super::query_err)?;
        let operators = match row {
            Some(row) => {
                let schema_name: String = row.try_get(0).map_err(super::query_err)?;
                let usable: bool = row.try_get(1).map_err(super::query_err)?;
                if usable {
                    WalkOperators::qualified(&schema_name)
                } else {
                    self.warn_sorting(index, &schema_name);
                    WalkOperators::bare()
                }
            }
            None => WalkOperators::bare(),
        };
        self.lock_walk_operators()
            .insert(cache_key, operators.clone());
        Ok(operators)
    }

    fn warn_sorting(&self, index: &IndexName, operator_schema: &str) {
        let first = self
            .sorting_warned
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(index.clone());
        if first {
            tracing::warn!(
                %index,
                operator_schema,
                remediation = %format!("GRANT USAGE ON SCHEMA {operator_schema} TO <flusso's role>"),
                "the backfill of this index can't name its key's ordering operators, which live in \
                 a schema this role may not use: each page sorts the rest of the table instead of \
                 reading the primary key index in order",
            );
        }
    }

    fn lock_walk_operators(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<(String, String), WalkOperators>> {
        self.walk_operators
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Forget what a walk of `schema`'s root read from the catalog.
    fn forget_walk(&self, schema: &IndexSchema) {
        self.forget_tables_of(schema);
        self.lock_walk_operators()
            .remove(&(schema.db_schema.to_string(), schema.table.to_string()));
    }

    fn failed_walk(
        &self,
        index: &IndexName,
        schema: &IndexSchema,
        error: sqlx::Error,
    ) -> SourceError {
        self.forget_walk(schema);
        self.failed_query(index, schema, error)
    }

    /// The column `index`'s pages walk: its root's single-column primary
    /// key, when the declared key is that column or unset. `None` (warned
    /// once per index) for any other root.
    async fn walked_key(
        &self,
        index: &IndexName,
        schema: &IndexSchema,
    ) -> Result<Option<ColumnName>> {
        let primary_key = self.root_table(schema).await?.primary_key;
        let skipped = match (primary_key.as_ref(), schema.primary_key.as_ref()) {
            ([single], None) => return Ok(Some(single.clone())),
            ([single], Some(declared)) if single == declared => return Ok(Some(single.clone())),
            ([], _) => "its root table has no primary key",
            ([_], Some(_)) => "its declared primary_key is not its root table's primary key",
            _ => "its root table's primary key is composite",
        };
        let first = self
            .unwalked_warned
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(index.clone());
        if first {
            tracing::warn!(
                %index,
                table = %schema.table,
                reason = skipped,
                "skipping the backfill of this index: {skipped}",
            );
        }
        Ok(None)
    }
}

/// The schema of the root's primary key index's btree `<` (and with it `>`
/// and `<=`, of the same operator family), and whether the role may name it
/// (`$usable`). `$1` is the root table, `$2` its key column.
macro_rules! walk_operators_sql {
    ($usable:literal) => {
        concat!(
            "SELECT quote_ident(n.nspname), ",
            $usable,
            " FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = i.indkey[0] \
             JOIN pg_opclass c ON c.oid = i.indclass[0] \
             JOIN pg_amop o ON o.amopfamily = c.opcfamily AND o.amopmethod = c.opcmethod \
               AND o.amoplefttype = c.opcintype AND o.amoprighttype = c.opcintype \
               AND o.amopstrategy = 1 \
             JOIN pg_operator op ON op.oid = o.amopopr \
             JOIN pg_namespace n ON n.oid = op.oprnamespace \
             WHERE i.indrelid = $1::regclass AND i.indisprimary AND i.indnatts = 1 \
               AND a.attname = $2"
        )
    };
}

const WALK_OPERATORS_SQL: &str =
    walk_operators_sql!("n.nspname = 'pg_catalog' OR has_schema_privilege(n.oid, 'USAGE')");

/// [`WALK_OPERATORS_SQL`] for a role that may not run
/// `has_schema_privilege`: only `pg_catalog`, which every role may use.
const WALK_OPERATORS_SQL_UNPROBED: &str = walk_operators_sql!("n.nspname = 'pg_catalog'");

/// A key's value of the walked column. A key the pages didn't hand out
/// (another column, a null) is a caller's mistake, not a page.
fn walked_value<'a>(walked: &'a RowKey, key: &ColumnName) -> Result<&'a GenericValue> {
    match walked.0.as_slice() {
        [(column, value)] if column == key && !matches!(value, GenericValue::Null) => Ok(value),
        _ => Err(SourceError::Query(format!(
            "a backfill page can only follow a key of `{key}` it handed out"
        ))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use kernel::GenericValue;

    use super::*;

    #[test]
    fn a_page_follows_only_a_key_it_handed_out() {
        let id = ColumnName::try_new("id").unwrap();
        let other = ColumnName::try_new("other").unwrap();
        let key = |column: &ColumnName, value| RowKey(vec![(column.clone(), value)]);
        assert!(walked_value(&key(&id, GenericValue::BigInt(1)), &id).is_ok());
        assert!(walked_value(&key(&other, GenericValue::BigInt(1)), &id).is_err());
        assert!(walked_value(&key(&id, GenericValue::Null), &id).is_err());
        assert!(walked_value(&RowKey(vec![]), &id).is_err());
    }
}
