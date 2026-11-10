//! Postgres's concrete backing for [`CaptureProvisioning`](source::CaptureProvisioning):
//! a **publication**.
//!
//! Logical replication only streams a table that belongs to the subscribed
//! publication, so "can flusso stream these tables?" becomes "does the
//! publication cover them?". This module answers that read-only
//! ([`inspect_publication`]) and, when the role is privileged enough, closes the
//! gap ([`apply_publication`]).
//!
//! A **partitioned** table needs one more thing: by default pgoutput streams
//! its changes under each *partition's* name, which no index reads. A
//! publication flusso creates sets `publish_via_partition_root = true`, so they
//! arrive under the partitioned table's own name; an existing publication
//! without it is altered (or reported) only when an index reads a partitioned
//! table. The neutral
//! [`CaptureProvisioning`](source::CaptureProvisioning) impl on
//! [`WalChangeCapture`](super::WalChangeCapture) wraps both — callers never see
//! the word "publication".
//!
//! Why this is privilege-gated: creating a publication needs `CREATE` on the
//! database plus ownership of every listed table (or superuser); extending one
//! needs ownership of the publication plus the added tables. That is a stronger
//! grant than the `REPLICATION + SELECT` flusso otherwise runs as, so when the
//! role can't do it we report the exact SQL instead of failing.

use std::collections::BTreeSet;

use source::{CoverageReport, QualifiedTable, Result, SourceError};

use super::quote_ident;
use sqlx::{AssertSqlSafe, PgPool, Row};

fn query_err(e: sqlx::Error) -> SourceError {
    SourceError::Query(e.to_string())
}

/// `"schema"."table"` — a qualified table reference for a `FOR TABLE` list.
fn qualified_ident(table: &QualifiedTable) -> String {
    format!(
        "{}.{}",
        quote_ident(table.schema.as_ref()),
        quote_ident(table.table.as_ref())
    )
}

/// The DDL that would cover `missing` and, when `reroute`, publish partitioned
/// tables under their own name: `CREATE PUBLICATION … FOR TABLE … WITH
/// (publish_via_partition_root = true)` when the publication does not exist
/// yet (the option is always set on a new one), else `ALTER PUBLICATION … ADD
/// TABLE …` and/or `ALTER PUBLICATION … SET (publish_via_partition_root = true)`.
fn publication_sql(
    exists: bool,
    name: &str,
    missing: &[QualifiedTable],
    reroute: bool,
) -> Vec<String> {
    let name = quote_ident(name);
    let tables = missing
        .iter()
        .map(qualified_ident)
        .collect::<Vec<_>>()
        .join(", ");
    if !exists {
        return vec![format!(
            "CREATE PUBLICATION {name} FOR TABLE {tables} WITH ({VIA_ROOT} = true);"
        )];
    }
    let mut statements = Vec::new();
    if !missing.is_empty() {
        statements.push(format!("ALTER PUBLICATION {name} ADD TABLE {tables};"));
    }
    if reroute {
        statements.push(format!("ALTER PUBLICATION {name} SET ({VIA_ROOT} = true);"));
    }
    statements
}

/// The publication option that streams a partitioned table's changes under the
/// table's own name rather than each partition's.
const VIA_ROOT: &str = "publish_via_partition_root";

/// Per required table: whether the role owns it, whether it is partitioned, the
/// tables above it in a partition tree (`"schema.table"`, nearest first), and
/// whether the publication `$3` covers it. No row when the table is absent.
///
/// Membership is read from the catalog that records it — a table or one of its
/// ancestors listed (`pg_publication_rel`), `FOR ALL TABLES`, or, when
/// `schemas` (Postgres 15+), the schema of either under `FOR TABLES IN SCHEMA`
/// — never from `pg_publication_tables`, which lists a partitioned member's
/// leaves rather than the member unless the publication publishes via the root,
/// and lists nothing for a member with no (publishable) partitions yet.
fn table_sql(schemas: bool) -> String {
    let in_schema = if schemas {
        "OR EXISTS (SELECT 1 FROM pg_publication_namespace pn \
                    JOIN pg_class r ON r.relnamespace = pn.pnnspid \
                    WHERE pn.pnpubid = p.oid AND r.oid IN (SELECT oid FROM tree))"
    } else {
        ""
    };
    format!(
        "WITH target AS ( \
           SELECT c.oid, c.relkind, c.relowner FROM pg_class c \
           JOIN pg_namespace n ON n.oid = c.relnamespace \
           WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p') \
         ), tree AS ( \
           SELECT oid FROM target \
           UNION SELECT a.relid FROM target, pg_partition_ancestors(target.oid) a \
         ) \
         SELECT pg_has_role(current_user, target.relowner, 'USAGE') AS owned, \
           target.relkind = 'p' AS partitioned, \
           ARRAY(SELECT an.nspname || '.' || ac.relname \
                 FROM pg_partition_ancestors(target.oid) WITH ORDINALITY AS a(relid, depth) \
                 JOIN pg_class ac ON ac.oid = a.relid \
                 JOIN pg_namespace an ON an.oid = ac.relnamespace \
                 WHERE a.relid <> target.oid ORDER BY a.depth) AS ancestors, \
           COALESCE(( \
             SELECT p.puballtables \
               OR EXISTS (SELECT 1 FROM pg_publication_rel pr \
                          WHERE pr.prpubid = p.oid AND pr.prrelid IN (SELECT oid FROM tree)) \
               {in_schema} \
             FROM pg_publication p WHERE p.pubname = $3 \
           ), false) AS published \
         FROM target"
    )
}

/// Read-only coverage + privilege report for publication `name` against the
/// `required` tables. Computes which tables are missing, which are misrouted
/// (partitioned, under a publication that streams their partitions' names),
/// whether the current role could fix both, and the SQL that would.
pub(crate) async fn inspect_publication(
    pool: &PgPool,
    name: &str,
    required: &BTreeSet<QualifiedTable>,
) -> Result<CoverageReport> {
    // A row iff the publication exists; owning it is what ALTER needs.
    let publication = sqlx::query(
        "SELECT pg_has_role(current_user, p.pubowner, 'USAGE') AS owned, \
                p.pubviaroot AS via_root \
         FROM pg_publication p WHERE p.pubname = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(query_err)?;
    let (exists, owns_pub, via_root) = match publication {
        Some(row) => (
            true,
            row.try_get::<bool, _>("owned").map_err(query_err)?,
            row.try_get::<bool, _>("via_root").map_err(query_err)?,
        ),
        // A publication flusso creates publishes via the root.
        None => (false, false, true),
    };

    let version: i32 = sqlx::query_scalar("SELECT current_setting('server_version_num')::int")
        .fetch_one(pool)
        .await
        .map_err(query_err)?;
    let table_sql = table_sql(version >= 150_000);
    let names: BTreeSet<String> = required.iter().map(ToString::to_string).collect();

    // Partition the requested set (iterating the BTreeSet keeps the lists
    // sorted), noting ownership of each missing table — required to add it.
    let mut present = Vec::new();
    let mut missing = Vec::new();
    let mut misrouted = Vec::new();
    let mut blockers = Vec::new();
    let mut owns_all_missing = true;
    for table in required {
        let row = sqlx::query(AssertSqlSafe(table_sql.clone()))
            .bind(table.schema.as_ref())
            .bind(table.table.as_ref())
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(query_err)?;
        let Some(row) = row else {
            missing.push(table.clone());
            owns_all_missing = false;
            blockers.push(format!("table {table} does not exist"));
            continue;
        };
        let owned: bool = row.try_get("owned").map_err(query_err)?;
        let partitioned: bool = row.try_get("partitioned").map_err(query_err)?;
        let published: bool = row.try_get("published").map_err(query_err)?;
        let ancestors: Vec<String> = row.try_get("ancestors").map_err(query_err)?;
        if let Some(ancestor) = ancestors.iter().find(|a| names.contains(*a)) {
            tracing::warn!(
                table = %table,
                partitioned = %ancestor,
                "an index reads a partition directly, and another reads its partitioned table: \
                 published via the root, the partition's changes arrive as the partitioned \
                 table's, so the index on the partition sees none of them",
            );
        }
        if published {
            present.push(table.clone());
        } else {
            missing.push(table.clone());
            if !owned {
                owns_all_missing = false;
                blockers.push(format!("role does not own table {table}"));
            }
        }
        if partitioned && !via_root {
            misrouted.push(table.clone());
        }
    }
    let satisfied = missing.is_empty() && misrouted.is_empty();

    let is_super: bool = sqlx::query_scalar(
        "SELECT COALESCE(rolsuper, false) FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_optional(pool)
    .await
    .map_err(query_err)?
    .unwrap_or(false);
    let can_create_db: bool = sqlx::query_scalar(
        "SELECT has_database_privilege(current_user, current_database(), 'CREATE')",
    )
    .fetch_one(pool)
    .await
    .map_err(query_err)?;

    let manageable = if satisfied || is_super {
        true
    } else if exists {
        if !owns_pub {
            blockers.push(format!("role does not own publication \"{name}\""));
        }
        owns_all_missing && owns_pub
    } else {
        if !can_create_db {
            blockers.push("role lacks the CREATE privilege on the database".to_owned());
        }
        owns_all_missing && can_create_db
    };
    // Blockers explain a *negative* verdict only; a manageable gap has none.
    if manageable {
        blockers.clear();
    }

    let remediation = if satisfied {
        Vec::new()
    } else {
        publication_sql(exists, name, &missing, !misrouted.is_empty())
    };

    Ok(CoverageReport {
        satisfied,
        present,
        missing,
        misrouted,
        manageable,
        blockers,
        remediation,
    })
}

/// Provision `missing` into publication `name` and, when `reroute`, publish
/// partitioned tables via their root: create the publication when it does not
/// exist, else alter it — in one transaction. A no-op when there is nothing to
/// do. The caller is responsible for having checked privilege (a denied grant
/// surfaces here as a [`SourceError::Setup`]).
pub(crate) async fn apply_publication(
    pool: &PgPool,
    name: &str,
    missing: &[QualifiedTable],
    reroute: bool,
) -> Result<()> {
    if missing.is_empty() && !reroute {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_publication WHERE pubname = $1)")
            .bind(name)
            .fetch_one(pool)
            .await
            .map_err(query_err)?;
    let setup = |e: sqlx::Error| {
        SourceError::Setup(format!(
            "failed to {} publication '{name}': {e}",
            if exists { "alter" } else { "create" },
        ))
    };
    let mut tx = pool.begin().await.map_err(setup)?;
    // The SQL is assembled from `nutype`-validated, double-quoted identifiers
    // (no user free-text reaches it), so it is safe to run as a dynamic string.
    for statement in publication_sql(exists, name, missing, reroute) {
        sqlx::query(AssertSqlSafe(statement))
            .execute(&mut *tx)
            .await
            .map_err(setup)?;
    }
    tx.commit().await.map_err(setup)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
