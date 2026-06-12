//! Full-pipeline benchmark: real Postgres → the engine → real OpenSearch, on
//! the *most complex document the builder supports*.
//!
//! This lives in the `engine` crate because the engine is the one component
//! that legitimately spans both halves of the pipeline (a source crate has no
//! business depending on a specific sink). It stands up a Postgres 17 and an
//! OpenSearch 2 container and exercises the real path. Nothing is mocked.
//!
//! ## The document
//!
//! Each `users` document folds in essentially every assembly feature, so the
//! server-side SQL is a worst-case-shaped query rather than a single join:
//!
//! - a **1:1 join** to `profiles`,
//! - a **1:N join** to `orders` (ordered, newest-first) with a **nested 1:N
//!   join** to `order_items` inside each order — three relation levels deep,
//! - a **M:N join** to `tags` **through** the `user_tags` junction,
//! - **seven aggregates** over the related rows: count, sum, avg, min, max, a
//!   **filtered** count (`status = 'fulfilled'`), and a count **through** the
//!   junction,
//! - a **group** sub-object, a column with **transforms** (`trim` + `lowercase`)
//!   and a **default**, and a **constant** field,
//! - a **soft-delete** column, so the root filter runs too.
//!
//! ## What is measured
//!
//! - `baseline`: the two fixed I/O floors — a Postgres `SELECT 1` round-trip and
//!   a single-document OpenSearch bulk flush — so the figures below can be
//!   attributed rather than taken on faith.
//! - `change`: one `order_items` change propagated end to end. Resolving it is a
//!   **multi-hop reverse lookup** (item → order → user) before the complex
//!   document is reassembled and flushed. Composed by hand because
//!   [`Engine::run`] is a run-once daemon (backfill, then follow `live` forever)
//!   and does not fit a per-iteration request/response benchmark.
//! - `backfill`: the real [`Engine::run`] driving its backfill — `ensure_index`,
//!   then the source snapshot streamed through the engine's queue → worker →
//!   resolve → build → sink → flush path, assembling every complex document.
//!   `live` is stubbed to an empty stream so `run` terminates after seeding;
//!   that tail is the only thing not exercised, and not what we measure.
//! - `change_burst`: the steady-state path under volume. A burst of changes is
//!   drained through the real [`Engine::run`] live loop at different
//!   [`BatchPolicy::max_changes`] values, so the curve shows what the engine's
//!   flush-batching buys: `max_changes = 1` is the old flush-per-change cost,
//!   larger values collapse the burst into ⌈N / max_changes⌉ bulk flushes. This
//!   is where the batching win actually shows up (a single change cannot
//!   benefit from batching — only volume can).
//!
//! Requires Docker. Run with:
//!
//! ```text
//! cargo bench -p engine --bench pipeline
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use engine::{BatchPolicy, Engine};
use futures::stream::{self, BoxStream};
use schema_core::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnName, Config, ConnectionSpec,
    DatabaseSchema, Direction, Field, FieldName, FieldSource, Filter, FilterOp, FilterValue,
    FlussoType, GenericValue, Index, IndexMapping, IndexName, IndexSchema, Join, JoinKind, OrderBy,
    Relation, Secret, SinkName, SoftDelete, SoftDeleteColumn, Source, SourceType, TableName,
    Through, Transform, ValueOpFilter,
};
use sinks_core::{Result as SinkResult, Sink};
use sinks_opensearch::OpensearchSink;
use sources_core::cdc::{Ack, AckSink, Change, ChangeCapture, ChangeEvent};
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_core::{Result as SourceResult, RowKey, SnapshotTable};
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::runtime::Runtime;

/// Root documents seeded (one per user).
const USER_COUNT: usize = 200;
/// Orders nested under each user.
const ORDERS_PER_USER: usize = 5;
/// Items nested under each order — the third relation level.
const ITEMS_PER_ORDER: usize = 4;
/// Distinct tags, linked many-to-many through the junction.
const TAG_COUNT: usize = 8;
/// Tags linked to each user.
const TAGS_PER_USER: usize = 4;

/// Changes drained per `change_burst` iteration — one per seeded user, so each
/// resolves to a distinct document (no buffer-collapsing on a shared id).
const BURST: usize = USER_COUNT;

/// An `order_items` row that exists (user 1, first order, first item), used by
/// the `change` bench to drive a multi-hop reverse resolution. Its id follows
/// the seeding scheme below: `order_id * 100 + item_index`, where the first
/// order of user 1 is `1 * 1000 + 0`.
const CHANGED_ITEM_ID: i64 = 1000 * 100;

/// Wraps the real [`WalChangeCapture`] but returns an empty `live` stream, so
/// [`Engine::run`] does `ensure_index` + backfill (both real) and then returns
/// immediately instead of following replication forever. The `snapshot` — the
/// part the backfill actually measures — is the genuine one.
#[derive(Debug)]
struct BackfillOnly {
    inner: WalChangeCapture,
}

#[async_trait]
impl ChangeCapture for BackfillOnly {
    async fn live(&self) -> SourceResult<BoxStream<'static, SourceResult<Change>>> {
        Ok(Box::pin(stream::empty()))
    }

    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> SourceResult<BoxStream<'static, SourceResult<Change>>> {
        self.inner.snapshot(tables).await
    }
}

/// Wraps the real [`OpensearchSink`] but always reports "not seeded" and never
/// records seeding — so every `Engine::run` re-runs the backfill rather than
/// the engine's `mark_seeded` short-circuiting every iteration after the first.
/// Writes are forwarded unchanged, so the indexing path is the real one.
#[derive(Debug)]
struct AlwaysUnseeded {
    inner: OpensearchSink,
}

#[async_trait]
impl Sink for AlwaysUnseeded {
    async fn ensure_index(&self, mapping: &IndexMapping) -> SinkResult<()> {
        self.inner.ensure_index(mapping).await
    }
    async fn upsert(&self, index: &IndexName, id: &str, document: &GenericValue) -> SinkResult<()> {
        self.inner.upsert(index, id, document).await
    }
    async fn delete(&self, index: &IndexName, id: &str) -> SinkResult<()> {
        self.inner.delete(index, id).await
    }
    async fn flush(&self, caught_up: bool) -> SinkResult<()> {
        self.inner.flush(caught_up).await
    }
    // `is_seeded` / `mark_seeded` deliberately keep the trait defaults
    // (`false` / no-op) so the backfill runs on every iteration.
}

/// A capture whose `live` stream yields a fixed burst of `count` upsert changes
/// (one per user, ids `1..=count`) and then ends, so `Engine::run` drains the
/// burst and returns. This is the steady-state path under volume — exactly where
/// the engine's flush-batching pays off. `snapshot` keeps the default empty
/// stream; the burst bench skips backfill anyway.
#[derive(Debug)]
struct BurstCapture {
    count: usize,
}

#[async_trait]
impl ChangeCapture for BurstCapture {
    async fn live(&self) -> SourceResult<BoxStream<'static, SourceResult<Change>>> {
        let ack_sink: Arc<dyn AckSink> = Arc::new(NoopAck);
        let changes: Vec<SourceResult<Change>> = (1..=self.count as i64)
            .map(|id| {
                Ok(Change {
                    event: ChangeEvent::Upsert {
                        table: table("users"),
                        key: row_key(id),
                    },
                    ack: Ack::new(id as u64, Arc::clone(&ack_sink)),
                })
            })
            .collect();
        Ok(Box::pin(stream::iter(changes)))
    }
}

/// A no-op ack endpoint — the burst bench measures throughput, not durability.
#[derive(Debug)]
struct NoopAck;

impl AckSink for NoopAck {
    fn confirm(&self, _seq: u64) {}
}

/// Everything held alive for the benchmark's duration.
struct Services {
    _postgres: ContainerAsync<Postgres>,
    _opensearch: ContainerAsync<GenericImage>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    /// Raw pool to the same Postgres, for the round-trip baseline.
    pool: sqlx::PgPool,
    /// Replication config + URL to build a fresh capture each backfill iteration
    /// (`Engine::run` consumes the engine, and with it the capture).
    replication: ReplicationConfig,
    url: String,
}

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

async fn start_opensearch() -> (ContainerAsync<GenericImage>, String) {
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
    (container, format!("http://127.0.0.1:{port}"))
}

/// The e-commerce schema + a sizeable dataset, seeded server-side with
/// `generate_series` so setup stays fast even at thousands of rows.
async fn seed(pool: &sqlx::PgPool) {
    for ddl in [
        "CREATE TABLE users (
            id int PRIMARY KEY, name text, email text, status text NOT NULL,
            bio text, archived boolean NOT NULL DEFAULT false)",
        "CREATE TABLE profiles (id int PRIMARY KEY, user_id int NOT NULL, headline text)",
        "CREATE TABLE orders (
            id int PRIMARY KEY, user_id int NOT NULL, total numeric(10,2) NOT NULL,
            status text NOT NULL, placed_at timestamptz NOT NULL)",
        "CREATE TABLE order_items (
            id int PRIMARY KEY, order_id int NOT NULL, sku text NOT NULL,
            qty int NOT NULL, price numeric(10,2) NOT NULL)",
        "CREATE TABLE tags (id int PRIMARY KEY, label text NOT NULL)",
        "CREATE TABLE user_tags (user_id int NOT NULL, tag_id int NOT NULL,
            PRIMARY KEY (user_id, tag_id))",
        "CREATE INDEX profiles_user_id_idx ON profiles (user_id)",
        "CREATE INDEX orders_user_id_idx ON orders (user_id)",
        "CREATE INDEX order_items_order_id_idx ON order_items (order_id)",
        "CREATE INDEX user_tags_tag_id_idx ON user_tags (tag_id)",
    ] {
        sqlx::query(ddl).execute(pool).await.unwrap();
    }

    let (users, orders, items, tags, tags_per_user) = (
        USER_COUNT as i32,
        ORDERS_PER_USER as i32,
        ITEMS_PER_ORDER as i32,
        TAG_COUNT as i32,
        TAGS_PER_USER as i32,
    );

    // Static SQL with bound counts (sqlx 0.9 rejects dynamic strings).
    // Emails carry surrounding whitespace + uppercase to exercise trim+lowercase;
    // bio is null on even ids to exercise the column default.
    sqlx::query(
        "INSERT INTO users (id, name, email, status, bio, archived)
         SELECT g, 'Customer ' || g, '  USER' || g || '@EXAMPLE.IO  ',
                CASE WHEN g % 3 = 0 THEN 'banned' ELSE 'active' END,
                CASE WHEN g % 2 = 0 THEN NULL ELSE 'bio ' || g END,
                false
         FROM generate_series(1, $1) g",
    )
    .bind(users)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO profiles (id, user_id, headline)
         SELECT g, g, 'Headline for ' || g FROM generate_series(1, $1) g",
    )
    .bind(users)
    .execute(pool)
    .await
    .unwrap();

    // order id = user*1000 + n; half fulfilled, half pending.
    sqlx::query(
        "INSERT INTO orders (id, user_id, total, status, placed_at)
         SELECT u * 1000 + n, u, (n + 1) * 10.50,
                CASE WHEN n % 2 = 0 THEN 'fulfilled' ELSE 'pending' END,
                TIMESTAMPTZ '2021-01-01' + (u * $2 + n) * INTERVAL '1 hour'
         FROM generate_series(1, $1) u, generate_series(0, $2 - 1) n",
    )
    .bind(users)
    .bind(orders)
    .execute(pool)
    .await
    .unwrap();

    // item id = order_id*100 + k.
    sqlx::query(
        "INSERT INTO order_items (id, order_id, sku, qty, price)
         SELECT (u * 1000 + n) * 100 + k, u * 1000 + n, 'sku-' || k, k + 1, (k + 1) * 2.25
         FROM generate_series(1, $1) u,
              generate_series(0, $2 - 1) n,
              generate_series(0, $3 - 1) k",
    )
    .bind(users)
    .bind(orders)
    .bind(items)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO tags (id, label) SELECT g, 'tag-' || g FROM generate_series(1, $1) g")
        .bind(tags)
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO user_tags (user_id, tag_id)
         SELECT u, ((u + k) % $1) + 1
         FROM generate_series(1, $2) u, generate_series(0, $3 - 1) k
         ON CONFLICT DO NOTHING",
    )
    .bind(tags)
    .bind(users)
    .bind(tags_per_user)
    .execute(pool)
    .await
    .unwrap();
}

/// Bring up both services, seed Postgres, and create the OpenSearch index from
/// the builder's own resolved mapping (the real `ensure_index` path).
async fn setup() -> Services {
    let postgres = Postgres::default().start().await.unwrap();
    let pg_port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");

    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    seed(&pool).await;

    let builder = PgDocumentBuilder::connect(&url, Arc::new(config(&url)))
        .await
        .unwrap();
    let documents: Arc<dyn DocumentBuilder> = Arc::new(builder);

    let (opensearch, os_url) = start_opensearch().await;
    let os_config = schema_core::OpensearchSink {
        url: Secret::Value(os_url.clone()),
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
        refresh_interval: "10s".to_owned(),
        text_analysis: schema_core::TextAnalysis::Builtin,
        auto_subfields: true,
    };
    let os_name = SinkName::try_new("bench").unwrap();
    let inner = OpensearchSink::from_config(&os_name, &os_config).unwrap();
    // Create the index from the builder's resolved, fully-typed mapping so the
    // `change` and `baseline` benches (which write before any backfill) land in
    // a real index. `Engine::run` also calls this — it is idempotent.
    for mapping in documents.index_mappings().await.unwrap() {
        inner.ensure_index(&mapping).await.unwrap();
    }
    let sink: Arc<dyn Sink> = Arc::new(AlwaysUnseeded { inner });

    // Used only to build a fresh `BackfillOnly` capture per backfill iteration;
    // `live` is stubbed, so these replication values are never connected with.
    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(pg_port);

    Services {
        _postgres: postgres,
        _opensearch: opensearch,
        documents,
        sink,
        pool,
        replication,
        url,
    }
}

/// Resolve a changed row to its documents, assemble each, and write it to the
/// sink — the engine's per-change inner loop, composed by hand. Returns without
/// flushing.
async fn propagate(services: &Services, table: &TableName, key: &RowKey) {
    let ids = services.documents.resolve(table, key).await.unwrap();
    for id in &ids {
        match services.documents.build(id).await.unwrap() {
            Document::Upsert { id, body } => {
                services
                    .sink
                    .upsert(&id.index, &doc_id_string(&id), &body)
                    .await
                    .unwrap();
            }
            Document::Delete { id } => {
                services
                    .sink
                    .delete(&id.index, &doc_id_string(&id))
                    .await
                    .unwrap();
            }
        }
    }
}

fn bench(c: &mut Criterion) {
    let rt = runtime();
    // Keep the runtime entered for the whole scope so the containers' async
    // teardown (testcontainers drops them via the reactor) has one to run on.
    let _guard = rt.enter();
    let services = rt.block_on(setup());

    // baseline: the two fixed I/O floors the figures below are built on.
    let mut group = c.benchmark_group("baseline");
    group.measurement_time(Duration::from_secs(10));
    // Postgres round-trip floor (resolve + build each pay at least this).
    group.bench_function("pg_select_1", |b| {
        b.to_async(&rt).iter(|| async {
            sqlx::query("SELECT 1")
                .fetch_one(&services.pool)
                .await
                .unwrap();
        });
    });
    // OpenSearch bulk round-trip floor: a single-document upsert + flush.
    group.bench_function("os_flush_1", |b| {
        let index = index_name("users");
        let mut body = BTreeMap::new();
        body.insert("id".to_owned(), GenericValue::Int(1));
        let body = GenericValue::Map(body);
        b.to_async(&rt).iter(|| async {
            services
                .sink
                .upsert(&index, "baseline", &body)
                .await
                .unwrap();
            services.sink.flush(true).await.unwrap();
        });
    });
    group.finish();

    // Steady state: an order_item change propagated end to end (hand-composed;
    // see the module docs). Resolving it walks item → order → user (multi-hop)
    // before the complex document is reassembled.
    let mut group = c.benchmark_group("change");
    group.warm_up_time(Duration::from_secs(5));
    group.measurement_time(Duration::from_secs(15));
    group.bench_function("item_update", |b| {
        let key = row_key(CHANGED_ITEM_ID);
        b.to_async(&rt).iter(|| async {
            propagate(&services, &table("order_items"), &key).await;
            services.sink.flush(true).await.unwrap();
        });
    });
    group.finish();

    // Live-path throughput under volume: a burst of `BURST` changes drained
    // through the real `Engine::run` live loop, at several batch sizes.
    // `max_changes = 1` is flush-per-change (the pre-batching cost); larger
    // values collapse the burst into ⌈BURST / max_changes⌉ bulk flushes. The
    // wide `max_delay` never fires — the burst is already queued, so batches
    // fill on count, not time.
    let mut group = c.benchmark_group("change_burst");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(25));
    group.throughput(Throughput::Elements(BURST as u64));
    for &max_changes in &[1usize, 16, 256] {
        group.bench_with_input(
            BenchmarkId::from_parameter(max_changes),
            &max_changes,
            |b, &max_changes| {
                let documents = Arc::clone(&services.documents);
                let sink = Arc::clone(&services.sink);
                b.to_async(&rt).iter(|| {
                    let documents = Arc::clone(&documents);
                    let sink = Arc::clone(&sink);
                    async move {
                        Engine::new(Arc::new(BurstCapture { count: BURST }), documents, sink)
                            .with_batch(BatchPolicy {
                                max_changes,
                                max_delay: Duration::from_secs(10),
                            })
                            .skip_backfill(true)
                            .run()
                            .await
                            .unwrap();
                    }
                });
            },
        );
    }
    group.finish();

    // Backfill: the real `Engine::run` seeding every complex document through
    // the engine's queue → worker → resolve → build → sink → flush path.
    let mut group = c.benchmark_group("backfill");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.throughput(Throughput::Elements(USER_COUNT as u64));
    group.bench_function("engine_run", |b| {
        b.to_async(&rt).iter(|| async {
            let capture = BackfillOnly {
                inner: WalChangeCapture::new(services.replication.clone(), services.url.clone()),
            };
            let engine = Engine::new(
                Arc::new(capture),
                Arc::clone(&services.documents),
                Arc::clone(&services.sink),
            );
            engine.run().await.unwrap();
        });
    });
    group.finish();
}

/// The document's `_id` for OpenSearch: the string form of its root key value.
fn doc_id_string(id: &DocumentId) -> String {
    match id.key.0.first().map(|(_, v)| v) {
        Some(GenericValue::Int(n)) => n.to_string(),
        Some(GenericValue::String(s)) => s.clone(),
        other => format!("{other:?}"),
    }
}

/// The most complex `users` document the builder supports — see the module docs.
fn config(connection_url: &str) -> Config {
    // 1:1 join to the user's profile.
    let profile = join_field(
        "profile",
        Join {
            table: table("profiles"),
            primary_key: column("id"),
            kind: JoinKind::HasOne {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![col("headline", "headline")],
        },
    );

    // 1:N join to orders (newest first), each folding in its items (nested 1:N).
    let orders = join_field(
        "orders",
        Join {
            table: table("orders"),
            primary_key: column("id"),
            kind: JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: Some(vec![order_by("placed_at", Direction::Desc)]),
            limit: None,
            fields: vec![
                col("id", "id"),
                col("status", "status"),
                col("total", "total"),
                join_field(
                    "items",
                    Join {
                        table: table("order_items"),
                        primary_key: column("id"),
                        kind: JoinKind::HasMany {
                            foreign_key: column("order_id"),
                        },
                        filters: None,
                        order_by: Some(vec![order_by("sku", Direction::Asc)]),
                        limit: None,
                        fields: vec![col("sku", "sku"), col("qty", "qty"), col("price", "price")],
                    },
                ),
            ],
        },
    );

    // M:N join to tags through the user_tags junction, labels A→Z.
    let tags = join_field(
        "tags",
        Join {
            table: table("tags"),
            primary_key: column("id"),
            kind: JoinKind::ManyToMany {
                through: Through {
                    table: table("user_tags"),
                    left_key: column("user_id"),
                    right_key: column("tag_id"),
                },
            },
            filters: None,
            order_by: Some(vec![order_by("label", Direction::Asc)]),
            limit: None,
            fields: vec![col("label", "label")],
        },
    );

    // A group sub-object: a transformed column plus a constant, nested without
    // reading another table.
    let contact = group_field(
        "contact",
        vec![
            col_full(
                "email",
                "email",
                vec![Transform::Trim, Transform::Lowercase],
                None,
            ),
            constant_field("source", GenericValue::String("seed".into())),
        ],
    );

    let fields = vec![
        col("id", "id"),
        col("name", "name"),
        col_full(
            "bio",
            "bio",
            Vec::new(),
            Some(GenericValue::String("(no bio)".into())),
        ),
        contact,
        profile,
        orders,
        tags,
        agg_field("order_count", orders_agg(AggregateOp::Count, None)),
        agg_field(
            "total_spent",
            orders_agg(AggregateOp::Sum(column("total")), None),
        ),
        agg_field(
            "avg_order",
            orders_agg(AggregateOp::Avg(column("total")), None),
        ),
        agg_field(
            "min_order",
            orders_agg(AggregateOp::Min(column("total")), None),
        ),
        agg_field(
            "max_order",
            orders_agg(AggregateOp::Max(column("total")), None),
        ),
        agg_field(
            "fulfilled_orders",
            orders_agg(
                AggregateOp::Count,
                Some(vec![Filter::ValueOp(ValueOpFilter {
                    column: column("status"),
                    op: FilterOp::Eq,
                    value: FilterValue::Single("fulfilled".into()),
                })]),
            ),
        ),
        agg_field(
            "tag_count",
            Aggregate {
                table: table("tags"),
                op: AggregateOp::Count,
                key: AggregateKey::Through(Through {
                    table: table("user_tags"),
                    left_key: column("user_id"),
                    right_key: column("tag_id"),
                }),
                value_type: None,
                filters: None,
            },
        ),
    ];

    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: Some(SoftDelete::Column(SoftDeleteColumn {
            column: column("archived"),
            when: None,
        })),
        fields,
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

// --- schema construction helpers (mirroring the config_coverage test) --------

fn col(name: &str, source_column: &str) -> Field {
    col_full(name, source_column, Vec::new(), None)
}

fn col_full(
    name: &str,
    source_column: &str,
    transforms: Vec<Transform>,
    default: Option<GenericValue>,
) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: column(source_column),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms,
            default,
        }),
    }
}

fn group_field(name: &str, fields: Vec<Field>) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Group(fields),
    }
}

fn constant_field(name: &str, value: GenericValue) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Constant(value),
    }
}

fn join_field(name: &str, join: Join) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(join)),
    }
}

fn agg_field(name: &str, aggregate: Aggregate) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
    }
}

fn orders_agg(op: AggregateOp, filters: Option<Vec<Filter>>) -> Aggregate {
    Aggregate {
        table: table("orders"),
        op,
        key: AggregateKey::Direct(column("user_id")),
        value_type: None,
        filters,
    }
}

fn order_by(col: &str, direction: Direction) -> OrderBy {
    OrderBy {
        column: column(col),
        direction: Some(direction),
    }
}

fn row_key(id: i64) -> RowKey {
    RowKey(vec![(column("id"), GenericValue::Int(id))])
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

criterion_group!(benches, bench);
criterion_main!(benches);
