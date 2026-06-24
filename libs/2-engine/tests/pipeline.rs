//! Full-pipeline e2e: real Postgres (logical replication) → the real
//! [`Engine::run`] → a **real OpenSearch sink**, asserting the *actual indexed
//! document* after each change. Nothing is mocked.
//!
//! This is the suite that proves the one thing flusso exists to do — keep an
//! OpenSearch index in step with Postgres. The sibling `wal.rs` drives a
//! *recording* sink, so it only sees *which op* the engine emitted; it cannot
//! catch a change the engine turns into the *wrong* op (an update silently
//! rebuilt as a tombstone, a soft-deleted row that should have been removed).
//! Here we read the index back over HTTP and assert the document's contents.
//!
//! Coverage:
//! - [`live_crud_round_trips_across_key_types`] — insert / update / delete on the
//!   live path, across `uuid` / `int` / `bigint` / `text` primary keys. The
//!   key-type matrix guards a class of bug where the WAL key decoder
//!   (`cdc/pgoutput.rs::typed_value`, by OID) and the read-back decoder
//!   (`document/value.rs::decode_column`, by SQL type name) disagree on the
//!   [`GenericValue`] variant for a column: the engine's key match then misses
//!   and the change is written as a tombstone. `bigint`/`text` are the controls
//!   the two decoders already agreed on; `uuid`/`int` are the ones they must.
//! - [`soft_delete_tombstones_when_set_and_restores_when_unset`] — with a
//!   soft-delete marker (boolean *and* timestamp), a row whose marker is set is
//!   removed from the index even though it still exists in Postgres, and clearing
//!   the marker brings it back.
//! - [`backfill_indexes_active_rows_and_skips_soft_deleted`] — the initial seed:
//!   rows that pre-date the slot are indexed by backfill, except soft-deleted
//!   ones, which must never appear.
//!
//! Each change is verified by polling a realtime `GET {index}/_doc/{id}` (reads
//! the translog, so no refresh wait) until the index reflects the expectation or
//! a deadline trips — so the assertions are deterministic despite async flush.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p engine --test pipeline -- --ignored
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine::Engine;
use schema_core::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, IndexName,
    IndexSchema, Secret, SinkName, SoftDelete, SoftDeleteColumn, TableName,
};
use sinks_core::Sink;
use sinks_opensearch::OpensearchSink;
use sources_core::SourceSpec;
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::AssertSqlSafe;
use sqlx::postgres::{PgPool, PgPoolOptions};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

// ───────────────────────────── test 1: live CRUD ─────────────────────────────

/// One index in the key-type matrix: a Postgres `id` column type, the SQL
/// literal for a concrete id, and that id's string form as the OpenSearch `_id`.
struct Case {
    /// Logical index / table name.
    index: &'static str,
    /// SQL type of the `id` primary key.
    id_type: &'static str,
    /// SQL literal for the row's id (`'…'` for text/uuid, bare for integers).
    id_literal: &'static str,
    /// The document `_id` the sink derives — the key value in string form.
    doc_id: &'static str,
}

const CASES: &[Case] = &[
    // The two the WAL/read-back decoders must be made to agree on.
    Case {
        index: "by_uuid",
        id_type: "uuid",
        id_literal: "'095b7d61-cbbc-49d6-842c-231d06b81e7a'",
        doc_id: "095b7d61-cbbc-49d6-842c-231d06b81e7a",
    },
    Case {
        index: "by_int",
        id_type: "int",
        id_literal: "7",
        doc_id: "7",
    },
    // Controls: types the decoders already agreed on. They must keep working.
    Case {
        index: "by_bigint",
        id_type: "bigint",
        id_literal: "8",
        doc_id: "8",
    },
    Case {
        index: "by_text",
        id_type: "text",
        id_literal: "'sku-9'",
        doc_id: "sku-9",
    },
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn live_crud_round_trips_across_key_types() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    let tables = CASES.iter().map(|c| c.index).collect::<Vec<_>>();
    for case in CASES {
        create_table(
            &pg.pool,
            &format!(
                "CREATE TABLE \"{}\" (id {} PRIMARY KEY, name text)",
                case.index, case.id_type
            ),
        )
        .await;
    }
    let spec = SourceSpec::new(
        CASES
            .iter()
            .map(|c| (index_name(c.index), simple_schema(c.index, None)))
            .collect::<BTreeMap<_, _>>(),
    );
    let live = start_pipeline(&pg, &os, spec, &tables).await;
    live.await_seeded(CASES.len()).await;

    for case in CASES {
        let what = case.index;

        // INSERT → the document appears with the inserted value.
        exec(
            &pg.pool,
            &format!(
                "INSERT INTO \"{}\" (id, name) VALUES ({}, 'first')",
                case.index, case.id_literal
            ),
        )
        .await;
        live.assert_eventually(case.index, case.doc_id, Some("first"))
            .await
            .unwrap_or_else(|got| panic!("[{what}] INSERT did not land: want first, saw {got}"));

        // UPDATE a non-key column → the document must be *updated*, not deleted.
        // This is the exact failure that motivated the suite: a live update on a
        // uuid/int key was rebuilt with a mismatched key and written as a
        // tombstone, so the row vanished from the index.
        exec(
            &pg.pool,
            &format!(
                "UPDATE \"{}\" SET name = 'second' WHERE id = {}",
                case.index, case.id_literal
            ),
        )
        .await;
        live.assert_eventually(case.index, case.doc_id, Some("second"))
            .await
            .unwrap_or_else(|got| {
                panic!(
                    "[{what}] UPDATE of a non-key column did not propagate: want second, saw \
                     {got} (\"absent\" means the update was wrongly applied as a tombstone)"
                )
            });

        // DELETE → the document is gone.
        exec(
            &pg.pool,
            &format!(
                "DELETE FROM \"{}\" WHERE id = {}",
                case.index, case.id_literal
            ),
        )
        .await;
        live.assert_eventually(case.index, case.doc_id, None)
            .await
            .unwrap_or_else(|got| {
                panic!("[{what}] DELETE did not tombstone: want absent, saw {got}")
            });
    }

    live.shutdown();
}

// ──────────────────────────── test 2: soft delete ────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn soft_delete_tombstones_when_set_and_restores_when_unset() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    // Two marker shapes: a boolean flag and a nullable timestamp. flusso treats a
    // marker as "deleted" when it is truthy — a true boolean, or *any* non-null
    // value — so both forms must tombstone when set and restore when cleared.
    create_table(
        &pg.pool,
        "CREATE TABLE soft_bool (id uuid PRIMARY KEY, name text, archived boolean NOT NULL DEFAULT false)",
    )
    .await;
    create_table(
        &pg.pool,
        "CREATE TABLE soft_ts (id int PRIMARY KEY, name text, deleted_at timestamptz)",
    )
    .await;

    let spec = SourceSpec::new(BTreeMap::from([
        (
            index_name("soft_bool"),
            simple_schema("soft_bool", Some(soft_delete_on("archived"))),
        ),
        (
            index_name("soft_ts"),
            simple_schema("soft_ts", Some(soft_delete_on("deleted_at"))),
        ),
    ]));
    let live = start_pipeline(&pg, &os, spec, &["soft_bool", "soft_ts"]).await;
    live.await_seeded(2).await;

    let uuid = "11111111-2222-3333-4444-555555555555";

    // ── boolean marker ──
    exec(
        &pg.pool,
        &format!("INSERT INTO soft_bool (id, name, archived) VALUES ('{uuid}', 'alive', false)"),
    )
    .await;
    live.assert_eventually("soft_bool", uuid, Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_bool] active row should be indexed, saw {got}"));

    // Set the marker → the row still exists in Postgres but must leave the index.
    exec(
        &pg.pool,
        &format!("UPDATE soft_bool SET archived = true WHERE id = '{uuid}'"),
    )
    .await;
    live.assert_eventually("soft_bool", uuid, None)
        .await
        .unwrap_or_else(|got| {
            panic!("[soft_bool] soft-deleted row should be tombstoned, saw {got}")
        });

    // Clear the marker → the row returns to the index.
    exec(
        &pg.pool,
        &format!("UPDATE soft_bool SET archived = false WHERE id = '{uuid}'"),
    )
    .await;
    live.assert_eventually("soft_bool", uuid, Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_bool] restored row should be re-indexed, saw {got}"));

    // ── timestamp marker ──
    exec(
        &pg.pool,
        "INSERT INTO soft_ts (id, name) VALUES (1, 'alive')",
    )
    .await;
    live.assert_eventually("soft_ts", "1", Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_ts] active row should be indexed, saw {got}"));

    exec(
        &pg.pool,
        "UPDATE soft_ts SET deleted_at = now() WHERE id = 1",
    )
    .await;
    live.assert_eventually("soft_ts", "1", None)
        .await
        .unwrap_or_else(|got| panic!("[soft_ts] soft-deleted row should be tombstoned, saw {got}"));

    exec(
        &pg.pool,
        "UPDATE soft_ts SET deleted_at = NULL WHERE id = 1",
    )
    .await;
    live.assert_eventually("soft_ts", "1", Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_ts] restored row should be re-indexed, saw {got}"));

    live.shutdown();
}

// ───────────────────────────── test 3: backfill ──────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn backfill_indexes_active_rows_and_skips_soft_deleted() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    create_table(
        &pg.pool,
        "CREATE TABLE seeded (id int PRIMARY KEY, name text, archived boolean NOT NULL DEFAULT false)",
    )
    .await;
    // Rows that exist *before* the slot — only backfill can surface them. One is
    // soft-deleted and must be skipped; the others must be indexed.
    exec(
        &pg.pool,
        "INSERT INTO seeded (id, name, archived) VALUES \
         (1, 'one', false), (2, 'two', false), (3, 'gone', true)",
    )
    .await;

    let spec = SourceSpec::new(BTreeMap::from([(
        index_name("seeded"),
        simple_schema("seeded", Some(soft_delete_on("archived"))),
    )]));
    let live = start_pipeline(&pg, &os, spec, &["seeded"]).await;
    live.await_seeded(1).await;

    // Active rows are backfilled; the soft-deleted one never appears.
    live.assert_eventually("seeded", "1", Some("one"))
        .await
        .unwrap_or_else(|got| panic!("[seeded] active row 1 should be backfilled, saw {got}"));
    live.assert_eventually("seeded", "2", Some("two"))
        .await
        .unwrap_or_else(|got| panic!("[seeded] active row 2 should be backfilled, saw {got}"));
    live.assert_eventually("seeded", "3", None)
        .await
        .unwrap_or_else(|got| panic!("[seeded] soft-deleted row 3 must not be indexed, saw {got}"));

    live.shutdown();
}

// ───────────────────────────────── harness ───────────────────────────────────

/// A running Postgres with logical replication and a connection pool.
struct Pg {
    _container: ContainerAsync<Postgres>,
    pool: PgPool,
    url: String,
    port: u16,
}

/// A running OpenSearch and its base URL.
struct Os {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

/// A live pipeline: the engine task plus a client for reading the index back.
struct Pipeline {
    engine: tokio::task::JoinHandle<engine::Result<()>>,
    http: reqwest::Client,
    os_url: String,
}

impl Pipeline {
    /// Wait until the engine has stood up and seeded every index — finished
    /// `ensure_index` + backfill and gone live. Issuing a change before that
    /// races startup (the change predates the live stream), which is a harness
    /// artifact, not what these tests probe. Each index writes one `flusso_meta`
    /// doc when marked seeded.
    async fn await_seeded(&self, want: usize) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let _ = self
                .http
                .post(format!("{}/flusso_meta/_refresh", self.os_url))
                .send()
                .await;
            let count = self
                .http
                .get(format!("{}/flusso_meta/_count", self.os_url))
                .send()
                .await
                .ok()
                .filter(|r| r.status().is_success());
            let count = match count {
                Some(r) => r
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|b| b.get("count").and_then(serde_json::Value::as_u64))
                    .unwrap_or(0),
                None => 0,
            };
            if count as usize >= want {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "engine did not seed {want} indexes in time (flusso_meta count = {count})"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Poll `{index}/_doc/{id}` until the document's `name` equals `expected`
    /// (or the document is absent, for `expected = None`). `Err(observed)` on a
    /// deadline, so the caller can report what it actually saw.
    async fn assert_eventually(
        &self,
        index: &str,
        id: &str,
        expected: Option<&str>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let observed = match self.fetch_name(index, id).await {
                Some(name) => name,
                None => "absent".to_owned(),
            };
            let matches = match expected {
                Some(want) => observed == want,
                None => observed == "absent",
            };
            if matches {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(observed);
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Realtime `GET {index}/_doc/{id}` (reads the translog — no refresh wait).
    /// `Some(name)` if the document exists, `None` if it does not.
    async fn fetch_name(&self, index: &str, id: &str) -> Option<String> {
        let resp = self
            .http
            .get(format!("{}/{index}/_doc/{id}", self.os_url))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: serde_json::Value = resp.json().await.ok()?;
        if body.get("found").and_then(serde_json::Value::as_bool) != Some(true) {
            return None;
        }
        Some(
            body.get("_source")
                .and_then(|s| s.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing>")
                .to_owned(),
        )
    }

    fn shutdown(self) {
        self.engine.abort();
    }
}

/// Bring up Postgres, the publication + slot over `tables`, and the real engine
/// driving the real OpenSearch sink for `spec`.
async fn start_pipeline(pg: &Pg, os: &Os, spec: SourceSpec, tables: &[&str]) -> Pipeline {
    let quoted = tables
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");
    exec(
        &pg.pool,
        &format!("CREATE PUBLICATION flusso FOR TABLE {quoted}"),
    )
    .await;
    exec(
        &pg.pool,
        "SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')",
    )
    .await;

    let documents = Arc::new(
        PgDocumentBuilder::connect(&pg.url, Arc::new(spec))
            .await
            .unwrap(),
    );
    let sink: Arc<dyn Sink> = Arc::new(opensearch_sink(&os.url));
    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(pg.port);
    let engine = Engine::new(
        Arc::new(WalChangeCapture::new(replication, pg.url.clone())),
        documents,
        sink,
    );
    Pipeline {
        engine: tokio::spawn(engine.run()),
        http: reqwest::Client::new(),
        os_url: os.url.clone(),
    }
}

async fn start_postgres() -> Pg {
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
    Pg {
        _container: container,
        pool,
        url,
        port,
    }
}

async fn start_opensearch() -> Os {
    let container = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/_cluster/health")
                .with_port(9200.tcp())
                .with_expected_status_code(200u16)
                .with_poll_interval(Duration::from_secs(1)),
        ))
        .with_env_var("discovery.type", "single-node")
        .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
        .with_env_var("DISABLE_INSTALL_DEMO_CONFIG", "true")
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let port = container.get_host_port_ipv4(9200).await.unwrap();
    Os {
        _container: container,
        url: format!("http://127.0.0.1:{port}"),
    }
}

async fn create_table(pool: &PgPool, ddl: &str) {
    exec(pool, ddl).await;
}

async fn exec(pool: &PgPool, sql: &str) {
    sqlx::query(AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

fn opensearch_sink(os_url: &str) -> OpensearchSink {
    let config = schema_core::OpensearchSink {
        url: Secret::Value(os_url.to_owned()),
        username: None,
        password: None,
        tls_verify: false,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 30,
        max_retries: 3,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 1,
        refresh_interval: "1s".to_owned(),
        text_analysis: schema_core::TextAnalysis::Builtin,
        auto_subfields: true,
    };
    OpensearchSink::from_config(&SinkName::try_new("e2e").unwrap(), &config).unwrap()
}

/// A root table with an `id` key, a `name` column, and an optional soft-delete
/// marker. (The marker column is read raw, so it is not a mapped field.)
fn simple_schema(table_name: &str, soft_delete: Option<SoftDelete>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table(table_name),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete,
        filters: None,
        fields: vec![column_field("id", "id"), column_field("name", "name")],
    }
}

fn soft_delete_on(col: &str) -> SoftDelete {
    SoftDelete::Column(SoftDeleteColumn {
        column: column(col),
        when: None,
    })
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
