//! ADR 0012 stage 10's spike S4, Postgres's half, against a live Postgres:
//! the role a deployment runs as (`LOGIN REPLICATION`, no superuser, no
//! `pg_monitor`) reads `max_slot_wal_keep_size` and how much WAL its own slot
//! may still retain (`pg_replication_slots.safe_wal_size`, `wal_status`), with
//! no grant. The headroom runs to the end of a WAL segment, so it can pass the
//! limit by up to one segment. The stage-10 note's §10 records what slice 4 reads.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-source-postgres --test detach_spikes --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// The setting, read three ways, and the slot's own view of it.
#[derive(Debug, PartialEq)]
struct Read {
    show: String,
    current: String,
    settings: (String, Option<String>),
    wal_status: Option<String>,
    safe_wal_size: Option<i64>,
}

async fn read(url: &str) -> Read {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .unwrap();
    let show: String = sqlx::query_scalar("SHOW max_slot_wal_keep_size")
        .fetch_one(&pool)
        .await
        .unwrap();
    let current: String = sqlx::query_scalar("SELECT current_setting('max_slot_wal_keep_size')")
        .fetch_one(&pool)
        .await
        .unwrap();
    let row =
        sqlx::query("SELECT setting, unit FROM pg_settings WHERE name = 'max_slot_wal_keep_size'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let slot = sqlx::query(
        "SELECT wal_status, safe_wal_size FROM pg_replication_slots WHERE slot_name = 'spike'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    Read {
        show,
        current,
        settings: (row.get("setting"), row.get("unit")),
        wal_status: slot.get("wal_status"),
        safe_wal_size: slot.get("safe_wal_size"),
    }
}

/// S4: the deployment's role reads the WAL limit and its slot's headroom,
/// unlimited (`-1`) and limited, with no grant beyond `REPLICATION`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn the_deployments_role_reads_the_slots_wal_limit() {
    let container = test_postgres::image()
        .with_cmd(["postgres", "-c", "wal_level=logical"])
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let admin_url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let admin = PgPoolOptions::new().connect(&admin_url).await.unwrap();
    let version: String = sqlx::query_scalar("SHOW server_version")
        .fetch_one(&admin)
        .await
        .unwrap();
    for statement in [
        "CREATE ROLE flusso_app LOGIN REPLICATION PASSWORD 'flusso'",
        "SELECT pg_create_logical_replication_slot('spike', 'pgoutput')",
    ] {
        sqlx::query(statement).execute(&admin).await.unwrap();
    }
    let role: (bool, bool) = sqlx::query_as(
        "SELECT rolsuper, pg_has_role('flusso_app', 'pg_monitor', 'member') \
         FROM pg_roles WHERE rolname = 'flusso_app'",
    )
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(
        role,
        (false, false),
        "the role has no superuser and no pg_monitor"
    );
    let app_url = format!("postgres://flusso_app:flusso@127.0.0.1:{port}/postgres");

    let unlimited = read(&app_url).await;
    assert_eq!(
        unlimited,
        Read {
            show: "-1".to_owned(),
            current: "-1".to_owned(),
            settings: ("-1".to_owned(), Some("MB".to_owned())),
            wal_status: Some("reserved".to_owned()),
            safe_wal_size: None,
        },
        "Postgres {version}"
    );

    sqlx::query("ALTER SYSTEM SET max_slot_wal_keep_size = '64MB'")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("SELECT pg_reload_conf()")
        .execute(&admin)
        .await
        .unwrap();
    let limited = read(&app_url).await;
    assert_eq!(limited.show, "64MB", "Postgres {version}: {limited:?}");
    assert_eq!(limited.current, "64MB", "Postgres {version}: {limited:?}");
    assert_eq!(
        limited.settings,
        ("64".to_owned(), Some("MB".to_owned())),
        "Postgres {version}: {limited:?}"
    );
    assert_eq!(
        limited.wal_status.as_deref(),
        Some("reserved"),
        "{limited:?}"
    );
    // Postgres counts the headroom to the end of the WAL segment the slot's
    // limit ends in, so it can pass the limit by up to one segment.
    let app = PgPoolOptions::new().connect(&app_url).await.unwrap();
    let segment: String = sqlx::query_scalar("SHOW wal_segment_size")
        .fetch_one(&app)
        .await
        .unwrap();
    assert_eq!(segment, "16MB", "Postgres {version}");
    let headroom = limited.safe_wal_size.unwrap();
    assert!(
        headroom > 0 && headroom <= (64 + 16) * 1024 * 1024,
        "Postgres {version}: {limited:?}"
    );
}
