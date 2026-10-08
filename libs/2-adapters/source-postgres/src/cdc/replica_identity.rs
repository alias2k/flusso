//! Postgres's backing for the pre-image half of
//! [`CaptureProvisioning`](source::CaptureProvisioning): does each table's
//! **replica identity** carry the columns reverse resolution needs from a
//! change's pre-image, and if not, set it.
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
//! A required column outside that set is a gap, closed by
//! `ALTER TABLE … REPLICA IDENTITY FULL` ([`apply`]). That needs ownership of the
//! table (or superuser) — the same grant publication management needs — and
//! takes an `ACCESS EXCLUSIVE` lock, so it runs under a short `lock_timeout`
//! rather than queueing behind a long transaction. The table then logs whole old
//! rows on update and delete.

use kernel::ColumnName;
use source::{PreImageColumns, PreImageGap, PreImageReport, Result, SourceError};
use sqlx::{PgPool, Row};

use super::quote_ident;

/// The identity setting and the columns it puts in an old tuple (for `d`, the
/// primary key; for `i`, the identity index). `None` when the table is absent.
const IDENTITY_SQL: &str = "SELECT c.relreplident::text AS identity, \
       ARRAY(SELECT a.attname::text FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = c.oid \
               AND CASE WHEN c.relreplident = 'i' THEN i.indisreplident ELSE i.indisprimary END \
       ) AS columns, \
       pg_has_role(current_user, c.relowner, 'USAGE') AS owned \
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
        let owned: bool = row
            .try_get("owned")
            .map_err(|e| SourceError::Query(e.to_string()))?;
        let missing = uncarried(ReplicaIdentity::from_code(&identity), &carried, columns);
        if !missing.is_empty() {
            report.gaps.push(PreImageGap {
                table: table.clone(),
                missing,
                manageable: owned,
                blockers: if owned {
                    Vec::new()
                } else {
                    vec![format!("role does not own table {table}")]
                },
                remediation: alter_sql(table.schema.as_ref(), table.table.as_ref()),
            });
        }
    }
    Ok(report)
}

/// How long [`apply`] waits for the table lock before giving up.
const LOCK_TIMEOUT: &str = "5s";

/// Close one gap: `REPLICA IDENTITY FULL` on its table, in its own transaction
/// under [`LOCK_TIMEOUT`]. A timeout or a denied grant is a
/// [`SourceError::Setup`] naming the table.
pub(crate) async fn apply(pool: &PgPool, gap: &PreImageGap) -> Result<()> {
    let setup = |e: sqlx::Error| {
        SourceError::Setup(format!(
            "failed to set REPLICA IDENTITY FULL on {}: {e}",
            gap.table
        ))
    };
    let mut tx = pool.begin().await.map_err(setup)?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SET LOCAL lock_timeout = '{LOCK_TIMEOUT}'"
    )))
    .execute(&mut *tx)
    .await
    .map_err(setup)?;
    // `remediation` is built from `nutype`-validated, double-quoted identifiers
    // (no user free-text reaches it), so it is safe to run as a dynamic string.
    sqlx::query(sqlx::AssertSqlSafe(gap.remediation.clone()))
        .execute(&mut *tx)
        .await
        .map_err(setup)?;
    tx.commit().await.map_err(setup)
}

/// A table's `pg_class.relreplident`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplicaIdentity {
    /// `d`: the primary key.
    Default,
    /// `i`: the columns of the identity index.
    Index,
    /// `f`: every column.
    Full,
    /// `n`, or a code this version doesn't know: nothing.
    Nothing,
}

impl ReplicaIdentity {
    pub(crate) fn from_code(code: &str) -> Self {
        match code {
            "d" => Self::Default,
            "i" => Self::Index,
            "f" => Self::Full,
            _ => Self::Nothing,
        }
    }
}

/// The `required` columns an old tuple under `identity` (with the identity's
/// own `carried` columns) does not hold.
pub(crate) fn uncarried(
    identity: ReplicaIdentity,
    carried: &[String],
    required: &[ColumnName],
) -> Vec<ColumnName> {
    required
        .iter()
        .filter(|column| match identity {
            ReplicaIdentity::Full => false,
            ReplicaIdentity::Default | ReplicaIdentity::Index => {
                !carried.iter().any(|name| name == column.as_ref())
            }
            ReplicaIdentity::Nothing => true,
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
