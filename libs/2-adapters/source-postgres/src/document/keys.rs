//! A backfill's key pages: an index's root keys, `limit` at a time, in the
//! order the root's primary key index sorts them (issue #386 stage 3).
//!
//! One statement per page, on the builder's pool, outside any transaction:
//!
//! ```text
//! SELECT <key text> AS "key", "id"::<key type> AS "key:typed"
//! FROM "public"."users"
//! WHERE "id" > $1::<bound>::<key type>    -- every page but the first
//! ORDER BY "id" USING <
//! LIMIT 1024
//! ```
//!
//! - **No snapshot across pages.** The slot exists before any backfill
//!   (#120), so a row that gets a key behind the walk is in the WAL and built
//!   live. A page holds no lock and no `xmin` past its own statement.
//! - **The cursor is the last key, read back and bound again.** It is read
//!   with [`value::key_select`] and bound with [`sql::key_cast`], as every
//!   key a build binds: the bound value is `=` to the stored key, so `>` is
//!   "after that row".
//! - **One operator orders and compares.** `USING <` resolves `<` the way
//!   the `WHERE` resolves `>`: a type whose operators sit in a schema off the
//!   search path (`citext` in `ext`) then orders and compares by `text`'s
//!   operators alike. A plain `ORDER BY` would sort by the index's while `>`
//!   compared as `text`, and skip the rows the two orders disagree on. Where
//!   the operator is the index's own (every other case) the plan is the
//!   index range scan with no sort.
//! - **Which roots.** Only a root whose single-column catalog primary key is
//!   the declared key (or no key is declared): the pages then partition the
//!   key space and each document is one row. Any other root answers an empty
//!   page with a warning, as the cursor's snapshot skipped it (#391).
//! - **A type changed under a page** (`0A000`) forgets the root's column
//!   types and reads the page once more, as a batched build does.

use kernel::{ColumnName, IndexName, IndexSchema};
use source::{Result, RowKey, SourceError};

use super::{PgDocumentBuilder, changed_result_type, query, value};

impl PgDocumentBuilder {
    /// Up to `limit` of `index`'s root keys after `after`, in key order.
    pub(super) async fn key_page(
        &self,
        index: &IndexName,
        after: Option<&RowKey>,
        limit: usize,
    ) -> Result<Vec<RowKey>> {
        let schema = self
            .spec
            .schema(index)
            .ok_or_else(|| SourceError::Query(format!("unknown index `{index}`")))?;
        let Some(key) = self.walked_key(index, schema).await? else {
            return Ok(Vec::new());
        };
        let after = match after {
            None => None,
            Some(after) => Some(after_value(after, &key)?),
        };
        let meta = self
            .column_meta(&schema.db_schema, &schema.table, &key)
            .await?;
        match self.read_page(schema, &key, &meta, after, limit).await? {
            Err(error) if changed_result_type(&error) => {
                tracing::debug!(%index, "reading a key page again with the column types read afresh");
                self.forget_tables_of(schema);
                let meta = self
                    .column_meta(&schema.db_schema, &schema.table, &key)
                    .await?;
                self.read_page(schema, &key, &meta, after, limit)
                    .await?
                    .map_err(|error| self.failed_query(index, schema, error))
            }
            Err(error) => Err(self.failed_query(index, schema, error)),
            Ok(page) => Ok(page),
        }
    }

    /// One page: an outer error is the builder's own, an inner one the
    /// statement's, which the caller may retry.
    async fn read_page(
        &self,
        schema: &IndexSchema,
        key: &ColumnName,
        meta: &super::ColumnMeta,
        after: Option<&kernel::GenericValue>,
        limit: usize,
    ) -> Result<std::result::Result<Vec<RowKey>, sqlx::Error>> {
        let text =
            query::key_page_query(schema, key, (&meta.key_type, &meta.base_name), after, limit)?;
        let mut statement = sqlx::query(text);
        if let Some(after) = after {
            statement = query::bind_param(statement, after)?;
        }
        Ok(statement.fetch_all(&self.pool).await.map(|rows| {
            rows.iter()
                .map(|row| {
                    RowKey(vec![(
                        key.clone(),
                        value::decode_key(row, query::KEY_PAGE_KEY),
                    )])
                })
                .collect()
        }))
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

/// `after`'s value of the walked column. A key the pages didn't hand out
/// (another column, a null) is a caller's mistake, not a page.
fn after_value<'a>(after: &'a RowKey, key: &ColumnName) -> Result<&'a kernel::GenericValue> {
    match after.0.as_slice() {
        [(column, value)] if column == key && !matches!(value, kernel::GenericValue::Null) => {
            Ok(value)
        }
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
        assert!(after_value(&key(&id, GenericValue::BigInt(1)), &id).is_ok());
        assert!(after_value(&key(&other, GenericValue::BigInt(1)), &id).is_err());
        assert!(after_value(&key(&id, GenericValue::Null), &id).is_err());
        assert!(after_value(&RowKey(vec![]), &id).is_err());
    }
}
