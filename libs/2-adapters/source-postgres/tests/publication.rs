//! End-to-end tests for publication management (the [`CaptureProvisioning`]
//! impl) against a real Postgres in a container. These exercise the coverage
//! inspection, the privilege verdict, and the actual `CREATE`/`ALTER PUBLICATION`
//! provisioning that unit tests can only check by generated-string assertion —
//! plus the replica-identity half: the report (`inspect_pre_image`) across
//! `DEFAULT`, `FULL`, `USING INDEX`, and a composite-key junction, and
//! `ensure_pre_image` setting `FULL` when privileged and refusing when not.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test publication -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeSet;

use kernel::{ColumnName, DatabaseSchema, TableName};
use source::{CaptureProvisioning, PreImageColumns, QualifiedTable};
use source_postgres::{ReplicationConfig, WalChangeCapture};
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
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int NOT NULL)",
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

    // Nor may it set a replica identity on a table it doesn't own.
    let child = links(&[("orders", "user_id")]);
    let report = cap.ensure_pre_image(&child, true).await.unwrap();
    let gap = report.gaps.first().unwrap();
    assert!(!gap.manageable);
    assert!(gap.blockers.iter().any(|b| b.contains("does not own")));
    assert!(!cap.inspect_pre_image(&child).await.unwrap().satisfied());
}

/// `table → [link columns]` under `public`, as `SourceSpec::pre_image_columns`
/// would name them.
fn links(entries: &[(&str, &str)]) -> PreImageColumns {
    entries
        .iter()
        .map(|(table, column)| {
            (
                required(&[table]).into_iter().next().unwrap(),
                vec![ColumnName::try_new(*column).unwrap()],
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn pre_image_gaps_follow_each_tables_replica_identity() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY)",
        "CREATE TABLE by_default (id int PRIMARY KEY, user_id int NOT NULL)",
        "CREATE TABLE by_full (id int PRIMARY KEY, user_id int NOT NULL)",
        "ALTER TABLE by_full REPLICA IDENTITY FULL",
        "CREATE TABLE by_index (id int PRIMARY KEY, user_id int NOT NULL)",
        "CREATE UNIQUE INDEX by_index_identity ON by_index (id, user_id)",
        "ALTER TABLE by_index REPLICA IDENTITY USING INDEX by_index_identity",
        "CREATE TABLE junction (user_id int, tag_id int, PRIMARY KEY (user_id, tag_id))",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let cap = capture(port, "postgres", "postgres", "postgres");
    let report = cap
        .inspect_pre_image(&links(&[
            ("by_default", "user_id"),
            ("by_full", "user_id"),
            ("by_index", "user_id"),
            ("junction", "user_id"),
            ("missing_table", "user_id"),
        ]))
        .await
        .unwrap();

    let gaps: Vec<String> = report.gaps.iter().map(|g| g.table.to_string()).collect();
    assert_eq!(gaps, ["public.by_default"]);
    let remediation = report.gaps.first().unwrap().remediation.clone();
    assert_eq!(
        remediation,
        "ALTER TABLE \"public\".\"by_default\" REPLICA IDENTITY FULL;"
    );

    assert!(
        report.gaps.first().unwrap().manageable,
        "superuser can set it"
    );

    // Opted out: reported, left alone.
    let default_only = links(&[("by_default", "user_id")]);
    cap.ensure_pre_image(&default_only, false).await.unwrap();
    assert!(
        !cap.inspect_pre_image(&default_only)
            .await
            .unwrap()
            .satisfied()
    );

    // Managed: flusso sets REPLICA IDENTITY FULL itself.
    let before = cap.ensure_pre_image(&default_only, true).await.unwrap();
    assert!(!before.satisfied(), "ensure returns what it found");
    assert!(
        cap.inspect_pre_image(&default_only)
            .await
            .unwrap()
            .satisfied()
    );
    let identity: String =
        sqlx::query_scalar("SELECT relreplident::text FROM pg_class WHERE relname = 'by_default'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(identity, "f");
}
