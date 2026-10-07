//! Postgres's answer to [`CaptureProvisioning::inspect_pre_image`](source::CaptureProvisioning::inspect_pre_image):
//! does each table's **replica identity** carry the columns reverse resolution
//! needs from a change's pre-image?
//!
//! pgoutput puts a table's replica-identity columns in the old tuple of an
//! update or delete. Which columns that is depends on `pg_class.relreplident`:
//!
//! | `relreplident` | Old tuple carries |
//! | --- | --- |
//! | `d` (default) | the primary key |
//! | `i` (`USING INDEX`) | the identity index's columns |
//! | `f` (`FULL`) | every column |
//! | `n` (`NOTHING`) | nothing |
//!
//! A required column outside that set is a gap. The remediation is
//! `ALTER TABLE … REPLICA IDENTITY FULL`; it is only ever printed, never run —
//! it takes a lock and grows the table's WAL, so it is the operator's call.

use kernel::ColumnName;
use source::{PreImageColumns, PreImageGap, PreImageReport, Result, SourceError};
use sqlx::{PgPool, Row};

/// The identity setting and the columns it puts in an old tuple (for `d`, the
/// primary key; for `i`, the identity index). `None` when the table is absent.
const IDENTITY_SQL: &str = "SELECT c.relreplident::text AS identity, \
       ARRAY(SELECT a.attname::text FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = c.oid \
               AND CASE WHEN c.relreplident = 'i' THEN i.indisreplident ELSE i.indisprimary END \
       ) AS columns \
     FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
     WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p')";

/// Inspect every `required` table's replica identity, read-only.
pub(crate) async fn inspect(pool: &PgPool, required: &PreImageColumns) -> Result<PreImageReport> {
    let mut report = PreImageReport::default();
    for (table, columns) in required {
        let row = sqlx::query(IDENTITY_SQL)
            .bind(table.schema.as_ref())
            .bind(table.table.as_ref())
            .fetch_optional(pool)
            .await
            .map_err(|e| SourceError::Query(e.to_string()))?;
        let Some(row) = row else {
            continue;
        };
        let identity: String = row
            .try_get("identity")
            .map_err(|e| SourceError::Query(e.to_string()))?;
        let carried: Vec<String> = row
            .try_get("columns")
            .map_err(|e| SourceError::Query(e.to_string()))?;
        let missing = uncarried(&identity, &carried, columns);
        if !missing.is_empty() {
            report
                .remediation
                .push(alter_sql(table.schema.as_ref(), table.table.as_ref()));
            report.gaps.push(PreImageGap {
                table: table.clone(),
                missing,
            });
        }
    }
    Ok(report)
}

/// The `required` columns an old tuple under `identity` (with the identity's
/// own `carried` columns) does not hold.
pub(crate) fn uncarried(
    identity: &str,
    carried: &[String],
    required: &[ColumnName],
) -> Vec<ColumnName> {
    required
        .iter()
        .filter(|column| match identity {
            "f" => false,
            "d" | "i" => !carried.iter().any(|name| name == column.as_ref()),
            _ => true,
        })
        .cloned()
        .collect()
}

/// The statement that makes `schema.table`'s pre-image carry every column.
pub(crate) fn alter_sql(schema: &str, table: &str) -> String {
    format!(
        "ALTER TABLE {}.{} REPLICA IDENTITY FULL;",
        quote_ident(schema),
        quote_ident(table)
    )
}

fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
