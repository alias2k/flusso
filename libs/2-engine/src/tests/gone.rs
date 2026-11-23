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
//!
//! The rules the fix rests on are tested on their own: a lane fed by hand
//! (which ticket seeds, what a stop after the cut leaves), and the ingest
//! engine answering two requests of one lane with one walk.

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
use stream::{Batch, Consumer, LaneItem, LaneKey, Producer, Request, Stream, Ticket};
use stream_channel::ChannelStream;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::{key, mapping, page_of, sink_name, users};
use crate::{BatchPolicy, IngestEngine, Observer, Progress, SinkControl, SinkEngine, Stop};

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
    /// Which target: a new one is created after each deletion, and each
    /// auto-create.
    generation: u64,
    exists: bool,
    /// Created by a write after a deletion (auto-create on), not by staging.
    impostor: bool,
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
    /// `forget_walked` fails this many times.
    forget_failures: Mutex<usize>,
    /// `ensure_index` creates a new target, then fails, this many times.
    ensure_failures_after_create: Mutex<usize>,
    /// A write to a deleted target recreates it (an impostor), as
    /// OpenSearch's default `action.auto_create_index` does, instead of
    /// reporting it missing.
    auto_create: bool,
    /// `ensure_index` is refused, changing nothing, this many times.
    ensure_refusals: Mutex<usize>,
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

/// The name `Generations` gives a target: its generation.
fn identity(generation: u64) -> sink::TargetId {
    sink::TargetId::new(generation.to_string())
}

fn row_of(envelope: &Envelope) -> i64 {
    let digits: String = envelope.id.chars().filter(char::is_ascii_digit).collect();
    digits.parse().unwrap()
}

#[async_trait]
impl Sink for Generations {
    async fn ensure_index(&self, _: &IndexMapping) -> sink::Result<sink::Ensured> {
        {
            let mut refusals = self.ensure_refusals.lock().unwrap();
            if *refusals > 0 {
                *refusals -= 1;
                return Err(sink::SinkError::Write("ensure refused".to_owned()));
            }
        }
        let mut target = self.target.lock().unwrap();
        if target.exists && !target.impostor {
            return Ok(sink::Ensured::existing(Some(identity(target.generation))));
        }
        target.generation += 1;
        target.exists = true;
        target.impostor = false;
        target.content.clear();
        target.seeded = false;
        target.walked = None;
        {
            let mut failures = self.ensure_failures_after_create.lock().unwrap();
            if *failures > 0 && target.generation > 1 {
                *failures -= 1;
                return Err(sink::SinkError::Write(
                    "created, then a later step failed".to_owned(),
                ));
            }
        }
        Ok(sink::Ensured::created(Some(identity(target.generation))))
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
        if !target.exists && self.auto_create {
            target.generation += 1;
            target.exists = true;
            target.impostor = true;
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
        // An impostor's checkpoint goes with it when staging replaces it.
        if !missing.is_empty() && !target.impostor {
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
        {
            let mut failures = self.forget_failures.lock().unwrap();
            if *failures > 0 {
                *failures -= 1;
                return Err(sink::SinkError::Write("forget failed once".to_owned()));
            }
        }
        self.target.lock().unwrap().walked = None;
        Ok(())
    }

    async fn mark_seeded(&self, _: &IndexName) -> sink::Result<()> {
        let mut target = self.target.lock().unwrap();
        if !target.exists || target.impostor {
            return Err(sink::SinkError::TargetChanged(
                "not the target staging made".to_owned(),
            ));
        }
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

/// One sink engine over the `users` lane, fed by hand, with the request lane
/// read by the test.
struct Lane {
    sink: Arc<Generations>,
    stream: Arc<ChannelStream>,
    lane: Box<dyn Producer<LaneItem>>,
    requests: Box<dyn Consumer<Request>>,
    stop: Stop,
    running: JoinHandle<crate::Result<()>>,
}

impl Lane {
    /// Staged (asking for the first backfill), then running.
    async fn start(sink: Arc<Generations>) -> Self {
        let stream = Arc::new(ChannelStream::over(1024, [users()], [primary()]));
        let stop = Stop::new();
        let engine = Arc::new(
            SinkEngine::new(
                primary(),
                Arc::clone(&sink) as Arc<dyn Sink>,
                Arc::clone(&stream) as Arc<dyn Stream>,
                vec![mapping()],
                Arc::new(Progress::of([users()], [primary()])),
            )
            .with_stop(stop.clone()),
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        let running = tokio::spawn(async move {
            let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
            engine.run(&mut rx).await
        });
        Self {
            sink,
            lane: stream
                .lane(&LaneKey::new(users(), primary()))
                .unwrap()
                .producer,
            requests: stream.requests().unwrap().consumer,
            stream,
            stop,
            running,
        }
    }

    /// The next backfill request, with its checkpoint and ticket.
    async fn request(&mut self) -> (Option<String>, Ticket) {
        let delivery = tokio::time::timeout(Duration::from_secs(600), self.requests.recv())
            .await
            .expect("no backfill request")
            .unwrap()
            .unwrap();
        let (Request::Backfill { after, ticket, .. }, handle) = delivery.into_parts() else {
            panic!("a backfill request")
        };
        handle.ack().await.unwrap();
        (after.map(|mark| mark.as_str().to_owned()), ticket)
    }

    /// A snapshot page of `rows`, as the walk answering `ticket` sends it.
    async fn page(&self, rows: std::ops::RangeInclusive<i64>, ticket: Ticket) {
        let last = *rows.end();
        let envelopes: Vec<Envelope> = rows
            .map(|row| {
                Envelope::upsert(
                    users(),
                    row.to_string(),
                    GenericValue::Map(BTreeMap::new()),
                    None,
                    chrono::Utc::now(),
                )
            })
            .collect();
        self.lane
            .publish(LaneItem::Batch(Batch {
                key: None,
                position: None,
                envelopes: envelopes.into(),
                hold: Default::default(),
                walked: Some(stream::WalkMark::new(format!("{last:04}"))),
                ticket: Some(ticket),
                ..Default::default()
            }))
            .await
            .unwrap();
    }

    async fn complete(&self, ticket: Ticket) {
        self.lane
            .publish(LaneItem::SnapshotComplete {
                index: users(),
                ticket,
            })
            .await
            .unwrap();
    }

    /// Everything published was taken and acknowledged.
    async fn drained(&self) {
        let drained = tokio::time::timeout(Duration::from_secs(600), async {
            while !self.stream.is_idle() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(drained.is_ok(), "the lane never drained");
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        self.running.abort();
    }
}

/// The cut, by hand: rows 1..=10 land, the target is deleted, row 11 finds
/// it missing, and staging asks again.
async fn cut(lane: &mut Lane) -> (Ticket, Ticket) {
    let (_, first) = lane.request().await;
    lane.page(1..=10, first).await;
    lane.drained().await;
    lane.sink.delete();
    lane.page(11..=11, first).await;
    let (after, fresh) = lane.request().await;
    assert_eq!(after, None, "a fresh backfill starts from the first key");
    assert!(fresh > first, "{fresh} after {first}");
    (first, fresh)
}

#[tokio::test(start_paused = true)]
async fn a_completion_answering_an_older_request_seeds_nothing() {
    let mut lane = Lane::start(Arc::default()).await;
    let (first, fresh) = cut(&mut lane).await;

    lane.page(12..=ROWS, first).await;
    lane.complete(first).await;
    lane.drained().await;
    assert!(lane.sink.seeds().is_empty(), "the cut backfill seeded");
    // The old request delivered again (an ingest restart before its
    // acknowledgement) completes once more, and still seeds nothing.
    lane.complete(first).await;
    lane.drained().await;
    assert!(
        lane.sink.seeds().is_empty(),
        "a repeated old completion seeded"
    );

    lane.page(1..=ROWS, fresh).await;
    lane.complete(fresh).await;
    lane.drained().await;
    assert_eq!(
        lane.sink.seeds(),
        vec![Seed {
            generation: 2,
            missing: Vec::new()
        }]
    );
    assert!(lane.sink.ahead().is_empty(), "{:?}", lane.sink.ahead());
}

/// After the cut, only the fresh request's completion arrives (as when one
/// walk answered both: `one_walk_answers_a_lanes_two_requests_under_the_later_ticket`):
/// it names the fresh ticket, and seeds.
#[tokio::test(start_paused = true)]
async fn a_completion_naming_the_fresh_ticket_seeds_with_no_old_completion_before_it() {
    let mut lane = Lane::start(Arc::default()).await;
    let (_, fresh) = cut(&mut lane).await;

    lane.page(1..=ROWS, fresh).await;
    lane.complete(fresh).await;
    lane.drained().await;
    assert_eq!(
        lane.sink.seeds(),
        vec![Seed {
            generation: 2,
            missing: Vec::new()
        }]
    );
}

/// A stop after the cut leaves the index unseeded, and records no mark of
/// either walk in the new target: the next start asks from the first key.
#[tokio::test(start_paused = true)]
async fn a_stop_after_the_cut_leaves_it_unseeded_for_a_full_backfill_on_the_next_start() {
    let mut lane = Lane::start(Arc::default()).await;
    let (first, fresh) = cut(&mut lane).await;
    lane.page(12..=ROWS, first).await;
    lane.complete(first).await;
    lane.drained().await;

    lane.stop.request();
    lane.page(1..=ROWS, fresh).await;
    lane.complete(fresh).await;
    lane.drained().await;
    assert!(lane.sink.seeds().is_empty(), "seeded past a stop");

    let sink = Arc::clone(&lane.sink);
    drop(lane);
    let mut next = Lane::start(sink).await;
    assert_eq!(
        next.request().await.0,
        None,
        "the next start resumes no walk of the deleted target or the cut one"
    );
}

/// Two requests of one lane, queued before the snapshot starts, are walked
/// once and answered under the later ticket; another lane of the same
/// snapshot keeps its own.
#[tokio::test(start_paused = true)]
async fn one_walk_answers_a_lanes_two_requests_under_the_later_ticket() {
    let names = [primary(), audit()];
    let stream = Arc::new(ChannelStream::over(1024, [users()], names.clone()));
    let requests = stream.requests().unwrap().producer;
    for (sink, ticket) in [(primary(), 1), (primary(), 2), (audit(), 7)] {
        requests
            .publish(Request::Backfill {
                index: users(),
                sink,
                after: None,
                ticket: Ticket::new(ticket),
            })
            .await
            .unwrap();
    }
    let ingest = IngestEngine::new(
        Arc::new(Quiet) as Arc<dyn ChangeCapture>,
        Arc::new(Table::new()) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        names.to_vec(),
        Arc::new(Progress::of([users()], names.clone())),
    )
    .with_batch(BatchPolicy {
        max_changes: 16,
        max_delay: Duration::from_millis(50),
    });
    let running = tokio::spawn(async move { ingest.run().await });

    for (sink, expected) in [(primary(), Ticket::new(2)), (audit(), Ticket::new(7))] {
        let mut consumer = stream
            .lane(&LaneKey::new(users(), sink.clone()))
            .unwrap()
            .consumer;
        let mut rows = BTreeSet::new();
        let mut completes = Vec::new();
        while completes.is_empty() {
            let delivery = tokio::time::timeout(Duration::from_secs(60), consumer.recv())
                .await
                .expect("the snapshot never completed")
                .unwrap()
                .unwrap();
            let (item, handle) = delivery.into_parts();
            match item {
                LaneItem::Batch(batch) => {
                    assert_eq!(batch.ticket, Some(expected), "{sink}");
                    rows.extend(batch.envelopes.iter().map(row_of));
                }
                LaneItem::SnapshotComplete { ticket, .. } => completes.push(ticket),
            }
            handle.ack().await.unwrap();
        }
        assert_eq!(completes, vec![expected], "{sink}");
        assert_eq!(rows, (1..=ROWS).collect::<BTreeSet<_>>(), "{sink}");
        assert!(consumer.is_empty(), "{sink}: one walk, one completion");
    }
    running.abort();
}

/// Auto-create on: the write after the deletion recreates the target, so
/// nothing is reported missing and the old backfill fills the impostor. Its
/// seed is refused (`TargetChanged`): the lane is staged again, which
/// replaces the impostor and cuts the backfill, and the fresh one seeds.
#[tokio::test(start_paused = true)]
async fn an_auto_created_target_is_never_seeded_and_the_index_is_backfilled_again() {
    let sink = Generations {
        auto_create: true,
        ..Generations::default()
    }
    .holding(1, 41);
    let deployment = Deployment::start(sink).await;
    deployment.delete_at(1, 41).await;
    deployment.settled().await;

    deployment.assert_primary_whole(3);
    assert_eq!(deployment.seen.requested(&primary()), 2);
    deployment.assert_audit_untouched();
}

/// (a) The target vanished while the sink engine restarts; staging creates
/// a new one, then fails to forget the old checkpoint. The next staging sees
/// the same new target as already there, and must still ask again.
#[tokio::test(start_paused = true)]
async fn a_cut_whose_first_staging_failed_to_forget_the_checkpoint_is_still_asked_again() {
    let sink = Generations::default().failing(1, 41, true);
    *sink.forget_failures.lock().unwrap() = 1;
    let deployment = Deployment::start(sink).await;
    deployment.settled().await;

    deployment.assert_primary_whole(2);
    deployment.assert_audit_untouched();
}

/// (b) Staging creates the new target, then fails on a later step of its
/// own: the engine never hears it was created, and must still ask again.
#[tokio::test(start_paused = true)]
async fn a_target_created_by_a_staging_that_then_failed_is_still_a_cut() {
    let sink = Generations::default().failing(1, 41, true);
    *sink.ensure_failures_after_create.lock().unwrap() = 1;
    let deployment = Deployment::start(sink).await;
    deployment.settled().await;

    deployment.assert_primary_whole(2);
    deployment.assert_audit_untouched();
}

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

/// A sink with one target per index, numbered as created, for the staging
/// cases: the test deletes a target; an index's `ensure_index` or any
/// `forget_walked` fails a set number of times; a write holding the id
/// `boom` fails as a whole once.
#[derive(Debug, Default)]
struct Targets {
    made: Mutex<u64>,
    live: Mutex<BTreeMap<IndexName, u64>>,
    ensure_failures: Mutex<BTreeMap<IndexName, usize>>,
    forget_failures: Mutex<usize>,
    forgets: Mutex<usize>,
    boom: Mutex<bool>,
}

impl Targets {
    fn delete(&self, index: &IndexName) {
        self.live.lock().unwrap().remove(index);
    }

    fn fail_ensure(&self, index: &IndexName, times: usize) {
        self.ensure_failures
            .lock()
            .unwrap()
            .insert(index.clone(), times);
    }
}

#[async_trait]
impl Sink for Targets {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        if let Some(left) = self.ensure_failures.lock().unwrap().get_mut(&mapping.index)
            && *left > 0
        {
            *left -= 1;
            return Err(sink::SinkError::Write("ensure failed".to_owned()));
        }
        let mut live = self.live.lock().unwrap();
        if let Some(made) = live.get(&mapping.index) {
            return Ok(sink::Ensured::existing(Some(identity(*made))));
        }
        let mut made = self.made.lock().unwrap();
        *made += 1;
        live.insert(mapping.index.clone(), *made);
        Ok(sink::Ensured::created(Some(identity(*made))))
    }

    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        if envelopes.iter().any(|envelope| envelope.id == "boom") {
            let mut boom = self.boom.lock().unwrap();
            if !*boom {
                *boom = true;
                return Err(sink::SinkError::Write("the write failed".to_owned()));
            }
        }
        let live = self.live.lock().unwrap();
        let mut report = WriteReport::clean();
        let gone: BTreeSet<&IndexName> = envelopes
            .iter()
            .map(|envelope| &envelope.index)
            .filter(|index| !live.contains_key(*index))
            .collect();
        for index in gone {
            report = report.missing_target(index.clone(), "the target is gone");
        }
        Ok(report)
    }

    async fn seeding(&self, _: &IndexName) -> sink::Result<Seeding> {
        Ok(Seeding::Unseeded { walked: None })
    }

    async fn forget_walked(&self, _: &IndexName) -> sink::Result<()> {
        *self.forgets.lock().unwrap() += 1;
        let mut left = self.forget_failures.lock().unwrap();
        if *left > 0 {
            *left -= 1;
            return Err(sink::SinkError::Write("forget failed".to_owned()));
        }
        Ok(())
    }
}

/// One sink engine over the `users` and `orders` lanes, fed by hand, with the
/// request lane read by the test.
struct Pair {
    sink: Arc<Targets>,
    engine: Arc<SinkEngine>,
    stream: Arc<ChannelStream>,
    requests: Box<dyn Consumer<Request>>,
}

/// A backfill request as the test reads it.
#[derive(Debug, Clone, PartialEq)]
struct Asked {
    index: IndexName,
    ticket: Ticket,
    after: Option<String>,
}

impl Pair {
    fn new(sink: Targets) -> Self {
        let sink = Arc::new(sink);
        let stream = Arc::new(ChannelStream::over(1024, [users(), orders()], [primary()]));
        let mapping_of = |index: IndexName| IndexMapping { index, ..mapping() };
        let engine = Arc::new(SinkEngine::new(
            primary(),
            Arc::clone(&sink) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![mapping_of(users()), mapping_of(orders())],
            Arc::new(Progress::of([users(), orders()], [primary()])),
        ));
        Self {
            requests: stream.requests().unwrap().consumer,
            sink,
            engine,
            stream,
        }
    }

    /// The requests published so far, acknowledged.
    async fn requests(&mut self) -> Vec<Asked> {
        let mut seen = Vec::new();
        while !self.requests.is_empty() {
            let (request, handle) = self.requests.recv().await.unwrap().unwrap().into_parts();
            handle.ack().await.unwrap();
            let Request::Backfill {
                index,
                after,
                ticket,
                ..
            } = request
            else {
                panic!("a backfill request")
            };
            seen.push(Asked {
                index,
                ticket,
                after: after.map(|mark| mark.as_str().to_owned()),
            });
        }
        seen
    }

    fn run(&self) -> JoinHandle<crate::Result<()>> {
        let engine = Arc::clone(&self.engine);
        tokio::spawn(async move {
            let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
            engine.run(&mut rx).await
        })
    }

    /// A snapshot page of one document, `id`, of `index`, answering `ticket`.
    async fn page(&self, index: &IndexName, id: &str, ticket: Ticket) {
        self.stream
            .lane(&LaneKey::new(index.clone(), primary()))
            .unwrap()
            .producer
            .publish(LaneItem::Batch(Batch {
                key: None,
                position: None,
                envelopes: vec![Envelope::upsert(
                    index.clone(),
                    id,
                    GenericValue::Map(BTreeMap::new()),
                    None,
                    chrono::Utc::now(),
                )]
                .into(),
                hold: Default::default(),
                walked: None,
                ticket: Some(ticket),
                ..Default::default()
            }))
            .await
            .unwrap();
    }
}

fn ticket_of(requests: &[Asked], index: &IndexName) -> Ticket {
    requests
        .iter()
        .find(|asked| &asked.index == index)
        .map(|asked| asked.ticket)
        .unwrap()
}

/// (c) Staging finds the `users` target new while `orders` fails to stage,
/// under a `Fresh` source, where one index failing fails the pass as a whole
/// (#471, R4): the next pass, finding `users` as it left it, must still ask
/// for it again (and not for `orders`).
#[tokio::test(start_paused = true)]
async fn a_cut_found_beside_another_index_failing_to_stage_is_kept() {
    let mut pair = Pair::new(Targets::default());
    pair.engine.stage(Continuity::Resumed).await.unwrap();
    let first = pair.requests().await;
    assert_eq!(first.len(), 2, "{first:?}");

    pair.sink.delete(&users());
    pair.sink.fail_ensure(&orders(), 1);
    assert!(pair.engine.stage(Continuity::Fresh).await.is_err());
    pair.engine.stage(Continuity::Fresh).await.unwrap();

    let again = pair.requests().await;
    assert_eq!(again.len(), 1, "{again:?}");
    let fresh = again.first().unwrap();
    assert_eq!(fresh.index, users());
    assert_eq!(fresh.after, None);
    assert!(fresh.ticket > ticket_of(&first, &users()), "{again:?}");
}

/// (c, #471) The same under `Resumed`: the pass goes on without `orders`
/// (left restaging) and asks for `users` again in that same pass.
#[tokio::test(start_paused = true)]
async fn a_cut_found_beside_an_index_left_restaging_is_asked_again_at_once() {
    let mut pair = Pair::new(Targets::default());
    pair.engine.stage(Continuity::Resumed).await.unwrap();
    let first = pair.requests().await;
    assert_eq!(first.len(), 2, "{first:?}");

    pair.sink.delete(&users());
    pair.sink.fail_ensure(&orders(), 1);
    pair.engine.stage(Continuity::Resumed).await.unwrap();

    let again = pair.requests().await;
    assert_eq!(again.len(), 1, "{again:?}");
    let fresh = again.first().unwrap();
    assert_eq!(fresh.index, users());
    assert_eq!(fresh.after, None);
    assert!(fresh.ticket > ticket_of(&first, &users()), "{again:?}");
    assert_eq!(
        pair.engine
            .lane_health(&stream::LaneKey::new(orders(), pair.engine.name().clone()))
            .kind(),
        "restaging"
    );
}

/// Review of #476, 1: an index a restart's staging left out (one ensure
/// failing for a moment) keeps its running backfill when its target is
/// intact: staged again with ensure's rules, nothing is cut, forgotten or
/// asked again, and the lane opens.
#[tokio::test(start_paused = true)]
async fn an_index_left_out_by_a_staging_keeps_its_backfill_when_its_target_is_intact() {
    let mut pair = Pair::new(Targets::default());
    pair.engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(pair.requests().await.len(), 2);

    pair.sink.fail_ensure(&orders(), 1);
    pair.engine.stage(Continuity::Resumed).await.unwrap();
    let lane = LaneKey::new(orders(), primary());
    let health = pair.engine.lane_health(&lane);
    assert!(
        matches!(&health, crate::LaneHealth::Restaging { reason, .. } if reason.contains("could not be ensured")),
        "{health:?}"
    );
    let running = pair.run();
    let opened = tokio::time::timeout(Duration::from_secs(60), async {
        while !pair.engine.lane_health(&lane).is_open() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
    assert!(opened.is_ok(), "the lane opens once its index is ensured");
    assert!(
        pair.requests().await.is_empty(),
        "its outstanding backfill goes on: nothing asked again"
    );
    assert_eq!(*pair.sink.forgets.lock().unwrap(), 0, "its walk is kept");
    running.abort();
}

/// Review of #476, 1, the counter-case: the target left out was recreated
/// meanwhile, so its outstanding backfill can't seed it: the later staging
/// cuts it and asks again from the first key.
#[tokio::test(start_paused = true)]
async fn an_index_left_out_by_a_staging_whose_target_changed_is_asked_again() {
    let mut pair = Pair::new(Targets::default());
    pair.engine.stage(Continuity::Resumed).await.unwrap();
    let first = pair.requests().await;

    pair.sink.fail_ensure(&orders(), 1);
    pair.sink.delete(&orders());
    pair.engine.stage(Continuity::Resumed).await.unwrap();
    let running = pair.run();
    let asked = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let asked = pair.requests().await;
            if !asked.is_empty() {
                return asked;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("asked again");
    assert_eq!(asked.len(), 1, "{asked:?}");
    let fresh = asked.first().unwrap();
    assert_eq!(fresh.index, orders());
    assert_eq!(fresh.after, None);
    assert!(fresh.ticket > ticket_of(&first, &orders()), "{asked:?}");
    assert_eq!(
        *pair.sink.forgets.lock().unwrap(),
        1,
        "its walk is forgotten"
    );
    running.abort();
}

/// (d) A restage finds the `users` target missing and creates a new one, then
/// fails to forget its checkpoint; before it is retried, the run fails on
/// `orders`. The restart's staging finds `users` as the restage left it, and
/// must still ask for it again.
#[tokio::test(start_paused = true)]
async fn a_cut_found_by_a_restage_that_failed_survives_the_run_failing() {
    let mut pair = Pair::new(Targets {
        forget_failures: Mutex::new(1),
        ..Targets::default()
    });
    pair.engine.stage(Continuity::Resumed).await.unwrap();
    let first = pair.requests().await;
    let running = pair.run();

    pair.sink.delete(&users());
    pair.page(&users(), "1", ticket_of(&first, &users())).await;
    let tried = tokio::time::timeout(Duration::from_secs(60), async {
        while *pair.sink.forgets.lock().unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
    assert!(
        tried.is_ok(),
        "the restage never tried to forget the checkpoint"
    );
    pair.page(&orders(), "boom", ticket_of(&first, &orders()))
        .await;
    assert!(running.await.unwrap().is_err(), "the run did not fail");

    pair.engine.stage(Continuity::Resumed).await.unwrap();
    let again = pair.requests().await;
    assert_eq!(again.len(), 1, "{again:?}");
    let fresh = again.first().unwrap();
    assert_eq!(fresh.index, users());
    assert_eq!(fresh.after, None);
    assert!(fresh.ticket > ticket_of(&first, &users()), "{again:?}");
}

/// A lane's two requests as an ingest restart in the middle of a cut leaves
/// them: the first resuming after a checkpoint, the second from the first
/// key. One walk serves both from the first key, under the later ticket.
#[tokio::test(start_paused = true)]
async fn a_resumed_request_and_a_fresh_one_of_a_lane_are_walked_once_from_the_first_key() {
    let names = [primary()];
    let stream = Arc::new(ChannelStream::over(1024, [users()], names.clone()));
    let requests = stream.requests().unwrap().producer;
    for (after, ticket) in [(Some("0040"), 1), (None, 2)] {
        requests
            .publish(Request::Backfill {
                index: users(),
                sink: primary(),
                after: after.map(stream::WalkMark::new),
                ticket: Ticket::new(ticket),
            })
            .await
            .unwrap();
    }
    let ingest = IngestEngine::new(
        Arc::new(Quiet) as Arc<dyn ChangeCapture>,
        Arc::new(Table::new()) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        names.to_vec(),
        Arc::new(Progress::of([users()], names.clone())),
    )
    .with_batch(BatchPolicy {
        max_changes: 16,
        max_delay: Duration::from_millis(50),
    });
    let running = tokio::spawn(async move { ingest.run().await });

    let mut consumer = stream
        .lane(&LaneKey::new(users(), primary()))
        .unwrap()
        .consumer;
    let mut rows = Vec::new();
    let mut completes = Vec::new();
    while completes.is_empty() {
        let delivery = tokio::time::timeout(Duration::from_secs(60), consumer.recv())
            .await
            .expect("the snapshot never completed")
            .unwrap()
            .unwrap();
        let (item, handle) = delivery.into_parts();
        match item {
            LaneItem::Batch(batch) => {
                assert_eq!(batch.ticket, Some(Ticket::new(2)));
                rows.extend(batch.envelopes.iter().map(row_of));
            }
            LaneItem::SnapshotComplete { ticket, .. } => completes.push(ticket),
        }
        handle.ack().await.unwrap();
    }
    assert_eq!(completes, vec![Ticket::new(2)]);
    assert_eq!(
        rows,
        (1..=ROWS).collect::<Vec<_>>(),
        "every row once, from the first key"
    );
    assert!(consumer.is_empty(), "one walk, one completion");
    running.abort();
}

/// `users` on [`Generations`], `orders` always fine and seeded: a sink of two
/// indexes, so a staging can leave `users` out while `orders` is ensured
/// (#471).
#[derive(Debug)]
struct Split(Arc<Generations>);

#[async_trait]
impl Sink for Split {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        if mapping.index == users() {
            return self.0.ensure_index(mapping).await;
        }
        Ok(sink::Ensured::existing(Some(sink::TargetId::new(
            "orders".to_owned(),
        ))))
    }

    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        let own: Vec<Envelope> = envelopes
            .iter()
            .filter(|envelope| envelope.index == users())
            .cloned()
            .collect();
        if own.is_empty() {
            return Ok(WriteReport::clean());
        }
        self.0.write(&own, caught_up).await
    }

    async fn seeding(&self, index: &IndexName) -> sink::Result<Seeding> {
        if *index == users() {
            return self.0.seeding(index).await;
        }
        Ok(Seeding::Seeded)
    }

    async fn record_walked(&self, index: &IndexName, mark: &str) -> sink::Result<()> {
        if *index == users() {
            return self.0.record_walked(index, mark).await;
        }
        Ok(())
    }

    async fn forget_walked(&self, index: &IndexName) -> sink::Result<()> {
        if *index == users() {
            return self.0.forget_walked(index).await;
        }
        Ok(())
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        if *index == users() {
            return self.0.mark_seeded(index).await;
        }
        Ok(())
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        if mapping.index == users() {
            return self.0.reindex(mapping).await;
        }
        Ok(())
    }
}

/// A sink engine over `users` and `orders` on [`Split`], driven by hand.
struct TwoLanes {
    sink: Arc<Generations>,
    stream: Arc<ChannelStream>,
    lane: Box<dyn Producer<LaneItem>>,
    requests: Box<dyn Consumer<Request>>,
    control: tokio::sync::mpsc::Sender<SinkControl>,
    receiver: Option<tokio::sync::mpsc::Receiver<SinkControl>>,
    engine: Arc<SinkEngine>,
}

impl TwoLanes {
    fn new(sink: Arc<Generations>, backoff: Duration, writes: usize) -> Self {
        let stream = Arc::new(ChannelStream::over(1024, [users(), orders()], [primary()]));
        let mapping_of = |index: IndexName| IndexMapping { index, ..mapping() };
        let engine = Arc::new(
            SinkEngine::new(
                primary(),
                Arc::new(Split(Arc::clone(&sink))) as Arc<dyn Sink>,
                Arc::clone(&stream) as Arc<dyn Stream>,
                vec![mapping_of(users()), mapping_of(orders())],
                Arc::new(Progress::of([users(), orders()], [primary()])),
            )
            .with_writes(writes)
            .with_write_documents(10)
            .with_backoff(crate::Backoff::new(backoff, backoff).unwrap()),
        );
        let (control, receiver) = tokio::sync::mpsc::channel::<SinkControl>(4);
        Self {
            sink,
            lane: stream
                .lane(&LaneKey::new(users(), primary()))
                .unwrap()
                .producer,
            requests: stream.requests().unwrap().consumer,
            stream,
            control,
            receiver: Some(receiver),
            engine,
        }
    }

    /// A run of the engine, with no control.
    fn run(&self) -> JoinHandle<crate::Result<()>> {
        let engine = Arc::clone(&self.engine);
        tokio::spawn(async move {
            let (_control, mut receiver) = tokio::sync::mpsc::channel::<SinkControl>(1);
            engine.run(&mut receiver).await
        })
    }

    /// A run of the engine taking the test's control channel.
    fn run_controlled(&mut self) -> JoinHandle<crate::Result<()>> {
        let engine = Arc::clone(&self.engine);
        let mut receiver = self.receiver.take().unwrap();
        tokio::spawn(async move { engine.run(&mut receiver).await })
    }

    /// Hold the write of row 21 to generation 2 for 15 s, past
    /// `RECORD_EVERY`, then let it land: what triggers a record after the
    /// first walk's pages landed in the new target.
    async fn land_late(&self) {
        let waiting = tokio::time::timeout(Duration::from_secs(120), async {
            while !self.sink.is_waiting(2, 21) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await;
        assert!(waiting.is_ok(), "row 21's write to generation 2 never came");
        tokio::time::sleep(Duration::from_secs(15)).await;
        self.sink.release(2, 21);
        self.drained().await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    }

    /// The next backfill request: its checkpoint and ticket.
    async fn request(&mut self) -> (Option<String>, Ticket) {
        next_backfill(&mut self.requests).await
    }

    /// A page of `rows` of the `users` walk answering `ticket`.
    async fn page(&self, rows: std::ops::RangeInclusive<i64>, ticket: Ticket) {
        self.lane.publish(users_page(rows, ticket)).await.unwrap();
    }

    async fn drained(&self) {
        let drained = tokio::time::timeout(Duration::from_secs(600), async {
            while !self.stream.is_idle() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(drained.is_ok(), "the lanes never drained");
    }

    fn users_health(&self) -> crate::LaneHealth {
        self.engine.lane_health(&LaneKey::new(users(), primary()))
    }

    /// What a new engine over the same sink asks for `users` (the next start).
    async fn next_start_asks(&self) -> Option<String> {
        let mut next = TwoLanes::new(Arc::clone(&self.sink), Duration::from_secs(1), 1);
        next.engine.stage(Continuity::Resumed).await.unwrap();
        next.request().await.0
    }
}

async fn next_backfill(requests: &mut Box<dyn Consumer<Request>>) -> (Option<String>, Ticket) {
    let delivery = tokio::time::timeout(Duration::from_secs(600), requests.recv())
        .await
        .expect("no backfill request")
        .unwrap()
        .unwrap();
    let (Request::Backfill { after, ticket, .. }, handle) = delivery.into_parts() else {
        panic!("a backfill request")
    };
    handle.ack().await.unwrap();
    (after.map(|mark| mark.as_str().to_owned()), ticket)
}

fn users_page(rows: std::ops::RangeInclusive<i64>, ticket: Ticket) -> LaneItem {
    let last = *rows.end();
    LaneItem::Batch(Batch {
        key: None,
        position: None,
        envelopes: rows
            .map(|row| {
                Envelope::upsert(
                    users(),
                    row.to_string(),
                    GenericValue::Map(BTreeMap::new()),
                    None,
                    chrono::Utc::now(),
                )
            })
            .collect::<Vec<_>>()
            .into(),
        hold: Default::default(),
        walked: Some(stream::WalkMark::new(format!("{last:04}"))),
        ticket: Some(ticket),
        ..Default::default()
    })
}

/// `users`' first walk lands rows 1..=10 in generation 1; then the run ends,
/// the test changes the sink, and the restart's staging leaves `users` out
/// (its ensure refused `refusals` times). The first walk's ticket.
async fn left_out(two: &mut TwoLanes, refusals: usize, delete: bool) -> Ticket {
    two.engine.stage(Continuity::Resumed).await.unwrap();
    let (_, first) = two.request().await;
    let running = two.run();
    two.page(1..=10, first).await;
    two.drained().await;
    running.abort();
    let _ = running.await;
    if delete {
        two.sink.delete();
    }
    *two.sink.ensure_refusals.lock().unwrap() = refusals;
    two.engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(two.users_health().kind(), "restaging", "left out");
    first
}

/// Review of #476b, 1 (the rule: a walk checkpoint is recorded only for the
/// backfill it belongs to), through `ensure_again`: `users` is left out, its
/// target recreated meanwhile; a page of the first walk is taken during the
/// backoff after a failed retry; the next retry cuts and asks again. That
/// page lands in the new target, but its mark is never recorded there, and
/// the next start asks from the first key.
#[tokio::test(start_paused = true)]
async fn a_cut_backfills_page_records_no_mark_in_the_new_target_through_ensure_again() {
    let mut two = TwoLanes::new(
        Arc::new(Generations::default().holding(2, 21)),
        Duration::from_secs(1),
        1,
    );
    let first = left_out(&mut two, 2, true).await;
    two.page(11..=20, first).await;
    two.page(21..=30, first).await;
    let running = two.run();
    let (after, fresh) = two.request().await;
    assert_eq!(after, None);
    assert!(fresh > first);
    two.land_late().await;
    assert!(two.sink.ahead().is_empty(), "{:?}", two.sink.ahead());
    running.abort();
    assert_eq!(
        two.next_start_asks().await,
        None,
        "the next start asks from the first key"
    );
}

/// The same rule through a reindex of a lane left out: the cut is the
/// reindex's, the old walk's page is never recorded in the new target.
#[tokio::test(start_paused = true)]
async fn a_cut_backfills_page_records_no_mark_in_the_new_target_through_a_reindex() {
    let mut two = TwoLanes::new(
        Arc::new(Generations::default().holding(2, 21)),
        Duration::from_secs(60),
        1,
    );
    let first = left_out(&mut two, 2, true).await;
    two.page(11..=20, first).await;
    two.page(21..=30, first).await;
    let running = two.run_controlled();
    let refused = tokio::time::timeout(Duration::from_secs(30), async {
        while *two.sink.ensure_refusals.lock().unwrap() > 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
    assert!(refused.is_ok(), "the first retry was refused");
    tokio::time::sleep(Duration::from_secs(1)).await;
    two.control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    let (after, fresh) = two.request().await;
    assert_eq!(after, None);
    assert!(fresh > first);
    two.land_late().await;
    assert!(two.sink.ahead().is_empty(), "{:?}", two.sink.ahead());
    running.abort();
    assert_eq!(two.next_start_asks().await, None);
}

/// The same rule on the missing-target path alone (`main`'s own exposure): a
/// write finds `users`' target gone while its staging again is refused; a
/// page of the first walk is taken meanwhile, then a reindex cuts. Its mark
/// is never recorded in the new target.
#[tokio::test(start_paused = true)]
async fn a_cut_backfills_page_records_no_mark_after_a_missing_target() {
    let mut two = TwoLanes::new(
        Arc::new(Generations::default().holding(2, 21)),
        Duration::from_secs(60),
        1,
    );
    two.engine.stage(Continuity::Resumed).await.unwrap();
    let (_, first) = two.request().await;
    let running = two.run_controlled();
    two.page(1..=10, first).await;
    two.drained().await;
    two.sink.delete();
    *two.sink.ensure_refusals.lock().unwrap() = 1;
    two.page(11..=11, first).await;
    let restaged = tokio::time::timeout(Duration::from_secs(30), async {
        while *two.sink.ensure_refusals.lock().unwrap() > 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
    assert!(restaged.is_ok(), "the restage was refused");
    assert_eq!(two.users_health().kind(), "restaging");
    two.page(12..=20, first).await;
    two.page(21..=30, first).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    two.control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    let (after, fresh) = two.request().await;
    assert_eq!(after, None);
    assert!(fresh > first);
    two.land_late().await;
    assert!(two.sink.ahead().is_empty(), "{:?}", two.sink.ahead());
    running.abort();
    assert_eq!(two.next_start_asks().await, None);
}

/// The same rule for a page whose write is still in flight when the cut is
/// asked for: the reindex waits for that write, which finds the target gone;
/// the page lands later in the new target, and its mark is never recorded.
#[tokio::test(start_paused = true)]
async fn a_cut_backfills_page_in_flight_when_the_cut_is_asked_records_no_mark() {
    let sink = Arc::new(Generations::default().holding(1, 11).holding(2, 21));
    let mut two = TwoLanes::new(Arc::clone(&sink), Duration::from_secs(60), 1);
    two.engine.stage(Continuity::Resumed).await.unwrap();
    let (_, first) = two.request().await;
    let running = two.run_controlled();
    two.page(1..=10, first).await;
    two.page(11..=20, first).await;
    two.page(21..=30, first).await;
    let waiting = tokio::time::timeout(Duration::from_secs(60), async {
        while !sink.is_waiting(1, 11) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
    assert!(waiting.is_ok(), "the page's write is in flight");
    two.control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    sink.delete();
    sink.release(1, 11);
    let (after, fresh) = two.request().await;
    assert_eq!(after, None);
    assert!(fresh > first);
    two.land_late().await;
    assert!(sink.ahead().is_empty(), "{:?}", sink.ahead());
    running.abort();
    assert_eq!(two.next_start_asks().await, None);
}

/// The other half of the rule: `users` left out with its target intact keeps
/// its walk. Pages of the first walk are taken before the lane reopens and
/// the first of them is held in flight: no mark is recorded past it, and
/// once it lands the walk's mark is recorded, and the next start resumes
/// after it.
#[tokio::test(start_paused = true)]
async fn an_intact_left_out_lane_keeps_its_walk_and_records_no_mark_past_a_page() {
    let sink = Arc::new(Generations::default().holding(1, 11).holding(1, 31));
    let mut two = TwoLanes::new(Arc::clone(&sink), Duration::from_secs(1), 8);
    let first = left_out(&mut two, 2, false).await;
    two.page(11..=20, first).await;
    two.page(21..=30, first).await;
    two.page(31..=40, first).await;
    let running = two.run();
    let waiting = tokio::time::timeout(Duration::from_secs(60), async {
        while !(sink.is_waiting(1, 11) && sink.is_waiting(1, 31)) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
    assert!(
        waiting.is_ok(),
        "the first and last pages' writes are in flight"
    );
    tokio::time::sleep(Duration::from_secs(15)).await;
    sink.release(1, 31);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        sink.ahead().is_empty(),
        "no mark past the held page: {:?}",
        sink.ahead()
    );
    sink.release(1, 11);
    two.drained().await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(sink.ahead().is_empty(), "{:?}", sink.ahead());
    assert_eq!(
        sink.target.lock().unwrap().walked.as_deref(),
        Some("0040"),
        "the walk's checkpoint is kept and recorded"
    );
    running.abort();
    assert_eq!(two.next_start_asks().await.as_deref(), Some("0040"));
}
