//! End-to-end tests for publication management (the [`CaptureProvisioning`]
//! impl) against a real Postgres in a container. These exercise the coverage
//! inspection, the privilege verdict, and the actual `CREATE`/`ALTER PUBLICATION`
//! provisioning that unit tests can only check by generated-string assertion.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test publication -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeSet;

use schema_core::{DatabaseSchema, TableName};
use sources_core::{CaptureProvisioning, QualifiedTable};
use sources_postgres::{ReplicationConfig, WalChangeCapture};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const PUBLICATION: &str = "flusso";

/// A capture pointed at `db` as `user`/`password`, with publication management
/// left to the per-call `manage` argument. The slot name is irrelevant —
/// provisioning never touches the slot.
fn capture(port: u16, user: &str, password: &str, db: &str) -> WalChangeCapture {
    let config =
        ReplicationConfig::new("127.0.0.1", user, password, db, "flusso-test", PUBLICATION)
            .with_port(port);
    let url = format!("postgres://{user}:{password}@127.0.0.1:{port}/{db}");
    WalChangeCapture::new(config, url)
}

fn required(tables: &[&str]) -> BTreeSet<QualifiedTable> {
    tables
        .iter()
        .map(|t| {
            QualifiedTable::new(
                DatabaseSchema::try_new("public").unwrap(),
                TableName::try_new(*t).unwrap(),
            )
        })
        .collect()
}

/// Tables the publication currently streams, as `schema.table` strings.
async fn published_tables(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT schemaname || '.' || tablename FROM pg_publication_tables \
         WHERE pubname = $1 ORDER BY 1",
    )
    .bind(PUBLICATION)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn privileged_role_creates_extends_and_respects_opt_out() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        "CREATE TABLE orders (id int PRIMARY KEY)",
        "CREATE TABLE items (id int PRIMARY KEY)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let cap = capture(port, "postgres", "postgres", "postgres");
    let two = required(&["users", "orders"]);

    // No publication yet: a gap, manageable (superuser), with CREATE remediation.
    let report = cap.inspect_coverage(&two).await.unwrap();
    assert!(!report.satisfied);
    assert_eq!(report.missing.len(), 2);
    assert!(report.manageable);
    assert!(report.remediation[0].contains("CREATE PUBLICATION"));

    // Opt-out (manage = false): inspect-only, nothing is created.
    cap.ensure_coverage(&two, false).await.unwrap();
    assert!(published_tables(&pool).await.is_empty());

    // manage = true: the publication is created covering both tables.
    cap.ensure_coverage(&two, true).await.unwrap();
    assert_eq!(
        published_tables(&pool).await,
        ["public.orders", "public.users"]
    );
    assert!(cap.inspect_coverage(&two).await.unwrap().satisfied);

    // A newly-referenced table is a partial gap → ALTER ADD, not re-CREATE.
    let three = required(&["users", "orders", "items"]);
    let report = cap.inspect_coverage(&three).await.unwrap();
    assert_eq!(
        report.missing,
        required(&["items"]).into_iter().collect::<Vec<_>>()
    );
    assert!(report.remediation[0].contains("ALTER PUBLICATION"));

    cap.ensure_coverage(&three, true).await.unwrap();
    assert_eq!(
        published_tables(&pool).await,
        ["public.items", "public.orders", "public.users"]
    );

    // Idempotent: ensuring an already-covered set is a no-op.
    cap.ensure_coverage(&three, true).await.unwrap();
    assert!(cap.inspect_coverage(&three).await.unwrap().satisfied);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn read_only_role_reports_gap_without_creating() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        // A least-privilege streaming role: can read, but owns nothing and
        // cannot create publications.
        "CREATE ROLE reader LOGIN PASSWORD 'reader'",
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO reader",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let cap = capture(port, "reader", "reader", "postgres");
    let one = required(&["users"]);

    let report = cap.inspect_coverage(&one).await.unwrap();
    assert!(!report.satisfied);
    assert!(
        !report.manageable,
        "a non-owner read-only role can't manage"
    );
    assert!(
        !report.blockers.is_empty(),
        "a non-manageable verdict must explain why"
    );
    assert!(report.remediation[0].contains("CREATE PUBLICATION"));

    // Even asked to manage, a read-only role must not (and cannot) create it.
    cap.ensure_coverage(&one, true).await.unwrap();
    assert!(
        published_tables(&pool).await.is_empty(),
        "nothing should have been created"
    );
}
