//! Review cases for the batched document build, against a live Postgres.
//!
//! `build_many` over the ids a reviewer of a source change keeps asking about:
//! how many, how many exist, which key type, which root shape. Each case's
//! ids come from keys read back the way the WAL decoder reads them (the key's
//! text, typed by its column type) and then resolved, so a key whose type the
//! build does not match shows up as tombstones instead of being measured as a
//! hit.
//!
//! | Group | Cases |
//! | --- | --- |
//! | `batch` | 1, 10, 100, 1000, 10000 ids of a plain `int` root |
//! | `hit_ratio` | 0, 50, 100 % of 1000 ids exist |
//! | `key` | `int`, `uuid`, `varchar`, `numeric`, `float8`, `char(n)`, `citext` |
//! | `root` | plain, hash- and range-partitioned, composite key, keyless (`REPLICA IDENTITY FULL`), a wide row with a large unread column, the same with a composite key, a never-analysed table |
//!
//! Every root has a `has_many` child keyed by the root's key type, two rows
//! per root. Before measuring, each case is built once and checked: a case
//! whose build returns another number of documents than its rows is skipped
//! with a message, so an A/B comparison lists it on one side only.
//!
//! Not part of the PR gate nor the `main` history: `just bench-components` and
//! `just review-ab` run it. Requires Docker, or `BENCH_PG_URL` (an empty
//! database; the bench creates `citext`). Run with:
//!
//! ```text
//! cargo bench -p flusso-source-postgres --bench cases
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, GenericValue,
    IndexName, IndexSchema, Join, JoinKind, Relation, TableName,
};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceSpec};
use source_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Row};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use tokio::runtime::Runtime;

/// Rows in the `plain` root, the largest batch.
const PLAIN_ROWS: i32 = 10_000;
/// Rows in every other root.
const ROWS: i32 = 1_000;
const BATCHES: &[usize] = &[1, 10, 100, 1_000, 10_000];
const HIT_PERCENTS: &[usize] = &[0, 50, 100];

/// How a root key's text becomes a value, as `text_value::typed_value` does it
/// for the WAL.
#[derive(Clone, Copy)]
enum KeyType {
    Int,
    Uuid,
    Numeric,
    Float8,
    Text,
}

impl KeyType {
    fn value(self, text: &str) -> GenericValue {
        match self {
            Self::Int => GenericValue::Int(text.parse().unwrap()),
            Self::Uuid => GenericValue::Uuid(uuid::Uuid::parse_str(text).unwrap()),
            Self::Numeric => {
                GenericValue::Decimal(rust_decimal::Decimal::from_str_exact(text).unwrap())
            }
            Self::Float8 => GenericValue::Double(text.parse().unwrap()),
            Self::Text => GenericValue::String(text.to_owned()),
        }
    }
}

/// One root table: its DDL, how a row's key is spelled from `g`, and the key
/// columns the WAL carries for it.
struct Root {
    table: &'static str,
    /// `CREATE TABLE …` plus any partitions or identity change.
    ddl: &'static [&'static str],
    /// Column list and `SELECT` over `generate_series(1, $1) g` that seeds it.
    insert: &'static str,
    sql_type: &'static str,
    key_type: KeyType,
    /// Columns of the key a change carries: the primary key, or every column
    /// of a table without one.
    wal_key: &'static [&'static str],
    rows: i32,
}

const fn root(
    table: &'static str,
    sql_type: &'static str,
    key_type: KeyType,
    ddl: &'static [&'static str],
    insert: &'static str,
) -> Root {
    Root {
        table,
        ddl,
        insert,
        sql_type,
        key_type,
        wal_key: &["id"],
        rows: ROWS,
    }
}

fn roots() -> Vec<Root> {
    vec![
        Root {
            rows: PLAIN_ROWS,
            ..root("plain", "int", KeyType::Int, &["CREATE TABLE plain (id int PRIMARY KEY, name text)"],
                "INSERT INTO plain (id, name) SELECT g, 'name ' || g FROM generate_series(1, $1) g")
        },
        root("k_int", "int", KeyType::Int, &["CREATE TABLE k_int (id int PRIMARY KEY, name text)"],
            "INSERT INTO k_int (id, name) SELECT g, 'name ' || g FROM generate_series(1, $1) g"),
        root("k_uuid", "uuid", KeyType::Uuid, &["CREATE TABLE k_uuid (id uuid PRIMARY KEY, name text)"],
            "INSERT INTO k_uuid (id, name) SELECT md5(g::text)::uuid, 'name ' || g FROM generate_series(1, $1) g"),
        root("k_varchar", "varchar(40)", KeyType::Text, &["CREATE TABLE k_varchar (id varchar(40) PRIMARY KEY, name text)"],
            "INSERT INTO k_varchar (id, name) SELECT 'key-' || g, 'name ' || g FROM generate_series(1, $1) g"),
        root("k_numeric", "numeric(12,2)", KeyType::Numeric, &["CREATE TABLE k_numeric (id numeric(12,2) PRIMARY KEY, name text)"],
            "INSERT INTO k_numeric (id, name) SELECT g + 0.25, 'name ' || g FROM generate_series(1, $1) g"),
        root("k_float8", "float8", KeyType::Float8, &["CREATE TABLE k_float8 (id float8 PRIMARY KEY, name text)"],
            "INSERT INTO k_float8 (id, name) SELECT g + 0.5, 'name ' || g FROM generate_series(1, $1) g"),
        root("k_char", "char(12)", KeyType::Text, &["CREATE TABLE k_char (id char(12) PRIMARY KEY, name text)"],
            "INSERT INTO k_char (id, name) SELECT lpad(g::text, 12, 'k'), 'name ' || g FROM generate_series(1, $1) g"),
        root("k_citext", "citext", KeyType::Text, &["CREATE TABLE k_citext (id citext PRIMARY KEY, name text)"],
            "INSERT INTO k_citext (id, name) SELECT 'Key-' || g, 'name ' || g FROM generate_series(1, $1) g"),
        root("s_hash", "int", KeyType::Int, &[
                "CREATE TABLE s_hash (id int PRIMARY KEY, name text) PARTITION BY HASH (id)",
                "CREATE TABLE s_hash_0 PARTITION OF s_hash FOR VALUES WITH (MODULUS 4, REMAINDER 0)",
                "CREATE TABLE s_hash_1 PARTITION OF s_hash FOR VALUES WITH (MODULUS 4, REMAINDER 1)",
                "CREATE TABLE s_hash_2 PARTITION OF s_hash FOR VALUES WITH (MODULUS 4, REMAINDER 2)",
                "CREATE TABLE s_hash_3 PARTITION OF s_hash FOR VALUES WITH (MODULUS 4, REMAINDER 3)",
            ],
            "INSERT INTO s_hash (id, name) SELECT g, 'name ' || g FROM generate_series(1, $1) g"),
        root("s_range", "int", KeyType::Int, &[
                "CREATE TABLE s_range (id int PRIMARY KEY, name text) PARTITION BY RANGE (id)",
                "CREATE TABLE s_range_0 PARTITION OF s_range FOR VALUES FROM (MINVALUE) TO (251)",
                "CREATE TABLE s_range_1 PARTITION OF s_range FOR VALUES FROM (251) TO (501)",
                "CREATE TABLE s_range_2 PARTITION OF s_range FOR VALUES FROM (501) TO (751)",
                "CREATE TABLE s_range_3 PARTITION OF s_range FOR VALUES FROM (751) TO (MAXVALUE)",
            ],
            "INSERT INTO s_range (id, name) SELECT g, 'name ' || g FROM generate_series(1, $1) g"),
        Root {
            wal_key: &["tenant", "id"],
            ..root("s_composite", "int", KeyType::Int,
                &["CREATE TABLE s_composite (tenant int, id int, name text, PRIMARY KEY (tenant, id))"],
                "INSERT INTO s_composite (tenant, id, name) SELECT g % 7, g, 'name ' || g FROM generate_series(1, $1) g")
        },
        Root {
            wal_key: &["id", "name"],
            ..root("s_keyless", "int", KeyType::Int, &[
                    "CREATE TABLE s_keyless (id int NOT NULL, name text NOT NULL)",
                    "ALTER TABLE s_keyless REPLICA IDENTITY FULL",
                ],
                "INSERT INTO s_keyless (id, name) SELECT g, 'name ' || g FROM generate_series(1, $1) g")
        },
        // About 19 KiB of incompressible text per row, so it is TOASTed: the
        // document never reads it.
        root("s_wide", "int", KeyType::Int, &["CREATE TABLE s_wide (id int PRIMARY KEY, name text, payload text)"],
            "INSERT INTO s_wide (id, name, payload)
             SELECT g, 'name ' || g, (SELECT string_agg(md5(g::text || i::text), '') FROM generate_series(1, 600) i)
             FROM generate_series(1, $1) g"),
        Root {
            wal_key: &["tenant", "id"],
            ..root("s_wide_composite", "int", KeyType::Int,
                &["CREATE TABLE s_wide_composite (tenant int, id int, name text, payload text, PRIMARY KEY (tenant, id))"],
                "INSERT INTO s_wide_composite (tenant, id, name, payload)
                 SELECT g % 7, g, 'name ' || g, (SELECT string_agg(md5(g::text || i::text), '') FROM generate_series(1, 600) i)
                 FROM generate_series(1, $1) g")
        },
        // Never analysed: the planner has no statistics for it.
        root("s_unanalysed", "int", KeyType::Int,
            &["CREATE TABLE s_unanalysed (id int PRIMARY KEY, name text) WITH (autovacuum_enabled = false)"],
            "INSERT INTO s_unanalysed (id, name) SELECT g, 'name ' || g FROM generate_series(1, $1) g"),
    ]
}

/// The group and name of the `root` case each table stands for.
fn shape_case(table: &str) -> Option<&'static str> {
    Some(match table {
        "plain" => "plain",
        "s_hash" => "hash_partitioned",
        "s_range" => "range_partitioned",
        "s_composite" => "composite_key",
        "s_keyless" => "keyless",
        "s_wide" => "wide_unread",
        "s_wide_composite" => "wide_composite_key",
        "s_unanalysed" => "never_analysed",
        _ => return None,
    })
}

fn key_case(table: &str) -> Option<&'static str> {
    Some(match table {
        "k_int" => "int",
        "k_uuid" => "uuid",
        "k_varchar" => "varchar",
        "k_numeric" => "numeric",
        "k_float8" => "float8",
        "k_char" => "char",
        "k_citext" => "citext",
        _ => return None,
    })
}

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

async fn connect() -> (Option<ContainerAsync<Postgres>>, String) {
    if let Ok(url) = std::env::var("BENCH_PG_URL") {
        return (None, url);
    }
    let container = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    (
        Some(container),
        format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"),
    )
}

async fn seed(pool: &sqlx::PgPool, roots: &[Root]) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS citext")
        .execute(pool)
        .await
        .unwrap();
    for root in roots {
        for ddl in root.ddl {
            sqlx::query(AssertSqlSafe(*ddl))
                .execute(pool)
                .await
                .unwrap();
        }
        sqlx::query(AssertSqlSafe(root.insert))
            .bind(root.rows)
            .execute(pool)
            .await
            .unwrap();
        let items = format!("{}_items", root.table);
        for sql in [
            format!(
                "CREATE TABLE {items} (id serial PRIMARY KEY, parent {} NOT NULL, label text)",
                root.sql_type
            ),
            format!("CREATE INDEX ON {items} (parent)"),
            format!(
                "INSERT INTO {items} (parent, label) SELECT id, 'item ' || n FROM {}, generate_series(1, 2) n",
                root.table
            ),
        ] {
            sqlx::query(AssertSqlSafe(sql)).execute(pool).await.unwrap();
        }
    }
    for root in roots {
        let vacuum = if root.table == "s_unanalysed" {
            "VACUUM"
        } else {
            "VACUUM ANALYZE"
        };
        for table in [root.table.to_owned(), format!("{}_items", root.table)] {
            sqlx::query(AssertSqlSafe(format!("{vacuum} {table}")))
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

/// Every root's change keys, in key order, as the WAL decoder would read them.
async fn wal_keys(pool: &sqlx::PgPool, root: &Root) -> Vec<RowKey> {
    let columns = root
        .wal_key
        .iter()
        .map(|c| format!("{c}::text"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("SELECT {columns} FROM {} ORDER BY id", root.table);
    let rows = sqlx::query(AssertSqlSafe(sql))
        .fetch_all(pool)
        .await
        .unwrap();
    rows.iter()
        .map(|row| {
            RowKey(
                root.wal_key
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let text: String = row.get(i);
                        let ty = if *name == "id" {
                            root.key_type
                        } else if *name == "name" {
                            KeyType::Text
                        } else {
                            KeyType::Int
                        };
                        (column(name), ty.value(&text))
                    })
                    .collect(),
            )
        })
        .collect()
}

/// A key no `plain` row has, for the misses.
fn missing_key(n: usize) -> RowKey {
    RowKey(vec![(
        column("id"),
        GenericValue::Int(1_000_000 + i32::try_from(n).unwrap()),
    )])
}

async fn resolve(
    builder: &PgDocumentBuilder,
    table: &str,
    keys: &[RowKey],
) -> Option<Vec<DocumentId>> {
    let mut ids = Vec::with_capacity(keys.len());
    for key in keys {
        match builder.resolve(&self::table(table), key, None, None).await {
            Ok(resolved) => ids.extend(resolved),
            Err(error) => {
                eprintln!("skipping `{table}`: resolving a root key failed: {error}");
                return None;
            }
        }
    }
    Some(ids)
}

struct Case {
    group: &'static str,
    name: String,
    ids: Vec<DocumentId>,
}

/// Whether the case builds exactly `hits` documents; a case that does not
/// would measure something else.
async fn builds(builder: &PgDocumentBuilder, case: &Case, hits: usize) -> bool {
    match builder.build_many(&case.ids).await {
        Ok(documents) => {
            let upserts = documents
                .iter()
                .filter(|d| matches!(d, Document::Upsert { .. }))
                .count();
            if documents.len() == case.ids.len() && upserts == hits {
                return true;
            }
            eprintln!(
                "skipping `{}/{}`: {} ids built {} documents, {upserts} upserts, expected {hits}",
                case.group,
                case.name,
                case.ids.len(),
                documents.len()
            );
        }
        Err(error) => eprintln!(
            "skipping `{}/{}`: build_many failed: {error}",
            case.group, case.name
        ),
    }
    false
}

async fn setup() -> (
    Option<ContainerAsync<Postgres>>,
    PgDocumentBuilder,
    Vec<Case>,
) {
    let (container, url) = connect().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    let roots = roots();
    seed(&pool, &roots).await;
    let builder = PgDocumentBuilder::connect(&url, Arc::new(spec(&roots)))
        .await
        .unwrap();

    let mut keys = BTreeMap::new();
    for root in &roots {
        keys.insert(root.table, wal_keys(&pool, root).await);
    }
    let mut planned: Vec<(Case, usize)> = Vec::new();
    let plain = &keys["plain"];
    for &n in BATCHES {
        if let Some(ids) = resolve(&builder, "plain", plain.get(..n).unwrap()).await {
            planned.push((
                Case {
                    group: "batch",
                    name: n.to_string(),
                    ids,
                },
                n,
            ));
        }
    }
    for &percent in HIT_PERCENTS {
        let hits = 1_000 * percent / 100;
        let mut mixed: Vec<RowKey> = plain.get(..hits).unwrap().to_vec();
        mixed.extend((hits..1_000).map(missing_key));
        if let Some(ids) = resolve(&builder, "plain", &mixed).await {
            planned.push((
                Case {
                    group: "hit_ratio",
                    name: percent.to_string(),
                    ids,
                },
                hits,
            ));
        }
    }
    for root in &roots {
        let names = [
            ("key", key_case(root.table)),
            ("root", shape_case(root.table)),
        ];
        for (group, name) in names {
            let Some(name) = name else { continue };
            let wanted = keys[root.table].get(..ROWS as usize).unwrap();
            if let Some(ids) = resolve(&builder, root.table, wanted).await {
                planned.push((
                    Case {
                        group,
                        name: name.to_owned(),
                        ids,
                    },
                    wanted.len(),
                ));
            }
        }
    }
    let mut cases = Vec::with_capacity(planned.len());
    for (case, hits) in planned {
        if builds(&builder, &case, hits).await {
            cases.push(case);
        }
    }
    (container, builder, cases)
}

fn bench(c: &mut Criterion) {
    let rt = runtime();
    // Keep the runtime entered so the container's async teardown has one.
    let _guard = rt.enter();
    let (_container, builder, cases) = rt.block_on(setup());

    for group_name in ["batch", "hit_ratio", "key", "root"] {
        let mut group = c.benchmark_group(group_name);
        group.sample_size(20);
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(4));
        for case in cases.iter().filter(|case| case.group == group_name) {
            group.throughput(Throughput::Elements(case.ids.len() as u64));
            group.bench_with_input(
                BenchmarkId::from_parameter(&case.name),
                &case.ids,
                |b, ids| {
                    b.to_async(&rt)
                        .iter(|| async { builder.build_many(ids).await.unwrap() });
                },
            );
        }
        group.finish();
    }
}

/// One index per root: `id`, `name`, and the `items` it has many of.
fn spec(roots: &[Root]) -> SourceSpec {
    let indexes = roots
        .iter()
        .map(|root| {
            let items = Field {
                field: field("items"),
                options: Default::default(),
                emit: true,
                source: FieldSource::Relation(Relation::Join(Join {
                    table: table(&format!("{}_items", root.table)),
                    primary_key: column("id"),
                    nullable: false,
                    kind: JoinKind::HasMany {
                        foreign_key: column("parent"),
                    },
                    filters: None,
                    order_by: None,
                    limit: None,
                    fields: vec![column_field("id"), column_field("label")],
                    indexed: true,
                    on: vec![],
                })),
            };
            let schema = IndexSchema {
                version: 1,
                table: table(root.table),
                db_schema: DatabaseSchema::try_new("public").unwrap(),
                primary_key: Some(column("id")),
                doc_id: None,
                soft_delete: None,
                filters: None,
                fields: vec![column_field("id"), column_field("name"), items],
            };
            (IndexName::try_new(root.table).unwrap(), schema)
        })
        .collect();
    SourceSpec::new(indexes)
}

fn column_field(name: &str) -> Field {
    Field {
        field: field(name),
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

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}
fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}
fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

criterion_group!(benches, bench);
criterion_main!(benches);
