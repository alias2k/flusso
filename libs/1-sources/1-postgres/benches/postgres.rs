//! Real-service benchmarks for the Postgres document builder.
//!
//! These run against a genuine Postgres 17 in a container — the same
//! server-side assembly SQL (`json_build_object` + `json_agg` over a join) and
//! reverse-resolution queries the pipeline issues in production. Nothing is
//! mocked.
//!
//! Measured:
//!
//! - `build`: assembling one document (root row + nested one-to-many orders)
//!   across nesting depth — 0, 1, 10, and 100 child rows folded in server-side.
//! - `resolve`: mapping a changed row back to the documents it affects — both
//!   the trivial root-table case and the reverse lookup from a related table.
//!
//! Requires Docker. Run with:
//!
//! ```text
//! cargo bench -p sources-postgres --bench postgres
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

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use schema_core::{
    Column, ColumnName, Config, ConnectionSpec, DatabaseSchema, Field, FieldName, FieldSource,
    FlussoType, GenericValue, Index, IndexName, IndexSchema, Join, JoinKey, JoinType, Relation,
    Secret, SoftDelete, SoftDeleteColumn, Source, SourceType, TableName,
};
use sources_core::RowKey;
use sources_core::document::{DocumentBuilder, DocumentId};
use sources_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::runtime::Runtime;

/// One seeded user per nesting depth: `(user_id, order_count)`. The user ids
/// are spread out so order-id ranges never collide.
const USERS: &[(i64, usize)] = &[(1, 0), (2, 1), (3, 10), (4, 100)];

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

/// Start Postgres, create the `users`/`orders` schema, and seed one user per
/// entry in [`USERS`] with that many orders. Returns the running container, a
/// connected, ready-to-query [`PgDocumentBuilder`], and a raw pool (used by the
/// round-trip baseline).
async fn setup() -> (
    testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    PgDocumentBuilder,
    sqlx::PgPool,
) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text, name text, deleted boolean NOT NULL DEFAULT false)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int NOT NULL, total numeric(10,2) NOT NULL, status text NOT NULL DEFAULT 'pending')",
        "CREATE INDEX orders_user_id_idx ON orders (user_id)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    for &(user_id, order_count) in USERS {
        sqlx::query("INSERT INTO users (id, email, name) VALUES ($1, $2, $3)")
            .bind(user_id as i32)
            .bind(format!("user{user_id}@example.com"))
            .bind(format!("Customer {user_id}"))
            .execute(&pool)
            .await
            .unwrap();
        // Order ids are partitioned by user (user_id * 1000 + n) so ranges
        // never overlap across the seeded users.
        for n in 0..order_count {
            let order_id = (user_id as i32) * 1000 + n as i32;
            sqlx::query("INSERT INTO orders (id, user_id, total, status) VALUES ($1, $2, $3, $4)")
                .bind(order_id)
                .bind(user_id as i32)
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
    (container, builder, pool)
}

fn bench(c: &mut Criterion) {
    let rt = runtime();
    // Keep the runtime entered for the whole scope so the container's async
    // teardown (testcontainers drops it via the reactor) has one to run on.
    let _guard = rt.enter();
    let (_container, builder, pool) = rt.block_on(setup());

    // Warm the catalog caches (primary keys, column types) so the first sample
    // doesn't pay a one-off cost the rest don't — we measure steady-state SQL.
    rt.block_on(async {
        builder.build(&document_id(4)).await.unwrap();
        builder
            .resolve(&table("orders"), &row_key(4000))
            .await
            .unwrap();
    });

    // baseline: the fixed costs to subtract from the figures below, so the
    // marginal cost of flusso's work is readable rather than buried.
    let mut group = c.benchmark_group("baseline");
    group.measurement_time(Duration::from_secs(10));
    // The Postgres round-trip floor: parse/plan/execute of the cheapest possible
    // query. `build`/`resolve` cannot beat this — subtract it to see real work.
    group.bench_function("select_1", |b| {
        b.to_async(&rt).iter(|| async {
            sqlx::query("SELECT 1").fetch_one(&pool).await.unwrap();
        });
    });
    // A change on a table no index references: pure in-memory dispatch, zero
    // queries — the framework overhead floor for `resolve`.
    group.bench_function("resolve_unrelated", |b| {
        let key = row_key(1);
        b.to_async(&rt).iter(|| async {
            builder.resolve(&table("products"), &key).await.unwrap();
        });
    });
    group.finish();

    // build: server-side document assembly across nesting depth.
    let mut group = c.benchmark_group("build");
    group.warm_up_time(Duration::from_secs(5));
    group.measurement_time(Duration::from_secs(15));
    for &(user_id, order_count) in USERS {
        let id = document_id(user_id);
        group.bench_with_input(BenchmarkId::from_parameter(order_count), &id, |b, id| {
            b.to_async(&rt).iter(|| async {
                builder.build(id).await.unwrap();
            });
        });
    }
    group.finish();

    // resolve: changed row -> affected document ids.
    let mut group = c.benchmark_group("resolve");
    group.warm_up_time(Duration::from_secs(5));
    group.measurement_time(Duration::from_secs(15));
    // A change on the document's own root table — the one-id fast path (no I/O).
    group.bench_function("root_table", |b| {
        let key = row_key(4);
        b.to_async(&rt).iter(|| async {
            builder.resolve(&table("users"), &key).await.unwrap();
        });
    });
    // A change on a related table — the reverse lookup back to the root.
    group.bench_function("related_table", |b| {
        let key = row_key(4000);
        b.to_async(&rt).iter(|| async {
            builder.resolve(&table("orders"), &key).await.unwrap();
        });
    });
    group.finish();
}

/// A `users` index whose documents fold in their `orders` as a nested
/// one-to-many array, with a soft-delete column — the shape the dev dataset and
/// integration tests use.
fn config(connection_url: &str) -> Config {
    let orders = Field {
        field: field("orders"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: table("orders"),
            join_type: JoinType::OneToMany,
            primary_key: column("id"),
            key: JoinKey::Direct(column("user_id")),
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![
                column_field("id", "id"),
                column_field("total", "total"),
                column_field("status", "status"),
            ],
        })),
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

fn document_id(id: i64) -> DocumentId {
    DocumentId {
        index: index_name("users"),
        key: row_key(id),
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
