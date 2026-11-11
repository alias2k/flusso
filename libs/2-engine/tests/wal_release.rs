//! WAL release e2e: the replication slot lets go of WAL flusso has no use for.
//!
//! A change that produces no document must still be confirmed to Postgres, or
//! the slot pins WAL forever (issue #111). Each test runs a real Postgres → the
//! ingest engine → one or more sink engines over recording sinks, writes WAL
//! that yields few or no documents, takes `pg_current_wal_lsn()` as the
//! target, and asserts on `pg_replication_slots` that:
//!
//! - `confirmed_flush_lsn` passes the target within [`CONFIRM_BOUND`], and
//! - `restart_lsn` passes it within [`RELEASE_BOUND`], so the slot retains
//!   none of the WAL the writes produced (`pg_current_wal_lsn() - restart_lsn`
//!   is back to near zero: only what Postgres wrote since, such as autovacuum),
//!
//! *and* that the sinks received exactly the operations the writes call for.
//! The second half is what makes a "fix" that confirms without processing fail
//! too. The two LSNs move on different clocks, so each has its own bound.
//!
//! The cases (issue #204):
//!
//! - **B** a pre-existing `FOR ALL TABLES` publication; writes to a table no
//!   index reads.
//! - **C** a burst of updates changing only a column no document reads (the
//!   #169 skip).
//! - **D** rows a root filter excludes, soft-deleted rows, children whose
//!   parent does not exist: tombstones only.
//! - **E** one large transaction of unread changes, then a transaction mixing
//!   read and unread changes split across batches.
//! - **F** idle after a burst: the slot reaches the WAL position the burst
//!   ended at, not one commit behind, then follows WAL that holds no change
//!   (a `CHECKPOINT`) to the server's current position.
//! - **G** several sinks: a stalled sink pins the slot at the last position it
//!   flushed (by design) and releases it once it catches up; a
//!   `backfill = false` sink pins nothing.
//! - **H** bursts and lone single changes that each produce a document: the
//!   slot confirms the last commit within [`CONFIRM_BOUND`] of the sink flush
//!   that landed it, not of the commit (issue #205).
//!
//! Each case prints how long confirmation and release took, so a latency
//! improvement shows up in `--no-capture` output. Every wait fails fast once
//! the deployment stopped.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --run-ignored all -E 'binary(wal_release)' --no-capture
//! ```

#![allow(clippy::unwrap_used, clippy::print_stdout, unused_crate_dependencies)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::{BatchPolicy, IngestEngine, SinkControl, SinkEngine};
use futures::FutureExt;
use futures::future::{BoxFuture, select_all};
use kernel::{
    DatabaseSchema, Envelope, Field, FieldSource, Filter, IndexName, IndexSchema, Join, JoinKind,
    NullCheckFilter, NullOp, Relation, SinkName, SoftDelete, SoftDeleteColumn,
};
use sink::{Result as SinkResult, Sink, SinkOptions};
use source::SourceSpec;
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::postgres::{PgPool, PgPoolOptions};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::task::JoinHandle;

mod common;
use common::{column, column_field, field, index_name, start_postgres, table, users_spec};

/// How long setup may take: the slot created, the backfill landed, a write
/// delivered to a sink. Not what these tests measure, so generous.
const SETUP_BOUND: Duration = Duration::from_secs(60);

/// How long `confirmed_flush_lsn` may take to pass the target once the writes
/// are committed.
///
/// It moves when flusso reports its position to Postgres: the part flusso
/// controls. The capture hands a confirmation to the replication client at once
/// and reports it every second, so 5 s leaves room for a slow runner (issue
/// #205).
const CONFIRM_BOUND: Duration = Duration::from_secs(5);

/// How long `restart_lsn`, and with it the WAL the slot retains, may take to
/// pass the target once the writes are committed.
///
/// It decides what WAL Postgres can recycle, and it moves only once Postgres
/// has logged, and flusso has decoded, a `running_xacts` record past the
/// target. The background writer logs one about every 15 s when there was
/// activity, a cadence flusso cannot shorten, so this bound stays loose.
const RELEASE_BOUND: Duration = Duration::from_secs(45);

/// How long a stalled sink is given to (wrongly) let the slot advance, and,
/// once it is unblocked, how long the slot has to confirm what it held back:
/// several reports, since the capture reports its position every second.
const STALL_WINDOW: Duration = Duration::from_secs(5);

const SLOT: &str = "flusso";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn writes_to_a_table_no_index_reads_release_the_slot() {
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE users (id int PRIMARY KEY, email text);
         CREATE TABLE noise (id serial PRIMARY KEY, payload text);
         CREATE PUBLICATION flusso FOR ALL TABLES;",
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let run = deploy(&url, users_spec(), vec![primary(&sink)], Tuning::default()).await;
    wait_until_live(&run, &pool, &sink, 0).await;

    for _ in 0..500 {
        exec(
            &pool,
            "INSERT INTO noise (payload) VALUES (repeat('x', 500))",
        )
        .await;
    }
    exec(&pool, "UPDATE noise SET payload = 'y'").await;
    let target = current_lsn(&pool).await;

    wait_until_released(&run, &pool, &target, "B: unread table").await;
    assert_eq!(sink.ops(), Vec::<String>::new());
    stop(run).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_burst_of_unread_column_updates_releases_the_slot() {
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int);
         INSERT INTO users SELECT g, 'u' || g, 0 FROM generate_series(1, 100) g;",
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let run = deploy(&url, users_spec(), vec![primary(&sink)], Tuning::default()).await;
    wait_until_live(&run, &pool, &sink, 100).await;
    let backfilled = sink.ops();

    for i in 0..500 {
        sqlx::query("UPDATE users SET last_seen = $1 WHERE id = $2")
            .bind(i)
            .bind(i % 100 + 1)
            .execute(&pool)
            .await
            .unwrap();
    }
    for i in 0..10 {
        sqlx::query("UPDATE users SET last_seen = $1")
            .bind(i)
            .execute(&pool)
            .await
            .unwrap();
    }
    let target = current_lsn(&pool).await;

    wait_until_released(&run, &pool, &target, "C: unread column").await;
    assert_eq!(
        sink.ops(),
        backfilled,
        "an unread-column update was rebuilt"
    );
    stop(run).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn filtered_soft_deleted_and_orphaned_rows_release_the_slot() {
    const ROWS: i32 = 50;
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE nodes (id int PRIMARY KEY, parent_id int, name text, \
         deleted boolean NOT NULL DEFAULT false);
         CREATE TABLE notes (id int PRIMARY KEY, node_id int, body text);",
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let run = deploy(&url, nodes_spec(), vec![primary(&sink)], Tuning::default()).await;
    wait_until_live(&run, &pool, &sink, 0).await;

    let mut expected = BTreeSet::new();
    for i in 0..ROWS {
        let (excluded, soft_deleted, missing_parent) = (10_000 + i, 20_000 + i, 90_000 + i);
        sqlx::query("INSERT INTO nodes (id, parent_id, name) VALUES ($1, 1, 'child')")
            .bind(excluded)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO nodes (id, name, deleted) VALUES ($1, 'gone', true)")
            .bind(soft_deleted)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO notes (id, node_id, body) VALUES ($1, $2, 'x')")
            .bind(30_000 + i)
            .bind(missing_parent)
            .execute(&pool)
            .await
            .unwrap();
        for id in [excluded, soft_deleted, missing_parent] {
            expected.insert(format!("delete nodes {id}"));
        }
    }
    exec(&pool, "UPDATE nodes SET name = 'z'; DELETE FROM notes;").await;
    let target = current_lsn(&pool).await;

    wait_until_released(&run, &pool, &target, "D: no document").await;
    let tombstoned: BTreeSet<String> = sink.ops().into_iter().collect();
    assert_eq!(tombstoned, expected, "only tombstones, one per touched id");
    stop(run).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn large_and_split_mixed_transactions_release_the_slot() {
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int);
         CREATE TABLE noise (id serial PRIMARY KEY, payload text);
         INSERT INTO users SELECT g, 'u' || g, 0 FROM generate_series(1, 200) g;",
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let tuning = Tuning {
        batch: BatchPolicy {
            max_changes: 3,
            max_delay: Duration::from_millis(50),
        },
        ..Tuning::default()
    };
    let run = deploy(&url, users_spec(), vec![primary(&sink)], tuning).await;
    wait_until_live(&run, &pool, &sink, 200).await;
    let backfilled = sink.ops();

    exec(
        &pool,
        "BEGIN;
         UPDATE users SET last_seen = 1; UPDATE users SET last_seen = 2;
         UPDATE users SET last_seen = 3; UPDATE users SET last_seen = 4;
         UPDATE users SET last_seen = 5;
         INSERT INTO noise (payload) SELECT repeat('x', 200) FROM generate_series(1, 5000);
         COMMIT;",
    )
    .await;
    let target = current_lsn(&pool).await;
    wait_until_released(&run, &pool, &target, "E: large unread transaction").await;
    assert_eq!(sink.ops(), backfilled, "an unread change was rebuilt");

    exec(
        &pool,
        "BEGIN;
         UPDATE users SET last_seen = 100 WHERE id <= 10;
         UPDATE users SET email = 'changed' WHERE id = 1;
         UPDATE users SET last_seen = 101 WHERE id <= 10;
         UPDATE users SET email = 'changed' WHERE id = 2;
         INSERT INTO noise (payload) SELECT 'y' FROM generate_series(1, 1000);
         COMMIT;",
    )
    .await;
    let target = current_lsn(&pool).await;
    wait_until_released(&run, &pool, &target, "E: split mixed transaction").await;
    assert_eq!(
        sink.ops().get(backfilled.len()..).unwrap(),
        ["upsert users 1", "upsert users 2"]
    );
    stop(run).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn idle_after_a_burst_the_slot_reaches_the_current_wal_position() {
    const ROWS: i32 = 200;
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int);",
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let run = deploy(&url, users_spec(), vec![primary(&sink)], Tuning::default()).await;
    wait_until_live(&run, &pool, &sink, 0).await;

    for id in 1..=ROWS {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'a')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let target = current_lsn(&pool).await;

    wait_until_released(&run, &pool, &target, "F: the burst's last commit").await;

    exec(&pool, "CHECKPOINT").await;
    let idle = current_lsn(&pool).await;
    wait_until_released(&run, &pool, &idle, "F: idle WAL past it").await;
    let upserted: BTreeSet<String> = sink.ops().into_iter().collect();
    let expected: BTreeSet<String> = (1..=ROWS).map(|id| format!("upsert users {id}")).collect();
    assert_eq!(upserted, expected);
    stop(run).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_stalled_sink_pins_the_slot_until_it_catches_up() {
    const ROWS: i32 = 50;
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int);",
    )
    .await;
    let fast = Arc::new(RecordingSink::default());
    let open = Arc::new(AtomicBool::new(true));
    let stalled = Arc::new(RecordingSink::gated(Arc::clone(&open)));
    let live_only = Arc::new(RecordingSink::default());
    let sinks = vec![
        SinkSpec::new("fast", &fast, true),
        SinkSpec::new("stalled", &stalled, true),
        SinkSpec::new("live_only", &live_only, false),
    ];
    let run = deploy(&url, users_spec(), sinks, Tuning::default()).await;
    wait_until_live(&run, &pool, &fast, 0).await;
    wait_until_live(&run, &pool, &stalled, 0).await;

    // Every lane, the stalled one included, acks a positioned batch first, so
    // the watermark is a real minimum over lanes rather than "no position yet".
    exec(&pool, "INSERT INTO users (id, email) VALUES (0, 'a')").await;
    let before_stall = current_lsn(&pool).await;
    for sink in [&fast, &stalled, &live_only] {
        wait_for_ops(&run, sink, 1).await;
    }
    wait_until_confirmed(&run, &pool, &before_stall, "G: before the stall").await;
    open.store(false, Ordering::Release);

    exec(&pool, "INSERT INTO users (id, email) VALUES (1, 'a')").await;
    let first_unflushed = current_lsn(&pool).await;
    for id in 2..=ROWS {
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'a')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let target = current_lsn(&pool).await;
    let expected: BTreeSet<String> = (0..=ROWS).map(|id| format!("upsert users {id}")).collect();

    wait_for_ops(&run, &fast, expected.len()).await;
    wait_for_ops(&run, &live_only, expected.len()).await;
    // The slot may confirm idle WAL past `before_stall`, but never the end of
    // the first commit the stalled sink holds.
    let window = Instant::now();
    while window.elapsed() < STALL_WINDOW {
        run.alive();
        assert!(
            !passes(&pool, SlotLsn::Confirmed, &first_unflushed).await,
            "the slot confirmed a change the stalled sink has not flushed: {}",
            describe_slot(&pool).await
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(stalled.ops(), ["upsert users 0"]);

    open.store(true, Ordering::Release);
    wait_until_released_within(
        &run,
        &pool,
        &target,
        STALL_WINDOW,
        "G: stalled sink caught up",
    )
    .await;
    for sink in [&fast, &stalled, &live_only] {
        let ops: BTreeSet<String> = sink.ops().into_iter().collect();
        assert_eq!(ops, expected);
    }
    stop(run).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_live_only_sink_beside_a_backfilled_one_does_not_pin_the_slot() {
    const BACKFILLED: usize = 1000;
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int);
         INSERT INTO users SELECT g, 'u', 0 FROM generate_series(1, 1000) g;",
    )
    .await;
    let backfilled = Arc::new(RecordingSink::default());
    let live_only = Arc::new(RecordingSink::default());
    let sinks = vec![
        SinkSpec::new("backfilled", &backfilled, true),
        SinkSpec::new("live_only", &live_only, false),
    ];
    let run = deploy(&url, users_spec(), sinks, Tuning::default()).await;
    wait_until_live(&run, &pool, &backfilled, BACKFILLED).await;

    for id in 1..=100 {
        sqlx::query("UPDATE users SET email = $1 WHERE id = $2")
            .bind(format!("e{id}"))
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let target = current_lsn(&pool).await;

    wait_until_released(&run, &pool, &target, "G: live-only sink").await;
    let live: BTreeSet<String> = (1..=100).map(|id| format!("upsert users {id}")).collect();
    let received: BTreeSet<String> = live_only.ops().into_iter().collect();
    assert_eq!(received, live, "the live-only sink gets live changes only");
    assert_eq!(
        backfilled.ops().len(),
        BACKFILLED + live.len(),
        "the backfilled sink gets the snapshot and every live change"
    );
    stop(run).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn the_slot_confirms_within_seconds_of_the_last_flush() {
    const ROWS: i32 = 200;
    let (_container, url) = start_postgres().await;
    let pool = connect(&url).await;
    exec(
        &pool,
        "CREATE TABLE users (id int PRIMARY KEY, email text, last_seen int);
         INSERT INTO users SELECT g, 'u' || g, 0 FROM generate_series(1, 200) g;",
    )
    .await;
    let sink = Arc::new(RecordingSink::default());
    let run = deploy(&url, users_spec(), vec![primary(&sink)], Tuning::default()).await;
    wait_until_live(&run, &pool, &sink, 200).await;

    for round in 0..3 {
        let expected = sink.ops().len() + usize::try_from(ROWS).unwrap();
        for id in 1..=ROWS {
            sqlx::query("UPDATE users SET email = $1 WHERE id = $2")
                .bind(format!("burst{round}-{id}"))
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let target = current_lsn(&pool).await;
        wait_for_ops(&run, &sink, expected).await;
        let case = format!("H: burst {round}");
        wait_until_released_after_flush(&run, &pool, &sink, &target, &case).await;
    }

    for id in 1..=3 {
        let expected = sink.ops().len() + 1;
        sqlx::query("UPDATE users SET email = 'lone' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let target = current_lsn(&pool).await;
        wait_for_ops(&run, &sink, expected).await;
        let case = format!("H: lone change {id}");
        wait_until_released_after_flush(&run, &pool, &sink, &target, &case).await;
        assert_eq!(sink.ops().last().unwrap(), &format!("upsert users {id}"));
    }
    stop(run).await;
}

/// A sink that records the operations it flushed and whether its backfill
/// completed. A gated one blocks in `flush` until its gate opens, holding its
/// lane's acks back.
#[derive(Debug, Default)]
struct RecordingSink {
    applied: Mutex<Vec<String>>,
    flushed: Mutex<Vec<String>>,
    last_flush: Mutex<Option<Instant>>,
    seeded: AtomicBool,
    open: Option<Arc<AtomicBool>>,
}

impl RecordingSink {
    fn gated(open: Arc<AtomicBool>) -> Self {
        Self {
            open: Some(open),
            ..Self::default()
        }
    }

    fn ops(&self) -> Vec<String> {
        self.flushed.lock().unwrap().clone()
    }

    /// When the last flush that landed an operation returned.
    fn last_flush(&self) -> Instant {
        self.last_flush.lock().unwrap().unwrap()
    }
}

#[async_trait]
impl Sink for RecordingSink {
    async fn apply(&self, envelope: &Envelope) -> SinkResult<()> {
        self.applied.lock().unwrap().push(format!(
            "{} {} {}",
            envelope.op,
            envelope.index.as_ref(),
            envelope.id
        ));
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> SinkResult<sink::FlushReport> {
        if let Some(open) = &self.open {
            while !open.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        let applied = std::mem::take(&mut *self.applied.lock().unwrap());
        if !applied.is_empty() {
            *self.last_flush.lock().unwrap() = Some(Instant::now());
            self.flushed.lock().unwrap().extend(applied);
        }
        Ok(sink::FlushReport::clean())
    }

    async fn mark_seeded(&self, _: &IndexName) -> SinkResult<()> {
        self.seeded.store(true, Ordering::Release);
        Ok(())
    }
}

struct SinkSpec {
    name: SinkName,
    sink: Arc<RecordingSink>,
    backfill: bool,
}

impl SinkSpec {
    fn new(name: &str, sink: &Arc<RecordingSink>, backfill: bool) -> Self {
        Self {
            name: SinkName::try_new(name).unwrap(),
            sink: Arc::clone(sink),
            backfill,
        }
    }
}

fn primary(sink: &Arc<RecordingSink>) -> SinkSpec {
    SinkSpec::new("primary", sink, true)
}

#[derive(Default)]
struct Tuning {
    batch: BatchPolicy,
}

/// The daemon's assembly with the CLI's capture: the publication and replica
/// identity managed, the ingest engine and every sink engine running until one
/// of them stops.
async fn deploy(url: &str, spec: SourceSpec, sinks: Vec<SinkSpec>, tuning: Tuning) -> Deployment {
    let port = url.rsplit(':').next().unwrap().split('/').next().unwrap();
    let replication =
        ReplicationConfig::new("127.0.0.1", "postgres", "postgres", "postgres", SLOT, SLOT)
            .with_port(port.parse().unwrap());
    let capture: Arc<dyn ChangeCapture> = Arc::new(
        WalChangeCapture::new(replication, url.to_owned())
            .with_publication_management(spec.all_tables(), true)
            .with_pre_image_management(spec.pre_images(), true),
    );
    let documents: Arc<dyn DocumentBuilder> = Arc::new(
        PgDocumentBuilder::connect(url, Arc::new(spec))
            .await
            .unwrap(),
    );
    let names: Vec<SinkName> = sinks.iter().map(|s| s.name.clone()).collect();
    let stream: Arc<dyn Stream> = Arc::new(ChannelStream::new(1024, names.clone()));
    let continuity = capture.continuity().await.unwrap();
    let mappings = documents.index_mappings().await.unwrap();

    let mut engines: Vec<BoxFuture<'static, engine::Result<()>>> = Vec::new();
    for spec in sinks {
        let engine = SinkEngine::new(spec.name, spec.sink, Arc::clone(&stream), mappings.clone())
            .with_options(SinkOptions {
                backfill: spec.backfill,
            });
        engine.stage(continuity).await.unwrap();
        engines.push(
            async move {
                let (_control, mut control_rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
                engine.run(&mut control_rx).await
            }
            .boxed(),
        );
    }
    let ingest = IngestEngine::new(capture, documents, stream, names).with_batch(tuning.batch);
    engines.push(async move { ingest.run().await }.boxed());
    let stopped = Arc::new(Mutex::new(None));
    let outcome = Arc::clone(&stopped);
    let task = tokio::spawn(async move {
        let result = select_all(engines).await.0;
        *outcome.lock().unwrap() = Some(format!("{result:?}"));
    });
    Deployment { task, stopped }
}

/// A running deployment: its task, and how it ended once one engine stopped.
struct Deployment {
    task: JoinHandle<()>,
    stopped: Arc<Mutex<Option<String>>>,
}

impl Deployment {
    /// Fails the test if an engine stopped, so a wait doesn't spin to its bound.
    fn alive(&self) {
        if let Some(result) = &*self.stopped.lock().unwrap() {
            panic!("the deployment stopped: {result}");
        }
        assert!(
            !self.task.is_finished(),
            "the deployment stopped: it panicked"
        );
    }
}

/// Fails if the deployment stopped on its own, then stops it.
async fn stop(run: Deployment) {
    run.alive();
    run.task.abort();
}

async fn connect(url: &str) -> PgPool {
    PgPoolOptions::new().connect(url).await.unwrap()
}

async fn exec(pool: &PgPool, sql: &'static str) {
    sqlx::raw_sql(sql).execute(pool).await.unwrap();
}

async fn current_lsn(pool: &PgPool) -> String {
    sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Waits for the slot to exist (the source prepared) and the backfill to
/// complete. Waiting on the op count alone would let a write the test makes
/// next land in the snapshot as well.
async fn wait_until_live(run: &Deployment, pool: &PgPool, sink: &RecordingSink, backfilled: usize) {
    let deadline = Instant::now() + SETUP_BOUND;
    loop {
        run.alive();
        let slot: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM pg_replication_slots WHERE slot_name = $1")
                .bind(SLOT)
                .fetch_optional(pool)
                .await
                .unwrap();
        let seeded = sink.seeded.load(Ordering::Acquire);
        if slot.is_some() && seeded && sink.ops().len() == backfilled {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "not live: slot {slot:?}, seeded {seeded}, {} of {backfilled} backfilled",
            sink.ops().len()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_ops(run: &Deployment, sink: &RecordingSink, count: usize) {
    let deadline = Instant::now() + SETUP_BOUND;
    while sink.ops().len() < count {
        run.alive();
        assert!(
            Instant::now() < deadline,
            "{} of {count} ops flushed",
            sink.ops().len()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A position the slot holds.
#[derive(Clone, Copy)]
enum SlotLsn {
    /// `confirmed_flush_lsn`: what flusso confirmed.
    Confirmed,
    /// `restart_lsn`: the oldest WAL the slot retains.
    Restart,
}

/// Whether the slot's `lsn` is at or past `target`.
async fn passes(pool: &PgPool, lsn: SlotLsn, target: &str) -> bool {
    let sql = match lsn {
        SlotLsn::Confirmed => {
            "SELECT pg_wal_lsn_diff(confirmed_flush_lsn, $1::pg_lsn)::bigint \
             FROM pg_replication_slots WHERE slot_name = $2"
        }
        SlotLsn::Restart => {
            "SELECT pg_wal_lsn_diff(restart_lsn, $1::pg_lsn)::bigint \
             FROM pg_replication_slots WHERE slot_name = $2"
        }
    };
    let ahead: i64 = sqlx::query_scalar(sql)
        .bind(target)
        .bind(SLOT)
        .fetch_one(pool)
        .await
        .unwrap();
    ahead >= 0
}

/// Waits for `lsn` to pass `target`, failing past `bound`; returns how long it
/// took.
async fn wait_until_passes(
    run: &Deployment,
    pool: &PgPool,
    lsn: SlotLsn,
    target: &str,
    bound: Duration,
    case: &str,
) -> Duration {
    let start = Instant::now();
    loop {
        run.alive();
        if passes(pool, lsn, target).await {
            return start.elapsed();
        }
        if start.elapsed() > bound {
            let name = match lsn {
                SlotLsn::Confirmed => "confirmed_flush_lsn",
                SlotLsn::Restart => "restart_lsn",
            };
            panic!(
                "[{case}] {name} did not pass {target} within {bound:?}: {}",
                describe_slot(pool).await
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Waits, within [`CONFIRM_BOUND`], for the slot to confirm `target`.
async fn wait_until_confirmed(run: &Deployment, pool: &PgPool, target: &str, case: &str) {
    let confirmed =
        wait_until_passes(run, pool, SlotLsn::Confirmed, target, CONFIRM_BOUND, case).await;
    println!("[{case}] confirmed after {confirmed:?}");
}

/// Waits for the slot to confirm `target` within [`CONFIRM_BOUND`] and to
/// retain no WAL before it within [`RELEASE_BOUND`], and prints both times.
async fn wait_until_released(run: &Deployment, pool: &PgPool, target: &str, case: &str) {
    wait_until_released_within(run, pool, target, CONFIRM_BOUND, case).await;
}

/// [`wait_until_released`], with `confirm_bound` for `confirmed_flush_lsn`.
async fn wait_until_released_within(
    run: &Deployment,
    pool: &PgPool,
    target: &str,
    confirm_bound: Duration,
    case: &str,
) {
    let start = Instant::now();
    let retained_at_target = retained(pool).await;
    let confirmed =
        wait_until_passes(run, pool, SlotLsn::Confirmed, target, confirm_bound, case).await;
    let left = RELEASE_BOUND.saturating_sub(start.elapsed());
    wait_until_passes(run, pool, SlotLsn::Restart, target, left, case).await;
    println!(
        "[{case}] confirmed after {confirmed:?}, released after {:?}; retained \
         {retained_at_target} B at the target, {} B now",
        start.elapsed(),
        retained(pool).await
    );
}

/// Waits for the slot to confirm `target` within [`CONFIRM_BOUND`] of the
/// sink's last flush, and to retain no WAL before it within [`RELEASE_BOUND`];
/// prints both times, measured from that flush.
async fn wait_until_released_after_flush(
    run: &Deployment,
    pool: &PgPool,
    sink: &RecordingSink,
    target: &str,
    case: &str,
) {
    let flushed = sink.last_flush();
    let left = CONFIRM_BOUND.saturating_sub(flushed.elapsed());
    wait_until_passes(run, pool, SlotLsn::Confirmed, target, left, case).await;
    let confirmed = flushed.elapsed();
    let left = RELEASE_BOUND.saturating_sub(flushed.elapsed());
    wait_until_passes(run, pool, SlotLsn::Restart, target, left, case).await;
    println!(
        "[{case}] confirmed {confirmed:?} after the last flush, released after {:?}",
        flushed.elapsed()
    );
}

/// The WAL the slot holds back: `pg_current_wal_lsn() - restart_lsn`, in bytes.
async fn retained(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn)::bigint \
         FROM pg_replication_slots WHERE slot_name = $1",
    )
    .bind(SLOT)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn describe_slot(pool: &PgPool) -> String {
    sqlx::query_scalar(
        "SELECT format('current %s, confirmed %s, restart %s', pg_current_wal_lsn(), \
         confirmed_flush_lsn, restart_lsn) FROM pg_replication_slots WHERE slot_name = $1",
    )
    .bind(SLOT)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Top-level (`parent_id IS NULL`), not soft-deleted `nodes`, with their `notes`.
fn nodes_spec() -> SourceSpec {
    let notes = Join {
        table: table("notes"),
        kind: JoinKind::HasMany {
            foreign_key: column("node_id"),
        },
        primary_key: column("id"),
        nullable: false,
        filters: None,
        order_by: None,
        limit: None,
        indexed: true,
        fields: vec![column_field("body", "body")],
        on: vec![],
    };
    let schema = IndexSchema {
        version: 1,
        table: table("nodes"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: Some(SoftDelete::Column(SoftDeleteColumn {
            column: column("deleted"),
            when: None,
        })),
        filters: Some(vec![Filter::NullCheck(NullCheckFilter {
            column: column("parent_id"),
            op: NullOp::IsNull,
        })]),
        fields: vec![
            column_field("id", "id"),
            column_field("name", "name"),
            Field {
                field: field("notes"),
                options: Default::default(),
                emit: true,
                source: FieldSource::Relation(Relation::Join(notes)),
            },
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("nodes"), schema)]))
}
