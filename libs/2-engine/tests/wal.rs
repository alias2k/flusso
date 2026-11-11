//! Full WAL pipeline e2e: a real Postgres (logical replication) → the ingest
//! engine → a lane → a sink engine over a recording sink. Inserts and deletes
//! on the source must surface as document upserts and tombstones, and an
//! update that changes no column a document reads must surface as nothing.
//! Aggregates across several hops (`via`) rebuild their parent from a change
//! on any hop, including hops back into the root's own table and hops under a
//! nested relation.
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
    Aggregate, AggregateKey, AggregateOp, Column, ColumnName, DatabaseSchema, Envelope, Field,
    FieldName, FieldSource, Filter, FilterOp, FilterValue, FlussoType, IndexName, IndexSchema,
    Join, JoinKind, OrderBy, RawFilter, RawFilterValue, Relation, SinkName, TableName, Through,
    ValueOpFilter, ViaHop,
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
    sql: &'static str,
    marker_sql: &'static str,
    marker: i32,
    marker_op: &str,
) -> Vec<String> {
    recorded.lock().unwrap().clear();
    sqlx::raw_sql(sql).execute(pool).await.unwrap();
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

fn users_spec() -> SourceSpec {
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![column_field("id", "id"), column_field("email", "email")],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
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
        op: AggregateOp::Sum(column("amount")),
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
                source: FieldSource::Column(Column {
                    column: column("id"),
                    ty: FlussoType::Long,
                    nullable: false,
                    transforms: Vec::new(),
                    default: None,
                    enum_order: Vec::new(),
                    subfields: true,
                    indexed: true,
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

fn aggregate_field(name: &str, aggregate: Aggregate) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
    }
}

fn column_field(name: &str, col: &str) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: column(col),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
        }),
    }
}

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}
fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}
fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}
fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}
