//! A target deleted in the middle of its index's backfill (issue #428). The
//! next write finds it missing, the index is staged again into an empty
//! target, and the backfill that was running can no longer seed it: the pages
//! it wrote before went with the old target. A fresh backfill is requested,
//! and the index is seeded only once that one landed, every row of it.
//!
//! Both engines run over real channel lanes on paused time. The sink keeps
//! one target per index, as the OpenSearch sink keeps a generation, and
//! checks two things as they happen: a seed holds every row of the table, and
//! a recorded walk mark has every key up to it in the target it names. A
//! second sink, `audit`, shares the first snapshot and is never touched.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, SinkName, TableName};
use sink::{Seeding, Sink, WriteReport};
use source::cdc::{ChangeCapture, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::{key, mapping, page_of, sink_name, users};
use crate::{BatchPolicy, IngestEngine, Observer, Progress, SinkControl, SinkEngine};

/// The table's rows, `1..=ROWS`.
const ROWS: i64 = 80;

/// How long the sink takes for one write: a walk of the table takes long
/// enough for its marks to be recorded (every ten seconds).
const PACE: Duration = Duration::from_secs(1);

fn id_of(key: &RowKey) -> i64 {
    match key.0.first() {
        Some((_, GenericValue::BigInt(id))) => *id,
        other => panic!("not an integer key: {other:?}"),
    }
}

fn primary() -> SinkName {
    sink_name("primary")
}

fn audit() -> SinkName {
    sink_name("audit")
}

/// A source whose live stream never yields: everything here is backfill.
#[derive(Debug)]
struct Quiet;

#[async_trait]
impl ChangeCapture for Quiet {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        Ok(Box::pin(futures::stream::pending()))
    }

    fn confirm(&self, _: source::cdc::Landed) {}
}

/// The `users` table, `1..=ROWS`, walked in key order with resumable marks:
/// a mark is the key, zero-padded.
#[derive(Debug)]
struct Table {
    rows: Vec<RowKey>,
}

impl Table {
    fn new() -> Self {
        Self {
            rows: (1..=ROWS).map(key).collect(),
        }
    }
}

#[async_trait]
impl DocumentBuilder for Table {
    async fn resolve(
        &self,
        _: &TableName,
        key: &RowKey,
        _: Option<&RowImage>,
        _: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: users(),
            key: key.clone(),
        }])
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(BTreeMap::new()),
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        vec![users()]
    }

    async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
        Ok(self.rows.last().cloned())
    }

    async fn keys(
        &self,
        _: &IndexName,
        after: Option<&RowKey>,
        through: &RowKey,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        Ok(page_of(&self.rows, after, through, limit))
    }

    async fn walk_mark(&self, _: &IndexName, key: &RowKey) -> source::Result<Option<String>> {
        Ok(Some(format!("{:04}", id_of(key))))
    }

    async fn resume_after(&self, _: &IndexName, mark: &str) -> source::Result<Option<RowKey>> {
        Ok(mark.parse::<i64>().ok().map(key))
    }

    async fn compare_keys(
        &self,
        _: &IndexName,
        a: &RowKey,
        b: &RowKey,
    ) -> source::Result<Option<std::cmp::Ordering>> {
        Ok(Some(id_of(a).cmp(&id_of(b))))
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok(vec![mapping()])
    }
}

/// The index's one target, as the sink holds it.
#[derive(Debug, Default)]
struct Target {
    /// Which target: a new one is created after each deletion.
    generation: u64,
    exists: bool,
    content: BTreeSet<i64>,
    seeded: bool,
    walked: Option<String>,
}

/// A seed, with the rows its target lacked when it was marked.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seed {
    generation: u64,
    missing: Vec<i64>,
}

/// A sink keeping one target for `users`. A write holding a row named in
/// `holds` (with the target's generation) waits until the test releases it;
/// one named in `failures` fails as a whole once, deleting the target first
/// when `failure_deletes`.
#[derive(Debug, Default)]
struct Generations {
    target: Mutex<Target>,
    holds: Mutex<BTreeSet<(u64, i64)>>,
    waiting: Mutex<BTreeSet<(u64, i64)>>,
    released: Notify,
    failures: Mutex<BTreeSet<(u64, i64)>>,
    failure_deletes: bool,
    seeds: Mutex<Vec<Seed>>,
    /// A walk mark recorded with a key up to it missing from the target.
    ahead: Mutex<Vec<String>>,
    /// How many walk marks each generation recorded.
    records: Mutex<BTreeMap<u64, usize>>,
    reindexed: Mutex<usize>,
}

impl Generations {
    fn holding(self, generation: u64, row: i64) -> Self {
        self.holds.lock().unwrap().insert((generation, row));
        self
    }

    fn failing(mut self, generation: u64, row: i64, deletes: bool) -> Self {
        self.failures.lock().unwrap().insert((generation, row));
        self.failure_deletes = deletes;
        self
    }

    fn is_waiting(&self, generation: u64, row: i64) -> bool {
        self.waiting.lock().unwrap().contains(&(generation, row))
    }

    fn release(&self, generation: u64, row: i64) {
        self.holds.lock().unwrap().remove(&(generation, row));
        self.released.notify_waiters();
    }

    /// An operator deletes the target: what it held is gone.
    fn delete(&self) {
        let mut target = self.target.lock().unwrap();
        target.exists = false;
        target.content.clear();
    }

    fn seeds(&self) -> Vec<Seed> {
        self.seeds.lock().unwrap().clone()
    }

    fn content(&self) -> BTreeSet<i64> {
        self.target.lock().unwrap().content.clone()
    }

    fn ahead(&self) -> Vec<String> {
        self.ahead.lock().unwrap().clone()
    }

    fn records(&self, generation: u64) -> usize {
        self.records
            .lock()
            .unwrap()
            .get(&generation)
            .copied()
            .unwrap_or(0)
    }
}

fn row_of(envelope: &Envelope) -> i64 {
    let digits: String = envelope.id.chars().filter(char::is_ascii_digit).collect();
    digits.parse().unwrap()
}

#[async_trait]
impl Sink for Generations {
    async fn ensure_index(&self, _: &IndexMapping) -> sink::Result<()> {
        let mut target = self.target.lock().unwrap();
        if !target.exists {
            target.generation += 1;
            target.exists = true;
            target.content.clear();
            target.seeded = false;
            target.walked = None;
        }
        Ok(())
    }

    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        let rows: Vec<i64> = envelopes.iter().map(row_of).collect();
        loop {
            let released = self.released.notified();
            let generation = self.target.lock().unwrap().generation;
            let held: Vec<(u64, i64)> = rows
                .iter()
                .map(|row| (generation, *row))
                .filter(|hold| self.holds.lock().unwrap().contains(hold))
                .collect();
            if held.is_empty() {
                break;
            }
            self.waiting.lock().unwrap().extend(held);
            released.await;
        }
        tokio::time::sleep(PACE).await;
        let mut target = self.target.lock().unwrap();
        let generation = target.generation;
        let failed = rows
            .iter()
            .any(|row| self.failures.lock().unwrap().remove(&(generation, *row)));
        if failed {
            if self.failure_deletes {
                target.exists = false;
                target.content.clear();
            }
            return Err(sink::SinkError::Write("the write failed once".to_owned()));
        }
        if !target.exists {
            return Ok(WriteReport::clean().missing_target(users(), "the target is gone"));
        }
        target.content.extend(rows);
        Ok(WriteReport::clean())
    }

    async fn seeding(&self, _: &IndexName) -> sink::Result<Seeding> {
        let target = self.target.lock().unwrap();
        Ok(if target.seeded {
            Seeding::Seeded
        } else {
            Seeding::Unseeded {
                walked: target.walked.clone(),
            }
        })
    }

    async fn record_walked(&self, _: &IndexName, mark: &str) -> sink::Result<()> {
        let mut target = self.target.lock().unwrap();
        let through: i64 = mark.parse().unwrap();
        let missing: Vec<i64> = (1..=through)
            .filter(|row| !target.content.contains(row))
            .collect();
        if !missing.is_empty() {
            self.ahead.lock().unwrap().push(format!(
                "generation {} recorded {mark} without {missing:?}",
                target.generation
            ));
        }
        *self
            .records
            .lock()
            .unwrap()
            .entry(target.generation)
            .or_default() += 1;
        target.walked = Some(mark.to_owned());
        Ok(())
    }

    async fn forget_walked(&self, _: &IndexName) -> sink::Result<()> {
        self.target.lock().unwrap().walked = None;
        Ok(())
    }

    async fn mark_seeded(&self, _: &IndexName) -> sink::Result<()> {
        let mut target = self.target.lock().unwrap();
        let missing = (1..=ROWS)
            .filter(|row| !target.content.contains(row))
            .collect();
        self.seeds.lock().unwrap().push(Seed {
            generation: target.generation,
            missing,
        });
        target.seeded = true;
        target.walked = None;
        Ok(())
    }

    async fn reindex(&self, _: &IndexMapping) -> sink::Result<()> {
        *self.reindexed.lock().unwrap() += 1;
        Ok(())
    }
}

/// What the engines told the observers, per sink: backfills asked for and
/// seeds reported (what a fence follows).
#[derive(Debug, Default)]
struct Seen {
    requested: Mutex<BTreeMap<SinkName, usize>>,
    seeded: Mutex<BTreeMap<SinkName, usize>>,
}

impl Seen {
    fn requested(&self, sink: &SinkName) -> usize {
        self.requested
            .lock()
            .unwrap()
            .get(sink)
            .copied()
            .unwrap_or(0)
    }

    fn seeded(&self, sink: &SinkName) -> usize {
        self.seeded.lock().unwrap().get(sink).copied().unwrap_or(0)
    }
}

impl Observer for Seen {
    fn on_backfill_requested(&self, sink: &SinkName, indexes: &[IndexName]) {
        *self
            .requested
            .lock()
            .unwrap()
            .entry(sink.clone())
            .or_default() += indexes.len();
    }

    fn on_index_seeded(&self, sink: &SinkName, _: &IndexName) {
        *self.seeded.lock().unwrap().entry(sink.clone()).or_default() += 1;
    }
}

/// One deployment: the ingest engine and two sink engines, each restarted
/// after a failed run as the daemon does (staged again, then run).
struct Deployment {
    primary: Arc<Generations>,
    audit: Arc<Generations>,
    seen: Arc<Seen>,
    control: tokio::sync::mpsc::Sender<SinkControl>,
    tasks: Vec<JoinHandle<()>>,
}

impl Deployment {
    async fn start(sink: Generations) -> Self {
        let primary_sink = Arc::new(sink);
        let audit_sink = Arc::new(Generations::default());
        let seen = Arc::new(Seen::default());
        let names = [primary(), audit()];
        let stream = Arc::new(ChannelStream::over(64, [users()], names.clone()));
        let progress = Arc::new(Progress::of([users()], names.clone()));
        let mut engines = Vec::new();
        for (name, sink) in [
            (primary(), Arc::clone(&primary_sink) as Arc<dyn Sink>),
            (audit(), Arc::clone(&audit_sink) as Arc<dyn Sink>),
        ] {
            let engine = SinkEngine::new(
                name,
                sink,
                Arc::clone(&stream) as Arc<dyn Stream>,
                vec![mapping()],
                Arc::clone(&progress),
            )
            .with_write_documents(4)
            .with_observer(Arc::clone(&seen) as Arc<dyn Observer>);
            engine.stage(Continuity::Resumed).await.unwrap();
            engines.push(Arc::new(engine));
        }
        let mut tasks = Vec::new();
        let mut controls = Vec::new();
        for engine in engines {
            let (control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(4);
            controls.push(control);
            tasks.push(tokio::spawn(async move {
                while engine.run(&mut rx).await.is_err() {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    while engine.stage(Continuity::Resumed).await.is_err() {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }));
        }
        let ingest = IngestEngine::new(
            Arc::new(Quiet) as Arc<dyn ChangeCapture>,
            Arc::new(Table::new()) as Arc<dyn DocumentBuilder>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            names.to_vec(),
            progress,
        )
        .with_batch(BatchPolicy {
            max_changes: 4,
            max_delay: Duration::from_millis(50),
        })
        .with_observer(Arc::clone(&seen) as Arc<dyn Observer>);
        tasks.push(tokio::spawn(async move {
            ingest.run().await.unwrap();
        }));
        Self {
            primary: primary_sink,
            audit: audit_sink,
            seen,
            control: controls.swap_remove(0),
            tasks,
        }
    }

    /// Wait (up to an hour of paused time) until `condition` holds.
    async fn until(&self, what: &str, mut condition: impl FnMut(&Self) -> bool) {
        let waited = tokio::time::timeout(Duration::from_secs(3600), async {
            while !condition(self) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(waited.is_ok(), "never saw {what}");
    }

    /// Both sinks seeded, then a while more for anything that would follow.
    async fn settled(&self) {
        self.until("both sinks seeded", |at| {
            !at.primary.seeds().is_empty() && !at.audit.seeds().is_empty()
        })
        .await;
        tokio::time::sleep(Duration::from_secs(120)).await;
    }

    /// Delete the primary's target while a write of `row` to `generation`
    /// waits, then let the write go: it finds the target missing.
    async fn delete_at(&self, generation: u64, row: i64) {
        self.until(
            &format!("a write of row {row} to generation {generation}"),
            |at| at.primary.is_waiting(generation, row),
        )
        .await;
        self.primary.delete();
        self.primary.release(generation, row);
    }

    /// The audit sink was backfilled once, from the first snapshot.
    fn assert_audit_untouched(&self) {
        assert_eq!(
            self.audit.seeds(),
            vec![Seed {
                generation: 1,
                missing: Vec::new()
            }]
        );
        assert_eq!(self.seen.requested(&audit()), 1);
        assert_eq!(self.seen.seeded(&audit()), 1);
    }

    /// The primary was seeded once, in `generation`, holding every row; no
    /// walk mark ran ahead of its target, and the walk that seeded it kept
    /// recording how far it got.
    fn assert_primary_whole(&self, generation: u64) {
        assert!(
            self.primary.ahead().is_empty(),
            "{:?}",
            self.primary.ahead()
        );
        assert!(
            self.primary.records(generation) > 0,
            "the walk that seeded generation {generation} recorded nothing"
        );
        assert_eq!(
            self.primary.seeds(),
            vec![Seed {
                generation,
                missing: Vec::new()
            }],
            "seeded with rows missing"
        );
        assert_eq!(self.primary.content(), (1..=ROWS).collect::<BTreeSet<_>>());
        assert_eq!(self.seen.seeded(&primary()), 1);
    }
}

impl Drop for Deployment {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_target_deleted_mid_backfill_is_seeded_only_after_a_full_backfill() {
    let deployment = Deployment::start(Generations::default().holding(1, 41)).await;
    deployment.delete_at(1, 41).await;
    deployment.settled().await;

    deployment.assert_primary_whole(2);
    assert_eq!(
        deployment.seen.requested(&primary()),
        2,
        "the first backfill and the fresh one"
    );
    deployment.assert_audit_untouched();
}

#[tokio::test(start_paused = true)]
async fn a_second_deletion_during_the_fresh_backfill_asks_for_another() {
    let deployment = Deployment::start(Generations::default().holding(1, 41).holding(2, 5)).await;
    deployment.delete_at(1, 41).await;
    // Row 5 reaches the second target only from the fresh backfill: the first
    // went on past row 41.
    deployment.delete_at(2, 5).await;
    deployment.settled().await;

    deployment.assert_primary_whole(3);
    assert_eq!(deployment.seen.requested(&primary()), 3);
    deployment.assert_audit_untouched();
}

#[tokio::test(start_paused = true)]
async fn a_target_deleted_while_its_sink_engine_restarts_is_backfilled_again() {
    let deployment = Deployment::start(Generations::default().failing(1, 41, true)).await;
    deployment.settled().await;

    deployment.assert_primary_whole(2);
    assert_eq!(deployment.seen.requested(&primary()), 2);
    deployment.assert_audit_untouched();
}

#[tokio::test(start_paused = true)]
async fn a_sink_engine_restart_with_its_target_intact_asks_for_nothing_more() {
    let deployment = Deployment::start(Generations::default().failing(1, 41, false)).await;
    deployment.settled().await;

    deployment.assert_primary_whole(1);
    assert_eq!(
        deployment.seen.requested(&primary()),
        1,
        "the restart re-staged the index while its backfill was outstanding"
    );
    deployment.assert_audit_untouched();
}

#[tokio::test(start_paused = true)]
async fn a_reindex_while_the_fresh_backfill_runs_stages_nothing() {
    let deployment = Deployment::start(Generations::default().holding(1, 41)).await;
    deployment.delete_at(1, 41).await;
    deployment
        .until("the fresh backfill asked for", |at| {
            at.seen.requested(&primary()) == 2
        })
        .await;
    deployment
        .control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    deployment.settled().await;

    assert_eq!(*deployment.primary.reindexed.lock().unwrap(), 0);
    deployment.assert_primary_whole(2);
    assert_eq!(deployment.seen.requested(&primary()), 2);
    deployment.assert_audit_untouched();
}
