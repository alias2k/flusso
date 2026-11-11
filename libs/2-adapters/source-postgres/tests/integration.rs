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

use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnName, DatabaseSchema, Field, FieldName,
    FieldSource, FlussoType, GenericValue, IndexName, IndexSchema, Join, JoinKind, Relation,
    SoftDelete, SoftDeleteColumn, TableName, Through,
};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey, SourceSpec};
use source_postgres::PgDocumentBuilder;
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
        "CREATE TABLE users (id bigint PRIMARY KEY, email text, deleted boolean NOT NULL DEFAULT false)",
        "CREATE TABLE orders (id bigint PRIMARY KEY, user_id bigint NOT NULL, total numeric NOT NULL)",
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
        .resolve(&table("orders"), &row_key(10), None, None)
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
        "CREATE TABLE users (id bigint PRIMARY KEY, email text, deleted boolean NOT NULL DEFAULT false)",
        "CREATE TABLE orders (id bigint PRIMARY KEY, user_id bigint NOT NULL, total numeric NOT NULL)",
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

/// Regression for `operator does not exist: uuid = text`: a `uuid` primary key
/// (and a `uuid` foreign key) must round-trip through every keyed path —
/// `build` (`id = $1::uuid`), `build_many` (`IN ($1::uuid, …)`), and `resolve`
/// (reverse `user_id`/`id = $1::uuid`) — not just integer keys.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn uuid_keys_round_trip_through_build_build_many_and_resolve() {
    const U1: &str = "11111111-1111-1111-1111-111111111111";
    const U2: &str = "22222222-2222-2222-2222-222222222222";
    const O1: &str = "aaaaaaaa-0000-0000-0000-000000000001";

    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id uuid PRIMARY KEY, email text, deleted boolean NOT NULL DEFAULT false)",
        "CREATE TABLE orders (id uuid PRIMARY KEY, user_id uuid NOT NULL, total numeric NOT NULL)",
        "INSERT INTO users (id, email) VALUES \
         ('11111111-1111-1111-1111-111111111111', 'ada@x.io'), \
         ('22222222-2222-2222-2222-222222222222', 'bob@x.io')",
        "INSERT INTO orders (id, user_id, total) VALUES \
         ('aaaaaaaa-0000-0000-0000-000000000001', '11111111-1111-1111-1111-111111111111', 19.99), \
         ('aaaaaaaa-0000-0000-0000-000000000002', '11111111-1111-1111-1111-111111111111', 5.00)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let builder = PgDocumentBuilder::connect(&url, Arc::new(users_spec()))
        .await
        .unwrap();

    // build: single uuid key (`WHERE id = $1::uuid`).
    let Document::Upsert { body, .. } = builder.build(&uuid_document_id(U1)).await.unwrap() else {
        panic!("expected an upsert for the uuid-keyed root");
    };
    let GenericValue::Map(map) = body else {
        panic!("expected a document object");
    };
    let Some(GenericValue::Array(orders)) = map.get("orders") else {
        panic!("expected an orders array");
    };
    assert_eq!(orders.len(), 2, "both uuid-keyed orders nest in");

    // build_many: batched `IN ($1::uuid, $2::uuid)` plus an absent key.
    let documents = builder
        .build_many(&[
            uuid_document_id(U1),
            uuid_document_id(U2),
            uuid_document_id("33333333-3333-3333-3333-333333333333"),
        ])
        .await
        .unwrap();
    assert_eq!(documents.len(), 3);
    let upserts = documents
        .iter()
        .filter(|d| matches!(d, Document::Upsert { .. }))
        .count();
    assert_eq!(
        upserts, 2,
        "both present uuid roots upsert; the absent one tombstones"
    );

    // resolve: a change to an order reverse-resolves to its user via the uuid FK
    // (`SELECT user_id FROM orders WHERE id = $1::uuid`).
    let affected = builder
        .resolve(&table("orders"), &uuid_row_key(O1), None, None)
        .await
        .unwrap();
    assert_eq!(affected, vec![uuid_document_id(U1)]);
}

/// The document body — assembled server-side as JSON, then coerced by each
/// field's declared type — reaches the engine as **typed** canonical values, not
/// strings: a `uuid` is a `Uuid`, a `date` a `Date`, a `timestamptz` a
/// `TimestampTz`, a `bigint` a `BigInt`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn typed_columns_coerce_to_canonical_variants_in_the_body() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE events (id uuid PRIMARY KEY, name text, born date, seen timestamptz, n bigint)",
        "INSERT INTO events (id, name, born, seen, n) VALUES \
         ('11111111-1111-1111-1111-111111111111', 'launch', '2024-01-02', \
          '2024-01-02T03:04:05Z', 9000000000)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let typed_field = |name: &str, ty: FlussoType| Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: column(name),
            ty,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
        }),
    };
    let schema = IndexSchema {
        version: 1,
        table: table("events"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            typed_field("id", FlussoType::Uuid),
            typed_field("name", FlussoType::Keyword),
            typed_field("born", FlussoType::Date),
            typed_field("seen", FlussoType::Timestamp),
            typed_field("n", FlussoType::Long),
        ],
    };
    let spec = SourceSpec::new(BTreeMap::from([(index_name("events"), schema)]));
    let builder = PgDocumentBuilder::connect(&url, Arc::new(spec))
        .await
        .unwrap();

    let id = DocumentId {
        index: index_name("events"),
        key: uuid_row_key("11111111-1111-1111-1111-111111111111"),
    };
    let Document::Upsert { body, .. } = builder.build(&id).await.unwrap() else {
        panic!("expected an upsert");
    };
    let GenericValue::Map(map) = body else {
        panic!("expected a document object");
    };
    assert!(
        matches!(map.get("id"), Some(GenericValue::Uuid(_))),
        "{map:?}"
    );
    assert!(
        matches!(map.get("born"), Some(GenericValue::Date(_))),
        "{map:?}"
    );
    assert!(
        matches!(map.get("seen"), Some(GenericValue::TimestampTz(_))),
        "{map:?}"
    );
    assert_eq!(map.get("n"), Some(&GenericValue::BigInt(9_000_000_000)));
    assert_eq!(
        map.get("name"),
        Some(&GenericValue::String("launch".to_owned()))
    );
}

/// `items` is a tree: 1 is the root, 2 its child, 3 its grandchild, 4 a second
/// root with no children.
async fn items_tree(statements: &[&'static str]) -> (impl Drop, String) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE items (id bigint PRIMARY KEY, parent_id bigint REFERENCES items (id), name text)",
        "INSERT INTO items (id, parent_id, name) VALUES (1, NULL, 'root'), (2, 1, 'child'), (3, 2, 'grandchild'), (4, NULL, 'other')",
    ]
    .iter()
    .chain(statements)
    .copied()
    {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    (container, url)
}

async fn items_builder(url: &str, relation: Field) -> PgDocumentBuilder {
    PgDocumentBuilder::connect(url, Arc::new(items_spec(vec![relation])))
        .await
        .unwrap()
}

/// The `items` keys a change on `items.<id>` resolves to, sorted; fails on a
/// duplicate.
async fn resolved_keys(
    builder: &PgDocumentBuilder,
    id: i64,
    parent_before: Option<i64>,
) -> Vec<i64> {
    let before = parent_before.map(|parent| {
        RowImage(vec![
            (column("id"), GenericValue::BigInt(id)),
            (column("parent_id"), GenericValue::BigInt(parent)),
        ])
    });
    let ids = builder
        .resolve(&table("items"), &row_key(id), before.as_ref(), None)
        .await
        .unwrap();
    let mut keys: Vec<i64> = ids
        .iter()
        .map(|document| {
            assert_eq!(document.index, index_name("items"));
            match document.key.0.as_slice() {
                [(_, GenericValue::BigInt(key))] => *key,
                other => panic!("unexpected key {other:?}"),
            }
        })
        .collect();
    keys.sort_unstable();
    let distinct = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), distinct, "duplicate document ids: {ids:?}");
    keys
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_self_referencing_has_many_resolves_the_parent() {
    let (_container, url) = items_tree(&[]).await;
    let builder = items_builder(&url, children(vec![])).await;

    assert_eq!(
        resolved_keys(&builder, 2, None).await,
        [1, 2],
        "a child rebuilds its parent"
    );
    assert_eq!(
        resolved_keys(&builder, 3, None).await,
        [2, 3],
        "one declared level, one parent"
    );
    assert_eq!(
        resolved_keys(&builder, 4, None).await,
        [4],
        "a root rebuilds only itself"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_deleted_or_reparented_self_referencing_child_rebuilds_its_old_parent() {
    let (_container, url) = items_tree(&[
        "DELETE FROM items WHERE id = 3",
        "UPDATE items SET parent_id = 4 WHERE id = 2",
    ])
    .await;
    let builder = items_builder(&url, children(vec![])).await;

    assert_eq!(resolved_keys(&builder, 3, Some(2)).await, [2, 3], "deleted");
    assert_eq!(
        resolved_keys(&builder, 2, Some(1)).await,
        [1, 2, 4],
        "re-parented"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_two_level_self_join_rebuilds_the_grandparent() {
    let (_container, url) = items_tree(&["DELETE FROM items WHERE id = 3"]).await;
    let builder = items_builder(&url, children(vec![children(vec![])])).await;

    assert_eq!(resolved_keys(&builder, 2, None).await, [1, 2]);
    assert_eq!(
        resolved_keys(&builder, 3, Some(2)).await,
        [1, 2, 3],
        "a deleted grandchild rebuilds its parent and grandparent",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_row_that_is_its_own_parent_resolves_once() {
    let (_container, url) = items_tree(&[
        "INSERT INTO items (id, parent_id, name) VALUES (7, NULL, 'self')",
        "UPDATE items SET parent_id = 7 WHERE id = 7",
    ])
    .await;
    let builder = items_builder(&url, children(vec![children(vec![])])).await;

    assert_eq!(resolved_keys(&builder, 7, None).await, [7]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_parent_cycle_in_the_data_terminates() {
    let (_container, url) = items_tree(&[
        "INSERT INTO items (id, parent_id, name) VALUES (5, NULL, 'a'), (6, 5, 'b')",
        "UPDATE items SET parent_id = 6 WHERE id = 5",
    ])
    .await;
    let builder = items_builder(&url, children(vec![children(vec![])])).await;

    assert_eq!(resolved_keys(&builder, 5, None).await, [5, 6]);
    assert_eq!(resolved_keys(&builder, 6, None).await, [5, 6]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_belongs_to_back_at_the_root_rebuilds_its_referrers() {
    let (_container, url) = items_tree(&[]).await;
    let parent = join_field(
        "parent",
        "items",
        JoinKind::BelongsTo {
            column: column("parent_id"),
        },
    );
    let builder = items_builder(&url, parent).await;

    assert_eq!(resolved_keys(&builder, 1, None).await, [1, 2]);
    assert_eq!(resolved_keys(&builder, 3, None).await, [3]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_has_one_back_at_the_root_rebuilds_the_parent() {
    let (_container, url) = items_tree(&[]).await;
    let first_child = join_field(
        "firstChild",
        "items",
        JoinKind::HasOne {
            foreign_key: column("parent_id"),
        },
    );
    let builder = items_builder(&url, first_child).await;

    assert_eq!(resolved_keys(&builder, 2, None).await, [1, 2]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_many_to_many_back_at_the_root_rebuilds_the_linking_side() {
    let (_container, url) = items_tree(&[
        "CREATE TABLE item_links (from_id bigint NOT NULL, to_id bigint NOT NULL, PRIMARY KEY (from_id, to_id))",
        "INSERT INTO item_links (from_id, to_id) VALUES (4, 3)",
    ])
    .await;
    let related = join_field(
        "related",
        "items",
        JoinKind::ManyToMany {
            through: Through {
                table: table("item_links"),
                left_key: column("from_id"),
                right_key: column("to_id"),
                filters: None,
            },
        },
    );
    let builder = items_builder(&url, related).await;

    assert_eq!(resolved_keys(&builder, 3, None).await, [3, 4]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn an_aggregate_over_the_root_table_rebuilds_the_parent() {
    let (_container, url) = items_tree(&["DELETE FROM items WHERE id = 3"]).await;
    let child_count = Field {
        field: field("childCount"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: table("items"),
            op: AggregateOp::Count,
            key: AggregateKey::Direct(column("parent_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: Vec::new(),
        })),
    };
    let builder = items_builder(&url, child_count).await;

    assert_eq!(resolved_keys(&builder, 2, None).await, [1, 2]);
    assert_eq!(resolved_keys(&builder, 3, Some(2)).await, [2, 3], "deleted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_root_change_without_a_self_join_resolves_only_its_own_document() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let builder = PgDocumentBuilder::connect(&url, Arc::new(users_spec()))
        .await
        .unwrap();

    let affected = builder
        .resolve(&table("users"), &row_key(1), None, None)
        .await
        .unwrap();

    assert_eq!(affected, vec![document_id(1)]);
}

/// `items` (`id`, `name`) plus one extra field.
fn items_spec(extra: Vec<Field>) -> SourceSpec {
    let mut fields = vec![column_field("id", "id"), column_field("name", "name")];
    fields.extend(extra);
    let schema = IndexSchema {
        version: 1,
        table: table("items"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields,
    };
    SourceSpec::new(BTreeMap::from([(index_name("items"), schema)]))
}

/// `children`: a `has_many` from `items` back at `items` over `parent_id`,
/// projecting `id`, `name`, and `nested`.
fn children(nested: Vec<Field>) -> Field {
    let mut field = join_field(
        "children",
        "items",
        JoinKind::HasMany {
            foreign_key: column("parent_id"),
        },
    );
    if let FieldSource::Relation(Relation::Join(join)) = &mut field.source {
        join.fields.extend(nested);
    }
    field
}

fn join_field(name: &str, on: &str, kind: JoinKind) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: table(on),
            primary_key: column("id"),
            nullable: true,
            kind,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("id", "id"), column_field("name", "name")],
            indexed: true,
        })),
    }
}

fn users_spec() -> SourceSpec {
    let orders = Field {
        field: field("orders"),
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
            fields: vec![column_field("id", "id"), column_field("total", "total")],
            indexed: true,
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
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
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
    RowKey(vec![(column("id"), GenericValue::BigInt(id))])
}

fn uuid_document_id(id: &str) -> DocumentId {
    DocumentId {
        index: index_name("users"),
        key: uuid_row_key(id),
    }
}

fn uuid_row_key(id: &str) -> RowKey {
    // A uuid column decodes to a typed `Uuid` value (not a string), so both the
    // change key and the reverse-resolved root key carry that variant.
    let parsed = uuid::Uuid::parse_str(id).expect("valid uuid");
    RowKey(vec![(column("id"), GenericValue::Uuid(parsed))])
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
