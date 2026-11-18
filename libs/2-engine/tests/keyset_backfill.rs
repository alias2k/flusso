//! Keyset backfill e2e (issue #386 stage 3): a real Postgres → the engines
//! → a sink keeping the last write per document, with rows written while the
//! backfill walks its key pages. After the backfill and the live stream have
//! settled, every index must equal the table: each row's document as a
//! `build` of it gives, and nothing for a row that is gone.
//!
//! The writes are placed, not timed: a builder decorator runs them right after
//! each full page is read, against rows **behind** the walk's cursor, rows
//! **ahead** of it and rows of the page just read (read, not yet built):
//! updates, deletes, soft-deletes, rows leaving a root filter, inserts at both
//! ends, key changes across the cursor in both directions, re-parents, and a
//! change to a parent another index embeds. Every interleaving the design
//! lists is in each run. The same runs on a hash-partitioned root, and with a
//! stop in the middle of the walk followed by a restart.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --test keyset_backfill --run-ignored all
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::{BatchPolicy, IngestEngine, SinkControl, SinkEngine, Stop};
use kernel::{
    DatabaseSchema, Envelope, Field, FieldSource, Filter, FilterOp, FilterValue, IndexMapping,
    IndexName, IndexSchema, Join, JoinKind, Relation, SinkName, SoftDelete, SoftDeleteColumn,
    TableName, ValueOpFilter,
};
use sink::{Result as SinkResult, Sink};
use source::cdc::ChangeCapture;
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey, SourceSpec};
use source_postgres::{PgDocumentBuilder, WalChangeCapture};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use stream::Stream;
use stream_channel::ChannelStream;

mod common;
use common::{column, column_field, field, index_name, replication, start_postgres_with, table};

const INDEXES: [&str; 3] = ["items", "live_items", "parents"];
const ITEMS: i64 = 5_000;
const PARENTS: i64 = 50;

/// The last write per `(index, id)`: `upsert <json>` or `delete`.
#[derive(Debug, Default)]
struct LastWrite {
    state: Mutex<BTreeMap<(String, String), String>>,
    seeded: Mutex<BTreeSet<String>>,
    /// How many upserts each `(index, id)` got.
    upserts: Mutex<BTreeMap<(String, String), usize>>,
    /// How long each write of backfill documents is held before it lands.
    hold_backfill: Duration,
    /// `(index, id, live, started, landed)` of every document written.
    writes: Mutex<Vec<(String, String, bool, Instant, Instant)>>,
}

#[async_trait]
impl Sink for LastWrite {
    async fn write(
        &self,
        envelopes: &[Envelope],
        _caught_up: bool,
    ) -> SinkResult<sink::WriteReport> {
        let started = Instant::now();
        let live = envelopes.iter().any(|envelope| envelope.position.is_some());
        if !live && !self.hold_backfill.is_zero() {
            tokio::time::sleep(self.hold_backfill).await;
        }
        let mut state = self.state.lock().unwrap();
        let mut upserts = self.upserts.lock().unwrap();
        let mut writes = self.writes.lock().unwrap();
        for envelope in envelopes {
            let id = (envelope.index.to_string(), envelope.id.clone());
            let written = match &envelope.document {
                Some(document) => {
                    *upserts.entry(id.clone()).or_default() += 1;
                    format!("upsert {}", sink::to_json(document))
                }
                None => "delete".to_owned(),
            };
            writes.push((id.0.clone(), id.1.clone(), live, started, Instant::now()));
            state.insert(id, written);
        }
        Ok(sink::WriteReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> SinkResult<bool> {
        Ok(self.seeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn mark_seeded(&self, index: &IndexName) -> SinkResult<()> {
        self.seeded.lock().unwrap().insert(index.to_string());
        Ok(())
    }
}

/// The Postgres builder, running [`writes`] right after each full page of
/// `items` or `live_items` is read, and requesting `stop` after the
/// `stop_after`th such page.
#[derive(Debug)]
struct WritesAfterEachPage {
    inner: PgDocumentBuilder,
    pool: PgPool,
    pages: Arc<AtomicUsize>,
    stop: Option<(usize, Stop)>,
    mode: Mode,
    /// A row of `items` and a statement run once, right after the first
    /// batch holding that row's document is built.
    after_build: Mutex<Option<(i64, String)>>,
}

/// What [`WritesAfterEachPage`] writes after each full page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// [`writes`]: every kind of write around the cursor.
    Around,
    /// A full page of new greatest keys: a writer outrunning the walk.
    Ahead,
    Nothing,
}

#[async_trait]
impl DocumentBuilder for WritesAfterEachPage {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        self.inner.resolve(table, key, before, after).await
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        self.inner.build(id).await
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        let built = self.inner.build_many(ids).await?;
        let fire = {
            let mut armed = self.after_build.lock().unwrap();
            let holds = |row: i64| {
                ids.iter()
                    .any(|id| id.index.as_ref() == "items" && id_of(&id.key) == row)
            };
            match armed.as_ref() {
                Some((row, _)) if holds(*row) => armed.take(),
                _ => None,
            }
        };
        if let Some((_, statement)) = fire {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&self.pool)
                .await
                .unwrap();
        }
        Ok(built)
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        self.inner.backfill_indexes()
    }

    async fn last_key(&self, index: &IndexName) -> source::Result<Option<RowKey>> {
        self.inner.last_key(index).await
    }

    async fn keys(
        &self,
        index: &IndexName,
        after: Option<&RowKey>,
        through: &RowKey,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        let page = self.inner.keys(index, after, through, limit).await?;
        let walked_by_writes = matches!(index.as_ref(), "items" | "live_items");
        if let (true, true, Some(last)) = (walked_by_writes, page.len() == limit, page.last()) {
            let n = self.pages.fetch_add(1, Ordering::SeqCst);
            match self.mode {
                Mode::Around => writes(&self.pool, n, id_of(last)).await,
                Mode::Ahead => ahead(&self.pool, n, limit).await,
                Mode::Nothing => {}
            }
            if let Some((at, stop)) = &self.stop
                && n + 1 == *at
            {
                stop.request();
            }
        }
        Ok(page)
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        self.inner.index_mappings().await
    }
}

fn id_of(key: &RowKey) -> i64 {
    match key.0.first() {
        Some((_, kernel::GenericValue::BigInt(id))) => *id,
        other => panic!("not an int8 key: {other:?}"),
    }
}

/// The `n`th page of new greatest keys: `limit` rows past every key.
async fn ahead(pool: &PgPool, n: usize, limit: usize) {
    let first = 1_000_000 + n * limit;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO items (id, parent_id, label) \
         SELECT g, g % {PARENTS} + 1, 'ahead' FROM generate_series({first}, {}) g",
        first + limit - 1
    )))
    .execute(pool)
    .await
    .unwrap();
}

/// The `n`th set of writes, around `k`, the last key of the page just read.
/// Rows `k - 2 …` are in that page (read, not built yet), `k - 300 …` behind
/// the cursor, `k + 300 …` ahead of it.
async fn writes(pool: &PgPool, n: usize, k: i64) {
    let n = i64::try_from(n).unwrap();
    let (inpage, behind, ahead) = (k - 2, k - 300, k + 300);
    let statements = [
        format!("UPDATE items SET label = 'u{n}' WHERE id IN ({inpage}, {behind}, {ahead})"),
        format!(
            "DELETE FROM items WHERE id IN ({}, {}, {})",
            inpage - 1,
            behind - 1,
            ahead + 1
        ),
        format!(
            "UPDATE items SET deleted = NOT deleted WHERE id IN ({}, {}, {})",
            inpage - 2,
            behind - 2,
            ahead + 2
        ),
        format!(
            "UPDATE items SET label = 'hidden' WHERE id IN ({}, {}, {})",
            inpage - 3,
            behind - 3,
            ahead + 3
        ),
        format!(
            "INSERT INTO items (id, parent_id, label) VALUES ({}, 1, 'new'), ({}, 2, 'new') \
             ON CONFLICT DO NOTHING",
            100_000 + n,
            -(n + 1)
        ),
        // A key moving ahead of the cursor, and one moving behind it.
        format!(
            "UPDATE items SET id = {} WHERE id = {}",
            200_000 + n,
            behind - 5
        ),
        format!(
            "UPDATE items SET id = {} WHERE id = {}",
            -(1_000 + n),
            ahead + 5
        ),
        format!(
            "UPDATE items SET parent_id = parent_id % {PARENTS} + 1 \
             WHERE id IN ({}, {}, {})",
            inpage - 4,
            behind - 6,
            ahead + 6
        ),
        format!(
            "UPDATE parents SET name = 'p{n}' WHERE id = {}",
            n % PARENTS + 1
        ),
    ];
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(pool)
            .await
            .unwrap();
    }
}

fn join(name: &str, table_name: &str, kind: JoinKind, fields: Vec<Field>) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: table(table_name),
            kind,
            primary_key: column("id"),
            nullable: true,
            filters: None,
            order_by: Some(vec![kernel::OrderBy {
                column: column("id"),
                direction: None,
            }]),
            limit: None,
            fields,
            indexed: true,
            on: vec![],
        })),
    }
}

fn root(table_name: &str, fields: Vec<Field>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table(table_name),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields,
    }
}

/// `items` (soft-deleted by `deleted`, embeds its parent), `live_items` (a
/// root filter on `label`) and `parents` (embeds its items).
fn spec() -> SourceSpec {
    let items = IndexSchema {
        soft_delete: Some(SoftDelete::Column(SoftDeleteColumn {
            column: column("deleted"),
            when: None,
        })),
        ..root(
            "items",
            vec![
                column_field("label", "label"),
                join(
                    "parent",
                    "parents",
                    JoinKind::BelongsTo {
                        column: column("parent_id"),
                    },
                    vec![column_field("name", "name")],
                ),
            ],
        )
    };
    let live_items = IndexSchema {
        filters: Some(vec![Filter::ValueOp(ValueOpFilter {
            column: column("label"),
            op: FilterOp::Neq,
            value: FilterValue::Single("hidden".to_owned()),
        })]),
        ..root("items", vec![column_field("label", "label")])
    };
    let parents = root(
        "parents",
        vec![
            column_field("name", "name"),
            join(
                "items",
                "items",
                JoinKind::HasMany {
                    foreign_key: column("parent_id"),
                },
                vec![column_field("label", "label")],
            ),
        ],
    );
    SourceSpec::new(BTreeMap::from([
        (index_name("items"), items),
        (index_name("live_items"), live_items),
        (index_name("parents"), parents),
    ]))
}

/// Postgres with the tables filled, a publication and a slot made before
/// anything is read (the order `prepare` keeps).
async fn database(partitioned: bool) -> (test_postgres::Container, u16, String, PgPool) {
    let (container, port, url) = start_postgres_with(&[]).await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    let mut statements = vec![
        "CREATE TABLE parents (id int PRIMARY KEY, name text)".to_owned(),
        format!(
            "CREATE TABLE items (id int8 PRIMARY KEY, parent_id int, label text, \
               deleted boolean NOT NULL DEFAULT false){}",
            if partitioned {
                " PARTITION BY HASH (id)"
            } else {
                ""
            }
        ),
    ];
    if partitioned {
        for n in 0..4 {
            statements.push(format!(
                "CREATE TABLE items_{n} PARTITION OF items FOR VALUES WITH (MODULUS 4, REMAINDER {n})"
            ));
            statements.push(format!("ALTER TABLE items_{n} REPLICA IDENTITY FULL"));
        }
    }
    statements.extend([
        "ALTER TABLE items REPLICA IDENTITY FULL".to_owned(),
        "ALTER TABLE parents REPLICA IDENTITY FULL".to_owned(),
        format!("INSERT INTO parents SELECT n, 'p' || n FROM generate_series(1, {PARENTS}) n"),
        format!(
            "INSERT INTO items (id, parent_id, label, deleted) SELECT n, n % {PARENTS} + 1, \
               CASE WHEN n % 89 = 0 THEN 'hidden' ELSE 'l' || n END, n % 97 = 0 \
             FROM generate_series(1, {ITEMS}) n"
        ),
        "CREATE PUBLICATION flusso FOR TABLE parents, items \
           WITH (publish_via_partition_root = true)"
            .to_owned(),
        "SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')".to_owned(),
    ]);
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .unwrap();
    }
    (container, port, url, pool)
}

/// Every index as a `build` of each current root row gives it: `upsert
/// <json>` or `delete`.
async fn expected(url: &str, pool: &PgPool) -> BTreeMap<(String, String), String> {
    let documents = PgDocumentBuilder::connect(url, Arc::new(spec()))
        .await
        .unwrap();
    let mut out = BTreeMap::new();
    for name in INDEXES {
        let table = if name == "parents" {
            "parents"
        } else {
            "items"
        };
        let ids: Vec<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT id::int8 FROM {table} ORDER BY id"
        )))
        .fetch_all(pool)
        .await
        .unwrap();
        let ids: Vec<DocumentId> = ids
            .into_iter()
            .map(|id| DocumentId {
                index: index_name(name),
                key: RowKey(vec![(
                    column("id"),
                    if table == "parents" {
                        kernel::GenericValue::Int(i32::try_from(id).unwrap())
                    } else {
                        kernel::GenericValue::BigInt(id)
                    },
                )]),
            })
            .collect();
        for chunk in ids.chunks(512) {
            for document in documents.build_many(chunk).await.unwrap() {
                let id = document.id().rendered();
                let written = match document {
                    Document::Upsert { body, .. } => format!("upsert {}", sink::to_json(&body)),
                    Document::Delete { .. } => "delete".to_owned(),
                    Document::Failed { failure, .. } => panic!("{failure:?}"),
                };
                out.insert((name.to_owned(), id), written);
            }
        }
    }
    out
}

/// Where the sink and the table disagree: a row's document missing or
/// different, or a document left for a row that is gone.
fn differences(
    state: &BTreeMap<(String, String), String>,
    want: &BTreeMap<(String, String), String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (id, document) in want {
        let written = state.get(id).map_or("nothing", String::as_str);
        let agrees = written == document || (document == "delete" && written == "nothing");
        if !agrees {
            out.push(format!("{id:?}: want {document}, got {written}"));
        }
    }
    for (id, written) in state {
        if !want.contains_key(id) && written != "delete" {
            out.push(format!("{id:?}: gone from the table, got {written}"));
        }
    }
    out
}

/// One run of both engines over the slot, the way the daemon runs them,
/// until every index is seeded (`Ok`) or `stop` ended the ingest engine.
async fn run(
    port: u16,
    url: &str,
    pool: &PgPool,
    sink: &Arc<LastWrite>,
    pages: &Arc<AtomicUsize>,
    stop_after: Option<usize>,
) -> Option<Engines> {
    let plan = Plan {
        stop_after,
        ..Plan::default()
    };
    run_with(port, url, pool, sink, pages, plan).await
}

/// How a run writes during its walk and how its sink writes.
struct Plan {
    stop_after: Option<usize>,
    mode: Mode,
    after_build: Option<(i64, String)>,
    /// Writes the sink engine keeps in flight.
    writes: usize,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            stop_after: None,
            mode: Mode::Around,
            after_build: None,
            writes: 1,
        }
    }
}

async fn run_with(
    port: u16,
    url: &str,
    pool: &PgPool,
    sink: &Arc<LastWrite>,
    pages: &Arc<AtomicUsize>,
    plan: Plan,
) -> Option<Engines> {
    let stop_after = plan.stop_after;
    let stop = Stop::new();
    let inner = PgDocumentBuilder::connect(url, Arc::new(spec()))
        .await
        .unwrap();
    let documents: Arc<dyn DocumentBuilder> = Arc::new(WritesAfterEachPage {
        inner,
        pool: pool.clone(),
        pages: Arc::clone(pages),
        stop: stop_after.map(|at| (at, stop.clone())),
        mode: plan.mode,
        after_build: Mutex::new(plan.after_build),
    });
    let capture: Arc<dyn ChangeCapture> =
        Arc::new(WalChangeCapture::new(replication(port), url.to_owned()));
    let name = SinkName::try_new("last").unwrap();
    let stream: Arc<dyn Stream> = Arc::new(ChannelStream::new(64, [name.clone()]));
    let continuity = capture.continuity().await.unwrap();
    let mappings = documents.index_mappings().await.unwrap();
    let progress = Arc::new(engine::Progress::new([name.clone()]));
    let sink_engine = SinkEngine::new(
        name.clone(),
        Arc::clone(sink) as Arc<dyn Sink>,
        Arc::clone(&stream),
        mappings,
        Arc::clone(&progress),
    )
    .with_writes(plan.writes)
    .with_stop(stop.clone());
    sink_engine.stage(continuity).await.unwrap();
    let ingest = IngestEngine::new(capture, documents, stream, vec![name], progress)
        .with_batch(BatchPolicy {
            max_changes: 64,
            max_delay: Duration::from_millis(10),
        })
        .with_stop(stop.clone());
    let sink_task = tokio::spawn(async move {
        let (_control, mut control_rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
        let _ = sink_engine.run(&mut control_rx).await;
    });
    let mut ingest_task = tokio::spawn(async move { ingest.run().await });
    if stop_after.is_some() {
        tokio::time::timeout(Duration::from_secs(30), &mut ingest_task)
            .await
            .expect("the stop drains")
            .unwrap()
            .unwrap();
        sink_task.abort();
        return None;
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    while sink.seeded.lock().unwrap().len() < INDEXES.len() {
        assert!(Instant::now() < deadline, "the backfill did not finish");
        assert!(
            !ingest_task.is_finished(),
            "the ingest engine stopped: {:?}",
            (&mut ingest_task).await
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Some(Engines {
        ingest: ingest_task,
        sink: sink_task,
    })
}

/// The engines of a run still following the live stream.
struct Engines {
    ingest: tokio::task::JoinHandle<engine::Result<()>>,
    sink: tokio::task::JoinHandle<()>,
}

impl Engines {
    fn abort(self) {
        self.ingest.abort();
        self.sink.abort();
    }
}

/// Wait until the sink equals the table, the live stream still running.
async fn settles(url: &str, pool: &PgPool, sink: &LastWrite) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let want = expected(url, pool).await;
        let diff = differences(&sink.state.lock().unwrap(), &want);
        if diff.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the index never matched the table: {} differences, first {:#?}",
            diff.len(),
            diff.iter().take(20).collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn every_write_during_the_walk_lands(partitioned: bool) {
    let (_container, port, url, pool) = database(partitioned).await;
    let sink = Arc::new(LastWrite::default());
    let pages = Arc::new(AtomicUsize::new(0));
    let engines = run(port, &url, &pool, &sink, &pages, None).await.unwrap();
    settles(&url, &pool, &sink).await;
    engines.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn rows_written_behind_ahead_and_inside_the_page_all_land() {
    every_write_during_the_walk_lands(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn on_a_partitioned_root_rows_written_during_the_walk_all_land() {
    every_write_during_the_walk_lands(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_backfill_stopped_mid_walk_is_walked_again_and_lands_every_row() {
    let (_container, port, url, pool) = database(false).await;
    let sink = Arc::new(LastWrite::default());
    let pages = Arc::new(AtomicUsize::new(0));
    assert!(
        run(port, &url, &pool, &sink, &pages, Some(2))
            .await
            .is_none()
    );
    assert!(
        sink.seeded.lock().unwrap().is_empty(),
        "a stopped backfill seeds nothing"
    );
    assert!(
        !sink.state.lock().unwrap().is_empty(),
        "the stop came after pages were written"
    );
    pages.store(100, Ordering::SeqCst);
    let engines = run(port, &url, &pool, &sink, &pages, None).await.unwrap();
    settles(&url, &pool, &sink).await;
    engines.abort();
}

/// A writer adding a full page of new greatest keys after every page read
/// doesn't keep the walk going (review of #386 stage 3): the walk ends at the
/// greatest key it found when it started, and the rows added during it reach
/// the index through the live stream alone. Every `items` document is built
/// once: a row there at the walk's start by its page, a row added during it
/// by its insert.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn rows_added_ahead_of_the_walk_end_it_where_it_began_and_are_built_once() {
    let (_container, port, url, pool) = database(false).await;
    let sink = Arc::new(LastWrite::default());
    let pages = Arc::new(AtomicUsize::new(0));
    let plan = Plan {
        mode: Mode::Ahead,
        ..Plan::default()
    };
    let engines = run_with(port, &url, &pool, &sink, &pages, plan)
        .await
        .unwrap();
    settles(&url, &pool, &sink).await;
    engines.abort();

    // `items` is walked first: its 5,000 rows are 4 full pages and a short
    // one. `live_items` starts after it and finds the 4 pages added meanwhile
    // too: 9,096 rows, 8 full pages and a short one.
    let page = engine::KEY_PAGE;
    let items = usize::try_from(ITEMS).unwrap() / page;
    let live_items = (usize::try_from(ITEMS).unwrap() + items * page) / page;
    assert_eq!(
        pages.load(Ordering::SeqCst),
        items + live_items,
        "each walk reads the rows it found at its start, not the pages added during it"
    );
    let twice: Vec<_> = sink
        .upserts
        .lock()
        .unwrap()
        .iter()
        .filter(|((index, _), count)| index == "items" && **count > 1)
        .map(|(id, count)| format!("{id:?} ×{count}"))
        .take(10)
        .collect();
    assert!(twice.is_empty(), "built more than once: {twice:?}");
}

/// A page's document built before a write to its row, written while a live
/// rebuild of that row waits behind it: with four writes in flight and the
/// backfill's writes held, the live document still lands last and stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_live_rebuild_lands_after_the_held_page_write_it_overtook() {
    let (_container, port, url, pool) = database(false).await;
    let sink = Arc::new(LastWrite {
        hold_backfill: Duration::from_millis(500),
        ..LastWrite::default()
    });
    let pages = Arc::new(AtomicUsize::new(0));
    let plan = Plan {
        mode: Mode::Nothing,
        after_build: Some((5, "UPDATE items SET label = 'live' WHERE id = 5".to_owned())),
        writes: 4,
        ..Plan::default()
    };
    let engines = run_with(port, &url, &pool, &sink, &pages, plan)
        .await
        .unwrap();
    settles(&url, &pool, &sink).await;
    engines.abort();

    let state = sink.state.lock().unwrap();
    let five = state.get(&("items".to_owned(), "5".to_owned())).unwrap();
    assert!(five.contains("\"live\""), "the live document stays: {five}");
    let writes = sink.writes.lock().unwrap();
    let of_five: Vec<_> = writes
        .iter()
        .filter(|(index, id, ..)| index == "items" && id == "5")
        .collect();
    let page = of_five
        .iter()
        .find(|(.., live, _, _)| !live)
        .expect("the page wrote it");
    let rebuilt = of_five
        .iter()
        .find(|(.., live, _, _)| *live)
        .expect("the live change rebuilt it");
    assert!(
        rebuilt.3 >= page.4,
        "the live write started only once the held page write had landed"
    );
}
