//! Partitioned-table e2e (issue #143): a range-partitioned table as an index
//! root and as a `has_many` child, against a real Postgres 16.
//!
//! Starting from a publication that streams the partitions under their own
//! names, `prepare` reroutes it via the partition root, so every change on a leaf
//! arrives under the partitioned table's name and resolves to its document.
//! `prepare` sets `REPLICA IDENTITY FULL` across the child's partition tree, so
//! a deleted child row still reaches its old parent through the pre-image.
//! Backfill reads the partitions through the parent.
//! A partition whose identity index leaves out the primary key (issue #224) is
//! set to `FULL` alone, or, unmanaged, reported: its delete arrives under a
//! null key that addresses no document, while a child's old row still rebuilds
//! its parent when the partitioned table is `FULL`.
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

/// Issue #224: a partition whose replica identity is an index without the
/// primary key logs an old tuple without the key. Its changes arrive under the
/// parent's relation, which flags the parent's primary key, so the key reads
/// as null. `prepare` sets that partition (and only it) to `FULL`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_partition_whose_identity_leaves_out_the_primary_key_is_set_to_full() {
    let (_container, pool, url, port) = keyless_partition_database().await;
    let spec = Arc::new(logs_spec());
    let capture = keyless_capture(&spec, &url, port, true);

    let report = capture.inspect_pre_image(&spec.pre_images()).await.unwrap();
    assert!(report.partial.is_empty(), "{report:?}");
    let [gap] = report.gaps.as_slice() else {
        panic!("the partitioned table is a correctness gap: {report:?}");
    };
    assert_eq!(gap.table.to_string(), "public.logs");
    assert!(gap.missing.is_empty(), "no parent link is missing");
    assert_eq!(gap.missing_key, [column("id")]);
    assert_eq!(
        gap.remediation, "ALTER TABLE \"public\".\"logs_low\" REPLICA IDENTITY FULL;",
        "only the partition that leaves out the key is named",
    );
    assert!(gap.will_manage(true));

    capture.prepare().await.unwrap();
    assert_eq!(
        identities(&pool, "logs%").await,
        [
            ("logs".to_owned(), "d".to_owned()),
            ("logs_high".to_owned(), "d".to_owned()),
            ("logs_low".to_owned(), "f".to_owned()),
        ],
        "only the partition whose identity leaves out the key is set",
    );
    assert!(
        capture
            .inspect_pre_image(&spec.pre_images())
            .await
            .unwrap()
            .complete(),
        "a partitioned table logging its key everywhere is left alone",
    );

    let changes = keyless_partition_changes(&capture, &pool).await;
    assert_eq!(
        changes,
        [
            ("upsert", row_key(1)),
            ("delete", row_key(2)),
            ("upsert", row_key(1500)),
        ],
        "the update and the delete in the partition land on their own ids",
    );
}

/// The same partition with `manage_replica_identity = false`: the report names
/// it as a correctness gap. Its delete arrives under a null key, which
/// resolves to no document rather than to one rendered as `"null"`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn an_unmanaged_partition_without_the_key_is_reported_and_its_null_key_delete_addresses_nothing()
 {
    let (_container, pool, url, port) = keyless_partition_database().await;
    let spec = Arc::new(logs_spec());
    let capture = keyless_capture(&spec, &url, port, false);

    capture.prepare().await.unwrap();
    assert_eq!(
        identities(&pool, "logs_low").await,
        [("logs_low".to_owned(), "i".to_owned())],
        "nothing is altered when management is off",
    );
    let report = capture.inspect_pre_image(&spec.pre_images()).await.unwrap();
    let [gap] = report.gaps.as_slice() else {
        panic!("the partitioned table is a correctness gap: {report:?}");
    };
    assert_eq!(gap.missing_key, [column("id")]);
    assert!(!gap.will_manage(false));

    let changes = keyless_partition_changes(&capture, &pool).await;
    let null_key = RowKey(vec![(column("id"), GenericValue::Null)]);
    assert_eq!(
        changes,
        [
            ("upsert", row_key(1)),
            ("delete", null_key.clone()),
            ("upsert", row_key(1500)),
        ],
        "the update stays an upsert of its row; the delete can't name its row",
    );
    let builder = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    let resolved = builder
        .resolve(&table("logs"), &null_key, None, None)
        .await
        .unwrap();
    assert!(resolved.is_empty(), "{resolved:?}");
}

/// A partitioned `has_many` child whose partitioned table is `FULL` and one
/// partition on an identity index holding the link but not the primary key,
/// with management off. The partition's old row reaches the stream under the
/// parent's relation, every column flagged: the key reads null, but the link
/// is there, so a deleted child and a re-parented one still rebuild the parent
/// they left.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_keyless_child_partition_under_a_full_parent_still_rebuilds_the_parent_it_left() {
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
        "CREATE TABLE users (id bigint PRIMARY KEY, email text)",
        "CREATE TABLE orders (id bigint PRIMARY KEY, user_id bigint NOT NULL, \
         code text NOT NULL) PARTITION BY RANGE (id)",
        "ALTER TABLE orders REPLICA IDENTITY FULL",
        "CREATE TABLE orders_low PARTITION OF orders FOR VALUES FROM (0) TO (1000)",
        "CREATE TABLE orders_high PARTITION OF orders FOR VALUES FROM (1000) TO (2000)",
        "CREATE UNIQUE INDEX orders_low_link ON orders_low (user_id, code)",
        "ALTER TABLE orders_low REPLICA IDENTITY USING INDEX orders_low_link",
        "INSERT INTO users VALUES (77, 'ada@x.io'), (78, 'bob@x.io')",
        "INSERT INTO orders VALUES (1, 77, 'a'), (2, 77, 'b')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let spec = Arc::new(users_spec());
    let capture = keyless_capture(&spec, &url, port, false);
    capture.prepare().await.unwrap();
    assert_eq!(
        identities(&pool, "orders%").await,
        [
            ("orders".to_owned(), "f".to_owned()),
            ("orders_high".to_owned(), "d".to_owned()),
            ("orders_low".to_owned(), "i".to_owned()),
        ],
        "nothing is altered when management is off",
    );
    let report = capture.inspect_pre_image(&spec.pre_images()).await.unwrap();
    let [gap] = report.gaps.as_slice() else {
        panic!("the child is a correctness gap: {report:?}");
    };
    assert_eq!(gap.missing_key, [column("id")]);
    assert!(!gap.will_manage(false));

    let mut stream = capture.live().await.unwrap();
    for statement in [
        "DELETE FROM orders WHERE id = 2",
        "UPDATE orders SET user_id = 78 WHERE id = 1",
        "INSERT INTO orders VALUES (1500, 78, 'sentinel')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let mut changes = Vec::new();
    while changes.len() < 3 {
        let (_, change) = tokio::time::timeout(Duration::from_secs(30), stream.next())
            .await
            .expect("timed out waiting for a change")
            .unwrap()
            .unwrap();
        changes.push(change);
    }
    let [delete, reparent, _sentinel] = changes.as_slice() else {
        unreachable!()
    };
    assert!(
        matches!(delete, ChangeEvent::Delete { .. }),
        "the delete is emitted, not skipped: {delete:?}"
    );
    assert_eq!(
        delete.key(),
        &RowKey(vec![(column("id"), GenericValue::Null)])
    );
    assert!(
        matches!(reparent, ChangeEvent::Upsert { .. }),
        "{reparent:?}"
    );
    assert_eq!(reparent.key(), &row_key(1));

    let builder = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    let resolve = async |change: &ChangeEvent| {
        let mut ids = builder
            .resolve(
                change.table(),
                change.key(),
                change.before(),
                change.after(),
            )
            .await
            .unwrap();
        ids.sort_by_key(DocumentId::rendered);
        ids
    };
    assert_eq!(
        resolve(delete).await,
        [document_id("users", 77)],
        "the deleted child's old row names its parent",
    );
    assert_eq!(
        resolve(reparent).await,
        [document_id("users", 77), document_id("users", 78)],
        "the re-parented child rebuilds the parent it left and the one it joined",
    );

    let source::document::Document::Upsert { body, .. } =
        builder.build(&document_id("users", 77)).await.unwrap()
    else {
        panic!("user 77 is still there");
    };
    let GenericValue::Map(body) = body else {
        panic!("a document is an object: {body:?}");
    };
    assert!(
        matches!(body.get("orders"), Some(GenericValue::Array(orders)) if orders.is_empty())
            || matches!(body.get("orders"), None | Some(GenericValue::Null)),
        "user 77 is rebuilt without its deleted and re-parented orders: {body:?}",
    );
}

/// [`spec`]'s `users` index alone: `orders` as its partitioned `has_many`.
fn users_spec() -> SourceSpec {
    let users = spec().schema(&index_name("users")).unwrap().clone();
    SourceSpec::new(BTreeMap::from([(index_name("users"), users)]))
}

/// `logs`, range-partitioned on `id` (the primary key), with one partition on
/// `REPLICA IDENTITY USING INDEX (code)`: legal, since a partition is an
/// ordinary table and its own unique index need not hold the partition key.
async fn keyless_partition_database() -> (
    testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    PgPool,
    String,
    u16,
) {
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
        "CREATE TABLE logs (id bigint PRIMARY KEY, code text NOT NULL) PARTITION BY RANGE (id)",
        "CREATE TABLE logs_low PARTITION OF logs FOR VALUES FROM (0) TO (1000)",
        "CREATE TABLE logs_high PARTITION OF logs FOR VALUES FROM (1000) TO (2000)",
        "CREATE UNIQUE INDEX logs_low_code ON logs_low (code)",
        "ALTER TABLE logs_low REPLICA IDENTITY USING INDEX logs_low_code",
        "INSERT INTO logs VALUES (1, 'a'), (2, 'b')",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    (container, pool, url, port)
}

fn keyless_capture(spec: &SourceSpec, url: &str, port: u16, manage: bool) -> WalChangeCapture {
    WalChangeCapture::new(
        ReplicationConfig::new(
            "127.0.0.1",
            "postgres",
            "postgres",
            "postgres",
            "flusso",
            "flusso",
        )
        .with_port(port),
        url.to_owned(),
    )
    .with_publication_management(spec.all_tables(), true)
    .with_pre_image_management(spec.pre_images(), manage)
}

/// Update row 1's identity column and delete row 2, both in the keyless
/// partition, then insert a sentinel (1500) in the other one; every change
/// read up to the sentinel, as `(op, key)`.
async fn keyless_partition_changes(
    capture: &WalChangeCapture,
    pool: &PgPool,
) -> Vec<(&'static str, RowKey)> {
    let mut stream = capture.live().await.unwrap();
    for statement in [
        "UPDATE logs SET code = 'renamed' WHERE id = 1",
        "DELETE FROM logs WHERE id = 2",
        "INSERT INTO logs VALUES (1500, 'sentinel')",
    ] {
        sqlx::query(statement).execute(pool).await.unwrap();
    }
    let mut changes = Vec::new();
    loop {
        let (_, change) = tokio::time::timeout(Duration::from_secs(30), stream.next())
            .await
            .expect("timed out waiting for a change")
            .unwrap()
            .unwrap();
        let op = match change {
            ChangeEvent::Delete { .. } => "delete",
            _ => "upsert",
        };
        let key = change.key().clone();
        let sentinel = key == row_key(1500);
        changes.push((op, key));
        if sentinel {
            return changes;
        }
    }
}

fn logs_spec() -> SourceSpec {
    let logs = IndexSchema {
        version: 1,
        table: table("logs"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![column_field("id"), column_field("code")],
    };
    SourceSpec::new(BTreeMap::from([(index_name("logs"), logs)]))
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
        emit: true,
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
        emit: true,
        source: FieldSource::Column(Column {
            column: column(name),
            ty: FlussoType::Keyword,
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
