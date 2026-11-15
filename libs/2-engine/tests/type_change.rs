//! A column an index reads changes type while the pipeline runs (issue #335):
//! every change still ends as the right document, and the engines recover by
//! themselves, with no restart of the process.
//!
//! The builder's pool holds several connections, each with statements
//! prepared before the change: a concurrent reader resolves and builds on the
//! same builder before, during and after each `ALTER`, so the engine's own
//! queries land on connections that served the old type. Changes are written
//! right before and right after each `ALTER`, so some are still in flight
//! when it lands.
//!
//! The types changed, in order: a `has_many` link (`bigint` → `int`), the
//! root key (`int` → `bigint`), a junction key, a `via` hop's key, a
//! `belongs_to` link, a `numeric` key's precision, a related table dropped
//! and recreated with a `bigint` key, the link again (→ `numeric`), and the
//! root key back to `int`.
//!
//! A link changed to a type its key has no `=` for (`text` against `int`) is
//! a schema the database no longer matches: no cache can fix it, every build
//! of the index fails the same way, validation reports it, and the engine
//! recovers by itself once the column is changed back.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --test type_change --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::{IngestEngine, SinkControl, SinkEngine};
use kernel::{
    Aggregate, AggregateKey, AggregateOp, DatabaseSchema, Envelope, Field, FieldSource, FlussoType,
    GenericValue, IndexSchema, Join, JoinKind, OrderBy, Relation, SinkName, Through, ViaHop,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sink::{Result as SinkResult, Sink};
use source::cdc::ChangeCapture;
use source::document::{DocumentBuilder, DocumentId};
use source::{DiagnosticKind, RowKey, SourceSpec};
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::postgres::PgPoolOptions;
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::task::JoinSet;

mod common;
use common::{column, field, index_name, start_postgres, table};

/// How many concurrent reads the reader runs: more than one connection's
/// worth, so the builder's pool opens several.
const READERS: usize = 6;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn every_change_lands_across_column_type_changes_without_a_restart() {
    let (_postgres, url, port) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE teams (id int PRIMARY KEY, name text)",
        "CREATE TABLE users (id int PRIMARY KEY, team_id int, name text)",
        "CREATE TABLE posts (id int PRIMARY KEY, user_id int, title text)",
        "CREATE TABLE events (id int PRIMARY KEY, user_id bigint, kind text)",
        "CREATE TABLE tags (id int PRIMARY KEY, label text)",
        "CREATE TABLE user_tags (user_id int, tag_id int, PRIMARY KEY (user_id, tag_id))",
        "CREATE TABLE projects (id int PRIMARY KEY, user_id int)",
        "CREATE TABLE tasks (id int PRIMARY KEY, project_id int)",
        "CREATE TABLE ledgers (id numeric PRIMARY KEY, user_id int, amount bigint)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
        "INSERT INTO teams VALUES (1, 'red')",
        "INSERT INTO users VALUES (1, 1, 'ada'), (2, 1, 'bob')",
        "INSERT INTO posts VALUES (1, 1, 'p1')",
        "INSERT INTO events VALUES (1, 1, 'e1')",
        "INSERT INTO tags VALUES (1, 't1'), (2, 't2')",
        "INSERT INTO user_tags VALUES (1, 1)",
        "INSERT INTO projects VALUES (1, 1)",
        "INSERT INTO tasks VALUES (1, 1)",
        "INSERT INTO ledgers VALUES (1.5, 1, 10)",
    ] {
        execute(&pool, statement).await;
    }
    create_slot(&pool).await;

    let spec = Arc::new(users_spec());
    let documents = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    let capture = WalChangeCapture::new(replication(port), url.clone())
        .with_column_types(documents.column_types());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let restarts = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(supervise(
        Arc::new(capture),
        Arc::new(documents.clone()),
        Arc::new(DocumentSink(Arc::clone(&recorded))),
        Arc::clone(&restarts),
        Arc::clone(&failures),
    ));
    let mut user = User::default();
    document_reaches(&mut engine, &recorded, "1", &user.document()).await;

    let mut types = Types::default();
    let steps: [Step; 10] = [
        Step {
            before: &["INSERT INTO events VALUES (2, 1, 'e2')"],
            alter: "ALTER TABLE events ALTER COLUMN user_id TYPE int",
            after: &["INSERT INTO events VALUES (3, 1, 'e3')"],
            apply: |user, _| user.events.extend(["e2", "e3"]),
        },
        Step {
            before: &["UPDATE users SET name = 'ada2' WHERE id = 1"],
            alter: "ALTER TABLE users ALTER COLUMN id TYPE bigint",
            after: &[
                "UPDATE users SET name = 'ada3' WHERE id = 1",
                "UPDATE users SET name = 'bob2' WHERE id = 2",
            ],
            apply: |user, types| {
                user.name = "ada3";
                types.users_id_bigint = true;
            },
        },
        Step {
            before: &["INSERT INTO user_tags VALUES (1, 2)"],
            alter: "ALTER TABLE user_tags ALTER COLUMN user_id TYPE bigint",
            after: &[
                "DELETE FROM user_tags WHERE tag_id = 1",
                "UPDATE tags SET label = 't2b' WHERE id = 2",
            ],
            apply: |user, types| {
                user.tags = vec!["t2b"];
                types.user_tags_user_id_bigint = true;
            },
        },
        Step {
            before: &["INSERT INTO tasks VALUES (2, 1)"],
            alter: "ALTER TABLE projects ALTER COLUMN user_id TYPE bigint",
            after: &["INSERT INTO tasks VALUES (3, 1)"],
            apply: |user, _| user.task_count = 3,
        },
        Step {
            before: &["UPDATE teams SET name = 'blue' WHERE id = 1"],
            alter: "ALTER TABLE users ALTER COLUMN team_id TYPE bigint",
            after: &["UPDATE teams SET name = 'green' WHERE id = 1"],
            apply: |user, _| user.team = "green",
        },
        Step {
            before: &["UPDATE ledgers SET amount = 11"],
            alter: "ALTER TABLE ledgers ALTER COLUMN id TYPE numeric(10, 2)",
            after: &["UPDATE ledgers SET amount = 12"],
            apply: |user, _| user.ledgers = vec![12],
        },
        Step {
            before: &["UPDATE tags SET label = 't2c' WHERE id = 2"],
            alter: "DROP TABLE tags; CREATE TABLE tags (id bigint PRIMARY KEY, label text)",
            after: &[
                "INSERT INTO tags VALUES (2, 't2d')",
                "UPDATE tags SET label = 't2e' WHERE id = 2",
            ],
            apply: |user, types| {
                user.tags = vec!["t2e"];
                types.tags_id_bigint = true;
            },
        },
        Step {
            before: &["INSERT INTO events VALUES (4, 1, 'e4')"],
            alter: "ALTER TABLE events ALTER COLUMN user_id TYPE numeric",
            after: &["INSERT INTO events VALUES (5, 1, 'e5')"],
            apply: |user, _| user.events.extend(["e4", "e5"]),
        },
        Step {
            before: &["UPDATE users SET name = 'ada4' WHERE id = 1"],
            alter: "ALTER TABLE users ALTER COLUMN id TYPE int",
            after: &["UPDATE users SET name = 'ada5' WHERE id = 1"],
            apply: |user, types| {
                user.name = "ada5";
                types.users_id_bigint = false;
            },
        },
        Step {
            before: &[],
            alter: "BEGIN; UPDATE users SET name = 'ada6' WHERE id = 1; \
                    ALTER TABLE users ALTER COLUMN id TYPE bigint; \
                    UPDATE users SET name = 'ada7' WHERE id = 1; \
                    UPDATE users SET name = 'bob3' WHERE id = 2; COMMIT",
            after: &[],
            apply: |user, types| {
                user.name = "ada7";
                types.users_id_bigint = true;
            },
        },
    ];
    for step in steps {
        read_concurrently(&documents, types).await.unwrap();
        let reading = tokio::spawn(read_until_stopped(documents.clone(), types));
        for statement in step.before {
            execute(&pool, statement).await;
        }
        execute(&pool, step.alter).await;
        for statement in step.after {
            execute(&pool, statement).await;
        }
        (step.apply)(&mut user, &mut types);
        document_reaches(&mut engine, &recorded, "1", &user.document()).await;
        reading.abort();
        read_concurrently(&documents, types)
            .await
            .unwrap_or_else(|error| panic!("after `{}`: {error}", step.alter));
    }
    let bob = json!({
        "name": "bob3", "team": { "name": "green" }, "posts": [], "events": [],
        "tags": [], "taskCount": 0, "ledgers": [],
    });
    document_reaches(&mut engine, &recorded, "2", &bob).await;

    let deleted: Vec<String> = recorded
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, document)| document.is_null())
        .map(|(id, _)| id.clone())
        .collect();
    assert!(deleted.is_empty(), "no user was deleted: {deleted:?}");
    let restarted = restarts.load(Ordering::SeqCst);
    assert_eq!(
        restarted,
        0,
        "the engine restarted {restarted} times: {:?}",
        failures.lock().unwrap()
    );
    engine.abort();
}

/// Not a stale statement: this passes on the code before issue #335 too. It
/// pins what a real mismatch does: every build fails with Postgres's
/// message, which names the index and `flusso check`; the engine's
/// supervised retries fail the same way; validation reports it; and once
/// the column is changed back the next retry builds, with no restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_link_changed_to_a_type_its_key_cannot_equal_fails_until_changed_back() {
    let (_postgres, url, port) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE users (id int PRIMARY KEY, name text)",
        "CREATE TABLE posts (id int PRIMARY KEY, user_id int, title text)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
        "INSERT INTO users VALUES (1, 'ada')",
        "INSERT INTO posts VALUES (1, 1, 'p1')",
    ] {
        execute(&pool, statement).await;
    }
    create_slot(&pool).await;
    let spec = Arc::new(SourceSpec::new(BTreeMap::from([(
        index_name("users"),
        posts_only_schema(),
    )])));
    let documents = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    let capture = WalChangeCapture::new(replication(port), url.clone())
        .with_column_types(documents.column_types());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let restarts = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(supervise(
        Arc::new(capture),
        Arc::new(documents),
        Arc::new(DocumentSink(Arc::clone(&recorded))),
        Arc::clone(&restarts),
        Arc::clone(&failures),
    ));
    let posts = |titles: &[&str]| {
        json!({
            "name": "ada",
            "posts": titles.iter().map(|title| json!({ "title": title })).collect::<Vec<_>>(),
        })
    };
    document_reaches(&mut engine, &recorded, "1", &posts(&["p1"])).await;

    execute(&pool, "ALTER TABLE posts ALTER COLUMN user_id TYPE text").await;
    execute(&pool, "INSERT INTO posts VALUES (2, '1', 'p2')").await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while restarts.load(Ordering::SeqCst) < 2 {
        assert!(Instant::now() < deadline, "the engine never failed");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for failure in failures.lock().unwrap().iter() {
        assert!(no_equality(failure), "{failure}");
        assert!(
            failure.contains("no longer matches index `users`; `flusso check` names the field"),
            "{failure}"
        );
    }
    let fresh = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    let error = fresh
        .document_by_id(&index_name("users"), "1")
        .await
        .unwrap_err()
        .to_string();
    assert!(
        no_equality(&error),
        "a new connection fails the same way: {error}"
    );
    let diagnostics = source::validate_indexes(&spec, &fresh).await.unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == DiagnosticKind::LinkTypeMismatch),
        "{diagnostics:?}"
    );

    execute(
        &pool,
        "ALTER TABLE posts ALTER COLUMN user_id TYPE int USING user_id::int",
    )
    .await;
    execute(&pool, "INSERT INTO posts VALUES (3, 1, 'p3')").await;
    document_reaches(&mut engine, &recorded, "1", &posts(&["p1", "p2", "p3"])).await;
    engine.abort();
}

/// A key column resized under a running engine (issue #335): a row keyed by
/// a value only the new size holds is built, never written as a tombstone.
/// An explicit cast to the old size (`varchar(5)`, `numeric(10,2)`,
/// `char(5)`) truncates or rounds without an error, so a key cast that way
/// matches no row.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_resized_key_column_keeps_every_document() {
    let (_postgres, url, port) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE vkeys (id varchar(5) PRIMARY KEY, name text)",
        "CREATE TABLE nkeys (id numeric(10, 2) PRIMARY KEY, name text)",
        "CREATE TABLE ckeys (id char(5) PRIMARY KEY, name text)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
        "INSERT INTO vkeys VALUES ('v', 'v1')",
        "INSERT INTO nkeys VALUES (1.5, 'n1')",
        "INSERT INTO ckeys VALUES ('c', 'c1')",
    ] {
        execute(&pool, statement).await;
    }
    create_slot(&pool).await;
    let spec = Arc::new(SourceSpec::new(BTreeMap::from(
        ["vkeys", "nkeys", "ckeys"].map(|name| (index_name(name), named(name))),
    )));
    let documents = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    let capture = WalChangeCapture::new(replication(port), url.clone())
        .with_column_types(documents.column_types());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let restarts = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(supervise(
        Arc::new(capture),
        Arc::new(documents),
        Arc::new(DocumentSink(Arc::clone(&recorded))),
        Arc::clone(&restarts),
        Arc::clone(&failures),
    ));
    let name = |name: &str| json!({ "name": name });
    for (id, value) in [("v", "v1"), ("1.50", "n1"), ("c    ", "c1")] {
        document_reaches(&mut engine, &recorded, id, &name(value)).await;
    }

    for (alter, insert, id, value) in [
        (
            "ALTER TABLE vkeys ALTER COLUMN id TYPE varchar(10)",
            "INSERT INTO vkeys VALUES ('k12345678', 'v2')",
            "k12345678",
            "v2",
        ),
        (
            "ALTER TABLE vkeys ALTER COLUMN id TYPE varchar(9)",
            "INSERT INTO vkeys VALUES ('k23456789', 'v3')",
            "k23456789",
            "v3",
        ),
        (
            "ALTER TABLE nkeys ALTER COLUMN id TYPE numeric(12, 4)",
            "INSERT INTO nkeys VALUES (1.2345, 'n2')",
            "1.2345",
            "n2",
        ),
        (
            "ALTER TABLE ckeys ALTER COLUMN id TYPE char(10)",
            "INSERT INTO ckeys VALUES ('abcdefgh', 'c2')",
            "abcdefgh  ",
            "c2",
        ),
    ] {
        execute(&pool, alter).await;
        execute(&pool, insert).await;
        document_reaches(&mut engine, &recorded, id, &name(value)).await;
    }

    assert_eq!(tombstones(&recorded), Vec::<String>::new());
    assert_eq!(
        restarts.load(Ordering::SeqCst),
        0,
        "{:?}",
        failures.lock().unwrap()
    );
    engine.abort();
}

/// A change on a `numeric` key read before the column took a scale
/// (`numeric` → `numeric(10,2)`) carries `1.5` where the row now reads
/// `1.50`. The two are equal, so the change is built, under the `_id` it
/// carries (`1.5`), as a single-document build of it would be, and never
/// written as a tombstone for a row that exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_numeric_key_read_before_its_scale_changed_keeps_its_document() {
    let (_postgres, url, port) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE nums (id numeric PRIMARY KEY, name text)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
        "INSERT INTO nums VALUES (1.5, 'a')",
    ] {
        execute(&pool, statement).await;
    }
    create_slot(&pool).await;
    execute(&pool, "UPDATE nums SET name = 'b' WHERE id = 1.5").await;
    execute(
        &pool,
        "ALTER TABLE nums ALTER COLUMN id TYPE numeric(10, 2)",
    )
    .await;

    let spec = Arc::new(SourceSpec::new(BTreeMap::from([(
        index_name("nums"),
        named("nums"),
    )])));
    let documents = PgDocumentBuilder::connect(&url, Arc::clone(&spec))
        .await
        .unwrap();
    let single = documents
        .document_by_id(&index_name("nums"), "1.5")
        .await
        .unwrap();
    assert!(
        matches!(single, Some(Ok(_))),
        "a single build finds the row: {single:?}"
    );
    let capture = WalChangeCapture::new(replication(port), url.clone())
        .with_column_types(documents.column_types());
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let restarts = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(Mutex::new(Vec::new()));
    let mut engine = tokio::spawn(supervise(
        Arc::new(capture),
        Arc::new(documents),
        Arc::new(DocumentSink(Arc::clone(&recorded))),
        Arc::clone(&restarts),
        Arc::clone(&failures),
    ));
    document_reaches(&mut engine, &recorded, "1.5", &json!({ "name": "b" })).await;
    assert_eq!(tombstones(&recorded), Vec::<String>::new());
    engine.abort();
}

/// A root key whose column changes kind, `int` → `numeric(10,2)` → `int`,
/// inside a migration that updates the row on both sides of the `ALTER`:
/// the batch holds the row keyed both ways (`Int(1)` and `Decimal(1.00)`),
/// and each is built, as a single-document build of it would be. None is a
/// tombstone for a row that exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_key_changing_between_integer_and_numeric_keeps_every_document() {
    let (_postgres, url, port) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE kinds (id int PRIMARY KEY, name text)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
        "INSERT INTO kinds VALUES (1, 'a1')",
    ] {
        execute(&pool, statement).await;
    }
    create_slot(&pool).await;
    let mut running = Running::start(&url, port, &url, "kinds").await;
    running.reaches("1", "a1").await;

    execute(
        &pool,
        "BEGIN; UPDATE kinds SET name = 'a2' WHERE id = 1; \
         ALTER TABLE kinds ALTER COLUMN id TYPE numeric(10, 2); \
         UPDATE kinds SET name = 'a3' WHERE id = 1; COMMIT",
    )
    .await;
    running.reaches("1.00", "a3").await;
    running.reaches("1", "a3").await;
    execute(
        &pool,
        "BEGIN; UPDATE kinds SET name = 'a4' WHERE id = 1; \
         ALTER TABLE kinds ALTER COLUMN id TYPE int; \
         UPDATE kinds SET name = 'a5' WHERE id = 1; COMMIT",
    )
    .await;
    running.reaches("1", "a5").await;
    running.reaches("1.00", "a5").await;
    running.finish();
}

/// The same for a key changing between `timestamp` and `timestamptz`, which
/// the builder's session (`TimeZone` UTC) converts at UTC.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_key_changing_between_timestamp_and_timestamptz_keeps_every_document() {
    let (_postgres, url, port) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE stamps (id timestamp PRIMARY KEY, name text)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
        "INSERT INTO stamps VALUES ('2020-01-02 10:00:00', 'a1')",
    ] {
        execute(&pool, statement).await;
    }
    create_slot(&pool).await;
    let naive = "2020-01-02 10:00:00";
    let zoned = "2020-01-02T10:00:00+00:00";
    let mut running = Running::start(&url, port, &url, "stamps").await;
    running.reaches(naive, "a1").await;

    execute(
        &pool,
        "BEGIN; UPDATE stamps SET name = 'a2'; \
         ALTER TABLE stamps ALTER COLUMN id TYPE timestamptz; \
         UPDATE stamps SET name = 'a3'; COMMIT",
    )
    .await;
    running.reaches(zoned, "a3").await;
    running.reaches(naive, "a3").await;
    execute(
        &pool,
        "BEGIN; UPDATE stamps SET name = 'a4'; \
         ALTER TABLE stamps ALTER COLUMN id TYPE timestamp; \
         UPDATE stamps SET name = 'a5'; COMMIT",
    )
    .await;
    running.reaches(naive, "a5").await;
    running.reaches(zoned, "a5").await;
    running.finish();
}

/// A key whose column is a domain over a type in a schema the builder's role
/// may not use (`app.dcit` over `ext.citext`, no USAGE on `ext`) is cast to
/// the domain, as before issue #335's fix, instead of to its base type: the
/// root key and a link of that type both index, backfill and live.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_key_of_a_domain_over_a_type_the_role_may_not_use_still_indexes() {
    let (_postgres, url, port) = start().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE SCHEMA ext",
        "CREATE EXTENSION citext SCHEMA ext",
        "CREATE SCHEMA app",
        "CREATE DOMAIN app.dcit AS ext.citext",
        "CREATE TABLE owners (id app.dcit PRIMARY KEY, name text)",
        "CREATE TABLE pets (id int PRIMARY KEY, owner_id app.dcit, name text)",
        "CREATE PUBLICATION flusso FOR ALL TABLES",
        "INSERT INTO owners VALUES ('ada', 'Ada')",
        "INSERT INTO pets VALUES (1, 'ada', 'rex')",
        "CREATE ROLE reader LOGIN PASSWORD 'reader'",
        "GRANT USAGE ON SCHEMA app TO reader",
        "GRANT SELECT ON owners, pets TO reader",
    ] {
        execute(&pool, statement).await;
    }
    create_slot(&pool).await;
    let reader = url.replace("postgres:postgres@", "reader:reader@");
    let spec = Arc::new(SourceSpec::new(BTreeMap::from([(
        index_name("owners"),
        owners_with_pets(),
    )])));
    let documents = PgDocumentBuilder::connect(&reader, spec).await.unwrap();
    let mut running = Running::with(documents, &url, port).await;
    let owner = |name: &str, pets: &[&str]| {
        json!({
            "name": name,
            "pets": pets.iter().map(|pet| json!({ "name": pet })).collect::<Vec<_>>(),
        })
    };
    document_reaches(
        &mut running.engine,
        &running.recorded,
        "ada",
        &owner("Ada", &["rex"]),
    )
    .await;

    execute(&pool, "INSERT INTO pets VALUES (2, 'ada', 'fido')").await;
    document_reaches(
        &mut running.engine,
        &running.recorded,
        "ada",
        &owner("Ada", &["rex", "fido"]),
    )
    .await;
    execute(&pool, "UPDATE owners SET name = 'Ada L' WHERE id = 'ada'").await;
    document_reaches(
        &mut running.engine,
        &running.recorded,
        "ada",
        &owner("Ada L", &["rex", "fido"]),
    )
    .await;
    running.finish();
}

/// `owners` with its `pets`, linked by `owner_id`.
fn owners_with_pets() -> IndexSchema {
    let mut schema = named("owners");
    schema.fields.push(relation(
        "pets",
        Relation::Join(join(
            "pets",
            JoinKind::HasMany {
                foreign_key: column("owner_id"),
            },
            vec![value_field("name", "name", FlussoType::Keyword)],
        )),
    ));
    schema
}

/// The engine running one deployment, recording what its sink receives.
struct Running {
    engine: tokio::task::JoinHandle<engine::Result<()>>,
    recorded: Arc<Mutex<Vec<(String, Value)>>>,
    restarts: Arc<AtomicUsize>,
    failures: Arc<Mutex<Vec<String>>>,
}

impl Running {
    /// Index `table` as `{ name }`, read over `builder_url`, captured over
    /// `capture_url`.
    async fn start(builder_url: &str, port: u16, capture_url: &str, table_name: &str) -> Self {
        let spec = Arc::new(SourceSpec::new(BTreeMap::from([(
            index_name(table_name),
            named(table_name),
        )])));
        let documents = PgDocumentBuilder::connect(builder_url, spec).await.unwrap();
        Self::with(documents, capture_url, port).await
    }

    async fn with(documents: PgDocumentBuilder, capture_url: &str, port: u16) -> Self {
        let capture = WalChangeCapture::new(replication(port), capture_url.to_owned())
            .with_column_types(documents.column_types());
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let restarts = Arc::new(AtomicUsize::new(0));
        let failures = Arc::new(Mutex::new(Vec::new()));
        let engine = tokio::spawn(supervise(
            Arc::new(capture),
            Arc::new(documents),
            Arc::new(DocumentSink(Arc::clone(&recorded))),
            Arc::clone(&restarts),
            Arc::clone(&failures),
        ));
        Self {
            engine,
            recorded,
            restarts,
            failures,
        }
    }

    async fn reaches(&mut self, id: &str, name: &str) {
        document_reaches(
            &mut self.engine,
            &self.recorded,
            id,
            &json!({ "name": name }),
        )
        .await;
    }

    /// Assert no tombstone was sent and the engine never restarted, then stop.
    fn finish(self) {
        assert_eq!(tombstones(&self.recorded), Vec::<String>::new());
        assert_eq!(
            self.restarts.load(Ordering::SeqCst),
            0,
            "{:?}",
            self.failures.lock().unwrap()
        );
        self.engine.abort();
    }
}

/// `table` indexed as `{ name }`, keyed by `id`.
fn named(table_name: &str) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table(table_name),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![value_field("name", "name", FlussoType::Keyword)],
    }
}

/// The ids a tombstone was recorded for.
fn tombstones(recorded: &Arc<Mutex<Vec<(String, Value)>>>) -> Vec<String> {
    recorded
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, document)| document.is_null())
        .map(|(id, _)| id.clone())
        .collect()
}

/// Whether `error` is Postgres finding no `=` between `text` and `integer`.
fn no_equality(error: &str) -> bool {
    error.contains("operator does not exist: text = integer")
        || error.contains("operator does not exist: integer = text")
}

/// One schema change: the changes written right before and right after it,
/// and what they do to user 1's document and to the key types.
struct Step {
    before: &'static [&'static str],
    alter: &'static str,
    after: &'static [&'static str],
    apply: fn(&mut User, &mut Types),
}

/// User 1's document, as the steps leave it.
#[derive(Debug, Clone)]
struct User {
    name: &'static str,
    team: &'static str,
    events: Vec<&'static str>,
    tags: Vec<&'static str>,
    task_count: u64,
    ledgers: Vec<u64>,
}

impl Default for User {
    fn default() -> Self {
        Self {
            name: "ada",
            team: "red",
            events: vec!["e1"],
            tags: vec!["t1"],
            task_count: 1,
            ledgers: vec![10],
        }
    }
}

impl User {
    fn document(&self) -> Value {
        let labelled = |key: &str, values: &[&str]| {
            values
                .iter()
                .map(|value| json!({ key: value }))
                .collect::<Vec<_>>()
        };
        json!({
            "name": self.name,
            "team": { "name": self.team },
            "posts": [{ "title": "p1" }],
            "events": labelled("kind", &self.events),
            "tags": labelled("label", &self.tags),
            "taskCount": self.task_count,
            "ledgers": self.ledgers.iter().map(|amount| json!({ "amount": amount })).collect::<Vec<_>>(),
        })
    }
}

/// Which key columns are `bigint` now, so the reader binds the type a
/// change of that table carries.
#[derive(Debug, Clone, Copy, Default)]
struct Types {
    users_id_bigint: bool,
    user_tags_user_id_bigint: bool,
    tags_id_bigint: bool,
}

impl Types {
    fn int(bigint: bool, value: i64) -> GenericValue {
        if bigint {
            GenericValue::BigInt(value)
        } else {
            GenericValue::Int(i32::try_from(value).unwrap())
        }
    }
}

/// Resolve a change on every table the index reads and build both users,
/// [`READERS`] times at once, so several pooled connections prepare each
/// statement.
async fn read_concurrently(documents: &PgDocumentBuilder, types: Types) -> Result<(), String> {
    let mut reads = JoinSet::new();
    for _ in 0..READERS {
        let documents = documents.clone();
        reads.spawn(async move { read_once(&documents, types).await });
    }
    while let Some(read) = reads.join_next().await {
        read.unwrap()?;
    }
    Ok(())
}

/// [`read_concurrently`] in a loop, ignoring failures: reads that hold
/// connections while the schema changes under them.
async fn read_until_stopped(documents: PgDocumentBuilder, types: Types) {
    loop {
        let _ = read_concurrently(&documents, types).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn read_once(documents: &PgDocumentBuilder, types: Types) -> Result<(), String> {
    let key = |name: &str, value: GenericValue| RowKey(vec![(column(name), value)]);
    let changes = [
        ("users", key("id", Types::int(types.users_id_bigint, 1))),
        ("teams", key("id", GenericValue::Int(1))),
        ("posts", key("id", GenericValue::Int(1))),
        ("events", key("id", GenericValue::Int(1))),
        ("tags", key("id", Types::int(types.tags_id_bigint, 2))),
        (
            "user_tags",
            RowKey(vec![
                (
                    column("user_id"),
                    Types::int(types.user_tags_user_id_bigint, 1),
                ),
                (column("tag_id"), GenericValue::Int(2)),
            ]),
        ),
        ("tasks", key("id", GenericValue::Int(1))),
        (
            "ledgers",
            key("id", GenericValue::Decimal(Decimal::new(15, 1))),
        ),
    ];
    for (name, key) in &changes {
        documents
            .resolve(&table(name), key, None, None)
            .await
            .map_err(|error| format!("resolving a change on `{name}`: {error}"))?;
    }
    let ids: Vec<DocumentId> = [1, 2]
        .into_iter()
        .map(|id| DocumentId {
            index: index_name("users"),
            key: key("id", Types::int(types.users_id_bigint, id)),
        })
        .collect();
    documents
        .build_many(&ids)
        .await
        .map_err(|error| format!("building users: {error}"))?;
    Ok(())
}

/// `users` with its team, posts, events, tags, a count of its projects'
/// tasks and its ledgers.
fn users_spec() -> SourceSpec {
    let has_many = |name: &str, target: &str, value: Field| {
        relation(
            name,
            Relation::Join(join(
                target,
                JoinKind::HasMany {
                    foreign_key: column("user_id"),
                },
                vec![value],
            )),
        )
    };
    let team = join(
        "teams",
        JoinKind::BelongsTo {
            column: column("team_id"),
        },
        vec![value_field("name", "name", FlussoType::Keyword)],
    );
    let tags = join(
        "tags",
        JoinKind::ManyToMany {
            through: Through {
                table: table("user_tags"),
                left_key: column("user_id"),
                right_key: column("tag_id"),
                filters: None,
            },
        },
        vec![value_field("label", "label", FlussoType::Keyword)],
    );
    let task_count = Aggregate {
        table: table("tasks"),
        op: AggregateOp::Count,
        key: AggregateKey::Direct(column("project_id")),
        value_type: None,
        filters: None,
        distinct: false,
        via: vec![ViaHop {
            table: table("projects"),
            foreign_key: column("user_id"),
            filters: None,
            on: vec![],
        }],
        on: vec![],
    };
    let schema = IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            value_field("name", "name", FlussoType::Keyword),
            relation("team", Relation::Join(team)),
            has_many(
                "posts",
                "posts",
                value_field("title", "title", FlussoType::Keyword),
            ),
            has_many(
                "events",
                "events",
                value_field("kind", "kind", FlussoType::Keyword),
            ),
            relation("tags", Relation::Join(tags)),
            relation("taskCount", Relation::Aggregate(task_count)),
            has_many(
                "ledgers",
                "ledgers",
                value_field("amount", "amount", FlussoType::Long),
            ),
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

/// `users` with its posts' titles.
fn posts_only_schema() -> IndexSchema {
    let posts = join(
        "posts",
        JoinKind::HasMany {
            foreign_key: column("user_id"),
        },
        vec![value_field("title", "title", FlussoType::Keyword)],
    );
    IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            value_field("name", "name", FlussoType::Keyword),
            relation("posts", Relation::Join(posts)),
        ],
    }
}

fn join(target: &str, kind: JoinKind, fields: Vec<Field>) -> Join {
    Join {
        table: table(target),
        kind,
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: Some(vec![OrderBy {
            column: column("id"),
            direction: None,
        }]),
        limit: None,
        fields,
        indexed: true,
        on: vec![],
    }
}

fn value_field(name: &str, col: &str, ty: FlussoType) -> Field {
    let mut field = common::column_field(name, col);
    if let FieldSource::Column(column) = &mut field.source {
        column.ty = ty;
    }
    field
}

fn relation(name: &str, relation: Relation) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(relation),
    }
}

async fn start() -> (
    testcontainers_modules::testcontainers::ContainerAsync<
        testcontainers_modules::postgres::Postgres,
    >,
    String,
    u16,
) {
    let (container, url) = start_postgres().await;
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    (container, url, port)
}

async fn create_slot(pool: &sqlx::PgPool) {
    sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
        .execute(pool)
        .await
        .unwrap();
}

async fn execute(pool: &sqlx::PgPool, sql: &str) {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

fn replication(port: u16) -> ReplicationConfig {
    ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(port)
}

/// Run the deployment the way the daemon supervises it: restart it after an
/// error, counting the restarts and keeping each error, until it ends
/// cleanly or has restarted twenty times.
async fn supervise(
    capture: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    restarts: Arc<AtomicUsize>,
    failures: Arc<Mutex<Vec<String>>>,
) -> engine::Result<()> {
    loop {
        let result = run_deployment(
            Arc::clone(&capture),
            Arc::clone(&documents),
            Arc::clone(&sink),
        )
        .await;
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                failures.lock().unwrap().push(error.to_string());
                if restarts.fetch_add(1, Ordering::SeqCst) >= 19 {
                    return Err(error);
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

/// One deployment the way the daemon runs it: stage the sink engine, then run
/// it and the ingest engine together until either stops.
async fn run_deployment(
    capture: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
) -> engine::Result<()> {
    let name = SinkName::try_new("recording").unwrap();
    let stream: Arc<dyn Stream> = Arc::new(ChannelStream::new(1024, [name.clone()]));
    let continuity = capture.continuity().await?;
    let mappings = documents.index_mappings().await?;
    let sink_engine = SinkEngine::new(name.clone(), sink, Arc::clone(&stream), mappings);
    sink_engine.stage(continuity).await?;
    let ingest = IngestEngine::new(capture, documents, stream, vec![name]);
    let (_control, mut control_rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    tokio::select! {
        result = ingest.run() => result,
        result = sink_engine.run(&mut control_rx) => result,
    }
}

/// Records each document as the OpenSearch sink renders it, by id; a
/// tombstone as `null`.
#[derive(Debug)]
struct DocumentSink(Arc<Mutex<Vec<(String, Value)>>>);

#[async_trait]
impl Sink for DocumentSink {
    async fn apply(&self, envelope: &Envelope) -> SinkResult<()> {
        let document = envelope
            .document
            .as_ref()
            .map_or(Value::Null, sink::to_json);
        self.0.lock().unwrap().push((envelope.id.clone(), document));
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> SinkResult<sink::FlushReport> {
        Ok(sink::FlushReport::clean())
    }
}

/// Wait until the latest document recorded for `id` is `expected`.
async fn document_reaches(
    engine: &mut tokio::task::JoinHandle<engine::Result<()>>,
    recorded: &Arc<Mutex<Vec<(String, Value)>>>,
    id: &str,
    expected: &Value,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if engine.is_finished() {
            panic!("engine stopped before {expected}: {:?}", engine.await);
        }
        let latest = recorded
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(recorded_id, _)| recorded_id == id)
            .map(|(_, document)| document.clone());
        if latest.as_ref() == Some(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected}; latest {latest:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
