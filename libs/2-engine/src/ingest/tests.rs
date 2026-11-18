//! The ingest engine builds several batches at once (ADR 0012 stage 4) and
//! keeps every guarantee it had building one at a time:
//!
//! - **per document**: never two builds of one document at once, and the
//!   versions published never go down (a later build reads newer state and
//!   lands later);
//! - **per lane**: items arrive in the order they were granted, so live
//!   positions only grow;
//! - **the gate**: up to `builds` at once, live first except every fourth
//!   contested grant, which goes to backfill;
//! - **failure and stop**: nothing after a failed build is published, and the
//!   builds in flight finish before the run ends.
//!
//! Each test runs the ingest engine and one sink engine over a real channel
//! stream, with a source and builder that model a database: a change is
//! visible once the stream yields it, and a build reads each document's
//! version when it starts. The randomized test replays with
//! `FLUSSO_BUILD_SCHEDULE_SEED=<seed>`.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{
    ColumnName, Envelope, GenericValue, IndexMapping, IndexName, Op, Position, SinkName, TableName,
};
use sink::{Sink, WriteReport};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{BuildFailure, Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey, SourceError};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::sync::Notify;

use crate::{
    BatchPolicy, FailurePolicies, FailurePolicy, IngestEngine, SinkControl, SinkEngine, Stop,
};

/// A small deterministic generator, so a schedule replays from its seed.
#[derive(Debug)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn users() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn sink_name() -> SinkName {
    SinkName::try_new("primary").unwrap()
}

fn key(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

fn id_of(key: &RowKey) -> i64 {
    match key.0.first() {
        Some((_, GenericValue::BigInt(id))) => *id,
        _ => -1,
    }
}

fn mapping() -> IndexMapping {
    IndexMapping {
        index: users(),
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// One change of a test's log: an upsert or a delete of `id`.
#[derive(Debug, Clone, Copy)]
struct Change {
    id: i64,
    delete: bool,
}

impl Change {
    fn upsert(id: i64) -> Self {
        Self { id, delete: false }
    }

    fn event(self) -> ChangeEvent {
        let table = TableName::try_new("users").unwrap();
        if self.delete {
            ChangeEvent::Delete {
                table,
                key: key(self.id),
                before: None,
            }
        } else {
            ChangeEvent::Upsert {
                table,
                key: key(self.id),
                before: None,
                after: None,
            }
        }
    }
}

/// The rows a test's changes write. Rows `0..rows` start at version 0; change
/// `i` of the log writes version `i + 1` (or deletes the row). A change is
/// visible once the stream has yielded it.
#[derive(Debug)]
struct Database {
    rows: i64,
    log: Vec<Change>,
    visible: AtomicUsize,
}

impl Database {
    fn new(rows: i64, log: Vec<Change>) -> Arc<Self> {
        Arc::new(Self {
            rows,
            log,
            visible: AtomicUsize::new(0),
        })
    }

    fn reveal(&self, through: usize) {
        self.visible.fetch_max(through, Ordering::SeqCst);
    }

    /// `id`'s version as a read now sees it; `None` when the row is absent.
    fn read(&self, id: i64) -> Option<u64> {
        let visible = self.visible.load(Ordering::SeqCst);
        let mut version = (0..self.rows).contains(&id).then_some(0);
        for (at, change) in self.log.iter().enumerate().take(visible) {
            if change.id == id {
                version = (!change.delete).then_some(at as u64 + 1);
            }
        }
        version
    }

    /// What every document should hold once everything has landed.
    fn expected(&self) -> BTreeMap<i64, u64> {
        let mut ids: HashSet<i64> = (0..self.rows).collect();
        ids.extend(self.log.iter().map(|change| change.id));
        ids.into_iter()
            .filter_map(|id| self.read(id).map(|version| (id, version)))
            .collect()
    }
}

/// A live stream over the database's log, from where the last run left off.
/// Each change is revealed as it is yielded, after an optional random gap.
/// With `open`, the stream stays pending after the last change instead of
/// ending.
#[derive(Debug)]
struct Feed {
    db: Arc<Database>,
    from: AtomicUsize,
    gaps: Option<Arc<Mutex<Rng>>>,
    /// A fixed wait before each change, when set.
    delay: Option<Duration>,
    open: bool,
    confirmed: Mutex<Vec<Position>>,
}

impl Feed {
    fn new(db: &Arc<Database>) -> Self {
        Self {
            db: Arc::clone(db),
            from: AtomicUsize::new(0),
            gaps: None,
            delay: None,
            open: false,
            confirmed: Mutex::default(),
        }
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    fn with_gaps(mut self, seed: u64) -> Self {
        self.gaps = Some(Arc::new(Mutex::new(Rng::new(seed))));
        self
    }

    fn open(mut self) -> Self {
        self.open = true;
        self
    }

    /// The next run's stream starts after `watermark`.
    fn resume_after(&self, watermark: Option<Position>) {
        let from = watermark.map_or(0, |position| position.seq as usize + 1);
        self.from.store(from, Ordering::SeqCst);
    }
}

#[async_trait]
impl ChangeCapture for Feed {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let db = Arc::clone(&self.db);
        let gaps = self.gaps.clone();
        let delay = self.delay;
        let open = self.open;
        let from = self.from.load(Ordering::SeqCst);
        Ok(Box::pin(futures::stream::unfold(from, move |next| {
            let db = Arc::clone(&db);
            let gaps = gaps.clone();
            async move {
                let Some(change) = db.log.get(next).copied() else {
                    if open {
                        std::future::pending::<()>().await;
                    }
                    return None;
                };
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
                if let Some(gaps) = gaps {
                    let gap = gaps.lock().unwrap().below(4);
                    if gap > 0 {
                        tokio::time::sleep(Duration::from_millis(gap)).await;
                    }
                }
                db.reveal(next + 1);
                Some((Ok((Position::new(next as u64), change.event())), next + 1))
            }
        })))
    }

    fn confirm(&self, landed: source::cdc::Landed) {
        let mut confirmed = self.confirmed.lock().unwrap();
        if let Some(seq) = landed.covered_seq()
            && confirmed.last().is_none_or(|last| last.seq != seq)
        {
            confirmed.push(Position::new(seq));
        }
    }
}

/// How a build behaves, decided per call.
#[derive(Debug, Default)]
struct Behaviour {
    /// Random delays and failures, from one generator so a seed replays.
    random: Option<Mutex<Rng>>,
    /// Up to this many random failures in all (one in ten builds).
    failures_left: AtomicUsize,
    /// Builds touching one of these ids wait until released.
    held: Mutex<HashSet<i64>>,
    released: Notify,
    /// A build touching one of these ids fails once (the id is then removed).
    fail_once: Mutex<HashSet<i64>>,
    /// A build reports this id's document failed once.
    document_fails_once: Mutex<HashSet<i64>>,
    /// Every build takes this long.
    delay: Option<Duration>,
    /// Every key page takes this long to read.
    keys_delay: Option<Duration>,
}

/// Builds documents from the [`Database`], reading each version when the
/// build starts, and records what a test asks about: each build's ids, two
/// builds of one document at once, how many builds ran at once.
#[derive(Debug)]
struct Builder {
    db: Arc<Database>,
    behaviour: Behaviour,
    grants: Mutex<Vec<Vec<i64>>>,
    building: Mutex<HashSet<i64>>,
    overlapped: AtomicBool,
    now: AtomicUsize,
    most: AtomicUsize,
}

impl Builder {
    fn new(db: &Arc<Database>, behaviour: Behaviour) -> Arc<Self> {
        Arc::new(Self {
            db: Arc::clone(db),
            behaviour,
            grants: Mutex::default(),
            building: Mutex::default(),
            overlapped: AtomicBool::new(false),
            now: AtomicUsize::new(0),
            most: AtomicUsize::new(0),
        })
    }

    fn release(&self, id: i64) {
        self.behaviour.held.lock().unwrap().remove(&id);
        self.behaviour.released.notify_waiters();
    }

    fn grants(&self) -> Vec<Vec<i64>> {
        self.grants.lock().unwrap().clone()
    }

    async fn held(&self, ids: &[i64]) {
        loop {
            let released = self.behaviour.released.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            let waiting = {
                let held = self.behaviour.held.lock().unwrap();
                ids.iter().any(|id| held.contains(id))
            };
            if !waiting {
                return;
            }
            released.await;
        }
    }

    /// The build's work, between marking its ids and unmarking them.
    async fn work(&self, ids: &[i64]) -> source::Result<Vec<Document>> {
        let versions: Vec<Option<u64>> = ids.iter().map(|id| self.db.read(*id)).collect();
        let (delay, fail) = match &self.behaviour.random {
            Some(random) => {
                let mut random = random.lock().unwrap();
                let delay = Duration::from_millis(random.below(20));
                let fail = random.below(10) == 0
                    && self
                        .behaviour
                        .failures_left
                        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                            left.checked_sub(1)
                        })
                        .is_ok();
                (Some(delay), fail)
            }
            None => (self.behaviour.delay, false),
        };
        if let Some(delay) = delay.filter(|delay| !delay.is_zero()) {
            tokio::time::sleep(delay).await;
        }
        self.held(ids).await;
        let failing = {
            let mut fail_once = self.behaviour.fail_once.lock().unwrap();
            ids.iter().any(|id| fail_once.remove(id))
        };
        if fail || failing {
            return Err(SourceError::Connection("the build failed".to_owned()));
        }
        let mut documents = Vec::with_capacity(ids.len());
        for (id, version) in ids.iter().zip(versions) {
            let document_id = DocumentId {
                index: users(),
                key: key(*id),
            };
            if self
                .behaviour
                .document_fails_once
                .lock()
                .unwrap()
                .remove(id)
            {
                documents.push(Document::Failed {
                    id: document_id,
                    failure: BuildFailure {
                        field: "label".to_owned(),
                        message: "boom".to_owned(),
                        position: None,
                    },
                });
                continue;
            }
            documents.push(match version {
                Some(version) => Document::Upsert {
                    id: document_id,
                    body: GenericValue::Map(BTreeMap::from([(
                        "v".to_owned(),
                        GenericValue::BigInt(version as i64),
                    )])),
                },
                None => Document::Delete { id: document_id },
            });
        }
        Ok(documents)
    }
}

#[async_trait]
impl DocumentBuilder for Builder {
    async fn resolve(
        &self,
        _table: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: users(),
            key: key.clone(),
        }])
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        let mut built = self.build_many(std::slice::from_ref(id)).await?;
        Ok(built.remove(0))
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        let ids: Vec<i64> = ids.iter().map(|id| id_of(&id.key)).collect();
        self.grants.lock().unwrap().push(ids.clone());
        {
            let mut building = self.building.lock().unwrap();
            for id in &ids {
                if !building.insert(*id) {
                    self.overlapped.store(true, Ordering::SeqCst);
                }
            }
        }
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        let result = self.work(&ids).await;
        self.now.fetch_sub(1, Ordering::SeqCst);
        let mut building = self.building.lock().unwrap();
        for id in &ids {
            building.remove(id);
        }
        result
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        vec![users()]
    }

    async fn keys(
        &self,
        _index: &IndexName,
        after: Option<&RowKey>,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        if let Some(delay) = self.behaviour.keys_delay {
            tokio::time::sleep(delay).await;
        }
        let start = after.map_or(0, |after| id_of(after) + 1);
        Ok((start..self.db.rows).take(limit).map(key).collect())
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok(vec![mapping()])
    }
}

/// What the sink was written, in lane order (it writes one item at a time):
/// each item's position and documents, versions going down, and the last
/// version of each document.
#[derive(Debug, Default)]
struct Recorder {
    seeded: AtomicBool,
    items: Mutex<Vec<(Option<Position>, Vec<(i64, Option<u64>)>)>>,
    last: Mutex<BTreeMap<i64, Option<u64>>>,
    highest: Mutex<BTreeMap<i64, u64>>,
    regressed: Mutex<Vec<String>>,
}

impl Recorder {
    fn seeded(seeded: bool) -> Arc<Self> {
        let recorder = Self::default();
        recorder.seeded.store(seeded, Ordering::SeqCst);
        Arc::new(recorder)
    }

    fn state(&self) -> BTreeMap<i64, u64> {
        self.last
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, version)| version.map(|version| (*id, version)))
            .collect()
    }

    fn positions(&self) -> Vec<u64> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(position, _)| position.map(|position| position.seq))
            .collect()
    }

    fn written_ids(&self) -> Vec<Vec<i64>> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, documents)| !documents.is_empty())
            .map(|(_, documents)| documents.iter().map(|(id, _)| *id).collect())
            .collect()
    }
}

#[async_trait]
impl Sink for Recorder {
    async fn write(&self, envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        let mut documents = Vec::with_capacity(envelopes.len());
        let mut last = self.last.lock().unwrap();
        for envelope in envelopes {
            let id: i64 = envelope.id.parse().unwrap_or(-1);
            let version = match (&envelope.op, &envelope.document) {
                (Op::Upsert, Some(GenericValue::Map(body))) => match body.get("v") {
                    Some(GenericValue::BigInt(v)) => Some(*v as u64),
                    _ => None,
                },
                _ => None,
            };
            if let Some(version) = version {
                let mut highest = self.highest.lock().unwrap();
                let seen = highest.entry(id).or_insert(version);
                if version < *seen {
                    self.regressed
                        .lock()
                        .unwrap()
                        .push(format!("{id}: {seen} then {version}"));
                }
                *seen = version.max(*seen);
            }
            last.insert(id, version);
            documents.push((id, version));
        }
        drop(last);
        let position = envelopes.first().and_then(|envelope| envelope.position);
        self.items.lock().unwrap().push((position, documents));
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded.load(Ordering::SeqCst))
    }

    async fn mark_seeded(&self, _: &IndexName) -> sink::Result<()> {
        self.seeded.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// One ingest engine and one sink engine over a channel stream.
struct Deployment {
    stream: Arc<ChannelStream>,
    feed: Arc<Feed>,
    recorder: Arc<Recorder>,
    ingest: IngestEngine,
    sink: Option<SinkEngine>,
}

impl Deployment {
    fn new(feed: Feed, builder: Arc<Builder>, recorder: Arc<Recorder>) -> Self {
        let stream = Arc::new(ChannelStream::new(64, [sink_name()]));
        let feed = Arc::new(feed);
        let ingest = IngestEngine::new(
            Arc::clone(&feed) as Arc<dyn ChangeCapture>,
            Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![sink_name()],
        );
        let sink = SinkEngine::new(
            sink_name(),
            Arc::clone(&recorder) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![mapping()],
        )
        .with_progress(ingest.progress());
        Self {
            stream,
            feed,
            recorder,
            ingest,
            sink: Some(sink),
        }
    }

    fn ingest(mut self, f: impl FnOnce(IngestEngine) -> IngestEngine) -> Self {
        self.ingest = f(self.ingest);
        self
    }

    /// Stage the sink (asking for a backfill when it is unseeded) and start
    /// its engine.
    async fn start_sink(&mut self) -> tokio::task::JoinHandle<crate::Result<()>> {
        let sink = self.sink.take().unwrap();
        sink.stage(Continuity::Resumed).await.unwrap();
        tokio::spawn(async move {
            let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
            sink.run(&mut rx).await
        })
    }

    /// Wait until the sink acknowledged everything published.
    async fn drained(&self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        while !self.stream.is_idle() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the lane never drained"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    /// Run the ingest engine until its live stream ends, restarting it after
    /// a failed run the way the daemon does: the stream resumes after what
    /// every lane acknowledged. Returns how many runs failed.
    async fn run_to_end(&mut self, most_restarts: usize) -> usize {
        let sink = self.start_sink().await;
        let mut failed = 0;
        loop {
            match self.ingest.run().await {
                Ok(()) => break,
                Err(error) => {
                    failed += 1;
                    assert!(
                        failed <= most_restarts,
                        "the ingest engine failed {failed} times; the last: {error}"
                    );
                    self.drained().await;
                    let landed = self.ingest.progress().landed();
                    self.feed
                        .resume_after(landed.covered_seq().map(Position::new));
                }
            }
        }
        self.drained().await;
        sink.abort();
        failed
    }
}

fn instant(db: &Arc<Database>) -> Arc<Builder> {
    Builder::new(db, Behaviour::default())
}

fn batch(max_changes: usize) -> BatchPolicy {
    BatchPolicy {
        max_changes,
        max_delay: Duration::from_millis(50),
    }
}

/// One random schedule: changes over a few ids, random gaps, build delays and
/// failures, maybe a backfill. Panics on the first broken guarantee.
async fn schedule(seed: u64, builds: usize) {
    let mut random = Rng::new(seed);
    let ids = 2 + random.below(7) as i64;
    let rows = if random.below(2) == 0 {
        0
    } else {
        1 + random.below(40) as i64
    };
    // With a backfill, live changes write rows the backfill also builds:
    // a row can change between its page's read and its build, and a page and
    // a live batch can hold the same document.
    let ids = if rows > 0 { ids.min(rows) } else { ids };
    let changes = 1 + random.below(120) as usize;
    let log: Vec<Change> = (0..changes)
        .map(|_| Change {
            id: random.below(ids as u64) as i64,
            delete: random.below(6) == 0,
        })
        .collect();
    let max_changes = 1 + random.below(8) as usize;
    let db = Database::new(rows, log);
    let builder = Builder::new(
        &db,
        Behaviour {
            random: Some(Mutex::new(Rng::new(seed.wrapping_mul(31)))),
            failures_left: AtomicUsize::new(random.below(4) as usize),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).with_gaps(seed.wrapping_mul(17)),
        Arc::clone(&builder),
        Recorder::seeded(rows == 0),
    )
    .ingest(|ingest| ingest.with_batch(batch(max_changes)).with_builds(builds));
    deployment.run_to_end(8).await;

    let context = format!(
        "seed {seed}, builds {builds}, {changes} changes over {ids} ids, {rows} rows, max_changes {max_changes}"
    );
    assert!(
        !builder.overlapped.load(Ordering::SeqCst),
        "two builds of one document ran at once ({context})"
    );
    assert!(
        builder.most.load(Ordering::SeqCst) <= builds,
        "more builds ran at once than the setting allows ({context})"
    );
    let regressed = deployment.recorder.regressed.lock().unwrap().clone();
    assert!(
        regressed.is_empty(),
        "a document's version went down: {regressed:?} ({context})"
    );
    let positions = deployment.recorder.positions();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "the lane's positions are out of order: {positions:?} ({context})"
    );
    assert_eq!(
        deployment.recorder.state(),
        db.expected(),
        "the documents differ from the rows ({context})"
    );
}

#[tokio::test(start_paused = true)]
async fn random_schedules_keep_every_document_in_order() {
    let seeds: Vec<u64> = match std::env::var("FLUSSO_BUILD_SCHEDULE_SEED") {
        Ok(seed) => vec![seed.parse().unwrap()],
        Err(_) => (1..=60).collect(),
    };
    for seed in seeds {
        for builds in [1, 2, 4] {
            schedule(seed, builds).await;
        }
    }
}

#[tokio::test(start_paused = true)]
async fn builds_run_at_once_up_to_the_setting() {
    for builds in [1, 2, 4] {
        let db = Database::new(64, Vec::new());
        let builder = Builder::new(
            &db,
            Behaviour {
                delay: Some(Duration::from_millis(10)),
                ..Behaviour::default()
            },
        );
        let mut deployment = Deployment::new(
            Feed::new(&db),
            Arc::clone(&builder),
            Recorder::seeded(false),
        )
        .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(builds));
        deployment.run_to_end(0).await;

        assert_eq!(
            builder.most.load(Ordering::SeqCst),
            builds,
            "a backfill of 16 batches runs `builds` = {builds} at once"
        );
        assert_eq!(deployment.recorder.state(), db.expected());
    }
}

#[tokio::test(start_paused = true)]
async fn a_lone_change_on_a_quiet_stream_still_commits_at_once() {
    let db = Database::new(0, vec![Change::upsert(1)]);
    let builder = instant(&db);
    let mut deployment = Deployment::new(Feed::new(&db).open(), builder, Recorder::seeded(true))
        .ingest(|ingest| {
            ingest
                .with_batch(BatchPolicy {
                    max_changes: 256,
                    max_delay: Duration::from_secs(60),
                })
                .with_builds(4)
        });
    let sink = deployment.start_sink().await;
    let started = tokio::time::Instant::now();
    let recorder = Arc::clone(&deployment.recorder);
    let ingest = deployment.ingest;
    let run = tokio::spawn(async move { ingest.run().await });
    while recorder.items.lock().unwrap().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "a lone change on a quiet stream sat out the batching delay"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(started.elapsed() < Duration::from_secs(1));
    run.abort();
    sink.abort();
}

/// A backfill page and a live batch holding one document, in either order:
/// the second waits for the first's build, reads the row after it, and lands
/// after it, so the row's change between the page's read and its build ends
/// indexed.
#[tokio::test(start_paused = true)]
async fn a_backfill_page_and_a_live_batch_of_one_document_build_one_after_the_other() {
    for page_first in [true, false] {
        // Rows 0..4, one page of four keys; the live change writes row 2.
        let db = Database::new(4, vec![Change::upsert(2)]);
        let builder = Builder::new(
            &db,
            Behaviour {
                held: Mutex::new(HashSet::from([2])),
                keys_delay: (!page_first).then_some(Duration::from_millis(5)),
                ..Behaviour::default()
            },
        );
        let feed = if page_first {
            Feed::new(&db).with_delay(Duration::from_millis(5))
        } else {
            Feed::new(&db)
        };
        let mut deployment = Deployment::new(feed, Arc::clone(&builder), Recorder::seeded(false))
            .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(4));
        let releaser = {
            let builder = Arc::clone(&builder);
            tokio::spawn(async move {
                while builder.grants().len() < 1 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert_eq!(
                    builder.grants().len(),
                    1,
                    "the second build of row 2 waits for the first"
                );
                builder.release(2);
            })
        };
        deployment.run_to_end(0).await;
        releaser.await.unwrap();

        let expected = if page_first {
            vec![vec![0, 1, 2, 3], vec![2]]
        } else {
            vec![vec![2], vec![0, 1, 2, 3]]
        };
        assert_eq!(builder.grants(), expected, "page first: {page_first}");
        assert!(!builder.overlapped.load(Ordering::SeqCst));
        assert!(
            deployment.recorder.regressed.lock().unwrap().is_empty(),
            "row 2 never went back to the page's older read (page first: {page_first})"
        );
        assert_eq!(deployment.recorder.state(), db.expected());
        assert_eq!(deployment.recorder.state().get(&2), Some(&1));
    }
}

/// On a stream that stays open, each change lands though the build before
/// it ran on its own task: a partial live batch waits only while a live build
/// runs, not for ever after.
#[tokio::test(start_paused = true)]
async fn changes_apart_on_an_open_stream_each_land_after_a_spawned_build() {
    for builds in [1, 4] {
        let db = Database::new(0, (0..3).map(Change::upsert).collect());
        let builder = Builder::new(
            &db,
            Behaviour {
                delay: Some(Duration::from_millis(10)),
                ..Behaviour::default()
            },
        );
        let mut deployment = Deployment::new(
            Feed::new(&db).with_delay(Duration::from_millis(50)).open(),
            builder,
            Recorder::seeded(true),
        )
        .ingest(|ingest| ingest.with_batch(batch(256)).with_builds(builds));
        let sink = deployment.start_sink().await;
        let recorder = Arc::clone(&deployment.recorder);
        let ingest = deployment.ingest;
        let run = tokio::spawn(async move { ingest.run().await });
        let started = tokio::time::Instant::now();
        while recorder.positions().len() < 3 {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "builds {builds}: only {:?} landed",
                recorder.positions()
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert_eq!(recorder.positions(), vec![0, 1, 2]);
        run.abort();
        sink.abort();
    }
}

#[tokio::test(start_paused = true)]
async fn a_document_is_never_built_twice_at_once() {
    // Changes 1, 2, 1, 3, 1 in batches of one: the first build of 1 is held;
    // 2 builds beside it, and the second batch of 1 waits for it.
    let db = Database::new(0, [1, 2, 1, 3, 1].into_iter().map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([1])),
            ..Behaviour::default()
        },
    );
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true))
            .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(4));
    let releaser = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            while builder.grants().len() < 2 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert_eq!(builder.grants().len(), 2, "the second batch of 1 waits");
            builder.release(1);
        })
    };
    deployment.run_to_end(0).await;
    releaser.await.unwrap();

    assert!(!builder.overlapped.load(Ordering::SeqCst));
    assert_eq!(
        builder.grants(),
        vec![vec![1], vec![2], vec![1], vec![3], vec![1]],
        "2 builds beside the held 1; the next 1 after it"
    );
    assert_eq!(deployment.recorder.positions(), vec![0, 1, 2, 3, 4]);
    assert_eq!(deployment.recorder.state(), db.expected());
}

#[tokio::test(start_paused = true)]
async fn every_fourth_contested_grant_goes_to_the_backfill() {
    // Live ids from 1000, backfill rows below 100; batches of one, one build
    // at a time, so a live and a backfill batch are both waiting at every
    // grant once the backfill has started.
    let live: Vec<Change> = (1000..1200).map(Change::upsert).collect();
    let db = Database::new(60, live);
    let builder = Builder::new(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(1)),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(1));
    deployment.run_to_end(0).await;

    let kinds: String = builder
        .grants()
        .iter()
        .map(|ids| if ids[0] >= 1000 { 'L' } else { 'B' })
        .collect();
    let first = kinds.find('B').unwrap();
    let last_backfill = kinds.rfind('B').unwrap();
    let contested = &kinds[first..=last_backfill];
    let expected: String = "BLLL".repeat(contested.len() / 4 + 1);
    assert!(
        contested.len() > 40,
        "the backfill should share the builds with live batches for a while: {kinds}"
    );
    assert_eq!(
        contested,
        &expected[..contested.len()],
        "while both wait, three live grants then one backfill grant: {kinds}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

#[tokio::test(start_paused = true)]
async fn a_failed_build_publishes_nothing_after_it() {
    // Batches of one over ids 0..8; the build of id 3 fails after the later
    // batches already built.
    let db = Database::new(0, (0..8).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([3])),
            fail_once: Mutex::new(HashSet::from([3])),
            ..Behaviour::default()
        },
    );
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true))
            .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(4));
    let sink = deployment.start_sink().await;
    let releaser = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            while builder.grants().len() < 6 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            builder.release(3);
        })
    };
    let result = deployment.ingest.run().await;
    releaser.await.unwrap();
    deployment.drained().await;
    sink.abort();

    assert!(result.is_err(), "the failed build fails the run");
    assert_eq!(
        builder.now.load(Ordering::SeqCst),
        0,
        "the builds in flight finish before the run ends"
    );
    assert_eq!(
        deployment.recorder.positions(),
        vec![0, 1, 2],
        "everything before the failed batch is published, nothing after it"
    );
}

#[tokio::test(start_paused = true)]
async fn a_document_failed_under_stop_publishes_nothing_after_it_and_skip_goes_on() {
    for policy in [FailurePolicy::Stop, FailurePolicy::Skip] {
        let db = Database::new(0, (0..8).map(Change::upsert).collect());
        let builder = Builder::new(
            &db,
            Behaviour {
                held: Mutex::new(HashSet::from([3])),
                document_fails_once: Mutex::new(HashSet::from([3])),
                ..Behaviour::default()
            },
        );
        let mut deployment =
            Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true)).ingest(
                |ingest| {
                    ingest
                        .with_batch(batch(1))
                        .with_builds(4)
                        .with_failure_policies(FailurePolicies::new(policy))
                },
            );
        let sink = deployment.start_sink().await;
        let releaser = {
            let builder = Arc::clone(&builder);
            tokio::spawn(async move {
                while builder.grants().len() < 6 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
                builder.release(3);
            })
        };
        let result = deployment.ingest.run().await;
        releaser.await.unwrap();
        deployment.drained().await;
        sink.abort();

        match policy {
            FailurePolicy::Stop => {
                assert!(result.is_err());
                assert_eq!(deployment.recorder.positions(), vec![0, 1, 2]);
            }
            FailurePolicy::Skip => {
                assert!(result.is_ok());
                assert_eq!(
                    deployment.recorder.written_ids(),
                    vec![
                        vec![0],
                        vec![1],
                        vec![2],
                        vec![4],
                        vec![5],
                        vec![6],
                        vec![7]
                    ],
                    "the failed document is left out and its batch still published, in order"
                );
            }
        }
        assert_eq!(builder.now.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn a_stop_finishes_the_builds_in_flight_and_starts_no_other() {
    // Three batches build, held; a stop arrives; nothing new is built and the
    // three are published once released.
    let db = Database::new(0, (0..3).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([0, 1, 2])),
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(4)
            .with_stop(stop.clone())
    });
    let sink = deployment.start_sink().await;
    let driver = {
        let builder = Arc::clone(&builder);
        let stop = stop.clone();
        tokio::spawn(async move {
            while builder.grants().len() < 3 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            stop.request();
            tokio::time::sleep(Duration::from_millis(20)).await;
            for id in 0..3 {
                builder.release(id);
            }
        })
    };
    let result = deployment.ingest.run().await;
    driver.await.unwrap();
    sink.abort();

    assert!(result.is_ok(), "a stop ends the run cleanly: {result:?}");
    assert_eq!(builder.grants().len(), 3, "nothing is built after the stop");
    assert_eq!(deployment.recorder.positions(), vec![0, 1, 2]);
    assert_eq!(
        deployment.feed.confirmed.lock().unwrap().last(),
        Some(&Position::new(2)),
        "the last position is confirmed before the run ends"
    );
}

/// A recorded run at `builds = 1`: the batches built and the items published,
/// as the engine produced them before stage 4. Live changes alone (the
/// stream yields with gaps), then a backfill alone, so no race between the
/// two decides the order.
#[tokio::test(start_paused = true)]
async fn one_build_at_a_time_builds_and_publishes_as_before() {
    let log: Vec<Change> = [1, 2, 1, 3, 4, 4, 5, 1, 6, 7, 2, 8, 9, 9, 9, 10]
        .into_iter()
        .map(Change::upsert)
        .collect();
    let db = Database::new(0, log);
    let builder = instant(&db);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_gaps(7),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(3)).with_builds(1));
    deployment.run_to_end(0).await;
    let live = (
        builder.grants(),
        deployment.recorder.written_ids(),
        deployment.recorder.positions(),
    );

    let db = Database::new(23, Vec::new());
    let builder = instant(&db);
    let mut deployment = Deployment::new(
        Feed::new(&db),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| ingest.with_batch(batch(5)).with_builds(1));
    deployment.run_to_end(0).await;
    let backfill = (builder.grants(), deployment.recorder.written_ids());

    assert_eq!(
        format!("{live:?}"),
        "([[1, 2], [1], [3], [4], [4], [5, 1], [6], [7], [2], [8, 9], [9], [9, 10]], \
         [[1, 2], [1], [3], [4], [4], [5, 1], [6], [7], [2], [8, 9], [9], [9, 10]], \
         [1, 2, 3, 4, 5, 7, 8, 9, 10, 12, 13, 15])",
        "live batches built and published as before"
    );
    assert_eq!(
        format!("{backfill:?}"),
        "([[0, 1, 2, 3, 4], [5, 6, 7, 8, 9], [10, 11, 12, 13, 14], [15, 16, 17, 18, 19], [20, 21, 22]], \
         [[0, 1, 2, 3, 4], [5, 6, 7, 8, 9], [10, 11, 12, 13, 14], [15, 16, 17, 18, 19], [20, 21, 22]])",
        "backfill batches built and published as before"
    );
}
