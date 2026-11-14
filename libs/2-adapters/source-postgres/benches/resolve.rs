//! In-process benchmark of reverse resolution: the per-change work
//! `PgDocumentBuilder::resolve` does before (and instead of) any query.
//!
//! Every live change is resolved once on the ingest side, so this is the hot
//! path's fixed cost. The three changes measured are the ones that need no
//! query, so the builder's pool never connects and no Docker is needed:
//!
//! - `root_table`: a change on the index's root table, one id.
//! - `unrelated_table`: a change on a table no index reads, no id.
//! - `unread_update`: an update of a child table that changed no column the
//!   document reads (issue #169), no id: walks the paths, skips each.
//!
//! The Docker-backed `postgres` bench measures the same `resolve` against a
//! live database, plus the reverse lookup from a related table.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p flusso-source-postgres --bench resolve --features bench
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

use criterion::{Criterion, criterion_group, criterion_main};
use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, GenericValue,
    IndexName, IndexSchema, Join, JoinKind, Relation, SoftDelete, SoftDeleteColumn, TableName,
};
use source::document::DocumentBuilder;
use source::{RowImage, RowKey, SourceSpec};

fn bench(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let builder = {
        let _guard = rt.enter();
        source_postgres::bench_offline_builder(Arc::new(spec()))
    };

    let users = table("users");
    let orders = table("orders");
    let products = table("products");
    let user_key = row_key(4);
    let order_key = row_key(4000);
    let before = order_row("first");
    let after = order_row("second");

    let mut group = c.benchmark_group("resolve_no_io");
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("root_table", |b| {
        b.to_async(&rt).iter(|| async {
            let ids = builder
                .resolve(&users, &user_key, None, None)
                .await
                .unwrap();
            assert_eq!(ids.len(), 1);
        });
    });
    group.bench_function("unrelated_table", |b| {
        b.to_async(&rt).iter(|| async {
            let ids = builder
                .resolve(&products, &user_key, None, None)
                .await
                .unwrap();
            assert!(ids.is_empty());
        });
    });
    group.bench_function("unread_update", |b| {
        b.to_async(&rt).iter(|| async {
            let ids = builder
                .resolve(&orders, &order_key, Some(&before), Some(&after))
                .await
                .unwrap();
            assert!(ids.is_empty());
        });
    });
    group.finish();
}

/// An `orders` row as a `FULL` replica identity logs it; `note` is read by no
/// document.
fn order_row(note: &str) -> RowImage {
    RowImage(vec![
        (column("id"), GenericValue::Int(4000)),
        (column("user_id"), GenericValue::Int(4)),
        (column("total"), GenericValue::String("19.99".into())),
        (column("status"), GenericValue::String("paid".into())),
        (column("note"), GenericValue::String(note.into())),
    ])
}

/// The `postgres` bench's index: `users` with its `orders` folded in as a
/// nested one-to-many array, soft-deleted by a column.
fn spec() -> SourceSpec {
    let orders = Field {
        field: field("orders"),
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
            fields: vec![
                column_field("id", "id"),
                column_field("total", "total"),
                column_field("status", "status"),
            ],
            indexed: true,
            on: vec![],
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
            column_field("name", "name"),
            orders,
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

fn column_field(name: &str, col: &str) -> Field {
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

fn row_key(id: i32) -> RowKey {
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
