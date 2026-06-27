//! End-to-end test for [`SchemaIntrospection`] against a real Postgres in a
//! container — that the catalog queries return the expected tables, columns,
//! suggested types, primary keys, and foreign keys.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test introspection -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use schema_core::FlussoType;
use sources_core::{SchemaIntrospection, junction_candidates};
use sources_postgres::{ReplicationConfig, WalChangeCapture};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn introspects_tables_columns_keys_and_junctions() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id bigint PRIMARY KEY, email varchar(255) NOT NULL, bio text)",
        "CREATE TABLE products (id integer PRIMARY KEY, price numeric(10,2) NOT NULL)",
        "CREATE TABLE tags (id integer PRIMARY KEY, label text)",
        "CREATE TABLE orders (id bigint PRIMARY KEY, user_id bigint NOT NULL REFERENCES users(id))",
        "CREATE TABLE product_tags (product_id integer NOT NULL REFERENCES products(id), \
         tag_id integer NOT NULL REFERENCES tags(id), PRIMARY KEY (product_id, tag_id))",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    let config = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port);
    let capture = WalChangeCapture::new(config, url);

    let catalog = capture.introspect().await.unwrap();
    let table = |name: &str| {
        catalog
            .tables
            .iter()
            .find(|t| t.name.as_ref() == name)
            .unwrap_or_else(|| panic!("table {name} missing from catalog"))
    };

    // Columns + suggested types.
    let users = table("users");
    let email = users
        .columns
        .iter()
        .find(|c| c.name.as_ref() == "email")
        .unwrap();
    assert_eq!(email.suggested_type, Some(FlussoType::Keyword));
    assert!(!email.nullable);
    let bio = users
        .columns
        .iter()
        .find(|c| c.name.as_ref() == "bio")
        .unwrap();
    assert_eq!(bio.suggested_type, Some(FlussoType::Text));
    assert!(bio.nullable);
    let price = table("products")
        .columns
        .iter()
        .find(|c| c.name.as_ref() == "price")
        .unwrap();
    assert_eq!(price.suggested_type, Some(FlussoType::Decimal));

    // Primary keys, including a composite one.
    assert_eq!(
        users
            .primary_key
            .iter()
            .map(|c| c.as_ref())
            .collect::<Vec<_>>(),
        ["id"]
    );
    assert_eq!(
        table("product_tags")
            .primary_key
            .iter()
            .map(|c| c.as_ref())
            .collect::<Vec<_>>(),
        ["product_id", "tag_id"]
    );

    // Foreign keys.
    let orders_fks = &table("orders").foreign_keys;
    assert_eq!(orders_fks.len(), 1);
    assert_eq!(orders_fks[0].columns[0].as_ref(), "user_id");
    assert_eq!(orders_fks[0].references_table.as_ref(), "users");
    assert_eq!(orders_fks[0].references_columns[0].as_ref(), "id");

    // Junction detection (a free function over the catalog).
    let junctions = junction_candidates(&catalog);
    assert_eq!(junctions.len(), 1, "product_tags is the only junction");
    assert_eq!(junctions[0].table.table.as_ref(), "product_tags");
    let refs: Vec<&str> = [&junctions[0].left, &junctions[0].right]
        .iter()
        .map(|fk| fk.references_table.as_ref())
        .collect();
    assert!(refs.contains(&"products") && refs.contains(&"tags"));
}
