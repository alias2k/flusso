//! Backfill e2e (issue #385): a real Postgres → the ingest engine → a
//! recording sink, seeding several indexes that read each other's root
//! tables. Each key a backfill page reads is its own document of the index
//! walked: the backfill runs no reverse-resolution query, builds every
//! document once, and writes exactly what a `build` of each root row gives.
//!
//! The indexes: `users` (soft-deleted by a column, embeds its orders),
//! `accounts` (a second index on `users`, filtered to active rows, counts
//! orders), `orders` (embeds its user) and `categories` (a tree: each
//! category embeds its children, a self-referencing `has_many`).
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --test backfill --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::{BatchPolicy, IngestEngine, SinkControl, SinkEngine};
use kernel::{
    Aggregate, AggregateKey, AggregateOp, DatabaseSchema, Envelope, Field, FieldSource, Filter,
    FilterOp, FilterValue, IndexName, IndexSchema, Join, JoinKind, Relation, SinkName, SoftDelete,
    SoftDeleteColumn, ValueOpFilter,
};
use sink::{Result as SinkResult, Sink};
use source::cdc::ChangeCapture;
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceSpec};
use source_postgres::{PgDocumentBuilder, WalChangeCapture};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use stream::Stream;
use stream_channel::ChannelStream;

mod common;
use common::{column, column_field, field, index_name, replication, start_postgres_with, table};

const INDEXES: [&str; 4] = ["users", "accounts", "orders", "categories"];

/// Records `"<op> <index> <id> <document>"` per applied envelope, and the
/// indexes it marked seeded. Seeded up front for the indexes it is given.
#[derive(Debug, Default)]
struct RecordingSink {
    applied: Mutex<Vec<String>>,
    seeded: Mutex<BTreeSet<String>>,
}

impl RecordingSink {
    fn seeded(indexes: &[&str]) -> Self {
        Self {
            applied: Mutex::default(),
            seeded: Mutex::new(indexes.iter().map(|i| (*i).to_owned()).collect()),
        }
    }
}

#[async_trait]
impl Sink for RecordingSink {
    async fn write(
        &self,
        envelopes: &[Envelope],
        _caught_up: bool,
    ) -> SinkResult<sink::WriteReport> {
        for envelope in envelopes {
            let document = envelope
                .document
                .as_ref()
                .map(|document| sink::to_json(document).to_string())
                .unwrap_or_default();
            self.applied.lock().unwrap().push(format!(
                "{} {} {} {document}",
                envelope.op,
                envelope.index.as_ref(),
                envelope.id
            ));
        }
        Ok(sink::WriteReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> SinkResult<bool> {
        Ok(self.seeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn mark_seeded(&self, index: &IndexName) -> SinkResult<()> {
        self.seeded
            .lock()
            .unwrap()
            .insert(index.as_ref().to_owned());
        Ok(())
    }
}

fn has_many(name: &str, table_name: &str, foreign_key: &str, fields: Vec<Field>) -> Field {
    join(
        name,
        table_name,
        JoinKind::HasMany {
            foreign_key: column(foreign_key),
        },
        fields,
    )
}

fn join(name: &str, table_name: &str, kind: JoinKind, fields: Vec<Field>) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: table(table_name),
            kind,
            primary_key: column("id"),
            nullable: true,
            filters: None,
            order_by: None,
            limit: None,
            fields,
            indexed: true,
            on: vec![],
        })),
    }
}

fn root(table_name: &str, fields: Vec<Field>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table(table_name),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields,
    }
}

fn spec() -> SourceSpec {
    let users = IndexSchema {
        soft_delete: Some(SoftDelete::Column(SoftDeleteColumn {
            column: column("deleted"),
            when: None,
        })),
        ..root(
            "users",
            vec![
                column_field("email", "email"),
                has_many(
                    "orders",
                    "orders",
                    "user_id",
                    vec![column_field("total", "total")],
                ),
            ],
        )
    };
    let accounts = IndexSchema {
        filters: Some(vec![Filter::ValueOp(ValueOpFilter {
            column: column("active"),
            op: FilterOp::Eq,
            value: FilterValue::Single("true".to_owned()),
        })]),
        ..root(
            "users",
            vec![
                column_field("email", "email"),
                Field {
                    field: field("orderCount"),
                    options: Default::default(),
                    emit: true,
                    source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                        table: table("orders"),
                        op: AggregateOp::Count,
                        key: AggregateKey::Direct(column("user_id")),
                        value_type: None,
                        filters: None,
                        distinct: false,
                        via: Vec::new(),
                        on: vec![],
                    })),
                },
            ],
        )
    };
    let orders = root(
        "orders",
        vec![
            column_field("total", "total"),
            join(
                "user",
                "users",
                JoinKind::BelongsTo {
                    column: column("user_id"),
                },
                vec![column_field("email", "email")],
            ),
        ],
    );
    let categories = root(
        "categories",
        vec![
            column_field("name", "name"),
            has_many(
                "children",
                "categories",
                "parent_id",
                vec![column_field("name", "name")],
            ),
        ],
    );
    SourceSpec::new(BTreeMap::from([
        (index_name("users"), users),
        (index_name("accounts"), accounts),
        (index_name("orders"), orders),
        (index_name("categories"), categories),
    ]))
}

/// A Postgres with `pg_stat_statements`, the four tables filled, a
/// publication and a slot. Twenty users (3 soft-deleted, 4 inactive), three
/// orders each plus two orphans, and a three-level category tree.
async fn database() -> (test_postgres::Container, u16, String, PgPool) {
    let (container, port, url) =
        start_postgres_with(&["-c", "shared_preload_libraries=pg_stat_statements"]).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE EXTENSION pg_stat_statements",
        "CREATE TABLE users (id int PRIMARY KEY, email text, \
           deleted boolean NOT NULL DEFAULT false, active boolean NOT NULL DEFAULT true)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int, total int)",
        "CREATE TABLE categories (id int PRIMARY KEY, parent_id int, name text)",
        "INSERT INTO users (id, email) SELECT n, 'u' || n FROM generate_series(1, 20) n",
        "UPDATE users SET deleted = true WHERE id = 3",
        "UPDATE users SET active = false WHERE id = 4",
        "INSERT INTO orders SELECT 100 * u + k, u, u * k FROM generate_series(1, 20) u, \
           generate_series(1, 3) k",
        "INSERT INTO orders VALUES (1, NULL, 5), (2, NULL, 6)",
        "INSERT INTO categories VALUES (1, NULL, 'root'), (2, 1, 'a'), (3, 1, 'b'), \
           (4, 2, 'a1'), (5, 2, 'a2'), (6, 3, 'b1'), (7, NULL, 'lone')",
        "CREATE PUBLICATION flusso FOR TABLE users, orders, categories",
        "SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    (container, port, url, pool)
}

/// What a `build` of every root row gives, in the sink's recorded form, for
/// each of `indexes`.
async fn expected(url: &str, pool: &PgPool, indexes: &[&str]) -> Vec<String> {
    let documents = PgDocumentBuilder::connect(url, Arc::new(spec()))
        .await
        .unwrap();
    let mut out = Vec::new();
    for name in indexes {
        let spec = spec();
        let schema = spec.schema(&index_name(name)).unwrap();
        let ids: Vec<i32> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT id FROM {} ORDER BY id",
            schema.table
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        for id in ids {
            let document = documents
                .build(&DocumentId {
                    index: index_name(name),
                    key: RowKey(vec![(column("id"), kernel::GenericValue::Int(id))]),
                })
                .await
                .unwrap();
            out.push(match document {
                Document::Upsert { body, .. } => {
                    format!("upsert {name} {id} {}", sink::to_json(&body))
                }
                Document::Delete { .. } => format!("delete {name} {id} "),
                Document::Failed { failure, .. } => panic!("{failure:?}"),
            });
        }
    }
    out.sort();
    out
}

/// Reverse-resolution statements run since the last reset: a `SELECT` from a
/// watched table that builds no document. Every document query selects
/// `"document"`, and a backfill's key pages and its bound order `USING` the
/// key's own operator.
async fn reverse_lookups(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT coalesce(sum(calls), 0)::int8 FROM pg_stat_statements \
         WHERE query LIKE 'SELECT %' \
           AND query ~ 'FROM \"public\"\\.\"(users|orders|categories)\"' \
           AND query NOT LIKE '%\"document\"%' \
           AND query NOT LIKE '%USING%'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Stage `sink`, run the engines until it has marked every index seeded, and
/// stop. Returns what it recorded, sorted.
async fn backfill(port: u16, url: &str, sink: Arc<RecordingSink>) -> Vec<String> {
    let documents: Arc<dyn DocumentBuilder> = Arc::new(
        PgDocumentBuilder::connect(url, Arc::new(spec()))
            .await
            .unwrap(),
    );
    let capture: Arc<dyn ChangeCapture> =
        Arc::new(WalChangeCapture::new(replication(port), url.to_owned()));
    let name = SinkName::try_new("recording").unwrap();
    let stream: Arc<dyn Stream> = Arc::new(ChannelStream::new(1024, [name.clone()]));
    let continuity = capture.continuity().await.unwrap();
    let mappings = documents.index_mappings().await.unwrap();
    let progress = Arc::new(engine::Progress::new([name.clone()]));
    let sink_engine = SinkEngine::new(
        name.clone(),
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream),
        mappings,
        Arc::clone(&progress),
    );
    sink_engine.stage(continuity).await.unwrap();
    // Small batches: a document built from a related row in one batch and
    // from its own root row in another is built twice.
    let ingest = IngestEngine::new(capture, documents, stream, vec![name], progress).with_batch(
        BatchPolicy {
            max_changes: 4,
            max_delay: Duration::from_millis(10),
        },
    );
    let (_control, mut control_rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    let mut engine = tokio::spawn(async move {
        tokio::select! {
            result = ingest.run() => result,
            result = sink_engine.run(&mut control_rx) => result,
        }
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    while sink.seeded.lock().unwrap().len() < INDEXES.len() {
        assert!(Instant::now() < deadline, "backfill did not finish");
        assert!(
            !engine.is_finished(),
            "engine stopped: {:?}",
            (&mut engine).await
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    engine.abort();
    let mut applied = sink.applied.lock().unwrap().clone();
    applied.sort();
    applied
}

fn assert_once(applied: &[String]) {
    let mut seen = BTreeSet::new();
    for line in applied {
        let id: Vec<&str> = line.splitn(4, ' ').skip(1).take(2).collect();
        assert!(
            seen.insert(id.clone()),
            "{id:?} written twice: {applied:#?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_backfill_of_related_indexes_reads_each_root_row_once_and_resolves_nothing() {
    let (_container, port, url, pool) = database().await;
    let want = expected(&url, &pool, &INDEXES).await;
    sqlx::query("SELECT pg_stat_statements_reset()")
        .execute(&pool)
        .await
        .unwrap();

    let applied = backfill(port, &url, Arc::new(RecordingSink::default())).await;

    assert_eq!(reverse_lookups(&pool).await, 0, "no reverse resolution");
    assert_once(&applied);
    assert_eq!(applied, want);
    assert!(
        applied.contains(&"delete users 3 ".to_owned()),
        "the soft-deleted user is a tombstone, as a build gives it"
    );
    assert!(
        applied.contains(&"delete accounts 4 ".to_owned()),
        "the filtered-out account is a tombstone, as a build gives it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_backfill_beside_seeded_related_indexes_writes_only_its_own() {
    let (_container, port, url, pool) = database().await;
    let want = expected(&url, &pool, &["orders"]).await;
    sqlx::query("SELECT pg_stat_statements_reset()")
        .execute(&pool)
        .await
        .unwrap();

    let sink = Arc::new(RecordingSink::seeded(&["users", "accounts", "categories"]));
    let applied = backfill(port, &url, sink).await;

    assert_eq!(reverse_lookups(&pool).await, 0, "no reverse resolution");
    assert_once(&applied);
    assert_eq!(applied, want);
}
