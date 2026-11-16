//! Issue #343: a change is read only once its transaction is visible.
//!
//! Postgres flushes a commit record before it takes the transaction out of the
//! proc array, so logical replication can deliver a change no other session
//! sees yet. Every scenario holds commits in that window
//! ([`common::SyncRep`]: a synchronous standby that never answers), then
//! releases them, and asserts what a reader must see:
//!
//! - **Nothing is written while a commit is held**, for longer than any grace
//!   period a "wait a bit" fix would pick ([`HELD`]), and **the document lands
//!   promptly once it is released** ([`LANDS`]): the pipeline waits for
//!   visibility, not for time.
//! - Every document the commit touches is then indexed **as committed**: a root
//!   update, a child insert, re-parent and delete, a count over the children.
//! - Order holds: an earlier held commit holds back every later one, a later one
//!   released first included, and the slot never confirms past it.
//! - A restart, a backfill and a transaction split across batches during a hold;
//!   a prepared transaction; a held commit on a table no index reads, which
//!   holds back nothing.
//! - flusso refuses to run as a synchronous standby of the server it reads,
//!   whose commits would wait on flusso while flusso waits on them.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --run-ignored all -E 'binary(visibility)'
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use engine::{IngestEngine, SinkControl, SinkEngine};
use kernel::{
    Aggregate, AggregateKey, AggregateOp, DatabaseSchema, Envelope, Field, FieldSource, IndexName,
    IndexSchema, Join, JoinKind, Relation, SinkName,
};
use sink::{Result as SinkResult, Sink};
use source::SourceSpec;
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source_postgres::{PgDocumentBuilder, WalChangeCapture};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use stream::Stream;
use stream_channel::ChannelStream;

mod common;
use common::{SyncRep, column, column_field, field, index_name, replication, sent_to, table};

/// How long a commit stays held with nothing written: past any fixed delay a
/// pipeline could wait out instead of checking visibility.
const HELD: Duration = Duration::from_secs(8);

/// How soon a released commit must be indexed.
const LANDS: Duration = Duration::from_secs(1);

/// A parent document as the tests compare it: its name, its kids' names
/// sorted, and their count.
type Parent = (String, Vec<String>, i64);

fn parent(name: &str, kids: &[&str], count: i64) -> Parent {
    (
        name.to_owned(),
        kids.iter().map(|kid| (*kid).to_owned()).collect(),
        count,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_root_update_read_before_it_is_visible_is_indexed_as_committed() {
    a_commit_read_before_it_is_visible(
        "UPDATE parents SET name = 'p1 renamed' WHERE id = 1",
        [
            (1, parent("p1 renamed", &["k1"], 1)),
            (2, parent("p2", &[], 0)),
        ],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_child_inserted_before_it_is_visible_rebuilds_its_parent() {
    a_commit_read_before_it_is_visible(
        "INSERT INTO kids VALUES (2, 1, 'k2')",
        [
            (1, parent("p1", &["k1", "k2"], 2)),
            (2, parent("p2", &[], 0)),
        ],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_child_reparented_before_it_is_visible_rebuilds_both_parents() {
    a_commit_read_before_it_is_visible(
        "UPDATE kids SET parent = 2 WHERE id = 1",
        [(1, parent("p1", &[], 0)), (2, parent("p2", &["k1"], 1))],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_child_deleted_before_it_is_visible_leaves_its_parent() {
    a_commit_read_before_it_is_visible(
        "DELETE FROM kids WHERE id = 1",
        [(1, parent("p1", &[], 0)), (2, parent("p2", &[], 0))],
    )
    .await;
}

/// A count over the child table, moved by two rows of one transaction.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_count_over_children_committed_before_they_are_visible_counts_them() {
    a_commit_read_before_it_is_visible(
        "INSERT INTO kids VALUES (2, 2, 'k2'), (3, 2, 'k3')",
        [
            (1, parent("p1", &["k1"], 1)),
            (2, parent("p2", &["k2", "k3"], 2)),
        ],
    )
    .await;
}

/// `sql` commits as one held transaction: nothing is written for [`HELD`],
/// then, once released, both parents read `after` within [`LANDS`].
async fn a_commit_read_before_it_is_visible(sql: &str, after: [(i32, Parent); 2]) {
    let mut scene = Scene::new(&[], "").await;
    scene.start().await;
    scene.assert_parents(
        &[(1, parent("p1", &["k1"], 1)), (2, parent("p2", &[], 0))],
        "the backfill",
    );
    let sync = SyncRep::configure(&scene.pool).await;
    scene.forget_ops();
    let held = sync.hold(sql).await;
    sent_to(&scene.pool, "flusso").await;
    scene.quiet(HELD, "while the commit is not visible").await;
    held.release().await;
    scene.lands(&after).await;
}

/// Releasing the later of two held commits first must not let it through:
/// visibility follows no commit order, so checking only a batch's newest
/// transaction would build the earlier one from the state before its commit.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_later_commit_released_first_waits_for_the_earlier_one() {
    let mut scene = Scene::new(&[], "").await;
    scene.start().await;
    let sync = SyncRep::configure(&scene.pool).await;
    scene.forget_ops();
    let earlier = sync
        .hold("UPDATE parents SET name = 'p1 earlier' WHERE id = 1")
        .await;
    let later = sync
        .hold("UPDATE parents SET name = 'p2 later' WHERE id = 2")
        .await;
    sent_to(&scene.pool, "flusso").await;
    scene
        .quiet(Duration::from_secs(3), "while both commits are held")
        .await;
    later.release().await;
    scene
        .quiet(
            Duration::from_secs(5),
            "while the later commit is visible and the earlier one is not",
        )
        .await;
    earlier.release().await;
    scene
        .lands(&[
            (1, parent("p1 earlier", &["k1"], 1)),
            (2, parent("p2 later", &[], 0)),
        ])
        .await;
}

/// A held commit holds back every commit after it, visible ones and those on
/// tables no index reads included, and the slot confirms nothing past it.
/// Once released, the commits are indexed in commit order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_held_commit_holds_back_every_later_one_and_the_slot() {
    let mut scene = Scene::new(&[], "").await;
    scene.start().await;
    let sync = SyncRep::configure(&scene.pool).await;
    scene.forget_ops();
    let before_hold = scene.current_lsn().await;
    let held = sync
        .hold("UPDATE parents SET name = 'p1 held' WHERE id = 1")
        .await;
    scene
        .exec("UPDATE parents SET name = 'p2 later' WHERE id = 2")
        .await;
    scene.exec("INSERT INTO noise VALUES (1)").await;
    sent_to(&scene.pool, "flusso").await;
    scene
        .quiet(HELD, "while an earlier commit is not visible")
        .await;
    let waited = scene
        .capture
        .as_ref()
        .and_then(|capture| capture.readable_wait());
    assert!(
        waited.is_some_and(|waited| waited >= HELD - Duration::from_secs(1)),
        "the capture reports the hold: {waited:?}"
    );
    let confirmed = scene.confirmed_lsn().await;
    assert!(
        scene.at_or_before(&confirmed, &before_hold).await,
        "the slot confirmed {confirmed} past the held commit, which starts after {before_hold}"
    );
    held.release().await;
    scene
        .lands(&[
            (1, parent("p1 held", &["k1"], 1)),
            (2, parent("p2 later", &[], 0)),
        ])
        .await;
    assert_eq!(
        scene
            .capture
            .as_ref()
            .and_then(|capture| capture.readable_wait()),
        None,
        "nothing waits once the commit is visible"
    );
    let ops = scene.ops();
    let first = |op: &str| ops.iter().position(|recorded| recorded == op);
    assert!(
        first("upsert parents 1") < first("upsert parents 2"),
        "indexed in commit order: {ops:?}"
    );
}

/// flusso restarting while a commit is held gets it again from the slot and
/// builds it once it is visible.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_restart_during_a_hold_builds_the_commit_once_it_is_visible() {
    let mut scene = Scene::new(&[], "").await;
    scene.start().await;
    let sync = SyncRep::configure(&scene.pool).await;
    scene.forget_ops();
    let held = sync
        .hold("UPDATE parents SET name = 'p1 held' WHERE id = 1")
        .await;
    sent_to(&scene.pool, "flusso").await;
    scene
        .quiet(Duration::from_secs(3), "before the restart")
        .await;
    scene.stop().await;
    scene.start().await;
    scene
        .quiet(Duration::from_secs(5), "after the restart, still held")
        .await;
    held.release().await;
    scene
        .lands(&[
            (1, parent("p1 held", &["k1"], 1)),
            (2, parent("p2", &[], 0)),
        ])
        .await;
}

/// A backfill that starts while a commit is held reads the state before it;
/// the commit itself is built once it is visible, after the backfill's rows.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_backfill_during_a_hold_is_followed_by_the_commit() {
    let mut scene = Scene::new(&[], "").await;
    let sync = SyncRep::configure(&scene.pool).await;
    let held = sync
        .hold("UPDATE parents SET name = 'p1 held' WHERE id = 1")
        .await;
    scene.start().await;
    scene.assert_parents(
        &[(1, parent("p1", &["k1"], 1)), (2, parent("p2", &[], 0))],
        "the backfill reads the state before the held commit",
    );
    scene.forget_ops();
    scene
        .quiet(Duration::from_secs(5), "after the backfill, while held")
        .await;
    held.release().await;
    scene
        .lands(&[
            (1, parent("p1 held", &["k1"], 1)),
            (2, parent("p2", &[], 0)),
        ])
        .await;
}

/// A held transaction larger than a batch is all held, then all indexed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_held_transaction_spanning_several_batches_is_indexed_whole() {
    let mut scene = Scene::new(
        &[],
        "INSERT INTO parents SELECT n, 'p' || n FROM generate_series(3, 1000) n",
    )
    .await;
    scene.start().await;
    let sync = SyncRep::configure(&scene.pool).await;
    scene.forget_ops();
    let held = sync.hold("UPDATE parents SET name = 'big'").await;
    sent_to(&scene.pool, "flusso").await;
    scene
        .quiet(HELD, "while the large transaction is held")
        .await;
    held.release().await;
    let released = Instant::now();
    let deadline = released + Duration::from_secs(5);
    loop {
        let renamed = (1..=1000)
            .filter(|id| scene.parent(*id).is_some_and(|(name, _, _)| name == "big"))
            .count();
        if renamed == 1000 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "{renamed} of 1000 parents indexed as committed 5 s after the release"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A held commit on a table no index reads holds back nothing: nothing reads
/// its rows, so it needs no visibility.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_held_commit_on_a_table_no_index_reads_holds_back_nothing() {
    let mut scene = Scene::new(&[], "").await;
    scene.start().await;
    let sync = SyncRep::configure(&scene.pool).await;
    let held = sync.hold("INSERT INTO noise VALUES (1)").await;
    sent_to(&scene.pool, "flusso").await;
    scene
        .exec("UPDATE parents SET name = 'p2 meanwhile' WHERE id = 2")
        .await;
    scene
        .lands(&[
            (1, parent("p1", &["k1"], 1)),
            (2, parent("p2 meanwhile", &[], 0)),
        ])
        .await;
    held.release().await;
}

/// `COMMIT PREPARED` waits on the standby like a commit, between its WAL
/// record and the proc array letting the prepared transaction go.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_prepared_transaction_is_indexed_once_its_commit_is_visible() {
    let mut scene = Scene::new(&["-c", "max_prepared_transactions=10"], "").await;
    scene.start().await;
    scene
        .exec(
            "BEGIN; UPDATE parents SET name = 'p1 prepared' WHERE id = 1; \
             PREPARE TRANSACTION 'flusso_343'",
        )
        .await;
    let sync = SyncRep::configure(&scene.pool).await;
    scene.forget_ops();
    let held = sync
        .hold_statement("COMMIT PREPARED 'flusso_343'".to_owned())
        .await;
    sent_to(&scene.pool, "flusso").await;
    scene
        .quiet(HELD, "while the prepared commit is not visible")
        .await;
    held.release().await;
    scene
        .lands(&[
            (1, parent("p1 prepared", &["k1"], 1)),
            (2, parent("p2", &[], 0)),
        ])
        .await;
}

/// A server that names flusso a synchronous standby would make every commit
/// wait on flusso while flusso waits for the commit: it refuses to start.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn flusso_refuses_to_start_as_a_synchronous_standby() {
    let mut scene = Scene::new(&[], "").await;
    scene.standby_names("FIRST 1 (other, \"Flusso\")").await;
    scene.spawn();
    let error = scene.stops_within(Duration::from_secs(15)).await;
    assert!(
        error.contains("synchronous_standby_names"),
        "the refusal names the setting: {error}"
    );
}

/// A server reconfigured to name flusso a synchronous standby while it runs:
/// the first commit then waits on flusso, flusso notices the wait it cannot
/// end, and stops with the same message instead of waiting forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn flusso_named_a_synchronous_standby_while_running_stops() {
    let mut scene = Scene::new(&[], "").await;
    scene.start().await;
    scene.standby_names("flusso").await;
    common::standbys_in_force(&scene.pool).await;
    let pool = scene.pool.clone();
    let waiting = tokio::spawn(async move {
        sqlx::query("UPDATE parents SET name = 'p1 waits on flusso' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
    });
    let error = scene.stops_within(Duration::from_secs(30)).await;
    assert!(
        error.contains("synchronous_standby_names"),
        "the stop names the setting: {error}"
    );
    scene.standby_names("").await;
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("the commit returns once flusso is no longer a standby")
        .unwrap();
}

/// One Postgres, its `flusso` slot and publication, and the engines over a
/// [`DocumentSink`], started and stopped as a test needs.
struct Scene {
    _container: test_postgres::Container,
    port: u16,
    url: String,
    pool: PgPool,
    sink: Arc<DocumentSink>,
    capture: Option<Arc<WalChangeCapture>>,
    engine: Option<tokio::task::JoinHandle<engine::Result<()>>>,
}

impl Scene {
    /// Parents 1 and 2, kid 1 under parent 1, a `noise` table no index reads,
    /// all published, the slot made; then `setup`.
    async fn new(server: &[&str], setup: &str) -> Self {
        let (container, port, url) = common::start_postgres_with(server).await;
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(&url)
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE parents (id int PRIMARY KEY, name text); \
             CREATE TABLE kids (id int PRIMARY KEY, parent int, name text); \
             CREATE TABLE noise (id int PRIMARY KEY); \
             ALTER TABLE kids REPLICA IDENTITY FULL; \
             CREATE PUBLICATION flusso FOR TABLE parents, kids, noise; \
             INSERT INTO parents VALUES (1, 'p1'), (2, 'p2'); \
             INSERT INTO kids VALUES (1, 1, 'k1');",
        )
        .execute(&pool)
        .await
        .unwrap();
        if !setup.is_empty() {
            sqlx::raw_sql(sqlx::AssertSqlSafe(setup))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')")
            .execute(&pool)
            .await
            .unwrap();
        Self {
            _container: container,
            port,
            url,
            pool,
            sink: Arc::new(DocumentSink::default()),
            capture: None,
            engine: None,
        }
    }

    /// Start the engines, as the CLI builds the capture: it knows which
    /// tables the indexes read.
    fn spawn(&mut self) {
        let spec = parents_with_kids_spec();
        let capture = Arc::new(
            WalChangeCapture::new(replication(self.port), self.url.clone())
                .with_publication_management(spec.all_tables(), false),
        );
        self.capture = Some(Arc::clone(&capture));
        let url = self.url.clone();
        let sink = Arc::clone(&self.sink) as Arc<dyn Sink>;
        self.engine = Some(tokio::spawn(async move {
            let documents = PgDocumentBuilder::connect(&url, Arc::new(spec)).await?;
            run_deployment(capture, Arc::new(documents), sink).await
        }));
    }

    /// Start the engines and return once the backfill is done and the stream
    /// has everything written so far.
    async fn start(&mut self) {
        self.spawn();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !self.sink.seeded.lock().unwrap().contains("parents") {
            self.assert_running().await;
            assert!(Instant::now() < deadline, "the backfill never completed");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        sent_to(&self.pool, "flusso").await;
    }

    /// Stop the engines and the walsender serving them, as a crashed flusso
    /// leaves the slot.
    async fn stop(&mut self) {
        if let Some(engine) = self.engine.take() {
            engine.abort();
            let _ = engine.await;
        }
        sqlx::query(
            "SELECT pg_terminate_backend(active_pid) FROM pg_replication_slots \
             WHERE slot_name = 'flusso' AND active_pid IS NOT NULL",
        )
        .execute(&self.pool)
        .await
        .unwrap();
        common::eventually("the slot released", async || {
            sqlx::query_scalar::<_, bool>(
                "SELECT NOT active FROM pg_replication_slots WHERE slot_name = 'flusso'",
            )
            .fetch_one(&self.pool)
            .await
            .unwrap()
        })
        .await;
    }

    /// Fail with the engines' outcome if they stopped.
    async fn assert_running(&mut self) {
        if self
            .engine
            .as_ref()
            .is_some_and(|engine| engine.is_finished())
            && let Some(engine) = self.engine.take()
        {
            panic!(
                "the engines stopped: {:?}; operations so far: {:?}",
                engine.await,
                self.ops()
            );
        }
    }

    /// Wait for the engines to stop on an error, and return it.
    async fn stops_within(&mut self, limit: Duration) -> String {
        let engine = self.engine.take().expect("the engines were started");
        match tokio::time::timeout(limit, engine).await {
            Err(_) => panic!("still running after {limit:?}"),
            Ok(joined) => match joined.unwrap() {
                Ok(()) => panic!("the engines stopped without an error"),
                Err(error) => error.to_string(),
            },
        }
    }

    /// Set `synchronous_standby_names` server-wide, and wait until a new
    /// session reads it.
    async fn standby_names(&self, names: &str) {
        let quoted = names.replace('\'', "''");
        common::configure(
            &self.pool,
            &[&format!(
                "ALTER SYSTEM SET synchronous_standby_names = '{quoted}'"
            )],
        )
        .await;
        let mut fresh = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.url)
            .await
            .unwrap();
        common::eventually("the reloaded standby names", async || {
            let shown: String = sqlx::query_scalar("SHOW synchronous_standby_names")
                .fetch_one(&fresh)
                .await
                .unwrap();
            if shown == names {
                return true;
            }
            fresh = PgPoolOptions::new()
                .max_connections(1)
                .connect(&self.url)
                .await
                .unwrap();
            false
        })
        .await;
    }

    async fn exec(&self, sql: &str) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn current_lsn(&self) -> String {
        sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn confirmed_lsn(&self) -> String {
        sqlx::query_scalar(
            "SELECT confirmed_flush_lsn::text FROM pg_replication_slots \
             WHERE slot_name = 'flusso'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn at_or_before(&self, lsn: &str, bound: &str) -> bool {
        sqlx::query_scalar("SELECT $1::pg_lsn <= $2::pg_lsn")
            .bind(lsn)
            .bind(bound)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    fn ops(&self) -> Vec<String> {
        self.sink.ops.lock().unwrap().clone()
    }

    fn forget_ops(&self) {
        self.sink.ops.lock().unwrap().clear();
    }

    /// Assert the sink writes nothing for `period`: no fixed delay, but the
    /// window a pipeline that reads early would write in.
    async fn quiet(&mut self, period: Duration, during: &str) {
        let until = Instant::now() + period;
        while Instant::now() < until {
            self.assert_running().await;
            let ops = self.ops();
            assert!(ops.is_empty(), "written {during}: {ops:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn parent(&self, id: i32) -> Option<Parent> {
        let documents = self.sink.documents.lock().unwrap();
        let document = documents.get(&format!("parents {id}"))?.as_ref()?;
        let mut kids: Vec<String> = document["kids"]
            .as_array()
            .map(|kids| {
                kids.iter()
                    .map(|kid| kid["name"].as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        kids.sort_unstable();
        Some((
            document["name"].as_str().unwrap().to_owned(),
            kids,
            document["kidCount"].as_i64().unwrap(),
        ))
    }

    fn parents(&self, expected: &[(i32, Parent)]) -> Vec<(i32, Option<Parent>)> {
        expected
            .iter()
            .map(|(id, _)| (*id, self.parent(*id)))
            .collect()
    }

    fn assert_parents(&self, expected: &[(i32, Parent)], what: &str) {
        let wanted: Vec<(i32, Option<Parent>)> = expected
            .iter()
            .map(|(id, parent)| (*id, Some(parent.clone())))
            .collect();
        assert_eq!(self.parents(expected), wanted, "{what}");
    }

    /// The parents read `expected` within [`LANDS`], and still do once a
    /// later commit is indexed.
    async fn lands(&mut self, expected: &[(i32, Parent)]) {
        let wanted: Vec<(i32, Option<Parent>)> = expected
            .iter()
            .map(|(id, parent)| (*id, Some(parent.clone())))
            .collect();
        let deadline = Instant::now() + LANDS;
        while self.parents(expected) != wanted {
            self.assert_running().await;
            assert!(
                Instant::now() < deadline,
                "not indexed as committed within {LANDS:?}: {:?}, expected {wanted:?}; \
                 operations: {:?}",
                self.parents(expected),
                self.ops()
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.exec("INSERT INTO parents VALUES (900, 'marker')")
            .await;
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.parent(900).is_none() {
            self.assert_running().await;
            assert!(Instant::now() < deadline, "the marker was never indexed");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(self.parents(expected), wanted, "after a later commit");
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

/// A sink that keeps the latest version of every document, keyed `index id`,
/// as the destination would hold it (`None`: deleted), every operation in
/// order, and which indexes it has seeded, so a restart skips the backfill.
#[derive(Debug, Default)]
struct DocumentSink {
    ops: Mutex<Vec<String>>,
    documents: Mutex<BTreeMap<String, Option<serde_json::Value>>>,
    seeded: Mutex<BTreeSet<String>>,
    applied: Mutex<Vec<(String, String, Option<serde_json::Value>)>>,
}

#[async_trait]
impl Sink for DocumentSink {
    async fn is_seeded(&self, index: &IndexName) -> SinkResult<bool> {
        Ok(self.seeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn mark_seeded(&self, index: &IndexName) -> SinkResult<()> {
        self.seeded
            .lock()
            .unwrap()
            .insert(index.as_ref().to_owned());
        Ok(())
    }

    async fn apply(&self, envelope: &Envelope) -> SinkResult<()> {
        let key = format!("{} {}", envelope.index.as_ref(), envelope.id);
        self.applied.lock().unwrap().push((
            format!("{} {key}", envelope.op),
            key,
            envelope.document.as_ref().map(sink::to_json),
        ));
        Ok(())
    }

    async fn flush(&self, _caught_up: bool) -> SinkResult<sink::FlushReport> {
        let applied = std::mem::take(&mut *self.applied.lock().unwrap());
        let mut ops = self.ops.lock().unwrap();
        let mut documents = self.documents.lock().unwrap();
        for (op, key, document) in applied {
            ops.push(op);
            documents.insert(key, document);
        }
        Ok(sink::FlushReport::clean())
    }
}

/// `parents` with its `has_many` `kids` (their `name`) and a `count` of them.
fn parents_with_kids_spec() -> SourceSpec {
    let kids = Join {
        table: table("kids"),
        kind: JoinKind::HasMany {
            foreign_key: column("parent"),
        },
        primary_key: column("id"),
        nullable: true,
        filters: None,
        order_by: None,
        limit: None,
        fields: vec![column_field("name", "name")],
        indexed: true,
        on: vec![],
    };
    let kid_count = Aggregate {
        table: table("kids"),
        op: AggregateOp::Count,
        key: AggregateKey::Direct(column("parent")),
        value_type: None,
        filters: None,
        distinct: false,
        via: Vec::new(),
        on: vec![],
    };
    let relation = |name: &str, relation: Relation| Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(relation),
    };
    let parents = IndexSchema {
        version: 1,
        table: table("parents"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            column_field("name", "name"),
            relation("kids", Relation::Join(kids)),
            relation("kidCount", Relation::Aggregate(kid_count)),
        ],
    };
    SourceSpec::new(BTreeMap::from([(index_name("parents"), parents)]))
}
