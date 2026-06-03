//! Full-pipeline benchmark: real Postgres → resolve + build → real OpenSearch.
//!
//! This is the most representative "whole system" number. It stands up both a
//! Postgres 17 and an OpenSearch 2 container and drives the exact path the
//! engine runs: a changed row is reverse-resolved to the documents it affects,
//! each document is assembled server-side from Postgres, and the result is
//! upserted into OpenSearch via the bulk API. Nothing is mocked or stubbed.
//!
//! Measured:
//!
//! - `change`: one related-table change → resolve → build → upsert → flush, the
//!   steady-state cost of propagating a single source change all the way to the
//!   search index.
//! - `backfill`: assembling N root documents from Postgres and bulk-indexing
//!   them into OpenSearch in one flush — the seeding path, reported as
//!   throughput per document.
//!
//! Requires Docker. Run with:
//!
//! ```text
//! cargo bench -p sources-postgres --bench pipeline
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

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use schema_core::{
    Column, ColumnName, Config, ConnectionUrl, DatabaseSchema, Field, FieldName, FieldSource,
    GenericValue, Index, IndexName, IndexSchema, Join, JoinKey, JoinType, Relation, SoftDelete,
    SoftDeleteColumn, Source, SourceType, TableName,
};
use sinks_core::Sink;
use sinks_opensearch::OpensearchSink;
use sources_core::RowKey;
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::runtime::Runtime;

/// Users seeded for the backfill, each with a handful of orders.
const USER_COUNT: usize = 200;
/// Orders per user.
const ORDERS_PER_USER: usize = 5;

/// Containers held alive for the duration of the benchmark.
struct Services {
    _postgres: ContainerAsync<Postgres>,
    _opensearch: ContainerAsync<GenericImage>,
    builder: PgDocumentBuilder,
    sink: OpensearchSink,
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

/// Bring up both services, seed Postgres, and create the OpenSearch index from
/// the builder's own resolved mapping (the real `ensure_index` path).
async fn setup() -> Services {
    let postgres = Postgres::default().start().await.unwrap();
    let pg_port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");

    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text, name text, deleted boolean NOT NULL DEFAULT false)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int NOT NULL, total numeric(10,2) NOT NULL, status text NOT NULL DEFAULT 'pending')",
        "CREATE INDEX orders_user_id_idx ON orders (user_id)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    for u in 1..=USER_COUNT {
        sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $2, $3)")
            .bind(u as i32)
            .bind(format!("user{u}@example.com"))
            .bind(format!("Customer {u}"))
            .execute(&pool)
            .await
            .unwrap();
        for n in 0..ORDERS_PER_USER {
            let order_id = (u as i32) * 1000 + n as i32;
            sqlx::query("INSERT INTO orders (id, user_id, total, status) VALUES ($1, $2, $3, $4)")
                .bind(order_id)
                .bind(u as i32)
                .bind(rust_decimal::Decimal::new(1999, 2))
                .bind("paid")
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    let builder = PgDocumentBuilder::connect(&url, Arc::new(config(&url)))
        .await
        .unwrap();

    let (opensearch, os_url) = start_opensearch().await;
    let config = schema_core::OpensearchSink {
        url: schema_core::HttpUrl::try_new(&os_url).unwrap(),
        username: None,
        password: None,
        tls_verify: false,
        batch_size: 1000,
        timeout_secs: 30,
        max_retries: 3,
        pipeline: None,
    };
    let sink = OpensearchSink::from_config(&config).unwrap();
    // Create the index from the builder's resolved, fully-typed mapping.
    for mapping in builder.index_mappings().await.unwrap() {
        sink.ensure_index(&mapping).await.unwrap();
    }

    Services { _postgres: postgres, _opensearch: opensearch, builder, sink }
}

/// Resolve a changed row to its documents, assemble each, and write it to the
/// sink — the engine's per-change inner loop. Returns without flushing.
async fn propagate(services: &Services, table: &TableName, key: &RowKey) {
    let ids = services.builder.resolve(table, key).await.unwrap();
    for id in &ids {
        match services.builder.build(id).await.unwrap() {
            Document::Upsert { id, body } => {
                services.sink.upsert(&id.index, &doc_id_string(&id), &body).await.unwrap();
            }
            Document::Delete { id } => {
                services.sink.delete(&id.index, &doc_id_string(&id)).await.unwrap();
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

    // Steady state: a single order change propagated end to end.
    c.bench_function("change", |b| {
        let key = row_key(1000); // order 1000 belongs to user 1
        b.to_async(&rt).iter(|| async {
            propagate(&services, &table("orders"), &key).await;
            services.sink.flush().await.unwrap();
        });
    });

    // Backfill: assemble every user document and bulk-index them in one flush.
    let mut group = c.benchmark_group("backfill");
    group.sample_size(10);
    group.throughput(Throughput::Elements(USER_COUNT as u64));
    group.bench_function("all_users", |b| {
        b.to_async(&rt).iter(|| async {
            for u in 1..=USER_COUNT {
                let id = document_id(u as i64);
                if let Document::Upsert { id, body } =
                    services.builder.build(&id).await.unwrap()
                {
                    services.sink.upsert(&id.index, &doc_id_string(&id), &body).await.unwrap();
                }
            }
            services.sink.flush().await.unwrap();
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

fn config(connection_url: &str) -> Config {
    let orders = Field {
        field: field("orders"),
        mapping: None,
        source: FieldSource::Relation(Relation::Join {
            join: Join {
                table: table("orders"),
                join_type: JoinType::OneToMany,
                key: JoinKey::Direct(column("user_id")),
                filters: None,
                order_by: None,
                limit: None,
            },
            fields: vec![
                column_field("id", "id"),
                column_field("total", "total"),
                column_field("status", "status"),
            ],
        }),
    };
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: Some(SoftDelete::Column(SoftDeleteColumn {
            column: column("deleted"),
            when: None,
        })),
        fields: vec![
            column_field("id", "id"),
            column_field("email", "email"),
            column_field("name", "name"),
            orders,
        ],
    };
    Config {
        source: Source {
            source_type: SourceType::Postgres,
            connection_url: ConnectionUrl::try_new(connection_url).unwrap(),
        },
        sinks: BTreeMap::new(),
        indexes: BTreeMap::from([(index_name("users"), Index { enabled: true, schema })]),
    }
}

fn column_field(name: &str, col: &str) -> Field {
    Field {
        field: field(name),
        mapping: None,
        source: FieldSource::Column(Column {
            column: column(col),
            transforms: Vec::new(),
            default: None,
        }),
    }
}

fn document_id(id: i64) -> DocumentId {
    DocumentId { index: index_name("users"), key: row_key(id) }
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
