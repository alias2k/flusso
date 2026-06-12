//! Full WAL pipeline e2e: a real Postgres (logical replication) → the engine →
//! a recording sink. Inserts and deletes on the source must surface as document
//! upserts and tombstones.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test wal -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::Engine;
use schema_core::{
    Column, ColumnName, Config, ConnectionSpec, DatabaseSchema, Field, FieldName, FieldSource,
    FlussoType, GenericValue, Index, IndexName, IndexSchema, Secret, Source, SourceType, TableName,
};
use sinks_core::{Result as SinkResult, Sink};
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// A sink that records the operations it receives, for assertions.
#[derive(Debug)]
struct RecordingSink {
    ops: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Sink for RecordingSink {
    async fn upsert(
        &self,
        index: &IndexName,
        id: &str,
        _document: &GenericValue,
    ) -> SinkResult<()> {
        self.ops
            .lock()
            .unwrap()
            .push(format!("upsert {} {id}", index.as_ref()));
        Ok(())
    }

    async fn delete(&self, index: &IndexName, id: &str) -> SinkResult<()> {
        self.ops
            .lock()
            .unwrap()
            .push(format!("delete {} {id}", index.as_ref()));
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> SinkResult<()> {
        Ok(())
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
        PgDocumentBuilder::connect(&url, Arc::new(users_config(&url)))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink {
        ops: Arc::clone(&recorded),
    });
    let engine = Engine::new(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        documents,
        sink,
    );

    let mut engine = tokio::spawn(engine.run());

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
        PgDocumentBuilder::connect(&url, Arc::new(users_config(&url)))
            .await
            .unwrap(),
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink {
        ops: Arc::clone(&recorded),
    });
    let engine = Engine::new(
        Arc::new(WalChangeCapture::new(replication, url.clone())),
        documents,
        sink,
    );

    let mut engine = tokio::spawn(engine.run());

    // Both pre-existing rows are seeded by the backfill.
    expect_op(&mut engine, &recorded, "upsert users 1").await;
    expect_op(&mut engine, &recorded, "upsert users 2").await;

    engine.abort();
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

fn users_config(connection_url: &str) -> Config {
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
    Config {
        source: Source {
            source_type: SourceType::Postgres,
            connection: Some(ConnectionSpec::Url(Secret::Value(
                connection_url.to_owned(),
            ))),
        },
        sinks: BTreeMap::new(),
        indexes: BTreeMap::from([(
            index_name("users"),
            Index {
                enabled: true,
                schema,
            },
        )]),
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
