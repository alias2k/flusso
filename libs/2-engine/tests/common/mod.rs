//! Helpers shared by the Postgres-backed e2e binaries (`wal`, `wal_release`):
//! a logical-replication Postgres container and the small schema builders the
//! index specs are written with.
//!
//! Each binary compiles this module on its own and uses a subset of it, hence
//! the `dead_code` allowance.

#![allow(dead_code)]

use std::collections::BTreeMap;

use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, IndexName,
    IndexSchema, TableName,
};
use source::SourceSpec;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

/// A PG 16 with logical replication, and its connection URL. PG 14+ is
/// required: the replication client requests the pgoutput `messages` option.
pub(crate) async fn start_postgres() -> (ContainerAsync<Postgres>, String) {
    let container = Postgres::default()
        .with_tag("16-alpine")
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
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, url)
}

/// `users` indexed as `{ id, email }`.
pub(crate) fn users_spec() -> SourceSpec {
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![column_field("id", "id"), column_field("email", "email")],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

pub(crate) fn column_field(name: &str, col: &str) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: column(col),
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

pub(crate) fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

pub(crate) fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

pub(crate) fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

pub(crate) fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}
