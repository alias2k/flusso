//! End-to-end tests for the Postgres document builder against a real database
//! in a container. These exercise the server-side document SQL and reverse
//! resolution that unit tests can only check by generated-string assertion.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test integration -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::Arc;

use schema_core::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, GenericValue,
    IndexName, IndexSchema, Join, JoinKind, Relation, SoftDelete, SoftDeleteColumn, TableName,
};
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_core::{RowKey, SourceSpec};
use sources_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn assembles_documents_resolves_and_tombstones() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    // Seed schema + data.
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text, deleted boolean NOT NULL DEFAULT false)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int NOT NULL, total numeric NOT NULL)",
        "INSERT INTO users (id, email) VALUES (1, 'ada@x.io')",
        "INSERT INTO orders (id, user_id, total) VALUES (10, 1, 19.99), (11, 1, 5.00)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let builder = PgDocumentBuilder::connect(&url, Arc::new(users_spec()))
        .await
        .unwrap();

    // build: the root row plus its nested one-to-many orders.
    let document = builder.build(&document_id(1)).await.unwrap();
    let Document::Upsert { body, .. } = document else {
        panic!("expected an upsert");
    };
    let GenericValue::Map(map) = body else {
        panic!("expected a document object");
    };
    assert_eq!(
        map.get("email"),
        Some(&GenericValue::String("ada@x.io".into()))
    );
    let Some(GenericValue::Array(orders)) = map.get("orders") else {
        panic!("expected an orders array");
    };
    assert_eq!(orders.len(), 2, "both orders should be nested in");

    // resolve: a change to an order reverse-resolves to its user document.
    let affected = builder
        .resolve(&table("orders"), &row_key(10))
        .await
        .unwrap();
    assert_eq!(affected, vec![document_id(1)]);

    // soft-delete: the document becomes a tombstone.
    sqlx::query("UPDATE users SET deleted = true WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    let document = builder.build(&document_id(1)).await.unwrap();
    assert!(
        matches!(document, Document::Delete { .. }),
        "a soft-deleted root yields a tombstone",
    );

    // a missing root row is also a tombstone.
    let missing = builder.build(&document_id(999)).await.unwrap();
    assert!(matches!(missing, Document::Delete { .. }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn build_many_assembles_a_set_and_tombstones_absent_keys() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, email text, deleted boolean NOT NULL DEFAULT false)",
        "CREATE TABLE orders (id int PRIMARY KEY, user_id int NOT NULL, total numeric NOT NULL)",
        "INSERT INTO users (id, email) VALUES (1, 'ada@x.io'), (2, 'bob@x.io'), (3, 'cy@x.io')",
        "INSERT INTO orders (id, user_id, total) VALUES (10, 1, 19.99), (11, 1, 5.00), (20, 2, 7.50)",
        // User 3 is soft-deleted, so it must come back as a tombstone.
        "UPDATE users SET deleted = true WHERE id = 3",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let builder = PgDocumentBuilder::connect(&url, Arc::new(users_spec()))
        .await
        .unwrap();

    // A mix: two live rows, one soft-deleted (3), one absent (999).
    let ids = vec![
        document_id(1),
        document_id(2),
        document_id(3),
        document_id(999),
    ];
    let documents = builder.build_many(&ids).await.unwrap();

    // One outcome per requested id; index by the document's key value to assert
    // regardless of the order rows came back in.
    assert_eq!(documents.len(), 4);
    let by_key = |target: i64| {
        documents
            .iter()
            .find(|d| d.id().key == row_key(target))
            .unwrap_or_else(|| panic!("no outcome for id {target}"))
    };

    // User 1: upsert with both orders nested in.
    let Document::Upsert { body, .. } = by_key(1) else {
        panic!("expected user 1 to upsert");
    };
    let GenericValue::Map(map) = body else {
        panic!("expected an object");
    };
    assert_eq!(
        map.get("email"),
        Some(&GenericValue::String("ada@x.io".into()))
    );
    let Some(GenericValue::Array(orders)) = map.get("orders") else {
        panic!("expected an orders array");
    };
    assert_eq!(orders.len(), 2, "user 1's two orders nest in");

    // User 2: upsert with its single order.
    assert!(matches!(by_key(2), Document::Upsert { .. }));

    // Soft-deleted and absent rows are tombstones.
    assert!(
        matches!(by_key(3), Document::Delete { .. }),
        "a soft-deleted root yields a tombstone",
    );
    assert!(
        matches!(by_key(999), Document::Delete { .. }),
        "an absent root yields a tombstone",
    );
}

fn users_spec() -> SourceSpec {
    let orders = Field {
        field: field("orders"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: table("orders"),
            primary_key: column("id"),
            kind: JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("id", "id"), column_field("total", "total")],
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
        filters: None,
        fields: vec![
            column_field("id", "id"),
            column_field("email", "email"),
            orders,
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
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
