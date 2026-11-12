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
//! Every table an index reads should log its whole old row: a required link
//! column outside the old tuple is a correctness gap (a delete or re-parent
//! leaves the old parent stale), and anything short of the whole row loses the
//! unread-column skip. Both are closed by `ALTER TABLE … REPLICA IDENTITY FULL`
//! ([`apply`]).
//!
//! One identity is **kept** instead: `USING INDEX` on an index that carries
//! the whole primary key and every link (see [`uncarried`]), for every table,
//! link or not. Someone chose it on
//! purpose, usually to hold down WAL volume for every consumer of the table,
//! and flusso only loses the unread-column skip there, so it is reported under
//! `partial` with [`PreImageGap::kept`] set and never altered. `DEFAULT`,
//! `NOTHING`, an identity index that misses a link or any primary-key column
//! (the decoder would key changes by the index's columns instead), a dropped
//! identity index (Postgres then logs nothing), and a table with no primary
//! key are still set.
//! That needs ownership of the table (or superuser) — the same grant publication
//! management needs — and takes an `ACCESS EXCLUSIVE` lock, so it runs under a
//! short `lock_timeout` rather than queueing behind a long transaction. The
//! table then logs whole old rows on update and delete.
//!
//! A **partitioned** table is checked and set across its whole tree. Its
//! changes are published under its own name (`publish_via_partition_root`, see
//! [`super::publication`]), so the parent's identity decides which columns the
//! announced relation flags; but each partition's own identity decides what its
//! WAL logs, and Postgres doesn't pass a parent's setting down — not to
//! existing partitions, nor to ones created or attached later. So every member
//! that isn't `FULL` is altered, and a partition added later is caught at the
//! next start. The kept rule is per member: a parent or partition whose
//! identity index carries its primary key and every link stays as it is while its siblings that
//! miss one are set. A partitioned table whose every member logs the primary
//! key and every link is left alone: its updates carry no post-image, so the
//! whole row would buy nothing. A member that leaves out part of the primary
//! key is not: its changes are decoded under the parent's relation, which
//! flags the parent's key, so they would carry a null key (a delete removes
//! nothing). Such a member is a correctness gap (`missing_key`) and is set,
//! alone, even with no link missing. Postgres keeps a partition's primary key
//! index (it can't be dropped), so `DEFAULT` always logs it; `NOTHING` and a
//! dropped identity index make Postgres refuse the partition's updates and
//! deletes once it is published, and are set too.

use kernel::ColumnName;
use source::{PreImageColumns, PreImageGap, PreImageReport, QualifiedTable, Result, SourceError};
use sqlx::{PgPool, Row};

use super::quote_ident;

/// One relation's identity setting, the columns it puts in an old tuple (for
/// `d`, the primary key; for `i`, the identity index), and the primary key
/// itself — for the named table and, when it is partitioned, every partition
/// beneath it, parent first — foreign partitions left out, since they have no
/// identity to set and log no WAL. No rows when the table is absent.
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
       c.relreplident::text AS identity, c.relkind = 'p' AS partitioned, \
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
     WHERE c.relkind IN ('r', 'p') \
     ORDER BY m.level, n.nspname, c.relname";

/// One relation of a table's partition tree (or the table alone), as
/// [`MEMBERS_SQL`] reports it.
struct Member {
    schema: String,
    name: String,
    identity: ReplicaIdentity,
    partitioned: bool,
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

    /// An identity set on an index that carries the whole primary key and
    /// every `required` link: left as the operator chose it, never altered.
    ///
    /// The primary key is checked even with no link to carry. Without it the
    /// decoder keys every change by the identity columns, so live writes land
    /// under other document ids than the backfill's and a delete removes
    /// nothing; and an identity index that was dropped carries nothing at all.
    fn kept(&self, required: &[ColumnName]) -> bool {
        self.identity == ReplicaIdentity::Index
            && !self.primary_key.is_empty()
            && self
                .primary_key
                .iter()
                .all(|pk| self.carried.iter().any(|name| name == pk))
            && self.uncarried(required).is_empty()
    }

    /// Whether flusso sets this relation to `FULL` when its table is reported.
    fn to_alter(&self, required: &[ColumnName]) -> bool {
        self.identity != ReplicaIdentity::Full && !self.kept(required)
    }

    /// The `key` columns this relation's old tuple leaves out.
    fn uncarried_key<'k>(&self, key: &'k [String]) -> Vec<&'k String> {
        key.iter()
            .filter(|column| match self.identity {
                ReplicaIdentity::Full => false,
                ReplicaIdentity::Default | ReplicaIdentity::Index => !self.carried.contains(column),
                ReplicaIdentity::Nothing => true,
            })
            .collect()
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
                partitioned: row.try_get("partitioned")?,
                carried: row.try_get("columns")?,
                primary_key: row.try_get("primary_key")?,
                owned: row.try_get("owned")?,
            })
        })
        .collect()
}

/// Inspect every `required` table's replica identity, read-only. A table that
/// is not `FULL` throughout is listed: under [`PreImageReport::gaps`] when a
/// member misses a link column or part of the primary key, else under
/// [`PreImageReport::partial`]. A
/// partitioned table is checked across its whole tree: the parent's identity
/// flags the columns of the relation pgoutput announces, while each
/// partition's decides what its WAL actually logs.
pub(crate) async fn inspect(pool: &PgPool, required: &PreImageColumns) -> Result<PreImageReport> {
    let mut report = PreImageReport::default();
    for (table, columns) in required {
        let members = members(pool, table)
            .await
            .map_err(|e| SourceError::Query(e.to_string()))?;
        let Some(gap) = shortfall(table, columns, &members) else {
            continue;
        };
        if gap.missing.is_empty() && gap.missing_key.is_empty() {
            report.partial.push(gap);
        } else {
            report.gaps.push(gap);
        }
    }
    Ok(report)
}

/// What a table's `members` fall short of a `FULL` identity throughout, and
/// which of them flusso sets.
struct Plan<'m> {
    /// The `required` link columns some member's old tuple misses.
    missing: Vec<ColumnName>,
    /// The table's primary-key columns some member's old tuple misses.
    missing_key: Vec<ColumnName>,
    /// Every member short of `FULL`.
    short: Vec<&'m Member>,
    /// The members flusso sets to `FULL`.
    altered: Vec<&'m Member>,
}

/// The [`Plan`] for a table's `members`, parent first. `None` when every
/// member is `FULL`, and for a partitioned table whose every member logs the
/// primary key and every link: its updates always rebuild (they carry no
/// post-image), so the rest of the row would buy nothing.
///
/// The key is the parent's: a partition's changes are decoded under the
/// parent's relation, which flags the parent's primary key, so a partition
/// whose identity leaves it out yields a null key. When that is a partitioned
/// table's only shortfall, only those members are set.
fn plan<'m>(required: &[ColumnName], members: &'m [Member]) -> Option<Plan<'m>> {
    let short: Vec<&Member> = members
        .iter()
        .filter(|member| member.identity != ReplicaIdentity::Full)
        .collect();
    if short.is_empty() {
        return None;
    }
    let missing = required
        .iter()
        .filter(|column| {
            short
                .iter()
                .any(|member| member.uncarried(required).contains(column))
        })
        .cloned()
        .collect::<Vec<_>>();
    let key = members
        .first()
        .map(|root| root.primary_key.as_slice())
        .unwrap_or_default();
    let missing_key = key
        .iter()
        .filter(|column| {
            short
                .iter()
                .any(|member| member.uncarried_key(key).contains(column))
        })
        .filter_map(|column| ColumnName::try_new(column.clone()).ok())
        .collect::<Vec<_>>();
    let partitioned = members.first().is_some_and(|root| root.partitioned);
    let altered: Vec<&Member> = if partitioned && missing.is_empty() {
        if missing_key.is_empty() {
            return None;
        }
        short
            .iter()
            .copied()
            .filter(|member| !member.uncarried_key(key).is_empty())
            .collect()
    } else {
        short
            .iter()
            .copied()
            .filter(|member| member.to_alter(required))
            .collect()
    };
    Some(Plan {
        missing,
        missing_key,
        short,
        altered,
    })
}

/// What `table` (its `members`) falls short of a `FULL` identity throughout:
/// the `required` link columns and primary-key columns some member's old tuple
/// misses, the members the role can't alter, and the statements for every
/// member flusso sets. `None` when [`plan`] finds nothing to report. A table
/// whose only shortfall is a kept identity (see the module doc) is reported
/// with [`PreImageGap::kept`] set, its remediation still the statement an
/// operator could run by hand.
fn shortfall(
    table: &QualifiedTable,
    required: &[ColumnName],
    members: &[Member],
) -> Option<PreImageGap> {
    let Plan {
        missing,
        missing_key,
        short,
        altered,
    } = plan(required, members)?;
    let (listed, kept) = if altered.is_empty() {
        (
            short,
            Some(
                "replica identity is set on an index that carries the primary key and every \
                 parent link"
                    .to_owned(),
            ),
        )
    } else {
        (altered, None)
    };
    let blockers: Vec<String> = listed
        .iter()
        .filter(|member| !member.owned)
        .map(|member| format!("role does not own table {}.{}", member.schema, member.name))
        .collect();
    Some(PreImageGap {
        table: table.clone(),
        missing,
        missing_key,
        manageable: blockers.is_empty(),
        blockers,
        remediation: listed
            .iter()
            .map(|member| member.alter_sql())
            .collect::<Vec<_>>()
            .join(" "),
        kept,
    })
}

/// How long [`apply`] waits for each table lock before giving up.
const LOCK_TIMEOUT: &str = "5s";

/// `REPLICA IDENTITY FULL` on every member of `gap`'s table (the table alone,
/// or its partition tree) that [`plan`] sets for the `required` links and
/// that the role owns; the members it doesn't own are already the gap's
/// blockers. A gap that is [`kept`](PreImageGap::kept) alters
/// nothing. Each relation gets its own transaction under [`LOCK_TIMEOUT`], so a
/// busy partition neither holds the parent's lock while it waits nor undoes its
/// siblings. A timeout or a denied grant is a [`SourceError::Setup`] naming the
/// table; the rest still run.
pub(crate) async fn apply(pool: &PgPool, gap: &PreImageGap, required: &[ColumnName]) -> Result<()> {
    if gap.kept.is_some() {
        return Ok(());
    }
    let setup = |e: sqlx::Error| {
        SourceError::Setup(format!(
            "failed to set REPLICA IDENTITY FULL on {}: {e}",
            gap.table
        ))
    };
    let members = members(pool, &gap.table).await.map_err(setup)?;
    let Some(plan) = plan(required, &members) else {
        return Ok(());
    };
    let mut first_error = None;
    for member in plan.altered {
        if !member.owned {
            continue;
        }
        if let Err(e) = alter(pool, member).await {
            first_error.get_or_insert(setup(e));
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// `REPLICA IDENTITY FULL` on one relation, in its own transaction.
async fn alter(pool: &PgPool, member: &Member) -> std::result::Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SET LOCAL lock_timeout = '{LOCK_TIMEOUT}'"
    )))
    .execute(&mut *tx)
    .await?;
    // Built from catalog names via `quote_ident` (no user free-text reaches
    // it), so it is safe to run as a dynamic string.
    sqlx::query(sqlx::AssertSqlSafe(member.alter_sql()))
        .execute(&mut *tx)
        .await?;
    tx.commit().await
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

/// Warn that `table`'s replica identity (or one of its partitions') leaves out
/// part of its primary key, `missing`, and flusso won't fix it, for `reason`.
/// Shared by startup and the stream.
pub(crate) fn warn_unkeyed(
    table: &dyn std::fmt::Display,
    missing: &[ColumnName],
    reason: &str,
    remediation: &str,
) {
    tracing::warn!(
        table = %table,
        missing_key = %join_columns(missing),
        reason = %reason,
        remediation = %remediation,
        "replica identity leaves out the primary key: a delete there can't be addressed to its \
         document, which stays in the index; run the printed SQL",
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
