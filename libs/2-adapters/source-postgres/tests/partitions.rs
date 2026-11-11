//! Partitioned-table e2e (issue #143): a range-partitioned table as an index
//! root and as a `has_many` child, against a real Postgres 16.
//!
//! Starting from a publication that streams the partitions under their own
//! names, `prepare` reroutes it via the partition root, so every change on a leaf
//! arrives under the partitioned table's name and resolves to its document.
//! `prepare` sets `REPLICA IDENTITY FULL` across the child's partition tree, so
//! a deleted child row still reaches its old parent through the pre-image.
//! Backfill reads the partitions through the parent.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-source-postgres --test partitions --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, GenericValue,
    IndexName, IndexSchema, Join, JoinKind, Relation, TableName,
};
use source::cdc::{ChangeCapture, ChangeEvent};
use source::document::{DocumentBuilder, DocumentId};
use source::{CaptureProvisioning, RowKey, SnapshotTable, SourceSpec};
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn changes_on_partitions_resolve_to_the_partitioned_tables_documents() {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .with_cmd(["postgres", "-c", "wal_level=logical"])
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE events (id bigint PRIMARY KEY, name text) PARTITION BY RANGE (id)",
        "CREATE TABLE events_low PARTITION OF events FOR VALUES FROM (0) TO (1000)",
        "CREATE TABLE events_high PARTITION OF events FOR VALUES FROM (1000) TO (2000)",
        "CREATE TABLE users (id bigint PRIMARY KEY, email text)",
        "CREATE TABLE orders (id bigint PRIMARY KEY, user_id bigint NOT NULL) \
         PARTITION BY RANGE (id)",
        "CREATE TABLE orders_low PARTITION OF orders FOR VALUES FROM (0) TO (1000)",
        "CREATE TABLE orders_high PARTITION OF orders FOR VALUES FROM (1000) TO (2000)",
        "INSERT INTO events VALUES (1, 'low'), (1500, 'high')",
        "INSERT INTO users VALUES (1, 'ada@x.io')",
        "INSERT INTO orders VALUES (10, 1)",
        // A publication someone else made: it streams the partitions under
        // their own names.
        "CREATE PUBLICATION flusso FOR TABLE events, users, orders",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let spec = Arc::new(spec());
    let capture = WalChangeCapture::new(
        ReplicationConfig::new(
            "127.0.0.1",
            "postgres",
            "postgres",
            "postgres",
            "flusso",
            "flusso",
        )
        .with_port(port),
        url.clone(),
    )
    .with_publication_management(spec.all_tables(), true)
    .with_pre_image_management(spec.pre_image_columns(), true);

    let report = capture.inspect_coverage(&spec.all_tables()).await.unwrap();
    assert!(!report.satisfied);
    assert!(report.missing.is_empty(), "the partitions are published");
    let misrouted: Vec<String> = report.misrouted.iter().map(ToString::to_string).collect();
    assert_eq!(misrouted, ["public.events", "public.orders"]);
    assert_eq!(
        report.remediation,
        ["ALTER PUBLICATION \"flusso\" SET (publish_via_partition_root = true);"]
    );

    capture.prepare().await.unwrap();
    assert!(
        capture
            .inspect_coverage(&spec.all_tables())
            .await
            .unwrap()
            .satisfied,
        "prepare reroutes the publication via the partition root, before the slot",
    );
    assert_eq!(
        identities(&pool, "orders%").await,
        [
            ("orders".to_owned(), "f".to_owned()),
            ("orders_high".to_owned(), "f".to_owned()),
            ("orders_low".to_owned(), "f".to_owned()),
        ],
        "the child's whole partition tree logs its parent link",
    );

    // Backfill reads every partition through the parent.
    let snapshot: Vec<ChangeEvent> = capture
        .snapshot(&[SnapshotTable {
            db_schema: DatabaseSchema::try_new("public").unwrap(),
            table: table("events"),
        }])
        .await
        .unwrap()
        .map(Result::unwrap)
        .collect()
        .await;
    let mut seeded: Vec<(String, RowKey)> = snapshot
        .iter()
        .map(|change| (change.table().to_string(), change.key().clone()))
        .collect();
    seeded.sort_by_key(|(_, key)| format!("{key:?}"));
    assert_eq!(
        seeded,
        [
            ("events".to_owned(), row_key(1)),
            ("events".to_owned(), row_key(1500)),
        ]
    );

    let mut stream = capture.live().await.unwrap();

    for statement in [
        "INSERT INTO events VALUES (2, 'new')",
        "UPDATE events SET name = 'renamed' WHERE id = 1500",
        "DELETE FROM events WHERE id = 1",
        "INSERT INTO orders VALUES (1001, 1)",
        "DELETE FROM orders WHERE id = 10",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let mut changes = Vec::new();
    while changes.len() < 5 {
        let (_, change) = tokio::time::timeout(Duration::from_secs(30), stream.next())
            .await
            .expect("timed out waiting for a change")
            .unwrap()
            .unwrap();
        changes.push(change);
    }
    let named: Vec<(String, RowKey, bool)> = changes
        .iter()
        .map(|change| {
            (
                change.table().to_string(),
                change.key().clone(),
                matches!(change, ChangeEvent::Delete { .. }),
            )
        })
        .collect();
    assert_eq!(
        named,
        [
            ("events".to_owned(), row_key(2), false),
            ("events".to_owned(), row_key(1500), false),
            ("events".to_owned(), row_key(1), true),
            ("orders".to_owned(), row_key(1001), false),
            ("orders".to_owned(), row_key(10), true),
        ],
        "every change arrives under the partitioned table's name",
    );

    let builder = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    for (change, expected) in changes.iter().zip([
        document_id("events", 2),
        document_id("events", 1500),
        document_id("events", 1),
        document_id("users", 1),
        // Order 10 is gone: only its pre-image names user 1.
        document_id("users", 1),
    ]) {
        let resolved = builder
            .resolve(
                change.table(),
                change.key(),
                change.before(),
                change.after(),
            )
            .await
            .unwrap();
        assert_eq!(resolved, [expected], "{change:?}");
    }

    // A change under a partition's own name is what no index reads.
    let dropped = builder
        .resolve(&table("events_low"), &row_key(2), None, None)
        .await
        .unwrap();
    assert!(dropped.is_empty());
}

/// Each relation matching `pattern` with its `relreplident`, sorted by name.
async fn identities(pool: &PgPool, pattern: &str) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT relname::text, relreplident::text FROM pg_class \
         WHERE relname LIKE $1 AND relkind IN ('r', 'p') ORDER BY relname",
    )
    .bind(pattern)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn spec() -> SourceSpec {
    let events = IndexSchema {
        version: 1,
        table: table("events"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![column_field("id"), column_field("name")],
    };
    let orders = Field {
        field: FieldName::try_new("orders").unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: table("orders"),
            primary_key: column("id"),
            nullable: false,
            kind: JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("id")],
            indexed: true,
            on: vec![],
        })),
    };
    let users = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![column_field("id"), column_field("email"), orders],
    };
    SourceSpec::new(BTreeMap::from([
        (index_name("events"), events),
        (index_name("users"), users),
    ]))
}

fn column_field(name: &str) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: column(name),
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

fn document_id(index: &str, id: i64) -> DocumentId {
    DocumentId {
        index: index_name(index),
        key: row_key(id),
    }
}

fn row_key(id: i64) -> RowKey {
    RowKey(vec![(column("id"), GenericValue::BigInt(id))])
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
