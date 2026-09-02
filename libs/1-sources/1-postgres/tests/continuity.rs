//! Continuity e2e: [`ChangeCapture::continuity`] must report
//! [`Continuity::Fresh`] exactly while the replication slot is missing and
//! [`Continuity::Resumed`] once it exists, without ever creating it itself;
//! [`ChangeCapture::prepare`] creates it, idempotently. Together that is what
//! lets the engine stage rebuilds *before* the slot comes into existence, so a
//! crash in between re-stages instead of trusting a stale seed (issue #120).
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
async fn continuity_is_read_only_and_prepare_creates_the_slot() {
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

    // One capture per "run", as each `flusso run` builds its own.
    let run = || {
        let replication = ReplicationConfig::new(
            "127.0.0.1",
            "postgres",
            "postgres",
            "postgres",
            "flusso",
            "flusso",
        )
        .with_port(port);
        WalChangeCapture::new(replication, url.clone())
    };
    let slot_exists = || async {
        sqlx::query("SELECT count(*) AS n FROM pg_replication_slots WHERE slot_name = 'flusso'")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get::<i64, _>("n")
            .unwrap()
            == 1
    };

    let first = run();
    assert_eq!(first.continuity().await.unwrap(), Continuity::Fresh);
    assert!(
        !slot_exists().await,
        "continuity is read-only: asking must not create the slot"
    );
    assert_eq!(
        first.continuity().await.unwrap(),
        Continuity::Fresh,
        "still fresh until prepare runs"
    );

    first.prepare().await.unwrap();
    assert!(slot_exists().await, "prepare created the slot");
    assert_eq!(first.continuity().await.unwrap(), Continuity::Resumed);
    first.prepare().await.unwrap();
    assert!(slot_exists().await, "prepare is idempotent");

    // A restart finds the slot the previous run created.
    let second = run();
    assert_eq!(second.continuity().await.unwrap(), Continuity::Resumed);

    // The operator drops the slot (a replaced database drops it too, since
    // Postgres refuses DROP DATABASE while a logical slot is bound to it).
    sqlx::query("SELECT pg_drop_replication_slot('flusso')")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        second.continuity().await.unwrap(),
        Continuity::Fresh,
        "a missing slot is a fresh start again"
    );
}
