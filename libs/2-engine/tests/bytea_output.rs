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
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --test bytea_output --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
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
    every_bytea_value_reads_as_hex("hex").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_bytea_value_reads_as_hex_under_bytea_output_escape() {
    every_bytea_value_reads_as_hex("escape").await;
}

/// The documents of `docs`, backfilled then changed live, on a database whose
/// `bytea_output` is `setting`, read through a pooler that resets every
/// server connection after each transaction.
async fn every_bytea_value_reads_as_hex(setting: &str) {
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
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        format!("ALTER DATABASE postgres SET bytea_output = '{setting}'"),
        "CREATE DOMAIN blob AS bytea".to_owned(),
        "CREATE TABLE owners (id int PRIMARY KEY, avatar bytea)".to_owned(),
        "CREATE TABLE docs (id int PRIMARY KEY, owner_id int, blob bytea, blobs bytea[], \
         tag bytea, wrapped blob, spare bytea)"
            .to_owned(),
        "CREATE TABLE parts (id bytea PRIMARY KEY, doc_id int, blob bytea)".to_owned(),
        "ALTER TABLE parts REPLICA IDENTITY FULL".to_owned(),
        "CREATE PUBLICATION flusso FOR TABLE owners, docs, parts".to_owned(),
        r"INSERT INTO owners VALUES (1, '\x0a0b')".to_owned(),
        r"INSERT INTO docs VALUES (1, 1, '\x0102ff', ARRAY['\x01'::bytea, NULL, '\xff00'], '\x6869', '\x7f', NULL)"
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
    assert_eq!(backfilled, expected_doc_1(json!("")), "the backfill");

    for statement in [
        r"UPDATE parts SET blob = '\xabcd' WHERE id = '\x0002'",
        r"INSERT INTO docs VALUES (2, NULL, '\x', '{}', NULL, NULL, '\x00')",
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
        expected_doc_1(json!("q80=")),
        "a live change on a child"
    );
    engine.abort();
}

/// Document 1, with `second_blob` as its second part's `blob`.
fn expected_doc_1(second_blob: Value) -> Value {
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
    let mut blobs = column_of("blobs", "blobs", FlussoType::Binary);
    if let FieldSource::Column(column) = &mut blobs.source {
        column.array = true;
    }
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
        ],
    }
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
