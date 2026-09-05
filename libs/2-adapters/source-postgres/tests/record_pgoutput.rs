//! Re-records the `pgoutput` bench fixture (`benches/fixtures/pgoutput.bin`)
//! from a real Postgres: a mix of inserts, updates, deletes, and a truncate
//! across tables keyed by `int`, `bigint`, `uuid`, and a composite key, with
//! one table on `REPLICA IDENTITY FULL` so full old tuples appear too.
//!
//! The fixture is the raw `XLogData` payloads, each prefixed with a `u32`
//! little-endian length. It is committed; run this only to refresh it, then
//! review the size and commit. Requires Docker. Ignored by default:
//!
//! ```text
//! cargo nextest run -p flusso-source-postgres --test record_pgoutput --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::path::PathBuf;
use std::time::Duration;

use pgwire_replication::{ReplicationClient, ReplicationConfig, ReplicationEvent};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const SLOT: &str = "bench";
const PUBLICATION: &str = "bench";

/// Transactions committed after the slot exists; the recorder stops once it has
/// seen this many `Commit`s.
const TRANSACTIONS: usize = 50;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker; rewrites benches/fixtures/pgoutput.bin"]
async fn record_pgoutput_fixture() {
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

    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text NOT NULL, full_name text, \
         tier text NOT NULL, score numeric(10,2), active boolean NOT NULL, \
         created_at timestamptz NOT NULL DEFAULT now(), meta jsonb)",
        "CREATE TABLE orders (id bigint PRIMARY KEY, user_id int NOT NULL, status text NOT NULL, \
         total numeric(12,2) NOT NULL, placed_at timestamptz NOT NULL DEFAULT now())",
        "CREATE TABLE sessions (id uuid PRIMARY KEY, user_id int NOT NULL, note text)",
        "CREATE TABLE user_tags (user_id int NOT NULL, tag_id int NOT NULL, PRIMARY KEY (user_id, tag_id))",
        "ALTER TABLE orders REPLICA IDENTITY FULL",
        "CREATE PUBLICATION bench FOR ALL TABLES",
        "SELECT pg_create_logical_replication_slot('bench', 'pgoutput')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    // Every statement below is one transaction (autocommit), so the recorder
    // can count commits. Values derive from the series index: deterministic.
    let mut statements = Vec::new();
    for batch in 0..10i32 {
        let lo = batch * 200 + 1;
        let hi = lo + 199;
        statements.push(format!(
            "INSERT INTO users (id, email, full_name, tier, score, active, meta) \
             SELECT g, 'user' || g || '@example.com', 'Customer ' || g, \
                    (ARRAY['free','pro','enterprise'])[1 + g % 3], (g * 7 % 1000) / 10.0, \
                    g % 2 = 0, jsonb_build_object('n', g, 'tags', ARRAY['a','b']) \
             FROM generate_series({lo}, {hi}) g"
        ));
        statements.push(format!(
            "INSERT INTO orders (id, user_id, status, total) \
             SELECT g * 10, (g % 2000) + 1, (ARRAY['pending','paid','shipped'])[1 + g % 3], (g * 13 % 50000) / 100.0 \
             FROM generate_series({lo}, {hi}) g"
        ));
        statements.push(format!(
            "INSERT INTO sessions (id, user_id, note) \
             SELECT gen_random_uuid(), g, 'session ' || g FROM generate_series({lo}, {hi}) g"
        ));
    }
    for batch in 0..5i32 {
        let lo = batch * 400 + 1;
        let hi = lo + 399;
        statements.push(format!(
            "INSERT INTO user_tags (user_id, tag_id) SELECT g, 1 + g % 5 FROM generate_series({lo}, {hi}) g"
        ));
        statements.push(format!(
            "UPDATE users SET tier = 'pro', score = score + 1 WHERE id BETWEEN {lo} AND {hi}"
        ));
        statements.push(format!(
            "UPDATE orders SET status = 'delivered' WHERE id BETWEEN {} AND {}",
            lo * 10,
            hi * 10
        ));
    }
    statements.push("UPDATE users SET id = id + 100000 WHERE id BETWEEN 1 AND 200".to_owned());
    statements.push("DELETE FROM sessions WHERE user_id BETWEEN 1 AND 600".to_owned());
    statements.push("DELETE FROM orders WHERE id BETWEEN 10 AND 4000".to_owned());
    statements.push("DELETE FROM user_tags WHERE user_id BETWEEN 1 AND 400".to_owned());
    statements.push("TRUNCATE sessions".to_owned());
    assert_eq!(statements.len(), TRANSACTIONS);
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .unwrap();
    }

    let config = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        SLOT,
        PUBLICATION,
    )
    .with_port(port);
    let mut client = ReplicationClient::connect(config).await.unwrap();

    let mut frames: Vec<u8> = Vec::new();
    let mut messages = 0usize;
    let mut commits = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    while commits < TRANSACTIONS {
        let event = tokio::time::timeout_at(deadline, client.recv())
            .await
            .expect("the stream stalled before every transaction arrived")
            .unwrap()
            .expect("the stream ended early");
        match event {
            ReplicationEvent::XLogData { data, .. } => {
                frames.extend_from_slice(&(data.len() as u32).to_le_bytes());
                frames.extend_from_slice(&data);
                messages += 1;
            }
            ReplicationEvent::Commit { .. } => commits += 1,
            _ => {}
        }
    }
    client.stop();

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/fixtures/pgoutput.bin");
    std::fs::write(&path, &frames).unwrap();
    assert!(messages > 5_000, "recorded only {messages} messages");
    eprintln!(
        "recorded {messages} pgoutput messages over {commits} transactions ({} bytes) to {}",
        frames.len(),
        path.display()
    );
}
