//! Full WAL pipeline e2e: a real Postgres (logical replication) → the ingest
//! engine → a lane → a sink engine over a recording sink. Inserts and deletes
//! on the source must surface as document upserts and tombstones, and an
//! update that changes no column a document reads must surface as nothing.
//! Aggregates across several hops (`via`) rebuild their parent from a change
//! on any hop, including hops back into the root's own table and hops under a
//! nested relation. Relations joined `on` extra column pairs rebuild from a
//! change on either side of a pair. An aggregate over an arithmetic
//! expression rebuilds from a change to any of its operands, and from nothing
//! else.
//! A child kept on a `USING INDEX` identity carrying its link still traces
//! deleted and re-parented rows to the old parent. A junction without a
//! primary key, keyed by its whole row, rebuilds every parent its rows link,
//! nulls in its other columns included; a `count:` over a child without one
//! stops the engine on its first change instead of resolving to nothing. A
//! root `primary_key` other than the table's own key writes no document.
//! Each key type the document builder reads works in every relation position
//! and the backfill, with the same document ids from both.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p engine --test wal -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::{IngestEngine, SinkControl, SinkEngine};
use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, DatabaseSchema, Envelope, Field, FieldSource,
    Filter, FilterOp, FilterValue, FlussoType, IndexSchema, Join, JoinKind, OrderBy, RawFilter,
    RawFilterValue, Relation, SinkName, Through, ValueOpFilter, ViaHop,
};
use sink::{Result as SinkResult, Sink};
use source::cdc::ChangeCapture;
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceSpec};
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::postgres::PgPoolOptions;
use stream::Stream;
use stream_channel::ChannelStream;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

mod common;
use common::{column, column_field, field, index_name, table, users_spec};

/// A sink that records the operations it receives, for assertions. A batch's
/// operations become visible together at its flush, so seeing a later change
/// means every operation of its batch is already recorded.
#[derive(Debug)]
struct RecordingSink {
    ops: Arc<Mutex<Vec<String>>>,
    applied: Mutex<Vec<String>>,
}

impl RecordingSink {
    fn new(ops: &Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            ops: Arc::clone(ops),
            applied: Mutex::default(),
        }
    }
}

#[async_trait]
impl Sink for RecordingSink {
    async fn apply(&self, envelope: &Envelope) -> SinkResult<()> {
        self.applied.lock().unwrap().push(format!(
            "{} {} {}",
            envelope.op,
            envelope.index.as_ref(),
            envelope.id
        ));
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> SinkResult<sink::FlushReport> {
        let applied = std::mem::take(&mut *self.applied.lock().unwrap());
        self.ops.lock().unwrap().extend(applied);
        Ok(sink::FlushReport::clean())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn wal_changes_flow_through_the_engine() {
    // Postgres with logical replication enabled. PG 14+ is required: the
    // replication client requests the pgoutput `messages` option, added in 14.
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

    // Schema, publication, and a logical slot using the pgoutput plugin.
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text)",
        "CREATE PUBLICATION flusso FOR TABLE users",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    // Engine: real WAL capture + real document builder + a recording sink.
    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(users_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        documents,
        sink,
    ));

    // Changes after slot creation are captured and replayed through the engine.
    sqlx::query("INSERT INTO users (id, email) VALUES (1, 'ada@x.io')")
        .execute(&pool)
        .await
        .unwrap();
    expect_op(&mut engine, &recorded, "upsert users 1").await;

    sqlx::query("DELETE FROM users WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    expect_op(&mut engine, &recorded, "delete users 1").await;

    engine.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn backfill_seeds_preexisting_rows() {
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
        "CREATE TABLE users (id int PRIMARY KEY, email text)",
        "CREATE PUBLICATION flusso FOR TABLE users",
        // Rows that exist *before* the slot — only a backfill can surface them,
        // since they are behind the slot's confirmed position in the WAL.
        "INSERT INTO users (id, email) VALUES (1, 'ada@x.io'), (2, 'grace@x.io')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(users_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        documents,
        sink,
    ));

    // Both pre-existing rows are seeded by the backfill.
    expect_op(&mut engine, &recorded, "upsert users 1").await;
    expect_op(&mut engine, &recorded, "upsert users 2").await;

    engine.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn an_update_of_unread_columns_rebuilds_nothing() {
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
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int, total int, status text, \
         placed_at int, note text)",
        "CREATE TABLE payments (id int PRIMARY KEY, user_id int, amount int, memo text)",
        // Left on the DEFAULT identity: an update carries no pre-image.
        "CREATE TABLE visits (id int PRIMARY KEY, user_id int, path text, seen int)",
        "ALTER TABLE users REPLICA IDENTITY FULL",
        "ALTER TABLE orders REPLICA IDENTITY FULL",
        "ALTER TABLE payments REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE users, orders, payments, visits",
        // Seeded before the slot, so only the backfill emits them: nothing of
        // the setup can arrive live after the checks below start.
        "INSERT INTO users VALUES (1, 'ada@x.io', 0), (2, 'grace@x.io', 0)",
        "INSERT INTO orders VALUES (10, 1, 5, 'open', 1, '')",
        "INSERT INTO payments VALUES (20, 1, 7, '')",
        "INSERT INTO visits VALUES (30, 1, '/', 0)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(users_with_relations_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        documents,
        sink,
    ));

    let exec = |sql: &'static str| {
        let pool = pool.clone();
        async move { sqlx::raw_sql(sql).execute(&pool).await.unwrap() }
    };
    expect_op(&mut engine, &recorded, "upsert users 1").await;
    expect_op(&mut engine, &recorded, "upsert users 2").await;

    // Unread columns on the root, a joined child, and an aggregated child: no
    // document is rebuilt. A marker user proves the stream got past them.
    for (marker, sql) in [
        (100, "UPDATE users SET last_seen = 1 WHERE id = 1"),
        (101, "UPDATE orders SET note = 'gift' WHERE id = 10"),
        (102, "UPDATE payments SET memo = 'card' WHERE id = 20"),
    ] {
        recorded.lock().unwrap().clear();
        exec(sql).await;
        sqlx::query("INSERT INTO users VALUES ($1, 'marker', 0)")
            .bind(marker)
            .execute(&pool)
            .await
            .unwrap();
        expect_op(&mut engine, &recorded, &format!("upsert users {marker}")).await;
        assert_eq!(
            *recorded.lock().unwrap(),
            vec![format!("upsert users {marker}")],
            "`{sql}` changed no column the document reads"
        );
    }

    // Every kind of read column still rebuilds, and so does an update on a
    // table without a pre-image.
    for (sql, rebuilt) in [
        (
            "UPDATE users SET email = 'ada@y.io' WHERE id = 1",
            &["1"][..],
        ),
        ("UPDATE orders SET total = 6 WHERE id = 10", &["1"]),
        ("UPDATE orders SET status = 'void' WHERE id = 10", &["1"]),
        ("UPDATE orders SET placed_at = 2 WHERE id = 10", &["1"]),
        ("UPDATE payments SET amount = 8 WHERE id = 20", &["1"]),
        ("UPDATE visits SET seen = 1 WHERE id = 30", &["1"]),
        ("UPDATE orders SET user_id = 2 WHERE id = 10", &["1", "2"]),
    ] {
        recorded.lock().unwrap().clear();
        exec(sql).await;
        for id in rebuilt {
            expect_op(&mut engine, &recorded, &format!("upsert users {id}")).await;
        }
    }

    engine.abort();
}

/// With replica identity management on (the default), flusso sets every table
/// an index reads to `REPLICA IDENTITY FULL` before the slot, so an update of
/// an unread column on the root or on a `belongs_to` target rebuilds nothing,
/// with no `ALTER TABLE` in the setup.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn managed_replica_identity_skips_unread_columns_on_the_root_and_a_belongs_to_target() {
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
        "CREATE TABLE teams (id int PRIMARY KEY, name text, motto text)",
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int, team_id int)",
        "CREATE PUBLICATION flusso FOR TABLE teams, users",
        "INSERT INTO teams VALUES (1, 'core', '')",
        "INSERT INTO users VALUES (1, 'ada@x.io', 0, 1)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let spec = users_with_team_spec();
    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let capture = WalChangeCapture::new(replication, url.clone())
        .with_pre_image_management(spec.pre_images(), true);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(spec))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(Arc::new(capture), documents, sink));
    expect_op(&mut engine, &recorded, "upsert users 1").await;

    let identities: Vec<(String, String)> = sqlx::query_as(
        "SELECT relname::text, relreplident::text FROM pg_class \
         WHERE relname IN ('teams', 'users') ORDER BY relname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        identities,
        [
            ("teams".to_owned(), "f".to_owned()),
            ("users".to_owned(), "f".to_owned())
        ]
    );

    let marker_sql = "INSERT INTO users (id) VALUES ($1)";
    let mut marker = 100;
    let mut rebuilt = async |sql: &'static str| {
        marker += 1;
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            sql,
            marker_sql,
            marker,
            &format!("upsert users {marker}"),
        )
        .await
    };
    assert_eq!(
        rebuilt("UPDATE users SET last_seen = 1 WHERE id = 1").await,
        Vec::<String>::new(),
        "no field reads the root's `last_seen`"
    );
    assert_eq!(
        rebuilt("UPDATE teams SET motto = 'ship it' WHERE id = 1").await,
        Vec::<String>::new(),
        "no field reads the team's `motto`"
    );
    assert_eq!(
        rebuilt("UPDATE teams SET name = 'platform' WHERE id = 1").await,
        sorted(&["upsert users 1"])
    );
    assert_eq!(
        rebuilt("UPDATE users SET email = 'ada@y.io' WHERE id = 1").await,
        sorted(&["upsert users 1"])
    );

    engine.abort();
}

/// A child table whose `USING INDEX` identity already carries its parent link
/// is kept as it is by replica identity management, while its siblings are set
/// to `FULL`. Its deletes and re-parents still reach the old parent through
/// the identity index; only the unread-column skip is lost there. The root,
/// kept on an identity index that includes its primary key, still keys its
/// updates and deletes by that key.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_kept_identity_index_still_traces_deleted_and_reparented_rows() {
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
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int NOT NULL DEFAULT 0)",
        "CREATE UNIQUE INDEX users_identity ON users (last_seen, id)",
        "ALTER TABLE users REPLICA IDENTITY USING INDEX users_identity",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int NOT NULL, total int, \
         status text, placed_at int, note text)",
        "CREATE UNIQUE INDEX orders_identity ON orders (id, user_id)",
        "ALTER TABLE orders REPLICA IDENTITY USING INDEX orders_identity",
        "CREATE TABLE payments (id int PRIMARY KEY, user_id int, amount int, memo text)",
        "CREATE TABLE visits (id int PRIMARY KEY, user_id int, path text, seen int)",
        "CREATE PUBLICATION flusso FOR TABLE users, orders, payments, visits",
        "INSERT INTO users VALUES (1, 'ada@x.io', 0), (2, 'grace@x.io', 0)",
        "INSERT INTO orders VALUES (10, 1, 5, 'open', 1, ''), (11, 1, 6, 'open', 2, '')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let spec = users_with_relations_spec();
    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let capture = WalChangeCapture::new(replication, url.clone())
        .with_pre_image_management(spec.pre_images(), true);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(spec))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(Arc::new(capture), documents, sink));
    expect_op(&mut engine, &recorded, "upsert users 1").await;
    expect_op(&mut engine, &recorded, "upsert users 2").await;

    let identities: Vec<(String, String)> = sqlx::query_as(
        "SELECT relname::text, relreplident::text FROM pg_class \
         WHERE relname IN ('users', 'orders', 'payments', 'visits') ORDER BY relname",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let identities: Vec<(&str, &str)> = identities
        .iter()
        .map(|(name, identity)| (name.as_str(), identity.as_str()))
        .collect();
    assert_eq!(
        identities,
        [
            ("orders", "i"),
            ("payments", "f"),
            ("users", "i"),
            ("visits", "f")
        ]
    );

    let marker_sql = "INSERT INTO users (id) VALUES ($1)";
    let mut marker = 100;
    let mut rebuilt = async |sql: &'static str| {
        marker += 1;
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            sql,
            marker_sql,
            marker,
            &format!("upsert users {marker}"),
        )
        .await
    };
    assert_eq!(
        rebuilt("UPDATE orders SET user_id = 2 WHERE id = 10").await,
        sorted(&["upsert users 1", "upsert users 2"]),
        "the re-parented order leaves its old parent"
    );
    assert_eq!(
        rebuilt("DELETE FROM orders WHERE id = 11").await,
        sorted(&["upsert users 1"]),
        "the deleted order leaves its parent"
    );
    assert_eq!(
        rebuilt("UPDATE orders SET note = 'gift' WHERE id = 10").await,
        sorted(&["upsert users 2"]),
        "no whole old row, so an unread column still rebuilds"
    );
    assert_eq!(
        rebuilt("UPDATE users SET last_seen = 7 WHERE id = 1").await,
        sorted(&["upsert users 1"]),
        "a kept root is keyed by its primary key, not its identity index"
    );
    assert_eq!(
        rebuilt("DELETE FROM users WHERE id = 1").await,
        sorted(&["delete users 1"]),
        "a delete on a kept root removes the document the backfill wrote"
    );

    engine.abort();
}

/// A raw filter is pasted into a correlated subquery, so it can read the root
/// row: a change to that root column must rebuild even though no field reads it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_raw_filter_reading_the_root_row_rebuilds_on_that_column() {
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
        "CREATE TABLE users (id int PRIMARY KEY, email text, trial_start int)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int, total int, placed_at int)",
        "ALTER TABLE users REPLICA IDENTITY FULL",
        "ALTER TABLE orders REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE users, orders",
        "INSERT INTO users VALUES (1, 'ada@x.io', 0)",
        "INSERT INTO orders VALUES (10, 1, 5, 3)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(users_with_recent_orders_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        documents,
        sink,
    ));
    expect_op(&mut engine, &recorded, "upsert users 1").await;

    recorded.lock().unwrap().clear();
    sqlx::query("UPDATE users SET trial_start = 5 WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users VALUES (100, 'marker', 0)")
        .execute(&pool)
        .await
        .unwrap();
    expect_op(&mut engine, &recorded, "upsert users 100").await;
    assert!(
        recorded
            .lock()
            .unwrap()
            .contains(&"upsert users 1".to_owned()),
        "the raw filter reads `root.trial_start`, so changing it rebuilds: {:?}",
        recorded.lock().unwrap()
    );

    engine.abort();
}

/// users → teams → projects → tasks → (task_tags) → tags. Every hop is a
/// foreign key on the lower table; a change on any of them rebuilds the user
/// it sits under, and its old user too when it moved or went away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn an_aggregate_via_hops_rebuilds_its_parent_from_every_hop() {
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
        "CREATE TABLE users (id int PRIMARY KEY, email text)",
        "CREATE TABLE teams (id int PRIMARY KEY, owner_id int, archived boolean, name text)",
        "CREATE TABLE projects (id int PRIMARY KEY, team_id int, archived boolean, note text)",
        "CREATE TABLE tasks (id int PRIMARY KEY, project_id int, status text, memo text)",
        "CREATE TABLE tags (id int PRIMARY KEY, label text)",
        "CREATE TABLE task_tags (id int PRIMARY KEY, task_id int, tag_id int)",
        "ALTER TABLE teams REPLICA IDENTITY FULL",
        "ALTER TABLE projects REPLICA IDENTITY FULL",
        "ALTER TABLE tasks REPLICA IDENTITY FULL",
        "ALTER TABLE task_tags REPLICA IDENTITY FULL",
        "ALTER TABLE tags REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE users, teams, projects, tasks, tags, task_tags",
        "INSERT INTO users VALUES (1, 'ada@x.io'), (2, 'grace@x.io')",
        "INSERT INTO teams VALUES (10, 1, false, ''), (20, 2, false, '')",
        "INSERT INTO projects VALUES (100, 10, false, ''), (200, 20, false, '')",
        "INSERT INTO tasks VALUES (1000, 100, 'open', ''), (2000, 200, 'open', '')",
        "INSERT INTO tags VALUES (1, 'urgent'), (2, 'later')",
        "INSERT INTO task_tags VALUES (2, 1000, 2)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(users_with_task_flags_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        sink,
    ));
    expect_op(&mut engine, &recorded, "upsert users 1").await;
    expect_op(&mut engine, &recorded, "upsert users 2").await;

    // (team count, active project count, has a completed task, tag count)
    let flags = |user: i32| {
        let documents = Arc::clone(&documents);
        async move {
            let id = DocumentId {
                index: index_name("users"),
                key: RowKey(vec![(column("id"), kernel::GenericValue::Int(user))]),
            };
            let Document::Upsert { body, .. } = documents.build(&id).await.unwrap() else {
                panic!("user {user} is not a document");
            };
            let body = sink::to_json(&body);
            (
                body["hasTeam"].as_bool().unwrap(),
                body["activeProjects"].as_i64().unwrap(),
                body["hasCompletedTask"].as_bool().unwrap(),
                body["tagCount"].as_i64().unwrap(),
            )
        }
    };
    assert_eq!(flags(1).await, (true, 1, false, 1));
    assert_eq!(flags(2).await, (true, 1, false, 0));

    let exec = |sql: &'static str| {
        let pool = pool.clone();
        async move { sqlx::raw_sql(sql).execute(&pool).await.unwrap() }
    };

    // An unread column on every hop rebuilds nothing.
    for (marker, sql) in [
        (900, "UPDATE teams SET name = 'core' WHERE id = 10"),
        (901, "UPDATE projects SET note = 'q3' WHERE id = 100"),
        (902, "UPDATE tasks SET memo = 'later' WHERE id = 1000"),
        (903, "UPDATE tags SET label = 'now' WHERE id = 2"),
    ] {
        recorded.lock().unwrap().clear();
        exec(sql).await;
        sqlx::query("INSERT INTO users VALUES ($1, 'marker')")
            .bind(marker)
            .execute(&pool)
            .await
            .unwrap();
        expect_op(&mut engine, &recorded, &format!("upsert users {marker}")).await;
        assert_eq!(
            *recorded.lock().unwrap(),
            vec![format!("upsert users {marker}")],
            "`{sql}` changed no column the document reads"
        );
    }

    // Each change rebuilds exactly these users, which then read these values.
    type Flags = (bool, i64, bool, i64);
    let steps: [(&str, &[(i32, Flags)]); 16] = [
        // A filter column on the aggregated table, three hops down.
        (
            "UPDATE tasks SET status = 'completed' WHERE id = 1000",
            &[(1, (true, 1, true, 1))],
        ),
        // A filter column on the first hop.
        (
            "UPDATE teams SET archived = true WHERE id = 10",
            &[(1, (true, 0, false, 1))],
        ),
        (
            "UPDATE teams SET archived = false WHERE id = 10",
            &[(1, (true, 1, true, 1))],
        ),
        // A filter column on the middle hop.
        (
            "UPDATE projects SET archived = true WHERE id = 100",
            &[(1, (true, 0, true, 1))],
        ),
        (
            "UPDATE projects SET archived = false WHERE id = 100",
            &[(1, (true, 1, true, 1))],
        ),
        // A junction row past the last hop: inserted, moved to another
        // user's task, deleted.
        (
            "INSERT INTO task_tags VALUES (1, 1000, 1)",
            &[(1, (true, 1, true, 2))],
        ),
        (
            "UPDATE task_tags SET task_id = 2000 WHERE id = 1",
            &[(1, (true, 1, true, 1)), (2, (true, 1, false, 1))],
        ),
        (
            "DELETE FROM task_tags WHERE id = 1",
            &[(2, (true, 1, false, 0))],
        ),
        // Re-parenting on each hop moves the flags between users.
        (
            "UPDATE tasks SET project_id = 200 WHERE id = 1000",
            &[(1, (true, 1, false, 0)), (2, (true, 1, true, 1))],
        ),
        (
            "UPDATE projects SET team_id = 10 WHERE id = 200",
            &[(1, (true, 2, true, 1)), (2, (true, 0, false, 0))],
        ),
        (
            "UPDATE teams SET owner_id = 2 WHERE id = 10",
            &[(1, (false, 0, false, 0)), (2, (true, 2, true, 1))],
        ),
        // Inserts and deletes on each hop.
        (
            "INSERT INTO tasks VALUES (1001, 100, 'completed', '')",
            &[(2, (true, 2, true, 1))],
        ),
        (
            "DELETE FROM tasks WHERE id = 1000",
            &[(2, (true, 2, true, 0))],
        ),
        (
            "DELETE FROM projects WHERE id = 100",
            &[(2, (true, 1, false, 0))],
        ),
        (
            "DELETE FROM teams WHERE id = 10",
            &[(2, (true, 0, false, 0))],
        ),
        (
            "INSERT INTO teams VALUES (30, 1, false, '')",
            &[(1, (true, 0, false, 0))],
        ),
    ];
    for (sql, rebuilt) in steps {
        recorded.lock().unwrap().clear();
        exec(sql).await;
        for (user, expected) in rebuilt {
            expect_op(&mut engine, &recorded, &format!("upsert users {user}")).await;
            assert_eq!(flags(*user).await, *expected, "user {user} after `{sql}`");
        }
    }

    engine.abort();
}

/// categories → categories → categories: a category counts its visible
/// grandchildren, one `via` hop back into its own table. A change on any row
/// can be a root, a middle hop, or a grandchild, so it rebuilds each of those
/// it is, and its old ancestors when it moved or went away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_self_reaching_via_rebuilds_every_ancestor_it_reaches() {
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
        "CREATE TABLE categories (id int PRIMARY KEY, parent_id int, hidden boolean NOT NULL DEFAULT false, name text)",
        "ALTER TABLE categories REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE categories",
        "INSERT INTO categories VALUES (1, NULL, false, ''), (2, NULL, false, ''), \
         (10, 1, false, ''), (20, 2, false, ''), (100, 10, false, '')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(categories_with_grandchildren_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        sink,
    ));
    for id in [1, 2, 10, 20, 100] {
        expect_op(&mut engine, &recorded, &format!("upsert categories {id}")).await;
    }

    // (visible grandchildren, has a visible grandchild)
    let grandchildren = |category: i32| {
        let documents = Arc::clone(&documents);
        async move {
            let id = DocumentId {
                index: index_name("categories"),
                key: RowKey(vec![(column("id"), kernel::GenericValue::Int(category))]),
            };
            let Document::Upsert { body, .. } = documents.build(&id).await.unwrap() else {
                panic!("category {category} is not a document");
            };
            let body = sink::to_json(&body);
            (
                body["grandchildren"].as_i64().unwrap(),
                body["hasGrandchild"].as_bool().unwrap(),
            )
        }
    };
    assert_eq!(grandchildren(1).await, (1, true));
    assert_eq!(grandchildren(2).await, (0, false));

    let mut marker = 900;
    let mut rebuilt_by = async |sql: &'static str| {
        marker += 1;
        let marker_sql = "INSERT INTO categories (id) VALUES ($1)";
        let marker_op = format!("upsert categories {marker}");
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            sql,
            marker_sql,
            marker,
            &marker_op,
        )
        .await
    };

    assert_eq!(
        rebuilt_by("UPDATE categories SET name = 'tools' WHERE id IN (1, 10, 100)").await,
        Vec::<String>::new(),
        "`name` is read by no document"
    );

    // Each change rebuilds exactly these documents; the upserted categories
    // then read these values.
    let steps: [(&str, &[&str], &[(i32, (i64, bool))]); 7] = [
        // A new grandchild: itself, its parent (a middle hop with a new
        // child), and its grandparent (a new grandchild).
        (
            "INSERT INTO categories VALUES (101, 10, false, '')",
            &[
                "upsert categories 1",
                "upsert categories 10",
                "upsert categories 101",
            ],
            &[(1, (2, true)), (10, (0, false)), (101, (0, false))],
        ),
        // A filter column only the grandchild level reads.
        (
            "UPDATE categories SET hidden = true WHERE id = 101",
            &["upsert categories 1"],
            &[(1, (1, true))],
        ),
        // A middle row moves: its old and new parent swap its children.
        (
            "UPDATE categories SET parent_id = 2 WHERE id = 10",
            &["upsert categories 1", "upsert categories 2"],
            &[(1, (0, false)), (2, (1, true))],
        ),
        // A deleted grandchild leaves its grandparent, found through the
        // pre-image.
        (
            "DELETE FROM categories WHERE id = 100",
            &[
                "delete categories 100",
                "upsert categories 10",
                "upsert categories 2",
            ],
            &[(10, (0, false)), (2, (0, false))],
        ),
        (
            "UPDATE categories SET hidden = false WHERE id = 101",
            &["upsert categories 2"],
            &[(2, (1, true))],
        ),
        // A deleted middle row takes its children's count from its parent.
        (
            "DELETE FROM categories WHERE id = 10",
            &["delete categories 10", "upsert categories 2"],
            &[(2, (0, false))],
        ),
        // An orphaned row reaches no ancestor.
        (
            "UPDATE categories SET hidden = true WHERE id = 101",
            &[],
            &[],
        ),
    ];
    for (sql, ops, values) in steps {
        assert_eq!(rebuilt_by(sql).await, sorted(ops), "after `{sql}`");
        for (category, expected) in values {
            assert_eq!(
                grandchildren(*category).await,
                *expected,
                "category {category} after `{sql}`"
            );
        }
    }

    engine.abort();
}

/// users → orgs → projects → tasks, where `orgs` is both a nested relation
/// (each org counts its tasks via `projects`) and the first `via` hop of the
/// user's own task count. A change on any hop rebuilds the user it sits under,
/// and its old user too when it moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_via_under_a_nested_relation_rebuilds_the_root_from_every_hop() {
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
        "CREATE TABLE users (id int PRIMARY KEY, email text)",
        "CREATE TABLE orgs (id int PRIMARY KEY, user_id int, name text)",
        "CREATE TABLE projects (id int PRIMARY KEY, org_id int, note text)",
        "CREATE TABLE tasks (id int PRIMARY KEY, project_id int, memo text)",
        "ALTER TABLE orgs REPLICA IDENTITY FULL",
        "ALTER TABLE projects REPLICA IDENTITY FULL",
        "ALTER TABLE tasks REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE users, orgs, projects, tasks",
        "INSERT INTO users VALUES (1, 'ada@x.io'), (2, 'grace@x.io')",
        "INSERT INTO orgs VALUES (10, 1, ''), (20, 2, '')",
        "INSERT INTO projects VALUES (100, 10, ''), (200, 20, '')",
        "INSERT INTO tasks VALUES (1000, 100, '')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(users_with_org_tasks_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        sink,
    ));
    expect_op(&mut engine, &recorded, "upsert users 1").await;
    expect_op(&mut engine, &recorded, "upsert users 2").await;

    // ([(org id, org task count, org has a task)] by org id, user task count)
    type Tasks = (Vec<(i64, i64, bool)>, i64);
    let tasks = |user: i32| {
        let documents = Arc::clone(&documents);
        async move {
            let id = DocumentId {
                index: index_name("users"),
                key: RowKey(vec![(column("id"), kernel::GenericValue::Int(user))]),
            };
            let Document::Upsert { body, .. } = documents.build(&id).await.unwrap() else {
                panic!("user {user} is not a document");
            };
            let body = sink::to_json(&body);
            let mut orgs: Vec<_> = body["orgs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|org| {
                    let has_task = org["hasTask"].as_bool().unwrap_or_else(|| {
                        panic!("`hasTask` is a boolean, got {}", org["hasTask"])
                    });
                    (
                        org["id"].as_i64().unwrap(),
                        org["taskCount"].as_i64().unwrap(),
                        has_task,
                    )
                })
                .collect();
            orgs.sort_unstable();
            (orgs, body["taskCount"].as_i64().unwrap())
        }
    };
    assert_eq!(tasks(1).await, (vec![(10, 1, true)], 1));
    assert_eq!(tasks(2).await, (vec![(20, 0, false)], 0));

    let mut marker = 900;
    let mut rebuilt_by = async |sql: &'static str| {
        marker += 1;
        let marker_sql = "INSERT INTO users VALUES ($1, 'marker')";
        let marker_op = format!("upsert users {marker}");
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            sql,
            marker_sql,
            marker,
            &marker_op,
        )
        .await
    };

    for sql in [
        "UPDATE orgs SET name = 'core' WHERE id = 10",
        "UPDATE projects SET note = 'q3' WHERE id = 100",
        "UPDATE tasks SET memo = 'later' WHERE id = 1000",
    ] {
        assert_eq!(
            rebuilt_by(sql).await,
            Vec::<String>::new(),
            "`{sql}` changed no column the document reads"
        );
    }

    let both = &["upsert users 1", "upsert users 2"];
    let steps: [(&str, &[&str], &[(i32, Tasks)]); 9] = [
        (
            "INSERT INTO tasks VALUES (1001, 200, '')",
            &["upsert users 2"],
            &[(2, (vec![(20, 1, true)], 1))],
        ),
        // Re-parenting on each hop moves the counts between users.
        (
            "UPDATE tasks SET project_id = 100 WHERE id = 1001",
            both,
            &[
                (1, (vec![(10, 2, true)], 2)),
                (2, (vec![(20, 0, false)], 0)),
            ],
        ),
        (
            "UPDATE projects SET org_id = 20 WHERE id = 100",
            both,
            &[
                (1, (vec![(10, 0, false)], 0)),
                (2, (vec![(20, 2, true)], 2)),
            ],
        ),
        (
            "UPDATE orgs SET user_id = 1 WHERE id = 20",
            both,
            &[
                (1, (vec![(10, 0, false), (20, 2, true)], 2)),
                (2, (vec![], 0)),
            ],
        ),
        // Inserts and deletes on each hop.
        (
            "INSERT INTO orgs VALUES (30, 2, '')",
            &["upsert users 2"],
            &[(2, (vec![(30, 0, false)], 0))],
        ),
        (
            "INSERT INTO projects VALUES (300, 30, '')",
            &["upsert users 2"],
            &[(2, (vec![(30, 0, false)], 0))],
        ),
        (
            "DELETE FROM tasks WHERE id = 1000",
            &["upsert users 1"],
            &[(1, (vec![(10, 0, false), (20, 1, true)], 1))],
        ),
        (
            "DELETE FROM projects WHERE id = 100",
            &["upsert users 1"],
            &[(1, (vec![(10, 0, false), (20, 0, false)], 0))],
        ),
        (
            "DELETE FROM orgs WHERE id = 20",
            &["upsert users 1"],
            &[(1, (vec![(10, 0, false)], 0))],
        ),
    ];
    for (sql, ops, values) in steps {
        assert_eq!(rebuilt_by(sql).await, sorted(ops), "after `{sql}`");
        for (user, expected) in values {
            assert_eq!(tasks(*user).await, *expected, "user {user} after `{sql}`");
        }
    }

    engine.abort();
}

/// orders scoped by `tenant_id`: a `belongs_to` customer, `has_many` lines, a
/// direct `count` of the lines, and a `count` of parcels `via` shipments, each
/// joined on `tenant_id` beside its key. A change to either side of a pair
/// rebuilds the order it joined before and the one it joins after; an unread
/// column rebuilds nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn on_pairs_rebuild_the_parent_from_either_side() {
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
        "CREATE TABLE orders (id int PRIMARY KEY, tenant_id text, customer_id int, note text)",
        "CREATE TABLE customers (id int PRIMARY KEY, tenant_id text, name text, bio text)",
        "CREATE TABLE lines (id int PRIMARY KEY, order_id int, tenant_id text, qty int, memo text)",
        "CREATE TABLE shipments (id int PRIMARY KEY, order_id int, tenant_id text, note text)",
        "CREATE TABLE parcels (id int PRIMARY KEY, shipment_id int, tenant_id text, memo text)",
        "ALTER TABLE orders REPLICA IDENTITY FULL",
        "ALTER TABLE customers REPLICA IDENTITY FULL",
        "ALTER TABLE lines REPLICA IDENTITY FULL",
        "ALTER TABLE shipments REPLICA IDENTITY FULL",
        "ALTER TABLE parcels REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE orders, customers, lines, shipments, parcels",
        "INSERT INTO orders VALUES (1, 't1', 10, ''), (2, 't2', 10, '')",
        "INSERT INTO customers VALUES (10, 't1', 'acme', '')",
        "INSERT INTO lines VALUES (100, 1, 't1', 5, ''), (101, 1, 't2', 7, '')",
        "INSERT INTO shipments VALUES (1000, 1, 't1', '')",
        "INSERT INTO parcels VALUES (5000, 1000, 't1', ''), (5001, 1000, 't2', '')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(tenant_orders_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        sink,
    ));
    expect_op(&mut engine, &recorded, "upsert orders 1").await;
    expect_op(&mut engine, &recorded, "upsert orders 2").await;

    type Built = (Option<String>, Vec<i64>, i64, i64);
    // (customer name, line quantities, line count, parcel count)
    let built = |order: i32| {
        let documents = Arc::clone(&documents);
        async move {
            let id = DocumentId {
                index: index_name("orders"),
                key: RowKey(vec![(column("id"), kernel::GenericValue::Int(order))]),
            };
            let Document::Upsert { body, .. } = documents.build(&id).await.unwrap() else {
                panic!("order {order} is not a document");
            };
            let body = sink::to_json(&body);
            let built: Built = (
                body["customer"]["name"].as_str().map(str::to_owned),
                body["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|line| line["qty"].as_i64().unwrap())
                    .collect(),
                body["lineCount"].as_i64().unwrap(),
                body["parcelCount"].as_i64().unwrap(),
            );
            built
        }
    };
    let acme = || Some("acme".to_owned());
    assert_eq!(built(1).await, (acme(), vec![5], 1, 1));
    assert_eq!(built(2).await, (None, vec![], 0, 0));

    // An unread column on every table rebuilds nothing.
    for (marker, sql) in [
        (900, "UPDATE orders SET note = 'rush' WHERE id = 1"),
        (901, "UPDATE customers SET bio = 'b2b' WHERE id = 10"),
        (902, "UPDATE lines SET memo = 'fragile' WHERE id = 100"),
        (903, "UPDATE shipments SET note = 'late' WHERE id = 1000"),
        (904, "UPDATE parcels SET memo = 'dented' WHERE id = 5000"),
    ] {
        recorded.lock().unwrap().clear();
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO orders VALUES ($1, 't9', NULL, 'marker')")
            .bind(marker)
            .execute(&pool)
            .await
            .unwrap();
        expect_op(&mut engine, &recorded, &format!("upsert orders {marker}")).await;
        assert_eq!(
            *recorded.lock().unwrap(),
            vec![format!("upsert orders {marker}")],
            "`{sql}` changed no column the document reads"
        );
    }

    // Each change rebuilds these orders, which then read these values.
    let steps: [(&str, &[(i32, Built)]); 12] = [
        (
            "INSERT INTO lines VALUES (102, 1, 't1', 3, '')",
            &[(1, (acme(), vec![3, 5], 2, 1))],
        ),
        // A `related` column moving a child out of, then into, the match.
        (
            "UPDATE lines SET tenant_id = 't2' WHERE id = 100",
            &[(1, (acme(), vec![3], 1, 1))],
        ),
        (
            "UPDATE lines SET tenant_id = 't1' WHERE id = 101",
            &[(1, (acme(), vec![3, 7], 2, 1))],
        ),
        // Re-parented to an order it doesn't match: the old one still rebuilds.
        (
            "UPDATE lines SET order_id = 2 WHERE id = 102",
            &[(1, (acme(), vec![7], 1, 1)), (2, (None, vec![], 0, 0))],
        ),
        // A `belongs_to` target's `related` column: every order pointing at it.
        (
            "UPDATE customers SET tenant_id = 't2' WHERE id = 10",
            &[(1, (None, vec![7], 1, 1)), (2, (acme(), vec![], 0, 0))],
        ),
        // A `local` column on the root: every relation re-matches.
        (
            "UPDATE orders SET tenant_id = 't2' WHERE id = 1",
            &[(1, (acme(), vec![5], 1, 0))],
        ),
        // A `via` hop's column, `related` to the order and `local` to parcels.
        (
            "UPDATE shipments SET tenant_id = 't2' WHERE id = 1000",
            &[(1, (acme(), vec![5], 1, 1))],
        ),
        (
            "UPDATE parcels SET tenant_id = 't2' WHERE id = 5000",
            &[(1, (acme(), vec![5], 1, 2))],
        ),
        (
            "DELETE FROM parcels WHERE id = 5001",
            &[(1, (acme(), vec![5], 1, 1))],
        ),
        (
            "DELETE FROM lines WHERE id = 100",
            &[(1, (acme(), vec![], 0, 1))],
        ),
        (
            "INSERT INTO parcels VALUES (5002, 1000, 't1', '')",
            &[(1, (acme(), vec![], 0, 1))],
        ),
        (
            "DELETE FROM customers WHERE id = 10",
            &[(1, (None, vec![], 0, 1)), (2, (None, vec![], 0, 0))],
        ),
    ];
    for (sql, rebuilt) in steps {
        recorded.lock().unwrap().clear();
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
        for (order, expected) in rebuilt {
            expect_op(&mut engine, &recorded, &format!("upsert orders {order}")).await;
            assert_eq!(
                built(*order).await,
                *expected,
                "order {order} after `{sql}`"
            );
        }
    }

    engine.abort();
}

/// `orders` folding `quantity * unit_price` (and friends) over its lines, and
/// `customers` summing the same expression `via` their orders: the built values
/// follow Postgres arithmetic (a null operand drops the row, integer division
/// truncates, a zero divisor nulls the row instead of failing the build), each
/// operand's update rebuilds the parents that read it, and a column no
/// expression reads rebuilds nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn an_aggregate_expression_rebuilds_from_each_operand() {
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
        "CREATE TABLE customers (id int PRIMARY KEY, name text)",
        "CREATE TABLE orders (id int PRIMARY KEY, customer_id int, note text)",
        "CREATE TABLE order_lines (id int PRIMARY KEY, order_id int, quantity int, \
         unit_price numeric, divisor int, memo text)",
        "ALTER TABLE orders REPLICA IDENTITY FULL",
        "ALTER TABLE order_lines REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE customers, orders, order_lines",
        "INSERT INTO customers VALUES (1, 'ada')",
        "INSERT INTO orders VALUES (1, 1, '')",
        "INSERT INTO order_lines VALUES (1, 1, 2, 1.5, 3, ''), (2, 1, 5, 2, 2, ''), \
         (3, 1, 4, NULL, 2, '')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(order_values_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::new(&recorded));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        sink,
    ));
    expect_op(&mut engine, &recorded, "upsert orders 1").await;
    expect_op(&mut engine, &recorded, "upsert customers 1").await;

    let body = |index: &'static str| {
        let documents = Arc::clone(&documents);
        async move {
            let id = DocumentId {
                index: index_name(index),
                key: RowKey(vec![(column("id"), kernel::GenericValue::Int(1))]),
            };
            let Document::Upsert { body, .. } = documents.build(&id).await.unwrap() else {
                panic!("{index} 1 is not a document");
            };
            sink::to_json(&body)
        }
    };
    // (value, mean value, smallest quantity / divisor, largest 1 - quantity)
    let order = || async {
        let order = body("orders").await;
        (
            order["value"].as_f64().unwrap(),
            order["meanValue"].as_f64().unwrap(),
            order["smallestShare"].as_i64().unwrap(),
            order["largestRemainder"].as_i64().unwrap(),
        )
    };
    let customer = || async { body("customers").await["value"].as_f64().unwrap() };

    // Line 3's null price drops it from the sum and the mean; 2 / 3 truncates to 0.
    assert_eq!(order().await, (13.0, 6.5, 0, -1));
    assert_eq!(customer().await, 13.0);

    let marker_sql = "INSERT INTO orders (id) VALUES ($1)";
    let mut marker = 900;
    let mut rebuilt = async |sql: &'static str| {
        marker += 1;
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            sql,
            marker_sql,
            marker,
            &format!("upsert orders {marker}"),
        )
        .await
    };

    assert_eq!(
        rebuilt("UPDATE order_lines SET memo = 'gift' WHERE id = 1").await,
        Vec::<String>::new(),
        "no expression reads `memo`"
    );
    assert_eq!(
        rebuilt("UPDATE order_lines SET quantity = 6 WHERE id = 1").await,
        sorted(&["upsert customers 1", "upsert orders 1"])
    );
    assert_eq!(order().await, (19.0, 9.5, 2, -3));
    assert_eq!(customer().await, 19.0);
    assert_eq!(
        rebuilt("UPDATE order_lines SET unit_price = 2 WHERE id = 3").await,
        sorted(&["upsert customers 1", "upsert orders 1"])
    );
    assert_eq!(order().await, (27.0, 9.0, 2, -3));
    assert_eq!(customer().await, 27.0);
    assert_eq!(
        rebuilt("UPDATE order_lines SET divisor = 1 WHERE id = 2").await,
        sorted(&["upsert orders 1"]),
        "only `orders` reads `divisor`"
    );
    assert_eq!(order().await, (27.0, 9.0, 2, -3));
    // (sum, mean, min, max of quantity / divisor, line count)
    let shares = || async {
        let order = body("orders").await;
        (
            order["shareSum"].as_i64(),
            order["meanShare"].as_f64(),
            order["smallestShare"].as_i64(),
            order["largestShare"].as_i64(),
            order["lineCount"].as_i64().unwrap(),
        )
    };
    assert_eq!(shares().await, (Some(9), Some(3.0), Some(2), Some(5), 3));

    assert_eq!(
        rebuilt("UPDATE order_lines SET divisor = 0 WHERE id = 2").await,
        sorted(&["upsert orders 1"]),
        "a zero divisor nulls that line's share"
    );
    assert_eq!(shares().await, (Some(4), Some(2.0), Some(2), Some(2), 3));
    assert_eq!(
        rebuilt("UPDATE order_lines SET divisor = 0").await,
        sorted(&["upsert orders 1"]),
        "every divisor zero"
    );
    assert_eq!(shares().await, (None, None, None, None, 3));
    assert_eq!(
        rebuilt("UPDATE order_lines SET divisor = 2 WHERE id = 1").await,
        sorted(&["upsert orders 1"]),
        "the pipeline keeps going"
    );
    assert_eq!(shares().await, (Some(3), Some(3.0), Some(3), Some(3), 3));

    engine.abort();
}

async fn start_logical_postgres() -> (
    testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    u16,
    String,
) {
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
    (container, port, url)
}

fn replication(port: u16) -> ReplicationConfig {
    ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_keyless_junction_rebuilds_every_parent_its_rows_link() {
    let (_container, port, url) = start_logical_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text)",
        "CREATE TABLE tags (id int PRIMARY KEY, name text)",
        "CREATE TABLE user_tags (user_id int, tag_id int, note text)",
        "ALTER TABLE user_tags REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE users, tags, user_tags",
        "INSERT INTO users VALUES (1, 'a@x'), (2, 'b@x')",
        "INSERT INTO tags VALUES (10, 'red'), (11, 'blue')",
        "INSERT INTO user_tags VALUES (1, 10, NULL)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(users_with_keyless_tags_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication(port), url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::new(RecordingSink::new(&recorded)),
    ));
    expect_op(&mut engine, &recorded, "upsert users 1").await;
    expect_op(&mut engine, &recorded, "upsert users 2").await;

    // (tag count, tag names) of user `id`.
    let tags = async |id: i32| {
        let id = DocumentId {
            index: index_name("users"),
            key: RowKey(vec![(column("id"), kernel::GenericValue::Int(id))]),
        };
        let Document::Upsert { body, .. } = documents.build(&id).await.unwrap() else {
            panic!("users {id:?} is not a document");
        };
        let body = sink::to_json(&body);
        let mut names: Vec<String> = body["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tag| tag["name"].as_str().unwrap().to_owned())
            .collect();
        names.sort_unstable();
        (body["tagCount"].as_i64().unwrap(), names)
    };
    assert_eq!(tags(1).await, (1, vec!["red".to_owned()]));

    let marker_sql = "INSERT INTO users (id) VALUES ($1)";
    let mut marker = 900;
    let mut rebuilt = async |sql: &'static str| {
        marker += 1;
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            sql,
            marker_sql,
            marker,
            &format!("upsert users {marker}"),
        )
        .await
    };

    assert_eq!(
        rebuilt("INSERT INTO user_tags VALUES (1, 11, NULL)").await,
        sorted(&["upsert users 1"]),
        "a null `note` is row data, not a missing key"
    );
    assert_eq!(
        tags(1).await,
        (2, vec!["blue".to_owned(), "red".to_owned()])
    );
    assert_eq!(
        rebuilt("UPDATE user_tags SET note = 'pinned' WHERE tag_id = 11").await,
        sorted(&["upsert users 1"])
    );
    assert_eq!(
        rebuilt("UPDATE user_tags SET user_id = 2 WHERE tag_id = 11").await,
        sorted(&["upsert users 1", "upsert users 2"]),
        "a re-parented row rebuilds the parent it left and the one it joined"
    );
    assert_eq!(tags(1).await, (1, vec!["red".to_owned()]));
    assert_eq!(tags(2).await, (1, vec!["blue".to_owned()]));
    assert_eq!(
        rebuilt("DELETE FROM user_tags WHERE tag_id = 11").await,
        sorted(&["upsert users 2"])
    );
    assert_eq!(tags(2).await, (0, Vec::new()));
    assert_eq!(
        rebuilt("INSERT INTO user_tags VALUES (NULL, 10, NULL)").await,
        Vec::<String>::new(),
        "a null `user_id` links no parent"
    );

    engine.abort();
}

/// A table without a primary key can't be a relation's target outside a
/// junction: every document build needs its key (issue #225). Its change used
/// to resolve to nothing when a column was null, so the index stayed stale with
/// no error; it now names the parent, whose build fails loudly.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_change_on_a_keyless_counted_table_stops_the_engine() {
    let (_container, port, url) = start_logical_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text)",
        "CREATE TABLE events (user_id int, kind text, note text)",
        "ALTER TABLE events REPLICA IDENTITY FULL",
        "CREATE PUBLICATION flusso FOR TABLE users, events",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let documents = PgDocumentBuilder::connect(&url, Arc::new(users_with_event_count_spec()))
        .await
        .unwrap();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication(port), url.clone())),
        Arc::new(documents),
        Arc::new(RecordingSink::new(&recorded)),
    ));
    sqlx::query("INSERT INTO events VALUES (1, 'click', NULL)")
        .execute(&pool)
        .await
        .unwrap();

    let stopped = tokio::time::timeout(Duration::from_secs(30), engine)
        .await
        .expect("the engine kept running: the change resolved to nothing")
        .unwrap();
    let error = stopped.unwrap_err().to_string();
    assert!(error.contains("has no primary key"), "{error}");
    assert!(recorded.lock().unwrap().is_empty());
}

/// One deployment the way the daemon runs it: stage the sink engine, then run
/// it and the ingest engine together until either stops. `Ok` when the live
/// stream ends; the first engine error otherwise.
async fn run_deployment(
    capture: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
) -> engine::Result<()> {
    let name = SinkName::try_new("recording").unwrap();
    let stream: Arc<dyn Stream> = Arc::new(ChannelStream::new(1024, [name.clone()]));
    let continuity = capture.continuity().await?;
    let mappings = documents.index_mappings().await?;
    let sink_engine = SinkEngine::new(name.clone(), sink, Arc::clone(&stream), mappings);
    sink_engine.stage(continuity).await?;
    let ingest = IngestEngine::new(capture, documents, stream, vec![name]);
    let (_control, mut control_rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    tokio::select! {
        result = ingest.run() => result,
        result = sink_engine.run(&mut control_rx) => result,
    }
}

/// Wait until the sink has recorded `op`, surfacing the engine's error if it
/// stops first, or failing on a deadline.
async fn expect_op(
    engine: &mut tokio::task::JoinHandle<engine::Result<()>>,
    recorded: &Arc<Mutex<Vec<String>>>,
    op: &str,
) {
    tokio::select! {
        result = &mut *engine => panic!("engine stopped before producing `{op}`: {result:?}"),
        () = poll_for(recorded, op) => {}
    }
}

async fn poll_for(recorded: &Arc<Mutex<Vec<String>>>, op: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if recorded
            .lock()
            .unwrap()
            .iter()
            .any(|recorded_op| recorded_op == op)
        {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for `{op}`; recorded so far: {:?}",
                recorded.lock().unwrap()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Run `sql`, then `marker_sql` bound to `marker`, and return every operation
/// recorded before the marker's `marker_op`, sorted and deduplicated. The
/// marker commits after `sql`, so seeing it means every operation `sql`
/// caused is recorded.
async fn rebuilt_by(
    engine: &mut tokio::task::JoinHandle<engine::Result<()>>,
    recorded: &Arc<Mutex<Vec<String>>>,
    pool: &sqlx::PgPool,
    sql: &str,
    marker_sql: &'static str,
    marker: i32,
    marker_op: &str,
) -> Vec<String> {
    recorded.lock().unwrap().clear();
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(marker_sql)
        .bind(marker)
        .execute(pool)
        .await
        .unwrap();
    expect_op(engine, recorded, marker_op).await;
    let mut ops: Vec<String> = recorded
        .lock()
        .unwrap()
        .iter()
        .filter(|op| *op != marker_op)
        .cloned()
        .collect();
    ops.sort_unstable();
    ops.dedup();
    ops
}

fn sorted(ops: &[&str]) -> Vec<String> {
    let mut ops: Vec<String> = ops.iter().map(|op| (*op).to_owned()).collect();
    ops.sort_unstable();
    ops
}

/// `users` reading `email` from the root, `orders` (projecting `total`,
/// filtered on `status`, ordered by `placed_at`), the sum of `payments.amount`,
/// and `visits.path`.
fn users_with_relations_spec() -> SourceSpec {
    let has_many = |target: &str, fields: Vec<Field>| Join {
        table: table(target),
        kind: JoinKind::HasMany {
            foreign_key: column("user_id"),
        },
        primary_key: column("id"),
        nullable: false,
        filters: None,
        order_by: None,
        limit: None,
        indexed: true,
        fields,
        on: vec![],
    };
    let relation = |name: &str, relation: Relation| Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(relation),
    };
    let mut orders = has_many("orders", vec![column_field("total", "total")]);
    orders.filters = Some(vec![Filter::ValueOp(ValueOpFilter {
        column: column("status"),
        op: FilterOp::Neq,
        value: FilterValue::Single("void".into()),
    })]);
    orders.order_by = Some(vec![OrderBy {
        column: column("placed_at"),
        direction: None,
    }]);
    let paid = Relation::Aggregate(Aggregate {
        table: table("payments"),
        op: AggregateOp::Sum(column("amount").into()),
        key: AggregateKey::Direct(column("user_id")),
        value_type: Some(FlussoType::Long),
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: vec![],
    });
    let visits = has_many("visits", vec![column_field("path", "path")]);
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("id", "id"),
            column_field("email", "email"),
            relation("orders", Relation::Join(orders)),
            relation("paid", paid),
            relation("visits", Relation::Join(visits)),
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

/// `users` reading `email` and the `orders` placed since the user's
/// `trial_start`, through a raw filter on the root row.
fn users_with_recent_orders_spec() -> SourceSpec {
    let orders = Join {
        table: table("orders"),
        kind: JoinKind::HasMany {
            foreign_key: column("user_id"),
        },
        primary_key: column("id"),
        nullable: false,
        filters: Some(vec![Filter::Raw(RawFilter {
            raw: RawFilterValue::try_new("placed_at >= root.trial_start").unwrap(),
        })]),
        order_by: None,
        limit: None,
        indexed: true,
        fields: vec![column_field("total", "total")],
        on: vec![],
    };
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("id", "id"),
            column_field("email", "email"),
            Field {
                field: field("orders"),
                options: Default::default(),
                emit: true,
                source: FieldSource::Relation(Relation::Join(orders)),
            },
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

/// `users` with aggregates across hops of users → teams → projects → tasks →
/// task_tags → tags: whether the user owns a team (one hop), the user's
/// unarchived projects in unarchived teams (two hops), whether one of those
/// holds a completed task (three hops), and the distinct tags on any of the
/// user's tasks (three hops and a junction).
fn users_with_task_flags_spec() -> SourceSpec {
    let not_archived = || {
        Some(vec![Filter::ValueOp(ValueOpFilter {
            column: column("archived"),
            op: FilterOp::Eq,
            value: FilterValue::Single("false".into()),
        })])
    };
    let hop = |target: &str, foreign_key: &str, filters| ViaHop {
        table: table(target),
        foreign_key: column(foreign_key),
        filters,
        on: vec![],
    };
    let aggregate = |name: &str, aggregate: Aggregate| Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
    };
    let has_team = Aggregate {
        table: table("teams"),
        op: AggregateOp::Exists,
        key: AggregateKey::Direct(column("owner_id")),
        value_type: None,
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: vec![],
    };
    let active_projects = Aggregate {
        table: table("projects"),
        op: AggregateOp::Count,
        key: AggregateKey::Direct(column("team_id")),
        value_type: None,
        filters: not_archived(),
        distinct: false,
        via: vec![hop("teams", "owner_id", not_archived())],
        on: vec![],
    };
    let has_completed_task = Aggregate {
        table: table("tasks"),
        op: AggregateOp::Exists,
        key: AggregateKey::Direct(column("project_id")),
        value_type: None,
        filters: Some(vec![Filter::ValueOp(ValueOpFilter {
            column: column("status"),
            op: FilterOp::Eq,
            value: FilterValue::Single("completed".into()),
        })]),
        distinct: false,
        via: vec![
            hop("teams", "owner_id", not_archived()),
            hop("projects", "team_id", None),
        ],
        on: vec![],
    };
    let tag_count = Aggregate {
        table: table("tags"),
        op: AggregateOp::Count,
        key: AggregateKey::Through(Through {
            table: table("task_tags"),
            left_key: column("task_id"),
            right_key: column("tag_id"),
            filters: None,
        }),
        value_type: None,
        filters: None,
        distinct: true,
        via: vec![
            hop("teams", "owner_id", None),
            hop("projects", "team_id", None),
            hop("tasks", "project_id", None),
        ],
        on: vec![],
    };
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("id", "id"),
            column_field("email", "email"),
            aggregate("hasTeam", has_team),
            aggregate("activeProjects", active_projects),
            aggregate("hasCompletedTask", has_completed_task),
            aggregate("tagCount", tag_count),
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

/// `categories` counting, and flagging, its visible grandchildren: one `via`
/// hop back into `categories`, then the grandchildren by `parent_id`.
fn categories_with_grandchildren_spec() -> SourceSpec {
    let grandchildren = |op| Aggregate {
        table: table("categories"),
        op,
        key: AggregateKey::Direct(column("parent_id")),
        value_type: None,
        filters: Some(vec![Filter::ValueOp(ValueOpFilter {
            column: column("hidden"),
            op: FilterOp::Eq,
            value: FilterValue::Single("false".into()),
        })]),
        distinct: false,
        via: vec![ViaHop {
            table: table("categories"),
            foreign_key: column("parent_id"),
            filters: None,
            on: vec![],
        }],
        on: vec![],
    };
    let schema = IndexSchema {
        version: 1,
        table: table("categories"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("id", "id"),
            aggregate_field("grandchildren", grandchildren(AggregateOp::Count)),
            aggregate_field("hasGrandchild", grandchildren(AggregateOp::Exists)),
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("categories"), schema)]))
}

/// `users` with its `orgs`, each counting and flagging its tasks via
/// `projects`, and the user's own task count via `orgs` then `projects`.
fn users_with_org_tasks_spec() -> SourceSpec {
    let hop = |target: &str, foreign_key: &str| ViaHop {
        table: table(target),
        foreign_key: column(foreign_key),
        filters: None,
        on: vec![],
    };
    let tasks = |op, via| Aggregate {
        table: table("tasks"),
        op,
        key: AggregateKey::Direct(column("project_id")),
        value_type: None,
        filters: None,
        distinct: false,
        via,
        on: vec![],
    };
    let orgs = Join {
        table: table("orgs"),
        kind: JoinKind::HasMany {
            foreign_key: column("user_id"),
        },
        primary_key: column("id"),
        nullable: false,
        filters: None,
        order_by: None,
        limit: None,
        indexed: true,
        fields: vec![
            Field {
                field: field("id"),
                options: Default::default(),
                emit: true,
                source: FieldSource::Column(Column {
                    column: column("id"),
                    ty: FlussoType::Long,
                    nullable: false,
                    transforms: Vec::new(),
                    default: None,
                    enum_order: Vec::new(),
                    subfields: true,
                    indexed: true,
                    array: false,
                }),
            },
            aggregate_field(
                "taskCount",
                tasks(AggregateOp::Count, vec![hop("projects", "org_id")]),
            ),
            aggregate_field(
                "hasTask",
                tasks(AggregateOp::Exists, vec![hop("projects", "org_id")]),
            ),
        ],
        on: vec![],
    };
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("id", "id"),
            column_field("email", "email"),
            Field {
                field: field("orgs"),
                options: Default::default(),
                emit: true,
                source: FieldSource::Relation(Relation::Join(orgs)),
            },
            aggregate_field(
                "taskCount",
                tasks(
                    AggregateOp::Count,
                    vec![hop("orgs", "user_id"), hop("projects", "org_id")],
                ),
            ),
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

/// `orders` with every relation joined on `tenant_id` beside its key.
fn tenant_orders_spec() -> SourceSpec {
    let tenant = || {
        vec![kernel::ColumnPair {
            local: column("tenant_id"),
            related: column("tenant_id"),
        }]
    };
    let join = |target: &str, kind: JoinKind, fields: Vec<Field>| Join {
        table: table(target),
        kind,
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: None,
        limit: None,
        fields,
        indexed: true,
        on: tenant(),
    };
    let mut lines = join(
        "lines",
        JoinKind::HasMany {
            foreign_key: column("order_id"),
        },
        vec![column_field("qty", "qty")],
    );
    lines.order_by = Some(vec![OrderBy {
        column: column("qty"),
        direction: None,
    }]);
    let customer = join(
        "customers",
        JoinKind::BelongsTo {
            column: column("customer_id"),
        },
        vec![column_field("name", "name")],
    );
    let line_count = Aggregate {
        table: table("lines"),
        op: AggregateOp::Count,
        key: AggregateKey::Direct(column("order_id")),
        value_type: None,
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: tenant(),
    };
    let parcel_count = Aggregate {
        table: table("parcels"),
        op: AggregateOp::Count,
        key: AggregateKey::Direct(column("shipment_id")),
        value_type: None,
        filters: None,
        distinct: false,
        via: vec![ViaHop {
            table: table("shipments"),
            foreign_key: column("order_id"),
            filters: None,
            on: tenant(),
        }],
        on: tenant(),
    };
    let relation = |name: &str, relation: Relation| Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(relation),
    };
    let schema = IndexSchema {
        version: 1,
        table: table("orders"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("id", "id"),
            relation("customer", Relation::Join(customer)),
            relation("lines", Relation::Join(lines)),
            aggregate_field("lineCount", line_count),
            aggregate_field("parcelCount", parcel_count),
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("orders"), schema)]))
}

/// `users` reading `email` and its `belongs_to` team's `name`.
fn users_with_team_spec() -> SourceSpec {
    let team = Join {
        table: table("teams"),
        kind: JoinKind::BelongsTo {
            column: column("team_id"),
        },
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: None,
        limit: None,
        fields: vec![column_field("name", "name")],
        indexed: true,
        on: vec![],
    };
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("email", "email"),
            Field {
                field: field("team"),
                options: Default::default(),
                emit: true,
                source: FieldSource::Relation(Relation::Join(team)),
            },
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

/// `orders` with four aggregates over arithmetic on `order_lines`, and
/// `customers` summing `quantity * unit_price` across their orders (`via`).
fn order_values_spec() -> SourceSpec {
    let lines = |op: AggregateOp, value_type: Option<FlussoType>| Aggregate {
        table: table("order_lines"),
        op,
        key: AggregateKey::Direct(column("order_id")),
        value_type,
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: vec![],
    };
    let expr = |input: &str| input.parse::<kernel::Expression>().unwrap();
    let schema = |root: &str, fields: Vec<Field>| IndexSchema {
        version: 1,
        table: table(root),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields,
    };
    let orders = schema(
        "orders",
        vec![
            column_field("note", "note"),
            aggregate_field(
                "value",
                lines(
                    AggregateOp::Sum(expr("quantity * unit_price")),
                    Some(FlussoType::Double),
                ),
            ),
            aggregate_field(
                "meanValue",
                lines(AggregateOp::Avg(expr("quantity * unit_price")), None),
            ),
            aggregate_field(
                "smallestShare",
                lines(
                    AggregateOp::Min(expr("quantity / divisor")),
                    Some(FlussoType::Long),
                ),
            ),
            aggregate_field(
                "shareSum",
                lines(
                    AggregateOp::Sum(expr("quantity / divisor")),
                    Some(FlussoType::Long),
                ),
            ),
            aggregate_field(
                "meanShare",
                lines(
                    AggregateOp::Avg(expr("quantity / divisor")),
                    Some(FlussoType::Double),
                ),
            ),
            aggregate_field(
                "largestShare",
                lines(
                    AggregateOp::Max(expr("quantity / divisor")),
                    Some(FlussoType::Long),
                ),
            ),
            aggregate_field("lineCount", lines(AggregateOp::Count, None)),
            aggregate_field(
                "largestRemainder",
                lines(
                    AggregateOp::Max(expr("-quantity + 1")),
                    Some(FlussoType::Long),
                ),
            ),
        ],
    );
    let mut customer_value = lines(
        AggregateOp::Sum(expr("quantity * unit_price")),
        Some(FlussoType::Double),
    );
    customer_value.via = vec![ViaHop {
        table: table("orders"),
        foreign_key: column("customer_id"),
        filters: None,
        on: vec![],
    }];
    let customers = schema(
        "customers",
        vec![
            column_field("name", "name"),
            aggregate_field("value", customer_value),
        ],
    );
    SourceSpec::new(BTreeMap::from([
        (index_name("orders"), orders),
        (index_name("customers"), customers),
    ]))
}

fn users_schema(fields: Vec<Field>) -> IndexSchema {
    let mut all = vec![column_field("email", "email")];
    all.extend(fields);
    IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: all,
    }
}

/// `users` with `tags` (many-to-many) and `tagCount` (a count), both through
/// `user_tags`, a junction without a primary key.
fn users_with_keyless_tags_spec() -> SourceSpec {
    let through = || Through {
        table: table("user_tags"),
        left_key: column("user_id"),
        right_key: column("tag_id"),
        filters: None,
    };
    let tags = Join {
        table: table("tags"),
        kind: JoinKind::ManyToMany { through: through() },
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: None,
        limit: None,
        fields: vec![column_field("name", "name")],
        indexed: true,
        on: vec![],
    };
    let tag_count = Aggregate {
        table: table("tags"),
        op: AggregateOp::Count,
        key: AggregateKey::Through(through()),
        value_type: None,
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: vec![],
    };
    let schema = users_schema(vec![
        Field {
            field: field("tags"),
            options: Default::default(),
            emit: true,
            source: FieldSource::Relation(Relation::Join(tags)),
        },
        aggregate_field("tagCount", tag_count),
    ]);
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

/// `users` counting `events`, a table without a primary key.
fn users_with_event_count_spec() -> SourceSpec {
    let events = Aggregate {
        table: table("events"),
        op: AggregateOp::Count,
        key: AggregateKey::Direct(column("user_id")),
        value_type: None,
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: vec![],
    };
    let schema = users_schema(vec![aggregate_field("eventCount", events)]);
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

fn aggregate_field(name: &str, aggregate: Aggregate) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
    }
}

/// A root `primary_key` naming a unique column other than the table's own key
/// never keeps a document (issue #225): the backfill and every root change
/// carry the table's key, the batched build matches it against the declared
/// column, finds nothing and writes a tombstone; a change on a related table
/// is addressed by the declared column, so its document is written under
/// that value, and the next root change deletes it again. This is why
/// `root_primary_key_mismatch` refuses the start.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_root_primary_key_other_than_the_tables_writes_no_document() {
    let (_container, port, url) = start_logical_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, code text UNIQUE NOT NULL, email text)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_code text, total text)",
        "CREATE PUBLICATION flusso FOR TABLE users, orders",
        "INSERT INTO users VALUES (1, 'u1', 'a@x'), (2, 'u2', 'b@x')",
        "INSERT INTO orders VALUES (10, 'u1', '5')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let orders = Join {
        table: table("orders"),
        kind: JoinKind::HasMany {
            foreign_key: column("user_code"),
        },
        primary_key: column("id"),
        nullable: false,
        filters: None,
        order_by: None,
        limit: None,
        indexed: true,
        fields: vec![column_field("total", "total")],
        on: vec![],
    };
    let mut schema = users_schema(vec![Field {
        field: field("orders"),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(orders)),
    }]);
    schema.primary_key = Some(column("code"));
    let spec = SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]));
    let documents = PgDocumentBuilder::connect(&url, Arc::new(spec))
        .await
        .unwrap();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication(port), url.clone())),
        Arc::new(documents),
        Arc::new(RecordingSink::new(&recorded)),
    ));

    expect_op(&mut engine, &recorded, "delete users 1").await;
    expect_op(&mut engine, &recorded, "delete users 2").await;
    for (sql, op) in [
        (
            "INSERT INTO users VALUES (3, 'u3', 'c@x')",
            "delete users 3",
        ),
        (
            "UPDATE users SET email = 'z@x' WHERE id = 1",
            "delete users 1",
        ),
    ] {
        recorded.lock().unwrap().clear();
        sqlx::query(sql).execute(&pool).await.unwrap();
        expect_op(&mut engine, &recorded, op).await;
    }
    assert!(
        recorded
            .lock()
            .unwrap()
            .iter()
            .all(|op| op.starts_with("delete")),
        "{:?}",
        recorded.lock().unwrap()
    );

    for (sql, op) in [
        (
            "INSERT INTO orders VALUES (11, 'u1', '7')",
            "upsert users u1",
        ),
        (
            "UPDATE users SET email = 'y@x' WHERE id = 1",
            "delete users 1",
        ),
    ] {
        recorded.lock().unwrap().clear();
        sqlx::query(sql).execute(&pool).await.unwrap();
        expect_op(&mut engine, &recorded, op).await;
    }
    engine.abort();
}

/// Every relation position keyed by one SQL type (issue #295): a root
/// `parents` keyed by `sql_type`, its `has_many` `kids` (whose `parent` link is
/// of that type, joined `on` it once more), a `count` reaching `grandkids`
/// `via` the kids, and a
/// `many_to_many` to `fars` (also keyed by that type) through `links`; plus a
/// `kids` index whose `owner` is a `belongs_to` back to `parents`. `a` and `b`
/// are SQL literals of the type; `id_a` and `id_b` the document ids the
/// backfill gives them. Every live change must rebuild the same ids, as
/// upserts.
async fn every_relation_position_keyed_by(
    sql_type: &str,
    setup: &[&str],
    [a, b]: [&str; 2],
    [id_a, id_b]: [&str; 2],
) {
    let (_container, port, url) = start_logical_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in setup {
        sqlx::raw_sql(sqlx::AssertSqlSafe(*statement))
            .execute(&pool)
            .await
            .unwrap();
    }
    for statement in [
        format!("CREATE TABLE parents (id {sql_type} PRIMARY KEY, name text)"),
        format!("CREATE TABLE kids (id int PRIMARY KEY, parent {sql_type}, name text)"),
        "CREATE TABLE grandkids (id int PRIMARY KEY, kid_id int, name text)".to_owned(),
        format!("CREATE TABLE fars (id {sql_type} PRIMARY KEY, name text)"),
        format!(
            "CREATE TABLE links (parent {sql_type}, far {sql_type}, PRIMARY KEY (parent, far))"
        ),
        "ALTER TABLE kids REPLICA IDENTITY FULL".to_owned(),
        "ALTER TABLE grandkids REPLICA IDENTITY FULL".to_owned(),
        "ALTER TABLE links REPLICA IDENTITY FULL".to_owned(),
        "CREATE PUBLICATION flusso FOR TABLE parents, kids, grandkids, fars, links".to_owned(),
        format!("INSERT INTO parents VALUES ({a}, 'pa'), ({b}, 'pb')"),
        format!("INSERT INTO kids VALUES (1, {a}, 'k1')"),
        "INSERT INTO grandkids VALUES (100, 1, NULL)".to_owned(),
        format!("INSERT INTO fars VALUES ({a}, 'fa')"),
        format!("INSERT INTO links VALUES ({a}, {a})"),
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(keyed_parents_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication(port), url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::new(RecordingSink::new(&recorded)),
    ));
    let parent = |id: &str| format!("upsert parents {id}");
    expect_op(&mut engine, &recorded, &parent(id_a)).await;
    expect_op(&mut engine, &recorded, &parent(id_b)).await;
    expect_op(&mut engine, &recorded, "upsert kids 1").await;

    let marker_sql = "INSERT INTO grandkids (id) VALUES ($1)";
    let mut marker = 900;
    let mut rebuilt = async |sql: String| {
        marker += 1;
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            &sql,
            marker_sql,
            marker,
            &format!("upsert grandkids {marker}"),
        )
        .await
    };
    let ops = |ops: &[&str]| sorted(ops);

    assert_eq!(
        rebuilt(format!("UPDATE parents SET name = 'pa2' WHERE id = {a}")).await,
        ops(&[&parent(id_a), "upsert kids 1"]),
        "a root change rebuilds itself and the kid that `belongs_to` it"
    );
    assert_eq!(
        rebuilt(format!("INSERT INTO kids VALUES (2, {a}, 'k2')")).await,
        ops(&[&parent(id_a), "upsert kids 2"]),
        "a new kid rebuilds its parent"
    );
    assert_eq!(
        rebuilt("UPDATE kids SET name = 'k2b' WHERE id = 2".to_owned()).await,
        ops(&[&parent(id_a), "upsert kids 2"]),
        "an updated kid rebuilds its parent"
    );
    assert_eq!(
        rebuilt(format!("UPDATE kids SET parent = {b} WHERE id = 2")).await,
        ops(&[&parent(id_a), &parent(id_b), "upsert kids 2"]),
        "a re-parented kid rebuilds the parent it left and the one it joined"
    );
    assert_eq!(
        rebuilt("DELETE FROM kids WHERE id = 2".to_owned()).await,
        ops(&[&parent(id_b), "delete kids 2"]),
        "a deleted kid rebuilds the parent it left"
    );
    assert_eq!(
        rebuilt("INSERT INTO grandkids VALUES (101, 1, NULL)".to_owned()).await,
        ops(&[&parent(id_a), "upsert grandkids 101"]),
        "a grandkid rebuilds its parent `via` its kid"
    );
    assert_eq!(
        rebuilt(format!("UPDATE fars SET name = 'fa2' WHERE id = {a}")).await,
        ops(&[&parent(id_a)]),
        "a far row rebuilds every parent it is linked to"
    );
    assert_eq!(
        rebuilt(format!("INSERT INTO links VALUES ({b}, {a})")).await,
        ops(&[&parent(id_b)]),
        "a new link rebuilds its parent"
    );
    assert_eq!(
        rebuilt(format!("DELETE FROM links WHERE parent = {b}")).await,
        ops(&[&parent(id_b)]),
        "a deleted link rebuilds the parent it left"
    );
    assert_eq!(
        rebuilt(format!("DELETE FROM parents WHERE id = {b}")).await,
        ops(&[&format!("delete parents {id_b}")]),
        "a deleted root is a tombstone"
    );

    engine.abort();
}

/// The two indexes of [`every_relation_position_keyed_by`].
fn keyed_parents_spec() -> SourceSpec {
    let join = |target: &str, kind: JoinKind| Join {
        table: table(target),
        kind,
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: None,
        limit: None,
        fields: vec![column_field("name", "name")],
        indexed: true,
        on: vec![],
    };
    let relation = |name: &str, relation: Relation| Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(relation),
    };
    let grandkids = Aggregate {
        table: table("grandkids"),
        op: AggregateOp::Count,
        key: AggregateKey::Direct(column("kid_id")),
        value_type: None,
        filters: None,
        distinct: false,
        via: vec![ViaHop {
            table: table("kids"),
            foreign_key: column("parent"),
            filters: None,
            on: vec![],
        }],
        on: vec![],
    };
    let schema = |root: &str, fields: Vec<Field>| IndexSchema {
        version: 1,
        table: table(root),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: [vec![column_field("name", "name")], fields].concat(),
    };
    let parents = schema(
        "parents",
        vec![
            relation(
                "kids",
                Relation::Join(Join {
                    on: vec![kernel::ColumnPair {
                        local: column("id"),
                        related: column("parent"),
                    }],
                    ..join(
                        "kids",
                        JoinKind::HasMany {
                            foreign_key: column("parent"),
                        },
                    )
                }),
            ),
            aggregate_field("grandkidCount", grandkids),
            relation(
                "fars",
                Relation::Join(join(
                    "fars",
                    JoinKind::ManyToMany {
                        through: Through {
                            table: table("links"),
                            left_key: column("parent"),
                            right_key: column("far"),
                            filters: None,
                        },
                    },
                )),
            ),
        ],
    );
    let kids = schema(
        "kids",
        vec![relation(
            "owner",
            Relation::Join(join(
                "parents",
                JoinKind::BelongsTo {
                    column: column("parent"),
                },
            )),
        )],
    );
    let grandkids = schema("grandkids", vec![]);
    SourceSpec::new(BTreeMap::from([
        (index_name("parents"), parents),
        (index_name("kids"), kids),
        (index_name("grandkids"), grandkids),
    ]))
}

macro_rules! keyed_by {
    ($($test:ident: $sql_type:literal, $setup:expr, [$a:literal, $b:literal], [$id_a:literal, $id_b:literal];)*) => {$(
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        #[ignore = "requires docker"]
        async fn $test() {
            every_relation_position_keyed_by($sql_type, $setup, [$a, $b], [$id_a, $id_b]).await;
        }
    )*};
}

keyed_by! {
    every_relation_position_keyed_by_int: "int", &[], ["1", "2"], ["1", "2"];
    every_relation_position_keyed_by_text: "text", &[], ["'a'", "'b'"], ["a", "b"];
    every_relation_position_keyed_by_uuid: "uuid", &[],
        ["'00000000-0000-0000-0000-00000000000a'", "'00000000-0000-0000-0000-00000000000b'"],
        ["00000000-0000-0000-0000-00000000000a", "00000000-0000-0000-0000-00000000000b"];
    every_relation_position_keyed_by_bytea: "bytea", &[], ["'\\x01ab'", "'\\x02cd'"], ["\\x01ab", "\\x02cd"];
    every_relation_position_keyed_by_numeric: "numeric", &[], ["1.5", "2.25"], ["1.5", "2.25"];
    every_relation_position_keyed_by_date: "date", &[], ["'2024-01-01'", "'2024-02-29'"], ["2024-01-01", "2024-02-29"];
    every_relation_position_keyed_by_timestamp: "timestamp", &[],
        ["'2024-01-01 10:00:00'", "'2024-01-01 10:00:00.5'"],
        ["2024-01-01 10:00:00", "2024-01-01 10:00:00.500"];
    every_relation_position_keyed_by_timestamptz: "timestamptz", &[],
        ["'2024-01-01 10:00:00+00'", "'2024-01-01 12:00:00.25+02'"],
        ["2024-01-01T10:00:00+00:00", "2024-01-01T10:00:00.250+00:00"];
    every_relation_position_keyed_by_time: "time", &[], ["'10:00:00'", "'23:59:59.5'"], ["10:00:00", "23:59:59.500"];
    every_relation_position_keyed_by_citext: "citext", &["CREATE EXTENSION citext"], ["'Ab'", "'cd'"], ["Ab", "cd"];
    every_relation_position_keyed_by_a_domain: "positive", &["CREATE DOMAIN positive AS int CHECK (VALUE > 0)"],
        ["1", "2"], ["1", "2"];
    every_relation_position_keyed_by_char: "char(3)", &[], ["'ab'", "'cd'"], ["ab ", "cd "];
    every_relation_position_keyed_by_float8: "float8", &[], ["1.5", "2.5"], ["1.5", "2.5"];
    every_relation_position_keyed_by_a_numeric_nan: "numeric", &[], ["'NaN'", "1.50"], ["NaN", "1.50"];
    every_relation_position_keyed_by_a_date_at_infinity: "date", &[],
        ["'infinity'", "'-infinity'"], ["infinity", "-infinity"];
    every_relation_position_keyed_by_a_timestamp_at_infinity: "timestamp", &[],
        ["'infinity'", "'-infinity'"], ["infinity", "-infinity"];
    every_relation_position_keyed_by_a_timestamptz_at_infinity: "timestamptz", &[],
        ["'infinity'", "'-infinity'"], ["infinity", "-infinity"];
    every_relation_position_keyed_by_a_date_before_christ: "date", &[],
        ["'0044-03-15 BC'", "'2024-01-01'"], ["-0043-03-15", "2024-01-01"];
    every_relation_position_keyed_by_a_timestamptz_before_christ: "timestamptz", &[],
        ["'0044-03-15 10:00:00+00 BC'", "'2024-01-01 10:00:00+00'"],
        ["-0043-03-15T10:00:00+00:00", "2024-01-01T10:00:00+00:00"];
    every_relation_position_keyed_by_date_under_sql_datestyle: "date",
        &["ALTER DATABASE postgres SET DateStyle = 'SQL, DMY'"],
        ["'2024-01-31'", "'2024-02-29'"], ["2024-01-31", "2024-02-29"];
    every_relation_position_keyed_by_timestamp_under_postgres_datestyle: "timestamp",
        &["ALTER DATABASE postgres SET DateStyle = 'Postgres, MDY'"],
        ["'2024-01-31 10:00:00'", "'2024-02-29 10:00:00.5'"],
        ["2024-01-31 10:00:00", "2024-02-29 10:00:00.500"];
    every_relation_position_keyed_by_timestamptz_under_german_datestyle: "timestamptz",
        &["ALTER DATABASE postgres SET DateStyle = 'German'", "ALTER DATABASE postgres SET TimeZone = 'Europe/Warsaw'"],
        ["'2024-01-31 10:00:00+00'", "'2024-07-01 12:00:00.25+02'"],
        ["2024-01-31T10:00:00+00:00", "2024-07-01T10:00:00.250+00:00"];
    every_relation_position_keyed_by_bytea_under_escape_output: "bytea",
        &["ALTER DATABASE postgres SET bytea_output = 'escape'"],
        ["'\\x01ab'", "'\\x02cd'"], ["\\x01ab", "\\x02cd"];
    every_relation_position_keyed_by_float8_under_fewer_float_digits: "float8",
        &["ALTER DATABASE postgres SET extra_float_digits = 0"],
        ["'0.3333333333333333'", "2.5"], ["0.3333333333333333", "2.5"];
}

/// A link spelled differently from the key it equals (issue #295): `kids`,
/// `links` and the `via` hop name the root as `link`, which Postgres compares
/// equal to the root's stored `root` (a citext's case, a numeric's scale, a
/// float's `-0`). Every document they rebuild is the root's own, under
/// `id`, the root's stored spelling, through the pre-image too, and the root's
/// delete leaves no other.
async fn a_link_spelled_differently_resolves_to_the_roots_own_key(
    sql_type: &str,
    setup: &[&str],
    [root, link]: [&str; 2],
    id: &str,
) {
    let (_container, port, url) = start_logical_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in setup {
        sqlx::raw_sql(sqlx::AssertSqlSafe(*statement))
            .execute(&pool)
            .await
            .unwrap();
    }
    for statement in [
        format!("CREATE TABLE parents (id {sql_type} PRIMARY KEY, name text)"),
        format!("CREATE TABLE kids (id int PRIMARY KEY, parent {sql_type}, name text)"),
        "CREATE TABLE grandkids (id int PRIMARY KEY, kid_id int, name text)".to_owned(),
        format!("CREATE TABLE fars (id {sql_type} PRIMARY KEY, name text)"),
        format!(
            "CREATE TABLE links (parent {sql_type}, far {sql_type}, PRIMARY KEY (parent, far))"
        ),
        "ALTER TABLE kids REPLICA IDENTITY FULL".to_owned(),
        "ALTER TABLE grandkids REPLICA IDENTITY FULL".to_owned(),
        "ALTER TABLE links REPLICA IDENTITY FULL".to_owned(),
        "CREATE PUBLICATION flusso FOR TABLE parents, kids, grandkids, fars, links".to_owned(),
        format!("INSERT INTO parents VALUES ({root}, 'pa')"),
        format!("INSERT INTO fars VALUES ({root}, 'fa')"),
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let documents = Arc::new(
        PgDocumentBuilder::connect(&url, Arc::new(keyed_parents_spec()))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication(port), url.clone())),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::new(RecordingSink::new(&recorded)),
    ));
    let parent = format!("upsert parents {id}");
    expect_op(&mut engine, &recorded, &parent).await;

    let marker_sql = "INSERT INTO grandkids (id) VALUES ($1)";
    let mut marker = 900;
    let mut rebuilt = async |sql: String| {
        marker += 1;
        rebuilt_by(
            &mut engine,
            &recorded,
            &pool,
            &sql,
            marker_sql,
            marker,
            &format!("upsert grandkids {marker}"),
        )
        .await
    };
    let ops = |ops: &[&str]| sorted(ops);

    for (sql, expected, what) in [
        (
            format!("INSERT INTO kids VALUES (2, {link}, 'k2')"),
            ops(&[&parent, "upsert kids 2"]),
            "a has_many child",
        ),
        (
            "INSERT INTO grandkids VALUES (101, 2, NULL)".to_owned(),
            ops(&[&parent, "upsert grandkids 101"]),
            "a row a `via` hop reaches",
        ),
        (
            format!("INSERT INTO links VALUES ({link}, {link})"),
            ops(&[&parent]),
            "a junction row",
        ),
        (
            format!("UPDATE fars SET name = 'fa2' WHERE id = {root}"),
            ops(&[&parent]),
            "a far row, through the junction",
        ),
        (
            format!("UPDATE parents SET name = 'pa2' WHERE id = {root}"),
            ops(&[&parent, "upsert kids 2"]),
            "the root, and the kid that `belongs_to` it",
        ),
        (
            "UPDATE kids SET parent = NULL WHERE id = 2".to_owned(),
            ops(&[&parent, "upsert kids 2"]),
            "a kid leaving, through its pre-image",
        ),
        (
            format!("UPDATE kids SET parent = {link} WHERE id = 2"),
            ops(&[&parent, "upsert kids 2"]),
            "a kid coming back",
        ),
        (
            "DELETE FROM kids WHERE id = 2".to_owned(),
            ops(&[&parent, "delete kids 2"]),
            "a deleted kid, through its pre-image",
        ),
        (
            "DELETE FROM links".to_owned(),
            ops(&[&parent]),
            "a deleted junction row",
        ),
        (
            format!("DELETE FROM parents WHERE id = {root}"),
            ops(&[&format!("delete parents {id}")]),
            "the root's delete",
        ),
    ] {
        assert_eq!(rebuilt(sql).await, expected, "{what}");
    }

    engine.abort();
}

macro_rules! linked_by_another_spelling {
    ($($test:ident: $sql_type:literal, $setup:expr, [$root:literal, $link:literal], $id:literal;)*) => {$(
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        #[ignore = "requires docker"]
        async fn $test() {
            a_link_spelled_differently_resolves_to_the_roots_own_key(
                $sql_type, $setup, [$root, $link], $id,
            )
            .await;
        }
    )*};
}

linked_by_another_spelling! {
    a_citext_link_in_another_case_rebuilds_the_roots_own_document:
        "citext", &["CREATE EXTENSION citext"], ["'Ab'", "'AB'"], "Ab";
    a_numeric_link_at_another_scale_rebuilds_the_roots_own_document:
        "numeric", &[], ["1.5", "1.500"], "1.5";
    a_float8_link_of_negative_zero_rebuilds_the_roots_own_document:
        "float8", &[], ["'0'", "'-0'"], "0";
    a_case_insensitive_text_link_rebuilds_the_roots_own_document:
        "text COLLATE folded",
        &["CREATE COLLATION folded (provider = icu, locale = 'und-u-ks-level2', deterministic = false)"],
        ["'Ab'", "'AB'"], "Ab";
}

/// PgBouncer in session mode with its stock `ignore_startup_parameters`, in
/// front of every connection flusso opens, the replication one included
/// (issue #295): it refuses any parameter inside a startup `options`, so the
/// settings a key's text depends on are set after connecting, or not needed.
/// A `bytea` key under `bytea_output = escape` backfills and follows.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn the_pipeline_runs_through_pgbouncer_in_session_mode() {
    use testcontainers_modules::testcontainers::GenericImage;
    use testcontainers_modules::testcontainers::core::IntoContainerPort;

    let (postgres, _port, url) = start_logical_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "ALTER DATABASE postgres SET bytea_output = 'escape'",
        "CREATE TABLE users (id bytea PRIMARY KEY, email text)",
        "CREATE PUBLICATION flusso FOR TABLE users",
        "INSERT INTO users VALUES ('\\x01', 'a@x')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(&pool)
        .await
        .unwrap();

    let host = postgres.get_bridge_ip_address().await.unwrap().to_string();
    let bouncer = GenericImage::new("edoburu/pgbouncer", "v1.26.1-p0")
        .with_exposed_port(5432.tcp())
        .with_env_var("DB_HOST", host)
        .with_env_var("DB_PORT", "5432")
        .with_env_var("DB_USER", "postgres")
        .with_env_var("DB_PASSWORD", "postgres")
        .with_env_var("AUTH_TYPE", "scram-sha-256")
        .with_env_var("POOL_MODE", "session")
        .with_env_var("IGNORE_STARTUP_PARAMETERS", "extra_float_digits")
        .start()
        .await
        .unwrap();
    let port = bouncer.get_host_port_ipv4(5432.tcp()).await.unwrap();
    let bounced = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let deadline = Instant::now() + Duration::from_secs(30);
    while PgPoolOptions::new().connect(&bounced).await.is_err() {
        assert!(
            Instant::now() < deadline,
            "PgBouncer never accepted a connection"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let users = IndexSchema {
        fields: vec![column_field("email", "email")],
        ..users_schema(vec![])
    };
    let spec = SourceSpec::new(BTreeMap::from([(index_name("users"), users)]));
    let documents = PgDocumentBuilder::connect(&bounced, Arc::new(spec))
        .await
        .unwrap();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication(port), bounced.clone())),
        Arc::new(documents),
        Arc::new(RecordingSink::new(&recorded)),
    ));
    expect_op(&mut engine, &recorded, "upsert users \\x01").await;
    sqlx::query("INSERT INTO users VALUES ('\\x02', 'b@x')")
        .execute(&pool)
        .await
        .unwrap();
    expect_op(&mut engine, &recorded, "upsert users \\x02").await;
    engine.abort();
}
