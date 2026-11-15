//! The batched build serves exactly the ids a single-document build serves
//! (issue #335), for any key against any stored row: a key decoded as one
//! kind (a change read before an `ALTER … TYPE`) against a row stored as
//! another, and every spelling Postgres reads or writes.
//!
//! A generated differential: every requested key (each `GenericValue` kind,
//! with the edge values Postgres's input and output functions treat
//! specially) against a table of each key type holding every stored text it
//! accepts. For each pair, `build_many` alone, in a batch that also asks for
//! every stored row (the row's other spellings), and in a batch of
//! [`PARAMETERS`] keys of the requested kind (the others of that kind a single
//! build accepts, then the key again) must give what `build` gives: the
//! same upsert, tombstone or error, and the same document body. The batches
//! reach both ways a batch sends its keys: a batch under sixteen keys is
//! written as an array constructor, one of [`PARAMETERS`] passes them as
//! array parameters, and a hash-partitioned twin of every table takes the
//! constructor at any size.
//! The database's own `TimeZone` is not UTC, so a `timestamp` ↔
//! `timestamptz` cast shows the builder's session zone.
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

use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
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

/// The key column types, one table each.
const KEY_TYPES: &[&str] = &[
    "int2",
    "int4",
    "int8",
    "numeric",
    "numeric(10,2)",
    "real",
    "float8",
    "bool",
    "text",
    "varchar(20)",
    "char(5)",
    "citext",
    "date",
    "time",
    "timestamp",
    "timestamptz",
    "uuid",
    "bytea",
    "name",
];

/// A batch this large on an unpartitioned root passes its keys as array
/// parameters (`query::KeyForm`).
const PARAMETERS: usize = 17;

/// Past `name`'s 63 bytes: a cast to `name` cuts it.
const LONG: &str = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqr";

const UUID: &str = "0b7f2d4e-1c2a-4d5e-8f90-1a2b3c4d5e6f";

/// Two more `uuid` rows, so keys of a batch name different rows there too.
const UUID_2: &str = "1c8e3f5a-2d3b-4e6f-9a01-2b3c4d5e6f70";
const UUID_3: &str = "2d9f4a6b-3e4c-4f70-8b12-3c4d5e6f7081";

/// Texts inserted into every table; each keeps those its key type accepts.
const STORED: &[&str] = &[
    "0",
    "1",
    "-1",
    "2",
    "01",
    "1.5",
    "1.50",
    "2.5",
    "-0",
    "1e+20",
    "100000000000000000000",
    "1e-05",
    "0.00001",
    "Infinity",
    "-Infinity",
    "NaN",
    "100000",
    "3000000000",
    "123456789012345678901234567890.5",
    "true",
    "false",
    "abc",
    "ABC",
    "a",
    "2020-01-02",
    "2020-01-03",
    "2020-01-02 00:00:00",
    "2020-01-02 10:00:00",
    "2020-01-02 10:00:00.5",
    "2020-01-02 10:00:00+00",
    "0044-03-15 BC",
    "infinity",
    "10:00:00",
    "10:00:00.5",
    "24:00:00",
    UUID,
    UUID_2,
    UUID_3,
    "\\x0102",
    LONG,
];

/// Keys as the WAL decoder hands them over, every kind, with edge values.
fn requested() -> Vec<GenericValue> {
    use GenericValue as V;
    let decimal = |text: &str| V::Decimal(Decimal::from_str(text).unwrap());
    let day = NaiveDate::from_ymd_opt(2020, 1, 2).unwrap();
    let time =
        |h: u32, m: u32, s: u32, micro: u32| NaiveTime::from_hms_micro_opt(h, m, s, micro).unwrap();
    let at = |t: NaiveTime| day.and_time(t);
    let mut keys = vec![
        V::SmallInt(0),
        V::SmallInt(1),
        V::SmallInt(-1),
        V::SmallInt(2),
        V::SmallInt(i16::MAX),
        V::Int(0),
        V::Int(1),
        V::Int(-1),
        V::Int(2),
        V::Int(100000),
        V::Int(i32::MAX),
        V::BigInt(1),
        V::BigInt(3_000_000_000),
        V::BigInt(9_210_000_000_000_000_000),
        V::BigInt(i64::MAX),
        decimal("0"),
        decimal("1"),
        decimal("1.00"),
        decimal("1.5"),
        decimal("1.50"),
        decimal("2.5"),
        decimal("-1.5"),
        decimal("0.5"),
        decimal("1.234567"),
        decimal("99999999.99"),
        decimal("100000000000000000000"),
        decimal("0.00001"),
        V::Double(0.0),
        V::Double(-0.0),
        V::Double(1.0),
        V::Double(1.5),
        V::Double(2.5),
        V::Double(0.5),
        V::Double(0.1),
        V::Double(1e15),
        V::Double(1e20),
        V::Double(1e-5),
        V::Double(9.21e18),
        V::Double(f64::INFINITY),
        V::Double(f64::NEG_INFINITY),
        V::Double(f64::NAN),
        V::Float(1.5),
        V::Float(0.1),
        V::Float(-0.0),
        V::Float(f32::INFINITY),
        V::Bool(true),
        V::Bool(false),
        V::Date(day),
        V::Date(NaiveDate::from_ymd_opt(-43, 3, 15).unwrap()),
        V::Time(time(10, 0, 0, 0)),
        V::Time(time(10, 0, 0, 500_000)),
        V::Timestamp(at(time(0, 0, 0, 0))),
        V::Timestamp(at(time(10, 0, 0, 0))),
        V::Timestamp(at(time(10, 0, 0, 500_000))),
        V::TimestampTz(Utc.from_utc_datetime(&at(time(0, 0, 0, 0)))),
        V::TimestampTz(Utc.from_utc_datetime(&at(time(10, 0, 0, 0)))),
        V::Uuid(uuid::Uuid::parse_str(UUID).unwrap()),
        V::Uuid(uuid::Uuid::parse_str(UUID_2).unwrap()),
        V::Uuid(uuid::Uuid::parse_str(UUID_3).unwrap()),
        V::Bytes(vec![1, 2]),
        V::Bytes(b"abc".to_vec()),
    ];
    for text in [
        "0",
        "1",
        " 1",
        "1 ",
        "+1",
        "01",
        "-1",
        "2",
        "1_000",
        "1e3",
        "1.5",
        "1.50",
        "2.5",
        "-0",
        "1e+20",
        "100000000000000000000",
        "1e-05",
        "infinity",
        "Infinity",
        "-Infinity",
        "NaN",
        "3000000000",
        "123456789012345678901234567890.5",
        "t",
        "tru",
        "true",
        "TRUE",
        "yes",
        "on",
        "off",
        "abc",
        "ABC",
        "a",
        "a    ",
        "2020-01-02",
        "2020-01-02 10:00:00",
        "2020-01-02 10:00:00+00",
        "2020-01-02T10:00:00Z",
        "2020-01-02 10:00:00.5",
        "2020-01-02 10:00:00.500",
        "epoch",
        "Jan 2 2020",
        "20200102",
        "0044-03-15 BC",
        "10:00",
        "10:00:00",
        "10:00:00.5",
        "10:00:00.500",
        "24:00:00",
        UUID,
        UUID_2,
        UUID_3,
        "0B7F2D4E-1C2A-4D5E-8F90-1A2B3C4D5E6F",
        "{0b7f2d4e-1c2a-4d5e-8f90-1a2b3c4d5e6f}",
        "0b7f2d4e1c2a4d5e8f901a2b3c4d5e6f",
        "\\x0102",
        LONG,
        &LONG[..64],
        &LONG[..63],
    ] {
        keys.push(V::String(text.into()));
    }
    keys
}

/// The SQL type a key is bound as: `sql::bound_type`.
fn bound_type(value: &GenericValue) -> &'static str {
    match value {
        GenericValue::SmallInt(_) => "int2",
        GenericValue::Int(_) => "int4",
        GenericValue::BigInt(_) => "int8",
        GenericValue::Decimal(_) => "numeric",
        GenericValue::Float(_) => "float4",
        GenericValue::Double(_) => "float8",
        GenericValue::Bool(_) => "bool",
        GenericValue::String(_) => "text",
        GenericValue::Uuid(_) => "uuid",
        GenericValue::Date(_) => "date",
        GenericValue::Time(_) => "time",
        GenericValue::Timestamp(_) => "timestamp",
        GenericValue::TimestampTz(_) => "timestamptz",
        GenericValue::Bytes(_) => "bytea",
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
    for statement in [
        "ALTER DATABASE postgres SET timezone = 'Asia/Tokyo'",
        "CREATE EXTENSION citext",
    ] {
        execute(&pool, statement).await.unwrap();
    }

    let mut schemas = BTreeMap::new();
    let mut stored_keys = BTreeMap::new();
    for (table, key_type) in tables() {
        let partitioned = table.starts_with('p');
        let create = format!("CREATE TABLE {table} (id {key_type} PRIMARY KEY, name text)");
        if partitioned {
            execute(&pool, &format!("{create} PARTITION BY HASH (id)"))
                .await
                .unwrap();
            for remainder in 0..4 {
                execute(
                    &pool,
                    &format!(
                        "CREATE TABLE {table}_{remainder} PARTITION OF {table} \
                         FOR VALUES WITH (MODULUS 4, REMAINDER {remainder})"
                    ),
                )
                .await
                .unwrap();
            }
        } else {
            execute(&pool, &create).await.unwrap();
        }
        for text in STORED {
            let quoted = text.replace('\'', "''");
            let _ = execute(
                &pool,
                &format!(
                    "INSERT INTO {table} VALUES ('{quoted}', '{quoted}') ON CONFLICT DO NOTHING"
                ),
            )
            .await;
        }
        let keys: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT format('%s', id) FROM {table}"
        )))
        .fetch_all(&pool)
        .await
        .unwrap();
        stored_keys.insert(table.clone(), keys);
        schemas.insert(index_name(&table), schema(&table));
    }
    let builder = PgDocumentBuilder::connect(&url, Arc::new(SourceSpec::new(schemas)))
        .await
        .unwrap();

    let mut disagreements = Vec::new();
    let mut pairs = 0;
    let requested = requested();
    for (table, key_type) in tables() {
        let others: Vec<DocumentId> = stored_keys[&table]
            .iter()
            .map(|text| id(&table, GenericValue::String(text.clone())))
            .collect();
        let mut singles = Vec::with_capacity(requested.len());
        for key in &requested {
            let id = id(&table, key.clone());
            singles.push(outcome(builder.build(&id).await, &id));
        }
        for (key, single) in requested.iter().zip(&singles) {
            let key = key.clone();
            let single = single.clone();
            pairs += 1;
            let id = id(&table, key.clone());
            let alone = outcome(
                builder
                    .build_many(std::slice::from_ref(&id))
                    .await
                    .map(|mut documents| documents.remove(0)),
                &id,
            );
            let mut batch = vec![id.clone()];
            batch.extend(others.iter().cloned());
            let mixed = outcome(
                builder
                    .build_many(&batch)
                    .await
                    .map(|mut documents| documents.remove(0)),
                &id,
            );
            let mut same_kind = vec![id.clone()];
            same_kind.extend(
                requested
                    .iter()
                    .zip(&singles)
                    .filter(|(other, built)| {
                        **other != key
                            && *built != "error"
                            && std::mem::discriminant(*other) == std::mem::discriminant(&key)
                    })
                    .map(|(other, _)| self::id(&table, other.clone())),
            );
            while same_kind.len() < PARAMETERS {
                same_kind.push(id.clone());
            }
            let wide = match builder.build_many(&same_kind).await {
                Ok(documents) => documents
                    .into_iter()
                    .zip(&same_kind)
                    .filter(|(_, requested)| **requested == id)
                    .map(|(document, _)| outcome(Ok(document), &id))
                    .find(|got| *got != single)
                    .unwrap_or_else(|| single.clone()),
                Err(error) => outcome(Err(error), &id),
            };
            if single != alone || single != mixed || single != wide {
                disagreements.push(format!(
                    "{key:?} ({}) against a {key_type} key in {table}: build {single}, \
                     alone {alone}, beside every stored row {mixed}, among {} keys of \
                     its kind {wide}",
                    bound_type(&key),
                    same_kind.len()
                ));
            }
        }
    }
    assert!(pairs >= 4000, "{pairs} pairs");
    assert!(
        disagreements.is_empty(),
        "{} of {pairs} pairs disagree:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

/// Every key type's table, `k<n>`, and its hash-partitioned twin, `p<n>`.
fn tables() -> impl Iterator<Item = (String, &'static str)> {
    KEY_TYPES
        .iter()
        .enumerate()
        .flat_map(|(n, key_type)| [(format!("k{n}"), *key_type), (format!("p{n}"), *key_type)])
}

/// What a build gives one id: the upsert's body, a tombstone, a failure or
/// an error. An upsert or tombstone must be under the requested id.
fn outcome(document: source::Result<Document>, id: &DocumentId) -> String {
    match document {
        Ok(Document::Upsert { id: built, body }) => {
            assert_eq!(&built, id);
            format!("upsert {}", sink::to_json(&body))
        }
        Ok(Document::Delete { id: built }) => {
            assert_eq!(&built, id);
            "delete".to_owned()
        }
        Ok(Document::Failed { .. }) => "failed".to_owned(),
        Err(_) => "error".to_owned(),
    }
}

fn id(table: &str, key: GenericValue) -> DocumentId {
    DocumentId {
        index: index_name(table),
        key: RowKey(vec![(column("id"), key)]),
    }
}

async fn execute(pool: &sqlx::PgPool, sql: &str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .map(|_| ())
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
