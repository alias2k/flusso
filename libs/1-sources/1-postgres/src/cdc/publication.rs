//! Postgres's concrete backing for [`CaptureProvisioning`](sources_core::CaptureProvisioning):
//! a **publication**.
//!
//! Logical replication only streams a table that belongs to the subscribed
//! publication, so "can flusso stream these tables?" becomes "does the
//! publication cover them?". This module answers that read-only
//! ([`inspect_publication`]) and, when the role is privileged enough, closes the
//! gap ([`apply_publication`]). The neutral
//! [`CaptureProvisioning`](sources_core::CaptureProvisioning) impl on
//! [`WalChangeCapture`](super::WalChangeCapture) wraps both — callers never see
//! the word "publication".
//!
//! Why this is privilege-gated: creating a publication needs `CREATE` on the
//! database plus ownership of every listed table (or superuser); extending one
//! needs ownership of the publication plus the added tables. That is a stronger
//! grant than the `REPLICATION + SELECT` flusso otherwise runs as, so when the
//! role can't do it we report the exact SQL instead of failing.

use std::collections::{BTreeSet, HashSet};

use sources_core::{CoverageReport, QualifiedTable, Result, SourceError};
use sqlx::{AssertSqlSafe, PgPool, Row};

fn query_err(e: sqlx::Error) -> SourceError {
    SourceError::Query(e.to_string())
}

/// Quote a SQL identifier (double-quote, doubling any embedded quote). The
/// identifiers here are `nutype`-validated (lowercase `[a-z0-9_]`), so this is
/// belt-and-braces, but it keeps the generated DDL unambiguous.
fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// `"schema"."table"` — a qualified table reference for a `FOR TABLE` list.
fn qualified_ident(table: &QualifiedTable) -> String {
    format!(
        "{}.{}",
        quote_ident(table.schema.as_ref()),
        quote_ident(table.table.as_ref())
    )
}

/// The DDL that would cover `missing`: `CREATE PUBLICATION … FOR TABLE …` when
/// the publication does not exist yet, else `ALTER PUBLICATION … ADD TABLE …`.
fn publication_sql(exists: bool, name: &str, missing: &[QualifiedTable]) -> String {
    let tables = missing
        .iter()
        .map(qualified_ident)
        .collect::<Vec<_>>()
        .join(", ");
    if exists {
        format!(
            "ALTER PUBLICATION {} ADD TABLE {};",
            quote_ident(name),
            tables
        )
    } else {
        format!(
            "CREATE PUBLICATION {} FOR TABLE {};",
            quote_ident(name),
            tables
        )
    }
}

/// Read-only coverage + privilege report for publication `name` against the
/// `required` tables. Computes which tables are missing, whether the current
/// role could provision them, and the SQL that would.
pub(crate) async fn inspect_publication(
    pool: &PgPool,
    name: &str,
    required: &BTreeSet<QualifiedTable>,
) -> Result<CoverageReport> {
    // Existence + ownership of the publication in one probe: a row iff it exists,
    // the bool telling us whether the current role owns it (needed to ALTER it).
    let owns_pub: Option<bool> = sqlx::query_scalar(
        "SELECT pg_has_role(current_user, p.pubowner, 'USAGE') \
         FROM pg_publication p WHERE p.pubname = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(query_err)?;
    let exists = owns_pub.is_some();
    let owns_pub = owns_pub.unwrap_or(false);

    // Tables the publication already streams (FOR ALL TABLES is expanded here).
    let present_rows =
        sqlx::query("SELECT schemaname, tablename FROM pg_publication_tables WHERE pubname = $1")
            .bind(name)
            .fetch_all(pool)
            .await
            .map_err(query_err)?;
    let mut present_set: HashSet<(String, String)> = HashSet::new();
    for row in present_rows {
        let schema: String = row.try_get("schemaname").map_err(query_err)?;
        let table: String = row.try_get("tablename").map_err(query_err)?;
        present_set.insert((schema, table));
    }

    // Partition the requested set (iterating the BTreeSet keeps both lists sorted).
    let mut present = Vec::new();
    let mut missing = Vec::new();
    for table in required {
        let key = (
            table.schema.as_ref().to_owned(),
            table.table.as_ref().to_owned(),
        );
        if present_set.contains(&key) {
            present.push(table.clone());
        } else {
            missing.push(table.clone());
        }
    }
    let satisfied = missing.is_empty();

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

    // Ownership of each missing table — required to add it to a publication.
    let mut blockers = Vec::new();
    let mut owns_all_missing = true;
    for table in &missing {
        let owned: Option<bool> = sqlx::query_scalar(
            "SELECT pg_has_role(current_user, c.relowner, 'USAGE') \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p')",
        )
        .bind(table.schema.as_ref())
        .bind(table.table.as_ref())
        .fetch_optional(pool)
        .await
        .map_err(query_err)?;
        match owned {
            Some(true) => {}
            Some(false) => {
                owns_all_missing = false;
                blockers.push(format!("role does not own table {table}"));
            }
            None => {
                owns_all_missing = false;
                blockers.push(format!("table {table} does not exist"));
            }
        }
    }

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
        vec![publication_sql(exists, name, &missing)]
    };

    Ok(CoverageReport {
        satisfied,
        present,
        missing,
        manageable,
        blockers,
        remediation,
    })
}

/// Provision `missing` into publication `name`: create it `FOR TABLE …` when it
/// does not exist, else `ALTER … ADD TABLE …`. Idempotent — a no-op when
/// `missing` is empty. The caller is responsible for having checked privilege
/// (a denied grant surfaces here as a [`SourceError::Setup`]).
pub(crate) async fn apply_publication(
    pool: &PgPool,
    name: &str,
    missing: &[QualifiedTable],
) -> Result<()> {
    if missing.is_empty() {
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_publication WHERE pubname = $1)")
            .bind(name)
            .fetch_one(pool)
            .await
            .map_err(query_err)?;
    // The SQL is assembled from `nutype`-validated, double-quoted identifiers
    // (no user free-text reaches it), so it is safe to run as a dynamic string.
    let sql = publication_sql(exists, name, missing);
    sqlx::query(AssertSqlSafe(sql))
        .execute(pool)
        .await
        .map_err(|e| {
            SourceError::Setup(format!(
                "failed to {} publication '{name}': {e}",
                if exists { "extend" } else { "create" },
            ))
        })?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use schema_core::{DatabaseSchema, TableName};

    use super::*;

    fn table(schema: &str, name: &str) -> QualifiedTable {
        QualifiedTable::new(
            DatabaseSchema::try_new(schema).unwrap(),
            TableName::try_new(name).unwrap(),
        )
    }

    #[test]
    fn create_sql_lists_all_tables_quoted() {
        let missing = vec![table("public", "books"), table("public", "reviews")];
        assert_eq!(
            publication_sql(false, "flusso", &missing),
            r#"CREATE PUBLICATION "flusso" FOR TABLE "public"."books", "public"."reviews";"#
        );
    }

    #[test]
    fn alter_sql_adds_missing_tables() {
        let missing = vec![table("public", "orders")];
        assert_eq!(
            publication_sql(true, "flusso", &missing),
            r#"ALTER PUBLICATION "flusso" ADD TABLE "public"."orders";"#
        );
    }
}
