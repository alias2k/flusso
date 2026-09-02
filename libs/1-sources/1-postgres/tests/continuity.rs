//! Continuity e2e: [`ChangeCapture::prepare`] must report [`Continuity::Fresh`]
//! exactly when it had to create the replication slot, and
//! [`Continuity::Resumed`] whenever the slot already exists — including on the
//! very next call, so a crash after a fresh `prepare` cannot re-trigger the
//! rebuild it announced (issue #120, case B).
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test continuity -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use sources_core::cdc::{ChangeCapture, Continuity};
use sources_postgres::{ReplicationConfig, WalChangeCapture};
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn prepare_reports_fresh_on_slot_creation_then_resumed() {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .with_cmd([
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_wal_senders=10",
            "-c",
            "max_replication_slots=10",
        ])
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let capture = WalChangeCapture::new(replication, url.clone());

    let slot_exists = || async {
        sqlx::query("SELECT count(*) AS n FROM pg_replication_slots WHERE slot_name = 'flusso'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get::<i64, _>("n")
            .unwrap()
            == 1
    };

    assert!(!slot_exists().await, "no slot before the first prepare");
    assert_eq!(
        capture.prepare().await.unwrap(),
        Continuity::Fresh,
        "creating the slot is a fresh start"
    );
    assert!(slot_exists().await, "prepare created the slot");

    // A second call — a restart, or a crash right after the first — finds the
    // slot it created and must *not* announce another fresh start.
    assert_eq!(capture.prepare().await.unwrap(), Continuity::Resumed);
    let another_run = WalChangeCapture::new(
        ReplicationConfig::new(
            "127.0.0.1",
            "postgres",
            "postgres",
            "postgres",
            "flusso",
            "flusso",
        )
        .with_port(port),
        url,
    );
    assert_eq!(another_run.prepare().await.unwrap(), Continuity::Resumed);

    // The operator drops the slot (a replaced database drops it too, since
    // Postgres refuses DROP DATABASE while a logical slot is bound to it).
    sqlx::query("SELECT pg_drop_replication_slot('flusso')")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        another_run.prepare().await.unwrap(),
        Continuity::Fresh,
        "a recreated slot is a fresh start again"
    );
}
