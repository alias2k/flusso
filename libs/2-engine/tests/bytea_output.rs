//! A `bytea` value reaches a document as the same text whatever the server's
//! `bytea_output` (issue #298): the backfill and a live change, through
//! PgBouncer in transaction mode with `server_reset_query_always`, so no
//! session `SET` survives to the next query, on a database set to `hex` (the
//! default) and to `escape`.
//!
//! Every place a document reads a `bytea` as text is covered: a `binary`
//! field, an `array: true` of `binary`, a `bytea` under `keyword`, `text`
//! and `custom`, a column `default`, a domain over `bytea`, a `belongs_to`
//! and a `has_many` (`json_agg`), an `ids` aggregate over a `bytea` key, and
//! `min`/`max` over a `bytea` column (Postgres 18 and later), so PG 18 runs it.
//! An array of a domain over `bytea`, and a domain over that, are rendered too.
//! A `bytea` inside a composite is out of the query's reach: over a direct
//! connection the builder's own session setting still renders it as hex,
//! behind the pooler it follows the server's setting.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --test bytea_output --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::{IngestEngine, SinkControl, SinkEngine};
use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, DatabaseSchema, Envelope, Field, FieldSource,
    FlussoType, IndexSchema, Join, JoinKind, OrderBy, Relation, SinkName,
};
use serde_json::{Value, json};
use sink::{Result as SinkResult, Sink};
use source::SourceSpec;
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::postgres::PgPoolOptions;
use stream::Stream;
use stream_channel::ChannelStream;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::IntoContainerPort;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{GenericImage, ImageExt};

mod common;
use common::{column, field, index_name, table};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_bytea_value_reads_as_hex_under_bytea_output_hex() {
    every_bytea_value_reads_as_hex("hex", Route::Pooler).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_bytea_value_reads_as_hex_under_bytea_output_escape() {
    every_bytea_value_reads_as_hex("escape", Route::Pooler).await;
}

/// A direct connection reads every shape as hex text, the ones the query
/// can't reach (inside a composite) included, as it did before #298.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_bytea_value_reads_as_hex_over_a_direct_connection_under_escape() {
    every_bytea_value_reads_as_hex("escape", Route::Direct).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_bytea_value_reads_as_hex_over_a_direct_connection_under_hex() {
    every_bytea_value_reads_as_hex("hex", Route::Direct).await;
}

/// How the document builder reaches Postgres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Direct,
    /// PgBouncer in transaction mode, resetting each server connection after
    /// every transaction: no session `SET` reaches the next query.
    Pooler,
}

/// PG 18 with logical replication: the first with `min`/`max` over `bytea`.
async fn start_postgres() -> (
    testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    u16,
    String,
) {
    let postgres = Postgres::default()
        .with_tag("18-alpine")
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
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (postgres, port, url)
}

/// The URL the builder connects with over `route`, and the pooler, if any.
async fn connect_over(
    route: Route,
    postgres: &testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    url: &str,
) -> (
    String,
    Option<testcontainers_modules::testcontainers::ContainerAsync<GenericImage>>,
) {
    if route == Route::Direct {
        return (url.to_owned(), None);
    }
    let host = postgres.get_bridge_ip_address().await.unwrap().to_string();
    let bouncer = GenericImage::new("edoburu/pgbouncer", "v1.26.1-p0")
        .with_exposed_port(5432.tcp())
        .with_env_var("DB_HOST", host)
        .with_env_var("DB_PORT", "5432")
        .with_env_var("DB_USER", "postgres")
        .with_env_var("DB_PASSWORD", "postgres")
        .with_env_var("AUTH_TYPE", "scram-sha-256")
        .with_env_var("POOL_MODE", "transaction")
        .with_env_var("SERVER_RESET_QUERY", "DISCARD ALL")
        .with_env_var("SERVER_RESET_QUERY_ALWAYS", "1")
        .with_env_var("IGNORE_STARTUP_PARAMETERS", "extra_float_digits")
        .start()
        .await
        .unwrap();
    let bounced_port = bouncer.get_host_port_ipv4(5432.tcp()).await.unwrap();
    let bounced = format!("postgres://postgres:postgres@127.0.0.1:{bounced_port}/postgres");
    let deadline = Instant::now() + Duration::from_secs(30);
    while PgPoolOptions::new().connect(&bounced).await.is_err() {
        assert!(
            Instant::now() < deadline,
            "PgBouncer never accepted a connection"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    (bounced, Some(bouncer))
}

/// The documents of `docs`, backfilled then changed live, on a database whose
/// `bytea_output` is `setting`, read over `route`.
async fn every_bytea_value_reads_as_hex(setting: &str, route: Route) {
    let (postgres, port, url) = start_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        format!("ALTER DATABASE postgres SET bytea_output = '{setting}'"),
        "CREATE DOMAIN blob AS bytea".to_owned(),
        "CREATE DOMAIN blobs_of_blob AS blob[]".to_owned(),
        "CREATE TYPE pair AS (n int, b bytea)".to_owned(),
        "CREATE TABLE owners (id int PRIMARY KEY, avatar bytea)".to_owned(),
        "CREATE TABLE docs (id int PRIMARY KEY, owner_id int, blob bytea, blobs bytea[], \
         tag bytea, wrapped blob, spare bytea, elems blob[], nested blobs_of_blob, pair pair)"
            .to_owned(),
        "CREATE TABLE parts (id bytea PRIMARY KEY, doc_id int, blob bytea)".to_owned(),
        "ALTER TABLE parts REPLICA IDENTITY FULL".to_owned(),
        "CREATE PUBLICATION flusso FOR TABLE owners, docs, parts".to_owned(),
        r"INSERT INTO owners VALUES (1, '\x0a0b')".to_owned(),
        r"INSERT INTO docs VALUES (1, 1, '\x0102ff', ARRAY['\x01'::bytea, NULL, '\xff00'], '\x6869', '\x7f', NULL, ARRAY['\x09', '\x5c5c']::blob[], ARRAY['\x0a']::blobs_of_blob, ROW(1, '\x0aff'))"
            .to_owned(),
        r"INSERT INTO parts VALUES ('\x0001', 1, '\x5c00'), ('\x0002', 1, '\x')".to_owned(),
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

    let (bounced, _bouncer) = connect_over(route, &postgres, &url).await;
    // A `bytea` inside a composite is past the query's reach: behind a pooler
    // that drops the session's `SET`, it follows the server's setting.
    let pair = if route == Route::Pooler && setting == "escape" {
        json!({ "n": 1, "b": r"\012\377" })
    } else {
        json!({ "n": 1, "b": r"\x0aff" })
    };

    let spec = SourceSpec::new(BTreeMap::from([(index_name("docs"), docs_schema())]));
    let documents = PgDocumentBuilder::connect(&bounced, Arc::new(spec))
        .await
        .unwrap();
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(run_deployment(
        Arc::new(WalChangeCapture::new(replication(port), bounced.clone())),
        Arc::new(documents),
        Arc::new(DocumentSink(Arc::clone(&recorded))),
    ));

    let backfilled = document(&mut engine, &recorded, "1").await;
    assert_eq!(
        backfilled,
        expected_doc_1(json!(""), pair.clone()),
        "the backfill"
    );

    for statement in [
        r"UPDATE parts SET blob = '\xabcd' WHERE id = '\x0002'",
        r"INSERT INTO docs VALUES (2, NULL, '\x', '{}', NULL, NULL, '\x00', NULL, NULL, NULL)",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(
        document(&mut engine, &recorded, "2").await,
        json!({
            "blob": "",
            "blobs": [],
            "tag": null,
            "tagText": null,
            "tagCustom": null,
            "wrapped": null,
            "fallback": r"\x00",
            "owner": null,
            "parts": [],
            "partIds": [],
            "firstPart": null,
            "lastPart": null,
            "elems": null,
            "elemsCustom": null,
            "nested": null,
            "pair": null,
        }),
        "a live insert"
    );
    let rebuilt = recorded
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(id, _)| id == "1")
        .map(|(_, document)| document.clone())
        .unwrap();
    assert_eq!(
        rebuilt,
        expected_doc_1(json!("q80="), pair),
        "a live change on a child"
    );
    engine.abort();
}

/// A column changing type while the pipeline runs (`bytea` → `text`): the
/// builder's cached column types go stale, the document query built from
/// them fails once, and the restarted engine builds from fresh types instead
/// of failing the same way until the process restarts.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_column_changing_type_mid_run_recovers_after_one_restart() {
    let (_postgres, port, url) = start_postgres().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE kws (id int PRIMARY KEY, b_kw bytea, t_kw text)",
        "CREATE PUBLICATION flusso FOR TABLE kws",
        r"INSERT INTO kws VALUES (1, '\x0102', 'hello')",
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
    let spec = SourceSpec::new(BTreeMap::from([(index_name("kws"), kws_schema())]));
    let documents = PgDocumentBuilder::connect(&url, Arc::new(spec))
        .await
        .unwrap();
    let capture = WalChangeCapture::new(replication(port), url.clone());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let restarts = Arc::new(AtomicUsize::new(0));
    let mut engine = tokio::spawn(supervise(
        Arc::new(capture),
        Arc::new(documents),
        Arc::new(DocumentSink(Arc::clone(&recorded))),
        Arc::clone(&restarts),
    ));
    let expect = |b_kw: &str, t_kw: &str| json!({ "bKw": b_kw, "tKw": t_kw });

    document_reaches(&mut engine, &recorded, "1", &expect(r"\x0102", "hello")).await;
    alter(
        &pool,
        "ALTER TABLE kws ALTER COLUMN b_kw TYPE text USING encode(b_kw, 'hex'); \
         UPDATE kws SET b_kw = 'now-text' WHERE id = 1",
    )
    .await;
    document_reaches(&mut engine, &recorded, "1", &expect("now-text", "hello")).await;
    alter(
        &pool,
        r"ALTER TABLE kws ALTER COLUMN t_kw TYPE bytea USING convert_to(t_kw, 'UTF8'); UPDATE kws SET t_kw = '\x0a0b' WHERE id = 1",
    )
    .await;
    document_reaches(&mut engine, &recorded, "1", &expect("now-text", r"\x0a0b")).await;
    assert!(
        restarts.load(Ordering::SeqCst) <= 1,
        "restarted {} times",
        restarts.load(Ordering::SeqCst)
    );
    engine.abort();
}

/// `kws` indexed as `{ bKw, tKw }`, both `keyword`.
fn kws_schema() -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table("kws"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_of("bKw", "b_kw", FlussoType::Keyword),
            column_of("tKw", "t_kw", FlussoType::Keyword),
        ],
    }
}

async fn alter(pool: &sqlx::PgPool, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

/// Run the deployment the way the daemon supervises it: restart it after an
/// error, counting the restarts, until it ends cleanly or has restarted five
/// times.
async fn supervise(
    capture: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    restarts: Arc<AtomicUsize>,
) -> engine::Result<()> {
    loop {
        let result = run_deployment(
            Arc::clone(&capture),
            Arc::clone(&documents),
            Arc::clone(&sink),
        )
        .await;
        match result {
            Ok(()) => return Ok(()),
            Err(error) if restarts.fetch_add(1, Ordering::SeqCst) >= 4 => return Err(error),
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}

/// Wait until the latest document recorded for `id` is `expected`.
async fn document_reaches(
    engine: &mut tokio::task::JoinHandle<engine::Result<()>>,
    recorded: &Arc<Mutex<Vec<(String, Value)>>>,
    id: &str,
    expected: &Value,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if engine.is_finished() {
            panic!("engine stopped before {expected}: {:?}", engine.await);
        }
        let latest = recorded
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(recorded_id, _)| recorded_id == id)
            .map(|(_, document)| document.clone());
        if latest.as_ref() == Some(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected}; latest {latest:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Document 1, with `second_blob` as its second part's `blob` and `pair` as
/// its composite.
fn expected_doc_1(second_blob: Value, pair: Value) -> Value {
    json!({
        "blob": "AQL/",
        "blobs": ["AQ==", "/wA="],
        "tag": r"\x6869",
        "tagText": r"\x6869",
        "tagCustom": r"\x6869",
        "wrapped": "fw==",
        "fallback": r"\x0909",
        "owner": { "avatar": "Cgs=" },
        "parts": [
            { "id": r"\x0001", "blob": "XAA=" },
            { "id": r"\x0002", "blob": second_blob },
        ],
        "partIds": ["AAE=", "AAI="],
        "firstPart": "AAE=",
        "lastPart": "AAI=",
        "elems": ["CQ==", "XFw="],
        "elemsCustom": [r"\x09", r"\x5c5c"],
        "nested": ["Cg=="],
        "pair": pair,
    })
}

fn docs_schema() -> IndexSchema {
    let parts = Join {
        table: table("parts"),
        kind: JoinKind::HasMany {
            foreign_key: column("doc_id"),
        },
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: Some(vec![OrderBy {
            column: column("id"),
            direction: None,
        }]),
        limit: None,
        fields: vec![
            column_of("id", "id", FlussoType::Keyword),
            column_of("blob", "blob", FlussoType::Binary),
        ],
        indexed: true,
        on: vec![],
    };
    let owner = Join {
        table: table("owners"),
        kind: JoinKind::BelongsTo {
            column: column("owner_id"),
        },
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: None,
        limit: None,
        fields: vec![column_of("avatar", "avatar", FlussoType::Binary)],
        indexed: true,
        on: vec![],
    };
    let over_parts = |op: AggregateOp, value_type: Option<FlussoType>| {
        Relation::Aggregate(Aggregate {
            table: table("parts"),
            op,
            key: AggregateKey::Direct(column("doc_id")),
            value_type,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: vec![],
        })
    };
    let blobs = array_of("blobs", "blobs", FlussoType::Binary);
    let mut fallback = column_of("fallback", "spare", FlussoType::Keyword);
    if let FieldSource::Column(column) = &mut fallback.source {
        column.default = Some(kernel::GenericValue::String(r"\x0909".to_owned()));
    }
    IndexSchema {
        version: 1,
        table: table("docs"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_of("blob", "blob", FlussoType::Binary),
            blobs,
            column_of("tag", "tag", FlussoType::Keyword),
            column_of("tagText", "tag", FlussoType::Text),
            column_of(
                "tagCustom",
                "tag",
                FlussoType::Custom {
                    postgres: vec!["bytea".to_owned()],
                    opensearch: "keyword".to_owned(),
                },
            ),
            column_of("wrapped", "wrapped", FlussoType::Binary),
            fallback,
            relation("owner", Relation::Join(owner)),
            relation("parts", Relation::Join(parts)),
            relation(
                "partIds",
                over_parts(
                    AggregateOp::Ids {
                        element_type: FlussoType::Binary,
                    },
                    None,
                ),
            ),
            relation(
                "firstPart",
                over_parts(
                    AggregateOp::Min(column("id").into()),
                    Some(FlussoType::Binary),
                ),
            ),
            relation(
                "lastPart",
                over_parts(
                    AggregateOp::Max(column("id").into()),
                    Some(FlussoType::Binary),
                ),
            ),
            array_of("elems", "elems", FlussoType::Binary),
            array_of(
                "elemsCustom",
                "elems",
                FlussoType::Custom {
                    postgres: vec!["blob[]".to_owned()],
                    opensearch: "keyword".to_owned(),
                },
            ),
            array_of("nested", "nested", FlussoType::Binary),
            column_of("pair", "pair", FlussoType::Json),
        ],
    }
}

fn array_of(name: &str, col: &str, ty: FlussoType) -> Field {
    let mut field = column_of(name, col, ty);
    if let FieldSource::Column(column) = &mut field.source {
        column.array = true;
    }
    field
}

fn column_of(name: &str, col: &str, ty: FlussoType) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: column(col),
            ty,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

fn relation(name: &str, relation: Relation) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(relation),
    }
}

/// Records each upserted document as the OpenSearch sink renders it, by id,
/// with `partIds` sorted (an `ids` aggregate has no order).
#[derive(Debug)]
struct DocumentSink(Arc<Mutex<Vec<(String, Value)>>>);

#[async_trait]
impl Sink for DocumentSink {
    async fn apply(&self, envelope: &Envelope) -> SinkResult<()> {
        if let Some(document) = &envelope.document {
            let mut document = sink::to_json(document);
            if let Some(Value::Array(ids)) = document.get_mut("partIds") {
                ids.sort_by_key(ToString::to_string);
            }
            self.0.lock().unwrap().push((envelope.id.clone(), document));
        }
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> SinkResult<sink::FlushReport> {
        Ok(sink::FlushReport::clean())
    }
}

/// The latest document recorded for `id`, once there is one.
async fn document(
    engine: &mut tokio::task::JoinHandle<engine::Result<()>>,
    recorded: &Arc<Mutex<Vec<(String, Value)>>>,
    id: &str,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if engine.is_finished() {
            panic!("engine stopped before document {id}: {:?}", engine.await);
        }
        let found = recorded
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(recorded_id, _)| recorded_id == id)
            .map(|(_, document)| document.clone());
        if let Some(document) = found {
            return document;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {id}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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

/// One deployment the way the daemon runs it: stage the sink engine, then run
/// it and the ingest engine together until either stops.
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
