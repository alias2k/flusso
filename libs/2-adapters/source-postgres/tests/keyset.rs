//! A backfill pages an index's root keys (`DocumentBuilder::keys`, issue
//! #386 stage 3): every row exactly once, in the order the primary key's
//! index sorts, for every key type a key may have, on every root shape the
//! review cases cover, at any page size, and with no lock held between pages.
//!
//! The order is checked through the built documents: each row's `name` is the
//! text its key was inserted as, so the names of the documents built from the
//! pages, in page order, must be `SELECT name … ORDER BY id`. That also proves
//! each key read back and bound again finds its own row.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-source-postgres --test keyset --run-ignored all
//! ```

#![allow(clippy::unwrap_used, clippy::print_stderr, unused_crate_dependencies)]

#[path = "../benches/cases.rs"]
#[allow(dead_code, unreachable_pub, unused_attributes, unused_imports)]
mod cases;

use std::sync::Arc;
use std::time::Duration;

use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, FlussoType, GenericValue,
    IndexName, IndexSchema, TableName,
};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceSpec};
use source_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// Page sizes every walk is taken at: one key, a few, and the engine's page.
const PAGES: &[usize] = &[1, 2, 7, 1024];

/// Every key type `KEY_TYPES` holds, spelled as a column, plus the shapes a
/// key takes beside its type: a collation, a domain.
const KEY_COLUMNS: &[(&str, &str)] = &[
    ("t_int2", "int2"),
    ("t_int4", "int4"),
    ("t_int8", "int8"),
    ("t_numeric", "numeric"),
    ("t_numeric_scaled", "numeric(10,2)"),
    ("t_real", "real"),
    ("t_float8", "float8"),
    ("t_bool", "bool"),
    ("t_text", "text"),
    ("t_text_c", "text COLLATE \"C\""),
    ("t_text_ci", "text COLLATE ci"),
    ("t_varchar", "varchar(20)"),
    ("t_char", "char(5)"),
    ("t_name", "name"),
    ("t_citext", "citext"),
    ("t_date", "date"),
    ("t_time", "time"),
    ("t_timestamp", "timestamp"),
    ("t_timestamptz", "timestamptz"),
    ("t_uuid", "uuid"),
    ("t_bytea", "bytea"),
    ("t_domain", "big_id"),
];

/// Texts inserted into every table; each keeps those its key type accepts.
const STORED: &[&str] = &[
    "0",
    "1",
    "-1",
    "2",
    "10",
    "1.5",
    "1.50",
    "2.5",
    "-0",
    "1e+20",
    "1e-05",
    "Infinity",
    "-Infinity",
    "NaN",
    "3000000000",
    "123456789012345678901234567890.5",
    "true",
    "false",
    "abc",
    "ABC",
    "aBd",
    "a",
    "B",
    "b ",
    "é",
    "e",
    "",
    "2020-01-02",
    "2020-01-03",
    "2020-01-02 10:00:00",
    "2020-01-02 10:00:00.5",
    "2020-01-02 10:00:00+05",
    "0044-03-15 BC",
    "0044-03-15 10:00:00 BC",
    "10000-01-01",
    "10000-01-01 00:00:00",
    "infinity",
    "-infinity",
    "10:00:00",
    "10:00:00.5",
    "24:00:00",
    "0b7f2d4e-1c2a-4d5e-8f90-1a2b3c4d5e6f",
    "1c8e3f5a-2d3b-4e6f-9a01-2b3c4d5e6f70",
    "\\x",
    "\\x00",
    "\\x0102",
    "\\xff",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn every_key_type_is_paged_once_in_key_order() {
    let container = test_postgres::image().start().await.unwrap();
    let url = url(&container).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "ALTER DATABASE postgres SET timezone = 'Asia/Tokyo'",
        "CREATE EXTENSION citext",
        "CREATE COLLATION ci (provider = icu, locale = 'und-u-ks-level2', deterministic = false)",
        "CREATE DOMAIN big_id AS int8",
    ] {
        execute(&pool, statement).await.unwrap();
    }

    let mut tables: Vec<(String, String)> = KEY_COLUMNS
        .iter()
        .map(|(table, ty)| ((*table).to_owned(), (*ty).to_owned()))
        .collect();
    for (table, ty) in KEY_COLUMNS {
        tables.push((format!("h{table}"), format!("{ty}|hash")));
    }
    tables.push(("r_int8".to_owned(), "int8|range".to_owned()));
    tables.push(("r_timestamptz".to_owned(), "timestamptz|range".to_owned()));

    for (table, spec) in &tables {
        let (ty, partition) = spec.split_once('|').unwrap_or((spec, ""));
        create_keyed_table(&pool, table, ty, partition).await;
        for text in STORED {
            let quoted = text.replace('\'', "''");
            let _ = execute(
                &pool,
                &format!("INSERT INTO {table} VALUES ('{quoted}', '{quoted}')"),
            )
            .await;
        }
        execute(&pool, &format!("ANALYZE {table}")).await.unwrap();
    }

    let names: Vec<&str> = tables.iter().map(|(table, _)| table.as_str()).collect();
    let builder = PgDocumentBuilder::connect(&url, Arc::new(spec(&names)))
        .await
        .unwrap();
    for table in &names {
        let expected: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT name FROM {table} ORDER BY id"
        )))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(expected.len() > 1, "`{table}` kept {} rows", expected.len());
        for &limit in PAGES {
            let keys = walk(&builder, table, limit).await;
            let read = names_of(&builder, table, &keys).await;
            assert_eq!(
                read, expected,
                "`{table}` paged {limit} at a time read another set or order of rows"
            );
        }
    }
}

/// A `citext` installed in a schema off the search path: `>` between two of
/// its values then means `text`'s, not the index's, so the walk must compare
/// and order by one operator, or it skips rows the index order and the `text`
/// order disagree on. And a domain over it that the role may not resolve:
/// the key is cast to the domain, as the builder casts every key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_key_whose_operators_are_off_the_search_path_is_paged_once() {
    let container = test_postgres::image().start().await.unwrap();
    let url = url(&container).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE SCHEMA ext",
        "CREATE EXTENSION citext SCHEMA ext",
        "CREATE SCHEMA app",
        "CREATE DOMAIN app.dcit AS ext.citext",
        "CREATE TABLE off_path (id ext.citext PRIMARY KEY, name text)",
        "CREATE TABLE off_path_domain (id app.dcit PRIMARY KEY, name text)",
        "CREATE ROLE limited LOGIN PASSWORD 'limited'",
        "GRANT USAGE ON SCHEMA app TO limited",
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO limited",
    ] {
        execute(&pool, statement).await.unwrap();
    }
    for table in ["off_path", "off_path_domain"] {
        execute(
            &pool,
            &format!(
                "INSERT INTO {table} SELECT k, k FROM unnest(ARRAY['a', 'B', 'c', 'D', 'e', \
                 'F', 'aa', 'AB', 'b1', 'Z', 'z1', '_', '0']) k"
            ),
        )
        .await
        .unwrap();
    }

    // A role may read a column of a type it can't name only through a
    // domain it can: the builder casts a key to the declared type then.
    let limited = url.replace("postgres:postgres@", "limited:limited@");
    let all: &[&str] = &["off_path", "off_path_domain"];
    for (connect, as_whom, tables) in [
        (&url, "the owner", all),
        (&limited, "a role without `ext`", &all[1..]),
    ] {
        let builder = PgDocumentBuilder::connect(connect, Arc::new(spec(tables)))
            .await
            .unwrap();
        for &table in tables {
            let mut expected: Vec<String> =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT name FROM {table}")))
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            expected.sort();
            for &limit in PAGES {
                let keys = walk(&builder, table, limit).await;
                let mut read = names_of(&builder, table, &keys).await;
                read.sort();
                assert_eq!(
                    read, expected,
                    "`{table}` paged {limit} at a time as {as_whom} read another set of rows"
                );
            }
        }
    }
}

/// The differential against the cursor the backfill read before: for every
/// root of the review cases, the pages read the keys a change carries (the
/// key's text typed by its column, as the WAL decoder reads it), each once,
/// and every one builds. A root whose declared key isn't its single
/// catalog primary key (composite, keyless) is walked as the cursor walked
/// it: not at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn every_review_root_is_paged_as_the_cursor_read_it() {
    let container = test_postgres::image().start().await.unwrap();
    let url = url(&container).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    let roots = cases::roots();
    cases::seed(&pool, &roots).await;
    let builder = PgDocumentBuilder::connect(&url, Arc::new(cases::spec(&roots)))
        .await
        .unwrap();
    for root in &roots {
        let expected = if root.wal_key() == ["id"] {
            cases::wal_keys(&pool, root).await
        } else {
            Vec::new()
        };
        for &limit in &[7, 1024] {
            // `wal_keys` reads them in the order of their text; the order of
            // the pages is `every_key_type_is_paged_once_in_key_order`'s.
            let keys = sorted(walk(&builder, root.table, limit).await);
            assert_eq!(
                keys,
                sorted(expected.clone()),
                "`{}` paged {limit} at a time read other keys than the cursor did",
                root.table
            );
        }
        let documents = build(&builder, root.table, &expected).await;
        assert!(
            documents
                .iter()
                .all(|document| matches!(document, Document::Upsert { .. })),
            "a key `{}` paged builds no document",
            root.table
        );
    }
}

/// No lock outlives a page: a migration on the root between two pages runs at
/// once. The cursor's transaction held its `ACCESS SHARE` until the whole
/// snapshot ended, so the `ALTER` waited out the backfill.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_migration_between_two_pages_waits_for_nothing() {
    let container = test_postgres::image().start().await.unwrap();
    let url = url(&container).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    execute(
        &pool,
        "CREATE TABLE migrated (id int8 PRIMARY KEY, name text)",
    )
    .await
    .unwrap();
    execute(
        &pool,
        "INSERT INTO migrated SELECT g, g::text FROM generate_series(1, 100) g",
    )
    .await
    .unwrap();
    let builder = PgDocumentBuilder::connect(&url, Arc::new(spec(&["migrated"])))
        .await
        .unwrap();
    let index = index_name("migrated");
    let through = builder.last_key(&index).await.unwrap().unwrap();
    let first = builder.keys(&index, None, &through, 10).await.unwrap();
    assert_eq!(first.len(), 10);

    let migrate = tokio::time::timeout(Duration::from_secs(5), async {
        execute(
            &pool,
            "SET lock_timeout = '1s'; ALTER TABLE migrated ADD COLUMN note text",
        )
        .await
    })
    .await;
    assert!(
        matches!(migrate, Ok(Ok(()))),
        "the migration waited on the backfill: {migrate:?}"
    );

    let rest = builder
        .keys(&index, first.last(), &through, 1024)
        .await
        .unwrap();
    assert_eq!(first.len() + rest.len(), 100);
}

fn sorted(mut keys: Vec<RowKey>) -> Vec<RowKey> {
    keys.sort_by_key(|key| format!("{key:?}"));
    keys
}

/// Every key of `table`'s index, `limit` at a time, as the engine walks
/// them: up to the greatest key when the walk starts.
async fn walk(builder: &PgDocumentBuilder, table: &str, limit: usize) -> Vec<RowKey> {
    walk_while(builder, table, limit, async |_: &[RowKey]| {}).await
}

/// [`walk`], running `between` after every page with the keys read so far.
async fn walk_while(
    builder: &PgDocumentBuilder,
    table: &str,
    limit: usize,
    mut between: impl AsyncFnMut(&[RowKey]),
) -> Vec<RowKey> {
    let index = index_name(table);
    let mut keys: Vec<RowKey> = Vec::new();
    let Some(through) = builder.last_key(&index).await.unwrap() else {
        return keys;
    };
    loop {
        let page = builder
            .keys(&index, keys.last(), &through, limit)
            .await
            .unwrap();
        assert!(page.len() <= limit, "a page of {limit} held {}", page.len());
        let last = page.len() < limit;
        keys.extend(page);
        if last {
            return keys;
        }
        between(&keys).await;
    }
}

async fn build(builder: &PgDocumentBuilder, table: &str, keys: &[RowKey]) -> Vec<Document> {
    let index = index_name(table);
    let ids: Vec<DocumentId> = keys
        .iter()
        .map(|key| DocumentId {
            index: index.clone(),
            key: key.clone(),
        })
        .collect();
    let mut documents = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(256) {
        let built = builder.build_many(chunk).await.unwrap();
        // `build_many` answers in any order: put it back in the page's.
        for id in chunk {
            let document = built
                .iter()
                .find(|document| document.id() == id)
                .unwrap_or_else(|| panic!("no document for {id:?}"));
            documents.push(document.clone());
        }
    }
    documents
}

/// The `name` of each key's document, in the keys' order.
async fn names_of(builder: &PgDocumentBuilder, table: &str, keys: &[RowKey]) -> Vec<String> {
    build(builder, table, keys)
        .await
        .into_iter()
        .map(|document| match document {
            Document::Upsert { body, id } => match body {
                GenericValue::Map(fields) => match fields.get("name") {
                    Some(GenericValue::String(name)) => name.clone(),
                    other => panic!("{id:?} has name {other:?}"),
                },
                other => panic!("{id:?} built {other:?}"),
            },
            other => panic!("a paged key built {other:?}"),
        })
        .collect()
}

async fn create_keyed_table(pool: &sqlx::PgPool, table: &str, ty: &str, partition: &str) {
    let create = format!("CREATE TABLE {table} (id {ty} PRIMARY KEY, name text)");
    match partition {
        "hash" => {
            execute(pool, &format!("{create} PARTITION BY HASH (id)"))
                .await
                .unwrap();
            for remainder in 0..4 {
                execute(
                    pool,
                    &format!(
                        "CREATE TABLE {table}_{remainder} PARTITION OF {table} \
                         FOR VALUES WITH (MODULUS 4, REMAINDER {remainder})"
                    ),
                )
                .await
                .unwrap();
            }
        }
        "range" => {
            execute(pool, &format!("{create} PARTITION BY RANGE (id)"))
                .await
                .unwrap();
            let bounds = if ty == "int8" {
                ["MINVALUE", "1", "100", "MAXVALUE"]
            } else {
                ["MINVALUE", "'2000-01-01'", "'2020-01-03'", "MAXVALUE"]
            };
            for (n, pair) in bounds.windows(2).enumerate() {
                execute(
                    pool,
                    &format!(
                        "CREATE TABLE {table}_{n} PARTITION OF {table} \
                         FOR VALUES FROM ({}) TO ({})",
                        pair[0], pair[1]
                    ),
                )
                .await
                .unwrap();
            }
        }
        _ => execute(pool, &create).await.unwrap(),
    }
}

async fn url(container: &test_postgres::Container) -> String {
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres")
}

async fn execute(pool: &sqlx::PgPool, sql: &str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .map(|_| ())
}

/// One index per table, keyed by `id`, holding `name`.
fn spec(tables: &[&str]) -> SourceSpec {
    SourceSpec::new(
        tables
            .iter()
            .map(|table| (index_name(table), schema(table)))
            .collect(),
    )
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

/// The walk ends at the greatest key it found when it started (review of
/// #386 stage 3): a writer adding a page of greater keys after every page
/// read, and the bound's own row deleted mid-walk, both end it where it
/// began, with every row that was there and is still there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_walk_ends_at_the_greatest_key_it_found_when_it_started() {
    let container = test_postgres::image().start().await.unwrap();
    let url = url(&container).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for table in ["grows", "shrinks"] {
        execute(
            &pool,
            &format!(
                "CREATE TABLE {table} (id int8 PRIMARY KEY, name text); \
                 INSERT INTO {table} SELECT g, g::text FROM generate_series(1, 10) g"
            ),
        )
        .await
        .unwrap();
    }
    let builder = PgDocumentBuilder::connect(&url, Arc::new(spec(&["grows", "shrinks"])))
        .await
        .unwrap();

    let mut added = 0;
    let keys = walk_while(&builder, "grows", 2, async |_: &[RowKey]| {
        for _ in 0..2 {
            added += 1;
            execute(
                &pool,
                &format!("INSERT INTO grows VALUES ({}, 'new')", 100 + added),
            )
            .await
            .unwrap();
        }
    })
    .await;
    assert_eq!(names_of(&builder, "grows", &keys).await, names(1..=10));
    assert_eq!(
        added, 10,
        "a page of greater keys after each of the five full pages"
    );

    let mut deleted = false;
    let keys = walk_while(&builder, "shrinks", 3, async |_: &[RowKey]| {
        if !deleted {
            deleted = true;
            execute(&pool, "DELETE FROM shrinks WHERE id = 10")
                .await
                .unwrap();
        }
    })
    .await;
    assert_eq!(names_of(&builder, "shrinks", &keys).await, names(1..=9));
}

fn names(range: std::ops::RangeInclusive<i64>) -> Vec<String> {
    let mut names: Vec<String> = range.map(|n| n.to_string()).collect();
    names.sort_by_key(|name| name.parse::<i64>().unwrap());
    names
}

/// The walk reads a key whose operators sit off the search path through its
/// primary key index, page after page: no page scans the table.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_key_whose_operators_are_off_the_search_path_is_walked_through_its_index() {
    let container = test_postgres::image().start().await.unwrap();
    let url = url(&container).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE SCHEMA ext",
        "CREATE EXTENSION citext SCHEMA ext",
        "CREATE TABLE scanned (id ext.citext PRIMARY KEY, name text)",
        "INSERT INTO scanned SELECT 'k' || g, g::text FROM generate_series(1, 5000) g",
        "ANALYZE scanned",
    ] {
        execute(&pool, statement).await.unwrap();
    }
    let builder = PgDocumentBuilder::connect(&url, Arc::new(spec(&["scanned"])))
        .await
        .unwrap();
    let scans = async || -> (i64, i64) {
        sqlx::query_as(
            "SELECT coalesce(seq_scan, 0), coalesce(idx_scan, 0) FROM pg_stat_user_tables \
             WHERE relname = 'scanned'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    // The setup's own counters (the primary key's build scans the table
    // once) arrive when its session reports them: wait for its inserts.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let inserted: i64 = sqlx::query_scalar(
            "SELECT coalesce(n_tup_ins, 0) FROM pg_stat_user_tables WHERE relname = 'scanned'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        if inserted == 5000 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the setup's statistics never arrived"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let (seq_before, idx_before) = scans().await;
    let keys = walk(&builder, "scanned", 100).await;
    assert_eq!(keys.len(), 5000);
    // A backend reports its table counters at the latest once idle for ten
    // seconds; the walk's index scans showing up means its seq scans would.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let (seq_after, idx_after) = loop {
        let (seq, idx) = scans().await;
        if idx >= idx_before + 51 || std::time::Instant::now() > deadline {
            break (seq, idx);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    assert!(
        idx_after >= idx_before + 51,
        "the bound and every page read the index: {idx_before} → {idx_after}"
    );
    assert_eq!(
        seq_after, seq_before,
        "no page scanned the table (index scans {idx_before} → {idx_after})"
    );
}
