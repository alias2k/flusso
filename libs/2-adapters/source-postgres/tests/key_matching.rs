//! The batched build serves exactly the ids a single-document build would
//! (issue #335): for a key decoded as one kind (as a change read before an
//! `ALTER … TYPE`) and a row stored as another, `build` and `build_many`
//! agree on upsert, tombstone or error, and on the `_id`.
//!
//! Every pair of key kinds Postgres casts between is driven over a real
//! database whose own `TimeZone` is not UTC, so a `timestamp` ↔
//! `timestamptz` cast shows which zone the builder's session uses.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-source-postgres --test key_matching --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, GenericValue,
    IndexName, IndexSchema, TableName,
};
use rust_decimal::Decimal;
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceSpec};
use source_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// Each key column type, with the texts stored in its table.
const KINDS: &[(&str, &[&str])] = &[
    ("int2", &["1", "2"]),
    ("int4", &["1", "2", "100000"]),
    ("int8", &["1", "2", "100000", "5000000000"]),
    ("numeric", &["1", "1.5", "2", "100000"]),
    ("numeric(10,2)", &["1", "1.5", "2"]),
    ("float8", &["1", "1.5", "0.1", "2"]),
    ("real", &["1", "1.5", "0.1", "2"]),
    (
        "text",
        &[
            "1",
            "1.5",
            "1.50",
            "abc",
            "2020-01-02",
            "2020-01-02 10:00:00",
            UUID,
            "true",
            "10:00:00",
        ],
    ),
    ("varchar(20)", &["1", "1.5", "abc", "2020-01-02"]),
    ("date", &["2020-01-02", "2020-01-03"]),
    ("timestamp", &["2020-01-02 00:00:00", "2020-01-02 10:00:00"]),
    (
        "timestamptz",
        &["2020-01-02 00:00:00+00", "2020-01-02 10:00:00+00"],
    ),
    ("uuid", &[UUID]),
    ("bool", &["true"]),
    ("time", &["10:00:00"]),
];

const UUID: &str = "0b7f2d4e-1c2a-4d5e-8f90-1a2b3c4d5e6f";

/// Keys as the WAL decoder hands them over, one or more per kind.
fn requested() -> Vec<GenericValue> {
    let decimal = |text: &str| GenericValue::Decimal(Decimal::from_str(text).unwrap());
    let day = NaiveDate::from_ymd_opt(2020, 1, 2).unwrap();
    let at = |hour: u32| -> NaiveDateTime { day.and_hms_opt(hour, 0, 0).unwrap() };
    vec![
        GenericValue::SmallInt(1),
        GenericValue::SmallInt(2),
        GenericValue::Int(1),
        GenericValue::Int(2),
        GenericValue::Int(100000),
        GenericValue::BigInt(1),
        GenericValue::BigInt(5_000_000_000),
        decimal("1"),
        decimal("1.00"),
        decimal("1.5"),
        decimal("1.50"),
        decimal("2.5"),
        GenericValue::Double(1.0),
        GenericValue::Double(1.5),
        GenericValue::Double(0.1),
        GenericValue::Float(1.5),
        GenericValue::Float(0.1),
        GenericValue::String("1".into()),
        GenericValue::String("1.5".into()),
        GenericValue::String("abc".into()),
        GenericValue::String("2020-01-02".into()),
        GenericValue::Date(day),
        GenericValue::Timestamp(at(0)),
        GenericValue::Timestamp(at(10)),
        GenericValue::TimestampTz(Utc.from_utc_datetime(&at(0))),
        GenericValue::TimestampTz(Utc.from_utc_datetime(&at(10))),
        GenericValue::Uuid(uuid::Uuid::parse_str(UUID).unwrap()),
        GenericValue::String(UUID.into()),
        GenericValue::Bool(true),
        GenericValue::String("true".into()),
        GenericValue::Time(chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
        GenericValue::String("10:00:00".into()),
    ]
}

/// The SQL type a key is bound as: `sql::bound_type`.
fn bound_type(value: &GenericValue) -> &'static str {
    match value {
        GenericValue::SmallInt(_) => "int2",
        GenericValue::Int(_) => "int4",
        GenericValue::BigInt(_) => "int8",
        GenericValue::Decimal(_) => "numeric",
        GenericValue::Double(_) => "float8",
        GenericValue::Float(_) => "float4",
        GenericValue::Uuid(_) => "uuid",
        GenericValue::Bool(_) => "bool",
        GenericValue::Time(_) => "time",
        GenericValue::String(_) => "text",
        GenericValue::Date(_) => "date",
        GenericValue::Timestamp(_) => "timestamp",
        GenericValue::TimestampTz(_) => "timestamptz",
        other => panic!("no requested key of {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn the_batched_build_serves_the_ids_a_single_build_serves() {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::raw_sql("ALTER DATABASE postgres SET timezone = 'Asia/Tokyo'")
        .execute(&pool)
        .await
        .unwrap();

    let mut schemas = BTreeMap::new();
    for (n, (kind, stored)) in KINDS.iter().enumerate() {
        let table = format!("k{n}");
        execute(
            &pool,
            &format!("CREATE TABLE {table} (id {kind} PRIMARY KEY, name text)"),
        )
        .await;
        for text in *stored {
            execute(
                &pool,
                &format!("INSERT INTO {table} VALUES ('{text}', '{text}') ON CONFLICT DO NOTHING"),
            )
            .await;
        }
        schemas.insert(index_name(&table), schema(&table));
    }
    let builder = PgDocumentBuilder::connect(&url, Arc::new(SourceSpec::new(schemas)))
        .await
        .unwrap();

    let mut disagreements = Vec::new();
    let mut compared = 0;
    for (n, (kind, _)) in KINDS.iter().enumerate() {
        let table = format!("k{n}");
        for key in requested() {
            let cast = format!("SELECT NULL::{}::{kind}", bound_type(&key));
            if sqlx::raw_sql(sqlx::AssertSqlSafe(cast))
                .execute(&pool)
                .await
                .is_err()
            {
                continue;
            }
            let id = DocumentId {
                index: index_name(&table),
                key: RowKey(vec![(column("id"), key.clone())]),
            };
            let single = outcome(builder.build(&id).await, &id);
            let batched = outcome(
                builder
                    .build_many(std::slice::from_ref(&id))
                    .await
                    .map(|mut documents| documents.remove(0)),
                &id,
            );
            compared += 1;
            if single != batched {
                disagreements.push(format!(
                    "{key:?} against a {kind} key: build {single}, build_many {batched}"
                ));
            }
        }
    }
    assert!(compared > 150, "compared {compared} pairs");
    assert!(
        disagreements.is_empty(),
        "{} of {compared} pairs disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

/// `upsert`, `delete` or `error`; an upsert must be under the requested id.
fn outcome(document: source::Result<Document>, id: &DocumentId) -> &'static str {
    match document {
        Ok(Document::Upsert { id: built, .. }) => {
            assert_eq!(&built, id);
            "upsert"
        }
        Ok(Document::Delete { id: built }) => {
            assert_eq!(&built, id);
            "delete"
        }
        Ok(Document::Failed { .. }) => "failed",
        Err(_) => "error",
    }
}

async fn execute(pool: &sqlx::PgPool, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

fn schema(table: &str) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new(table).unwrap(),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new("name").unwrap(),
            options: Default::default(),
            emit: true,
            source: FieldSource::Column(Column {
                column: column("name"),
                ty: FlussoType::Keyword,
                nullable: true,
                transforms: Vec::new(),
                default: None,
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
                array: false,
            }),
        }],
    }
}

fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}
