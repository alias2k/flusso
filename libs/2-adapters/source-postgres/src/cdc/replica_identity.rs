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
//!
//! A **partitioned** table is checked and set across its whole tree. Its
//! changes are published under its own name (`publish_via_partition_root`, see
//! [`super::publication`]), so the parent's identity decides which columns the
//! announced relation flags; but each partition's own identity decides what its
//! WAL logs, and Postgres doesn't pass a parent's setting down — not to
//! existing partitions, nor to ones created or attached later. So every member
//! that falls short is altered, and a partition added later is caught at the
//! next start.

use kernel::ColumnName;
use source::{PreImageColumns, PreImageGap, PreImageReport, QualifiedTable, Result, SourceError};
use sqlx::{PgPool, Row};

use super::quote_ident;

/// One relation's identity setting, the columns it puts in an old tuple (for
/// `d`, the primary key; for `i`, the identity index), and the primary key
/// itself — for the named table and, when it is partitioned, every partition
/// beneath it, parent first. No rows when the table is absent.
const MEMBERS_SQL: &str = "WITH target AS ( \
         SELECT c.oid FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p') \
       ), members AS ( \
         SELECT target.oid AS relid, 0 AS level FROM target \
         UNION ALL \
         SELECT t.relid, t.level FROM target, pg_partition_tree(target.oid::regclass) t \
         WHERE t.level > 0 \
       ) \
     SELECT n.nspname::text AS schema, c.relname::text AS name, \
       c.relreplident::text AS identity, \
       ARRAY(SELECT a.attname::text FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = c.oid \
               AND CASE WHEN c.relreplident = 'i' THEN i.indisreplident ELSE i.indisprimary END \
       ) AS columns, \
       ARRAY(SELECT a.attname::text FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = c.oid AND i.indisprimary \
       ) AS primary_key, \
       pg_has_role(current_user, c.relowner, 'USAGE') AS owned \
     FROM members m JOIN pg_class c ON c.oid = m.relid \
     JOIN pg_namespace n ON n.oid = c.relnamespace \
     ORDER BY m.level, n.nspname, c.relname";

/// One relation of a table's partition tree (or the table alone), as
/// [`MEMBERS_SQL`] reports it.
struct Member {
    schema: String,
    name: String,
    identity: ReplicaIdentity,
    carried: Vec<String>,
    primary_key: Vec<String>,
    owned: bool,
}

impl Member {
    /// The `required` columns this relation's old tuple can't deliver.
    fn uncarried(&self, required: &[ColumnName]) -> Vec<ColumnName> {
        let carries = |column: &str| match self.identity {
            ReplicaIdentity::Full => true,
            ReplicaIdentity::Default | ReplicaIdentity::Index => {
                self.carried.iter().any(|name| name == column)
            }
            ReplicaIdentity::Nothing => false,
        };
        uncarried(carries, &self.primary_key, required)
    }

    fn alter_sql(&self) -> String {
        alter_sql(&self.schema, &self.name)
    }
}

/// `table` and, when it is partitioned, every partition beneath it.
async fn members<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    table: &QualifiedTable,
) -> std::result::Result<Vec<Member>, sqlx::Error> {
    let rows = sqlx::query(MEMBERS_SQL)
        .bind(table.schema.as_ref())
        .bind(table.table.as_ref())
        .fetch_all(executor)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(Member {
                schema: row.try_get("schema")?,
                name: row.try_get("name")?,
                identity: ReplicaIdentity::from_code(&row.try_get::<String, _>("identity")?),
                carried: row.try_get("columns")?,
                primary_key: row.try_get("primary_key")?,
                owned: row.try_get("owned")?,
            })
        })
        .collect()
}

/// Inspect every `required` table's replica identity, read-only. A partitioned
/// table is checked across its whole tree: the parent's identity flags the
/// columns of the relation pgoutput announces, while each partition's decides
/// what its WAL actually logs, so every one of them must carry the link.
pub(crate) async fn inspect(pool: &PgPool, required: &PreImageColumns) -> Result<PreImageReport> {
    let mut report = PreImageReport::default();
    for (table, columns) in required {
        let members = members(pool, table)
            .await
            .map_err(|e| SourceError::Query(e.to_string()))?;
        let short: Vec<(&Member, Vec<ColumnName>)> = members
            .iter()
            .map(|member| (member, member.uncarried(columns)))
            .filter(|(_, missing)| !missing.is_empty())
            .collect();
        if short.is_empty() {
            continue;
        }
        let missing: Vec<ColumnName> = columns
            .iter()
            .filter(|column| short.iter().any(|(_, missing)| missing.contains(column)))
            .cloned()
            .collect();
        let blockers = short
            .iter()
            .filter(|(member, _)| !member.owned)
            .map(|(member, _)| format!("role does not own table {}.{}", member.schema, member.name))
            .collect::<Vec<_>>();
        report.gaps.push(PreImageGap {
            table: table.clone(),
            missing,
            manageable: blockers.is_empty(),
            blockers,
            remediation: short
                .iter()
                .map(|(member, _)| member.alter_sql())
                .collect::<Vec<_>>()
                .join(" "),
        });
    }
    Ok(report)
}

/// How long [`apply`] waits for each table lock before giving up.
const LOCK_TIMEOUT: &str = "5s";

/// Close one gap: `REPLICA IDENTITY FULL` on its table — and on every partition
/// that falls short, for a partitioned one — in one transaction under
/// [`LOCK_TIMEOUT`]. A timeout or a denied grant is a [`SourceError::Setup`]
/// naming the table.
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
    for member in members(&mut *tx, &gap.table).await.map_err(setup)? {
        if member.uncarried(&gap.missing).is_empty() {
            continue;
        }
        // Built from catalog names via `quote_ident` (no user free-text
        // reaches it), so it is safe to run as a dynamic string.
        sqlx::query(sqlx::AssertSqlSafe(member.alter_sql()))
            .execute(&mut *tx)
            .await
            .map_err(setup)?;
    }
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

/// The `required` link columns an old tuple can't deliver — the one rule the
/// catalog report and the WAL decoder share.
///
/// A column is delivered when the identity `carries` it, but only while the
/// identity also carries the whole (non-empty) primary key: otherwise the
/// decoder keys changes by the identity columns themselves, and a pre-image
/// that adds nothing beyond that key is dropped.
pub(crate) fn uncarried(
    carries: impl Fn(&str) -> bool,
    primary_key: &[impl AsRef<str>],
    required: &[ColumnName],
) -> Vec<ColumnName> {
    let keyed_by_primary_key =
        !primary_key.is_empty() && primary_key.iter().all(|pk| carries(pk.as_ref()));
    required
        .iter()
        .filter(|column| !keyed_by_primary_key || !carries(column.as_ref()))
        .cloned()
        .collect()
}

/// `columns` as a comma-separated list, for log fields.
pub(crate) fn join_columns(columns: &[ColumnName]) -> String {
    columns
        .iter()
        .map(ColumnName::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Warn that `table`'s replica identity drops its parent `missing` link and
/// flusso won't fix it, for `reason`. Shared by startup and the stream.
pub(crate) fn warn_untraceable(
    table: &dyn std::fmt::Display,
    missing: &[ColumnName],
    reason: &str,
    remediation: &str,
) {
    tracing::warn!(
        table = %table,
        missing = %join_columns(missing),
        reason = %reason,
        remediation = %remediation,
        "replica identity does not carry the parent link: a deleted or re-parented row leaves \
         its old parent's document stale; run the printed SQL",
    );
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
