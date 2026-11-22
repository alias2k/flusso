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
    BatchPolicy, Budget, FailurePolicies, FailurePolicy, IngestEngine, SinkControl, SinkEngine,
    Stop,
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

/// The indexes a [`Builder`] over several spreads its ids across, in order:
/// id `i` is a document of `INDEXES[i % n]`.
const INDEXES: [&str; 3] = ["users", "orders", "items"];

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
    /// How many documents a live change to a row touches: the row's own
    /// and `fan - 1` aliases (`id + k × ALIAS`), each holding the row.
    fan: i64,
}

/// What separates a row's aliases: a multiple of every index count, so an
/// alias is a document of its row's index.
const ALIAS: i64 = 600_000;

impl Database {
    fn new(rows: i64, log: Vec<Change>) -> Arc<Self> {
        Self::fanned(rows, log, 1)
    }

    /// A database whose live changes each touch `fan` documents.
    fn fanned(rows: i64, log: Vec<Change>, fan: i64) -> Arc<Self> {
        Arc::new(Self {
            rows,
            log,
            visible: AtomicUsize::new(0),
            fan: fan.max(1),
        })
    }

    /// The documents a live change to `id` touches.
    fn touched(&self, id: i64) -> impl Iterator<Item = i64> {
        (0..self.fan).map(move |alias| id + alias * ALIAS)
    }

    fn reveal(&self, through: usize) {
        self.visible.fetch_max(through, Ordering::SeqCst);
    }

    /// `id`'s version as a read now sees it; `None` when the row is absent.
    /// An alias reads its row.
    fn read(&self, id: i64) -> Option<u64> {
        let id = id.rem_euclid(ALIAS);
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
        ids.extend(self.log.iter().flat_map(|change| self.touched(change.id)));
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
    /// A wait before every `n`th change only: bursts of `n` back to back.
    bursts: Option<(usize, Duration)>,
    open: bool,
    confirmed: Mutex<Vec<Position>>,
    /// When each change arrives, from the stream's opening, whenever it is
    /// polled: a source's own clock, not a gap after each poll.
    arrivals: Option<Arc<Vec<Duration>>>,
    /// Hand the changes over through a tokio channel, whose receiving end
    /// spends the task's cooperative budget, as a real source's does.
    through_channel: bool,
}

impl Feed {
    fn new(db: &Arc<Database>) -> Self {
        Self {
            db: Arc::clone(db),
            from: AtomicUsize::new(0),
            gaps: None,
            delay: None,
            bursts: None,
            open: false,
            confirmed: Mutex::default(),
            arrivals: None,
            through_channel: false,
        }
    }

    /// Read through a tokio channel, which spends the reading task's
    /// cooperative budget per change.
    fn through_a_tokio_channel(mut self) -> Self {
        self.through_channel = true;
        self
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    fn with_bursts(mut self, every: usize, pause: Duration) -> Self {
        self.bursts = Some((every.max(1), pause));
        self
    }

    fn with_gaps(mut self, seed: u64) -> Self {
        self.gaps = Some(Arc::new(Mutex::new(Rng::new(seed))));
        self
    }

    /// Change `n` arrives `arrivals[n]` after the stream opens.
    fn with_arrivals(mut self, arrivals: Vec<Duration>) -> Self {
        self.arrivals = Some(Arc::new(arrivals));
        self
    }

    fn open(mut self) -> Self {
        self.open = true;
        self
    }

    /// The next run's stream starts after the newest position it was
    /// confirmed, as a replication slot resumes: whatever an engine run
    /// confirmed too early is never read again.
    fn resume_after_confirmed(&self) {
        let from = self
            .confirmed
            .lock()
            .unwrap()
            .iter()
            .map(|position| position.seq as usize + 1)
            .max()
            .unwrap_or(0);
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
        let bursts = self.bursts;
        let open = self.open;
        let arrivals = self.arrivals.clone();
        let opened = tokio::time::Instant::now();
        let from = self.from.load(Ordering::SeqCst);
        let through_channel = self.through_channel;
        let changes = Box::pin(futures::stream::unfold(from, move |next| {
            let db = Arc::clone(&db);
            let gaps = gaps.clone();
            let arrivals = arrivals.clone();
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
                if let Some(at) = arrivals.as_ref().and_then(|arrivals| arrivals.get(next)) {
                    tokio::time::sleep_until(opened + *at).await;
                }
                if let Some((every, pause)) = bursts
                    && next % every == 0
                {
                    tokio::time::sleep(pause).await;
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
        }));
        if !through_channel {
            return Ok(changes);
        }
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut changes = changes;
            while let Some(change) = futures::StreamExt::next(&mut changes).await {
                if sender.send(change).is_err() {
                    return;
                }
            }
        });
        Ok(Box::pin(futures::stream::poll_fn(move |cx| {
            receiver.poll_recv(cx)
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
    /// A build touching one of these ids fails, this many times each
    /// (slice 8: an index failing again after its restart).
    fail_times: Mutex<BTreeMap<i64, usize>>,
    /// A build touching one of these ids panics as it starts, once (on its
    /// first poll, which the builder runs inline).
    panic_once: Mutex<HashSet<i64>>,
    /// Reading a key page of one of these indexes fails, once.
    keys_fail_once: Mutex<HashSet<String>>,
    /// Resolving one of these ids fails, once.
    resolve_fails_once: Mutex<HashSet<i64>>,
    /// Walks can be resumed: a mark is the key's id, ordered by id.
    resumable: bool,
    /// Every build takes this long.
    delay: Option<Duration>,
    /// Every key page takes this long to read.
    keys_delay: Option<Duration>,
    /// Every upsert carries this many bytes of padding.
    padding: usize,
    /// Every resolve takes this long.
    resolve_delay: Option<Duration>,
    /// Resolving this id requests this stop.
    stop_on_resolve: Mutex<Option<(i64, Stop)>>,
    /// Resolving one of these ids fails.
    resolve_fails: Mutex<HashSet<i64>>,
    /// Resolving the first id releases the builds holding the second.
    release_on_resolve: Option<(i64, i64)>,
    /// Resolving this id panics.
    resolve_panics: Option<i64>,
}

/// Builds documents from the [`Database`], reading each version when the
/// build starts, and records what a test asks about: each build's ids, two
/// builds of one document at once, how many builds ran at once.
#[derive(Debug)]
struct Builder {
    db: Arc<Database>,
    behaviour: Behaviour,
    grants: Mutex<Vec<Vec<i64>>>,
    /// When each id was first built.
    built_at: Mutex<BTreeMap<i64, tokio::time::Instant>>,
    building: Mutex<HashSet<i64>>,
    overlapped: AtomicBool,
    now: AtomicUsize,
    most: AtomicUsize,
    /// Builds dropped before they finished.
    cancelled: Arc<AtomicUsize>,
    /// How many indexes its documents fall in: id `i` is a document of
    /// index `i % indexes` of [`INDEXES`].
    indexes: usize,
}

/// Counts a build dropped before it finished.
struct Unfinished(Option<Arc<AtomicUsize>>);

impl Drop for Unfinished {
    fn drop(&mut self) {
        if let Some(cancelled) = self.0.take() {
            cancelled.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl Builder {
    fn new(db: &Arc<Database>, behaviour: Behaviour) -> Arc<Self> {
        Arc::new(Self {
            db: Arc::clone(db),
            behaviour,
            grants: Mutex::default(),
            built_at: Mutex::default(),
            building: Mutex::default(),
            overlapped: AtomicBool::new(false),
            now: AtomicUsize::new(0),
            most: AtomicUsize::new(0),
            cancelled: Arc::default(),
            indexes: 1,
        })
    }

    /// The builder over two indexes, `users` (even ids) and `orders` (odd).
    fn two(db: &Arc<Database>, behaviour: Behaviour) -> Arc<Self> {
        Self::over(db, behaviour, 2)
    }

    /// The builder over the first `indexes` of [`INDEXES`]: id `i` is a
    /// document of index `i % indexes`.
    fn over(db: &Arc<Database>, behaviour: Behaviour, indexes: usize) -> Arc<Self> {
        let mut builder = Arc::into_inner(Self::new(db, behaviour)).unwrap();
        builder.indexes = indexes.clamp(1, INDEXES.len());
        Arc::new(builder)
    }

    fn index_of(&self, id: i64) -> IndexName {
        let at = id.rem_euclid(self.indexes as i64) as usize;
        IndexName::try_new(INDEXES.get(at).copied().unwrap_or("users")).unwrap()
    }

    fn indexes(&self) -> Vec<IndexName> {
        INDEXES
            .iter()
            .take(self.indexes)
            .map(|name| IndexName::try_new(*name).unwrap())
            .collect()
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
        } || {
            let mut fail_times = self.behaviour.fail_times.lock().unwrap();
            ids.iter().any(|id| match fail_times.get_mut(id) {
                Some(left) if *left > 0 => {
                    *left -= 1;
                    true
                }
                _ => false,
            })
        };
        if fail || failing {
            return Err(SourceError::Connection("the build failed".to_owned()));
        }
        let mut documents = Vec::with_capacity(ids.len());
        for (id, version) in ids.iter().zip(versions) {
            let document_id = DocumentId {
                index: self.index_of(*id),
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
                Some(version) => {
                    let mut body =
                        BTreeMap::from([("v".to_owned(), GenericValue::BigInt(version as i64))]);
                    if self.behaviour.padding > 0 {
                        body.insert(
                            "pad".to_owned(),
                            GenericValue::String("x".repeat(self.behaviour.padding)),
                        );
                    }
                    Document::Upsert {
                        id: document_id,
                        body: GenericValue::Map(body),
                    }
                }
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
        let id = id_of(key);
        if let Some(delay) = self.behaviour.resolve_delay {
            tokio::time::sleep(delay).await;
        }
        if let Some((at, stop)) = &*self.behaviour.stop_on_resolve.lock().unwrap()
            && *at == id
        {
            stop.request();
        }
        assert!(
            self.behaviour.resolve_panics != Some(id),
            "resolving {id} panicked"
        );
        if let Some((at, held)) = self.behaviour.release_on_resolve
            && at == id
        {
            self.release(held);
        }
        if self
            .behaviour
            .resolve_fails_once
            .lock()
            .unwrap()
            .remove(&id)
        {
            return Err(SourceError::Connection(format!(
                "resolving {id} failed once"
            )));
        }
        if self.behaviour.resolve_fails.lock().unwrap().contains(&id) {
            return Err(SourceError::Connection(format!("resolving {id} failed")));
        }
        Ok(self
            .db
            .touched(id)
            .map(|id| DocumentId {
                index: self.index_of(id),
                key: crate::ingest::tests::key(id),
            })
            .collect())
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        let mut built = self.build_many(std::slice::from_ref(id)).await?;
        Ok(built.remove(0))
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        let ids: Vec<i64> = ids.iter().map(|id| id_of(&id.key)).collect();
        {
            let mut panic_once = self.behaviour.panic_once.lock().unwrap();
            if let Some(id) = ids.iter().find(|id| panic_once.remove(id)) {
                drop(panic_once);
                panic!("building {id} panicked");
            }
        }
        self.grants.lock().unwrap().push(ids.clone());
        {
            let now = tokio::time::Instant::now();
            let mut built_at = self.built_at.lock().unwrap();
            for id in &ids {
                built_at.entry(*id).or_insert(now);
            }
        }
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
        let mut unfinished = Unfinished(Some(Arc::clone(&self.cancelled)));
        let result = self.work(&ids).await;
        unfinished.0 = None;
        self.now.fetch_sub(1, Ordering::SeqCst);
        let mut building = self.building.lock().unwrap();
        for id in &ids {
            building.remove(id);
        }
        result
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        self.indexes()
    }

    async fn last_key(&self, index: &IndexName) -> source::Result<Option<RowKey>> {
        Ok((0..self.db.rows)
            .rev()
            .find(|id| self.index_of(*id) == *index)
            .map(key))
    }

    async fn keys(
        &self,
        index: &IndexName,
        after: Option<&RowKey>,
        through: &RowKey,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        if let Some(delay) = self.behaviour.keys_delay {
            tokio::time::sleep(delay).await;
        }
        if self
            .behaviour
            .keys_fail_once
            .lock()
            .unwrap()
            .remove(index.as_ref())
        {
            return Err(SourceError::Query(format!("reading {index}'s keys failed")));
        }
        let start = after.map_or(0, |after| id_of(after) + 1);
        let end = self.db.rows.min(id_of(through) + 1);
        Ok((start..end)
            .filter(|id| self.index_of(*id) == *index)
            .take(limit)
            .map(key)
            .collect())
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok(self
            .indexes()
            .into_iter()
            .map(|index| IndexMapping { index, ..mapping() })
            .collect())
    }

    async fn walk_mark(&self, _: &IndexName, key: &RowKey) -> source::Result<Option<String>> {
        Ok(self.behaviour.resumable.then(|| id_of(key).to_string()))
    }

    async fn resume_after(&self, _: &IndexName, mark: &str) -> source::Result<Option<RowKey>> {
        Ok(self
            .behaviour
            .resumable
            .then(|| mark.parse().ok().map(key))
            .flatten())
    }

    async fn compare_keys(
        &self,
        _: &IndexName,
        a: &RowKey,
        b: &RowKey,
    ) -> source::Result<Option<std::cmp::Ordering>> {
        Ok(self.behaviour.resumable.then(|| id_of(a).cmp(&id_of(b))))
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
    /// While set, every write waits.
    stalled: AtomicBool,
    resumed: Notify,
    /// A budget and the most it may hold, checked at every write.
    watch: Mutex<Option<(Arc<Budget>, usize)>>,
    overdrawn: Mutex<Vec<usize>>,
    /// The most one index's live documents may hold, checked at every write
    /// for each of [`INDEXES`].
    watch_index: Mutex<Option<usize>>,
    overdrawn_index: Mutex<Vec<(String, usize)>>,

    /// The next write fails, once.
    fail_next: AtomicBool,
    /// How long each write takes, when set.
    write_delay: Mutex<Option<Duration>>,
    /// Indexes seeded on their own, beside `seeded`.
    seeded_indexes: Mutex<HashSet<String>>,
    /// Indexes whose every document is rejected: under `on_error = "stop"`
    /// their lane parks.
    refused_indexes: Mutex<HashSet<String>>,
    /// When each index was marked seeded.
    marked_at: Mutex<BTreeMap<String, tokio::time::Instant>>,
    /// When each document was first written.
    written_at: Mutex<BTreeMap<i64, tokio::time::Instant>>,
    /// Every write of a version: the document, the version, when.
    landings: Mutex<Vec<(i64, u64, tokio::time::Instant)>>,
}

impl Recorder {
    /// The documents of a refused index in `envelopes`, rejected.
    fn rejected(&self, envelopes: &[Envelope]) -> Vec<sink::RejectedDocument> {
        let refused = self.refused_indexes.lock().unwrap();
        envelopes
            .iter()
            .filter(|envelope| refused.contains(envelope.index.as_ref()))
            .map(|envelope| sink::RejectedDocument {
                index: envelope.index.as_ref().to_owned(),
                id: envelope.id.clone(),
                reason: "refused".to_owned(),
            })
            .collect()
    }

    fn stall(&self) {
        self.stalled.store(true, Ordering::SeqCst);
    }

    fn resume(&self) {
        self.stalled.store(false, Ordering::SeqCst);
        self.resumed.notify_waiters();
    }

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
        loop {
            let resumed = self.resumed.notified();
            tokio::pin!(resumed);
            resumed.as_mut().enable();
            if !self.stalled.load(Ordering::SeqCst) {
                break;
            }
            resumed.await;
        }
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sink::SinkError::Write("the write failed".to_owned()));
        }
        let delay = *self.write_delay.lock().unwrap();
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        let rejected = self.rejected(envelopes);
        let accepted: Vec<Envelope> = envelopes
            .iter()
            .filter(|envelope| {
                !rejected.iter().any(|refused| {
                    refused.id == envelope.id && refused.index == envelope.index.as_ref()
                })
            })
            .cloned()
            .collect();
        let envelopes = accepted.as_slice();
        if let Some((budget, most)) = &*self.watch.lock().unwrap() {
            let held = budget.held().total;
            if held > *most {
                self.overdrawn.lock().unwrap().push(held);
            }
            if let Some(most) = *self.watch_index.lock().unwrap() {
                for name in INDEXES {
                    let held = budget.live_held_by(&IndexName::try_new(name).unwrap());
                    if held > most {
                        self.overdrawn_index
                            .lock()
                            .unwrap()
                            .push((name.to_owned(), held));
                    }
                }
            }
        }
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
            if let Some(version) = version {
                self.landings
                    .lock()
                    .unwrap()
                    .push((id, version, tokio::time::Instant::now()));
            }
            self.written_at
                .lock()
                .unwrap()
                .entry(id)
                .or_insert_with(tokio::time::Instant::now);
            documents.push((id, version));
        }
        drop(last);
        let position = envelopes.first().and_then(|envelope| envelope.position);
        self.items.lock().unwrap().push((position, documents));
        Ok(WriteReport::clean().rejecting(rejected))
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded.load(Ordering::SeqCst)
            || self.seeded_indexes.lock().unwrap().contains(index.as_ref()))
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.seeded_indexes
            .lock()
            .unwrap()
            .insert(index.as_ref().to_owned());
        self.marked_at
            .lock()
            .unwrap()
            .insert(index.as_ref().to_owned(), tokio::time::Instant::now());
        Ok(())
    }
}

/// The channel stream, recording each position the ingest engine publishes,
/// in lane order: the order the watermark trusts. The sink may write items
/// in another order (it admits per document and takes live first).
#[derive(Debug)]
struct Spy {
    inner: Arc<ChannelStream>,
    published: Published,
    failing: FailingLanes,
}

/// The indexes whose lanes fail their next publish, once each.
type FailingLanes = Arc<Mutex<HashSet<String>>>;

/// Every position published, per lane, in lane order.
type Published = Arc<Mutex<BTreeMap<stream::LaneKey, Vec<u64>>>>;

/// Where an engine run ended and the next began, in [`Published`].
const RESTART: u64 = u64::MAX;

#[derive(Debug)]
struct SpyProducer {
    inner: Box<dyn stream::Producer<stream::LaneItem>>,
    lane: stream::LaneKey,
    published: Published,
    failing: FailingLanes,
}

#[async_trait]
impl stream::Producer<stream::LaneItem> for SpyProducer {
    async fn publish(&self, item: stream::LaneItem) -> stream::Result<()> {
        // The engine awaits each item's send before the next one's: logged
        // after the send, the log is lane order.
        let position = item.position();
        if self
            .failing
            .lock()
            .unwrap()
            .remove(self.lane.index.as_ref())
        {
            return Err(stream::QueueError::Publish("the lane failed".to_owned()));
        }
        self.inner.publish(item).await?;
        if let Some(position) = position {
            self.published
                .lock()
                .unwrap()
                .entry(self.lane.clone())
                .or_default()
                .push(position.seq);
        }
        Ok(())
    }
}

impl Stream for Spy {
    fn lane(&self, key: &stream::LaneKey) -> stream::Result<stream::Lane> {
        let lane = self.inner.lane(key)?;
        Ok(stream::Lane {
            producer: Box::new(SpyProducer {
                inner: lane.producer,
                lane: key.clone(),
                published: Arc::clone(&self.published),
                failing: Arc::clone(&self.failing),
            }),
            consumer: lane.consumer,
        })
    }

    fn lanes(&self, sink: &SinkName) -> Vec<stream::LaneKey> {
        self.inner.lanes(sink)
    }

    fn requests(&self) -> stream::Result<stream::RequestLane> {
        self.inner.requests()
    }

    fn is_idle(&self) -> bool {
        self.inner.is_idle()
    }
}

/// One ingest engine and one sink engine over a channel stream.
struct Deployment {
    stream: Arc<ChannelStream>,
    /// Every position published, per lane, in lane order.
    published: Published,
    /// Lanes to fail on their next publish.
    failing: FailingLanes,
    feed: Arc<Feed>,
    recorder: Arc<Recorder>,
    ingest: IngestEngine,
    sink: Option<SinkEngine>,
    budget: Option<Arc<Budget>>,
    /// How many indexes it runs: a budget is shared over them.
    indexes: usize,
    /// How long a failed engine run waits before the next (an outage).
    restart_delay: Duration,
}

impl Deployment {
    fn new(feed: Feed, builder: Arc<Builder>, recorder: Arc<Recorder>) -> Self {
        Self::with_lanes(feed, builder, recorder, 64)
    }

    /// [`new`](Self::new) with lanes of `capacity` items: a deployment's
    /// lanes hold over a million, so its budget binds before a lane fills.
    fn with_lanes(
        feed: Feed,
        builder: Arc<Builder>,
        recorder: Arc<Recorder>,
        capacity: usize,
    ) -> Self {
        let indexes = builder.indexes();
        let count = indexes.len();
        let stream = Arc::new(ChannelStream::over(
            capacity,
            indexes.clone(),
            [sink_name()],
        ));
        let feed = Arc::new(feed);
        let published: Published = Arc::default();
        let failing: FailingLanes = Arc::default();
        let spy = Arc::new(Spy {
            inner: Arc::clone(&stream),
            published: Arc::clone(&published),
            failing: Arc::clone(&failing),
        });
        let ingest = IngestEngine::new(
            Arc::clone(&feed) as Arc<dyn ChangeCapture>,
            Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
            spy as Arc<dyn Stream>,
            vec![sink_name()],
            Arc::new(crate::Progress::of(indexes.clone(), [sink_name()])),
        );
        let sink = SinkEngine::new(
            sink_name(),
            Arc::clone(&recorder) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            indexes
                .into_iter()
                .map(|index| IndexMapping { index, ..mapping() })
                .collect(),
            ingest.progress(),
        );
        Self {
            stream,
            published,
            failing,
            feed,
            recorder,
            ingest,
            sink: Some(sink),
            budget: None,
            indexes: count,
            restart_delay: Duration::ZERO,
        }
        .counting()
    }

    /// Count what the engine builds within a budget that never refuses, so
    /// every test checks every byte comes back.
    fn counting(mut self) -> Self {
        let budget = Arc::new(Budget::new(usize::MAX / 4));
        self.ingest = self.ingest.with_budget(Arc::clone(&budget));
        self.budget = Some(budget);
        self
    }

    /// Hold built documents within `bytes`, shared over its indexes as the
    /// daemon shares it (half each, with two or more), and check at every
    /// write that the budget holds at most `most`.
    fn with_budget(self, bytes: usize, most: usize) -> Self {
        let indexes = self.indexes;
        self.with_shared_budget(bytes, most, Some(indexes))
    }

    /// [`with_budget`](Self::with_budget), each index's live share `share`
    /// of the limit.
    fn with_budget_shared_at(mut self, bytes: usize, share: f64) -> Self {
        let budget = Arc::new(Budget::new(bytes).with_index_share(self.indexes, share));
        self.ingest = self.ingest.with_budget(Arc::clone(&budget));
        self.budget = Some(budget);
        self
    }

    /// [`with_budget`](Self::with_budget), shared over `indexes` (`None`:
    /// not shared, the budget's rules before slice 4).
    fn with_shared_budget(mut self, bytes: usize, most: usize, indexes: Option<usize>) -> Self {
        let budget = Budget::new(bytes);
        let budget = Arc::new(match indexes {
            Some(indexes) => budget.with_index_share(indexes, 0.5),
            None => budget,
        });
        self.ingest = self.ingest.with_budget(Arc::clone(&budget));
        *self.recorder.watch.lock().unwrap() = Some((Arc::clone(&budget), most));
        self.budget = Some(budget);
        self
    }

    /// Every byte the budget counted came back once the lanes drained.
    async fn assert_released(&self) {
        let Some(budget) = &self.budget else {
            return;
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        while budget.held() != crate::Held::default() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the budget still holds {:?} after the lanes drained",
                budget.held()
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let overdrawn = self.recorder.overdrawn.lock().unwrap().clone();
        assert!(
            overdrawn.is_empty(),
            "the budget held more than allowed at some write: {overdrawn:?}"
        );
        let overdrawn = self.recorder.overdrawn_index.lock().unwrap().clone();
        assert!(
            overdrawn.is_empty(),
            "an index's live documents held more than allowed at some write: {overdrawn:?}"
        );
    }

    fn ingest(mut self, f: impl FnOnce(IngestEngine) -> IngestEngine) -> Self {
        self.ingest = f(self.ingest);
        self
    }

    /// Stage the sink (asking for a backfill when it is unseeded) and start
    /// its engine, run again after a failed run as the daemon runs it.
    async fn start_sink(&mut self) -> tokio::task::JoinHandle<crate::Result<()>> {
        let sink = self.sink.take().unwrap();
        sink.stage(Continuity::Resumed).await.unwrap();
        tokio::spawn(async move {
            let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
            loop {
                match sink.run(&mut rx).await {
                    Ok(()) => return Ok(()),
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
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

    /// Run the ingest engine on a stream that stays open until every change
    /// of `db` has landed, then stop it, restarting it after a failed run as
    /// [`run_to_end`](Self::run_to_end) does.
    /// An engine run ended and the next starts: the positions published
    /// after it are another run's (`RESTART` in the record).
    fn restarted(&self) {
        for positions in self.published.lock().unwrap().values_mut() {
            positions.push(RESTART);
        }
    }

    async fn run_until_landed(
        &mut self,
        db: &Arc<Database>,
        stop: &Stop,
        most_restarts: usize,
    ) -> usize {
        let sink = self.start_sink().await;
        let watcher = {
            let db = Arc::clone(db);
            let stop = stop.clone();
            let recorder = Arc::clone(&self.recorder);
            let stream = Arc::clone(&self.stream);
            tokio::spawn(async move {
                loop {
                    if db.visible.load(Ordering::SeqCst) == db.log.len()
                        && stream.is_idle()
                        && recorder.state() == db.expected()
                    {
                        stop.request();
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
        };
        let mut failed = 0;
        loop {
            match self.ingest.run().await {
                Ok(()) => break,
                Err(error) => {
                    failed += 1;
                    self.restarted();
                    assert!(
                        failed <= most_restarts,
                        "the ingest engine failed {failed} times; the last: {error}"
                    );
                    self.drained().await;
                    self.feed.resume_after_confirmed();
                }
            }
        }
        watcher.await.unwrap();
        self.drained().await;
        self.assert_released().await;
        sink.abort();
        failed
    }

    /// Run the ingest engine until its live stream ends, restarting it after
    /// a failed run the way the daemon does: the stream resumes after what
    /// the source was confirmed. Returns how many runs failed.
    async fn run_to_end(&mut self, most_restarts: usize) -> usize {
        let sink = self.start_sink().await;
        let mut failed = 0;
        loop {
            match self.ingest.run().await {
                Ok(()) => break,
                Err(error) => {
                    failed += 1;
                    self.restarted();
                    assert!(
                        failed <= most_restarts,
                        "the ingest engine failed {failed} times; the last: {error}"
                    );
                    self.drained().await;
                    tokio::time::sleep(self.restart_delay).await;
                    self.feed.resume_after_confirmed();
                }
            }
        }
        self.drained().await;
        self.assert_released().await;
        sink.abort();
        failed
    }
}

mod builders;
mod exemption;
mod reading;
mod restart;
mod states;
mod walk_failures;
mod walks;

fn instant(db: &Arc<Database>) -> Arc<Builder> {
    Builder::new(db, Behaviour::default())
}

fn batch(max_changes: usize) -> BatchPolicy {
    BatchPolicy {
        max_changes,
        max_delay: Duration::from_millis(50),
    }
}

/// Counts the indexes restarted alone (slice 8).
#[derive(Debug, Default)]
struct IndexRestarts(AtomicUsize);

impl crate::Observer for IndexRestarts {
    fn on_index_restarted(&self, _: &IndexName, _: crate::Work) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// One random schedule: changes over a few ids, random gaps, build delays and
/// failures, maybe a backfill. Over two indexes a build's failure restarts
/// its index alone (slice 8), and some schedules also fail a whole run once
/// (a resolve), so the stream resumes from what was confirmed while an index
/// may hold documents it gave back. Panics on the first broken guarantee.
/// Returns how many indexes restarted alone and how many runs failed.
async fn schedule(seed: u64, builds: usize, open: bool) -> (usize, usize) {
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
    // Every third seed fans each change out to up to four documents, so the
    // document cap (O2) splits a change across builds, and a failure can
    // end a run between its parts.
    let fan = if seed % 3 == 0 {
        1 + (seed / 3 % 4) as i64
    } else {
        1
    };
    let db = Database::fanned(rows, log, fan);
    // Every other seed over two indexes (odd ids `orders`', even `users'`):
    // a batch splits by index, each lane in its own order.
    let two = seed % 2 == 0;
    let run_failure = (two && random.below(2) == 0).then(|| random.below(ids as u64) as i64);
    let behaviour = Behaviour {
        random: Some(Mutex::new(Rng::new(seed.wrapping_mul(31)))),
        failures_left: AtomicUsize::new(random.below(4) as usize),
        resolve_fails_once: Mutex::new(run_failure.into_iter().collect()),
        ..Behaviour::default()
    };
    let builder = if two {
        Builder::two(&db, behaviour)
    } else {
        Builder::new(&db, behaviour)
    };
    let feed = Feed::new(&db).with_gaps(seed.wrapping_mul(17));
    let feed = if open { feed.open() } else { feed };
    let stop = Stop::new();
    let recorder = Recorder::seeded(rows == 0);
    let restarts = Arc::new(IndexRestarts::default());
    let observer: Arc<dyn crate::Observer> = Arc::clone(&restarts) as Arc<dyn crate::Observer>;
    let mut deployment =
        Deployment::new(feed, Arc::clone(&builder), Arc::clone(&recorder)).ingest(|ingest| {
            ingest
                .with_batch(batch(max_changes))
                .with_builds(builds)
                .with_stop(stop.clone())
                .with_observer(observer)
        });
    // Over two indexes, every third seed holds `orders` stuck for a random
    // stretch (every document refused, its lane parked): held at its cap it
    // becomes exempt (6b-2) while failures restart the run, and nothing of it
    // may be confirmed before it lands.
    let stuck = (two && seed % 3 == 1).then(|| {
        let from = Duration::from_millis(random.below(200));
        let lasting = Duration::from_millis(50 + random.below(1000));
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(from).await;
            recorder
                .refused_indexes
                .lock()
                .unwrap()
                .insert("orders".to_owned());
            tokio::time::sleep(lasting).await;
            recorder.refused_indexes.lock().unwrap().clear();
        })
    });
    let failed = if open {
        deployment.run_until_landed(&db, &stop, 8).await
    } else {
        deployment.run_to_end(8).await
    };
    if let Some(stuck) = stuck {
        stuck.await.unwrap();
    }

    let context = format!(
        "seed {seed}, builds {builds}, two indexes {two}, {changes} changes over {ids} ids, {rows} rows, max_changes {max_changes}, fan {fan}, stuck {}",
        seed % 3 == 1 && two
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
    // Within one engine run a lane's positions never go down; the parts of
    // a change the document cap split share its position. A restart
    // replays from the confirmed position, the least over the lanes, so a
    // lane whose index was ahead of another's gets some of its positions
    // again.
    for (lane, positions) in deployment.published.lock().unwrap().iter() {
        for run in positions.split(|position| *position == RESTART) {
            assert!(
                run.windows(2).all(|pair| pair[0] <= pair[1]),
                "{lane}'s positions are out of order within a run: {positions:?} ({context})"
            );
        }
    }
    assert_eq!(
        deployment.recorder.state(),
        db.expected(),
        "the documents differ from the rows ({context})"
    );
    (restarts.0.load(Ordering::SeqCst), failed)
}

/// The seeds a schedule test runs: `FLUSSO_BUILD_SCHEDULE_SEED` alone, else
/// `1..=count`. Each runs on a runtime of its own, so a seed replays alone.
fn seeds(count: u64) -> Vec<u64> {
    match std::env::var("FLUSSO_BUILD_SCHEDULE_SEED") {
        Ok(seed) => vec![seed.parse().unwrap()],
        Err(_) => (1..=count).collect(),
    }
}

/// Counts what restarted across schedules: indexes alone, whole runs.
#[derive(Debug, Default)]
struct Restarted {
    indexes: usize,
    runs: usize,
}

impl Restarted {
    fn add(&mut self, (indexes, runs): (usize, usize)) {
        self.indexes += indexes;
        self.runs += runs;
    }

    /// Over every seed (not one replayed alone), the schedules restarted an
    /// index alone and a whole run, at times both in one schedule.
    fn assert_both(&self) {
        if std::env::var("FLUSSO_BUILD_SCHEDULE_SEED").is_ok() {
            return;
        }
        assert!(self.indexes > 0, "no schedule restarted an index alone");
        assert!(self.runs > 0, "no schedule restarted a whole run");
    }
}

#[test]
fn random_schedules_keep_every_document_in_order() {
    let mut restarted = Restarted::default();
    for seed in seeds(500) {
        for builds in [1, 2, 4] {
            restarted.add(crate::tests::paused_runtime().block_on(schedule(seed, builds, false)));
        }
    }
    restarted.assert_both();
}

/// The same schedules on a stream that stays open, as a deployment's does:
/// nothing ends the run but a stop, requested once every change has landed.
#[test]
fn random_schedules_on_an_open_stream_keep_every_document_in_order() {
    let mut restarted = Restarted::default();
    for seed in seeds(200) {
        for builds in [1, 4] {
            restarted.add(crate::tests::paused_runtime().block_on(schedule(seed, builds, true)));
        }
    }
    restarted.assert_both();
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

/// `main`'s admission point: a document leaves admission when its build
/// lands, not when it is published. Changes 1, 2, 2 in batches of one, four
/// builds: 1 is held, 2's first build lands and waits behind it to be
/// published, and the second batch of 2 is granted meanwhile (its slot is
/// later, so it still publishes after).
#[tokio::test(start_paused = true)]
async fn a_document_whose_build_landed_is_built_again_while_an_earlier_slot_waits() {
    let db = Database::new(0, [1, 2, 2].into_iter().map(Change::upsert).collect());
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
            let grants = builder.grants().len();
            builder.release(1);
            grants
        })
    };
    deployment.run_to_end(0).await;
    assert_eq!(
        releaser.await.unwrap(),
        3,
        "the second batch of 2 is granted while 1 is held"
    );
    assert!(!builder.overlapped.load(Ordering::SeqCst));
    assert_eq!(deployment.recorder.positions(), vec![0, 1, 2]);
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// A failed run leaves more than one batch behind unpublished: changes 1 to
/// 6 in batches of two over four builds, 1's build held, then failing once
/// the other two batches landed behind it. The next run forgets them, and
/// its stream, resumed before the failed change, replays every one: they are
/// rebuilt in batches of two, as read, never in one batch of all six, and
/// every sink ends with them.
#[tokio::test(start_paused = true)]
async fn what_a_failed_run_left_behind_is_rebuilt_from_the_replay_in_batches() {
    let db = Database::new(0, (1..=6).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([1])),
            fail_once: Mutex::new(HashSet::from([1])),
            ..Behaviour::default()
        },
    );
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true))
            .ingest(|ingest| ingest.with_batch(batch(2)).with_builds(4));
    let releaser = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            while builder.grants().len() < 3 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            builder.release(1);
        })
    };
    assert_eq!(deployment.run_to_end(1).await, 1, "one run failed");
    releaser.await.unwrap();
    let grants = builder.grants();
    assert!(
        grants.len() > 3 && grants.iter().all(|ids| ids.len() <= 2),
        "rebuilt as read, two at a time: {grants:?}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
    let confirmed = deployment.feed.confirmed.lock().unwrap().clone();
    assert!(
        confirmed.windows(2).all(|pair| pair[0].seq <= pair[1].seq),
        "confirmation never moves back: {confirmed:?}"
    );
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
    deployment.assert_released().await;
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
        deployment.assert_released().await;
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
    deployment.drained().await;
    deployment.assert_released().await;
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
/// as the engine produces them one build at a time. Live changes alone (the
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

const KIB: usize = 1024;

/// A budget of a few batches, a slow sink, live changes and a backfill over
/// the same rows, failures and restarts: the budget never holds more than
/// its limit plus one batch per build, nothing deadlocks, and every byte
/// comes back.
#[test]
fn random_schedules_under_a_tiny_budget_stay_within_it_and_finish() {
    for seed in seeds(200) {
        for builds in [1, 4] {
            crate::tests::paused_runtime().block_on(tiny_budget_schedule(seed, builds));
        }
    }
}

async fn tiny_budget_schedule(seed: u64, builds: usize) {
    let mut random = Rng::new(seed);
    let rows = 1 + random.below(40) as i64;
    let log: Vec<Change> = (0..1 + random.below(80))
        .map(|_| Change {
            id: random.below(rows as u64) as i64,
            delete: random.below(6) == 0,
        })
        .collect();
    let max_changes = 1 + random.below(8) as usize;
    // Every third seed fans each change out to up to four documents, so the
    // document cap (O2) splits a change across builds, and a failure can
    // end a run between its parts.
    let fan = if seed % 3 == 0 {
        1 + (seed / 3 % 4) as i64
    } else {
        1
    };
    let db = Database::fanned(rows, log, fan);
    let builder = Builder::new(
        &db,
        Behaviour {
            random: Some(Mutex::new(Rng::new(seed.wrapping_mul(31)))),
            failures_left: AtomicUsize::new(random.below(3) as usize),
            ..Behaviour::default()
        },
    );
    let limit = 8 * KIB;
    // A reservation is its estimate: at most `max_changes` documents
    // at the first estimate's 16 KiB, before any build landed.
    let batch_bytes = max_changes * 16 * KIB;
    let mut deployment = Deployment::new(
        Feed::new(&db).with_gaps(seed.wrapping_mul(17)),
        builder,
        Recorder::seeded(false),
    )
    .ingest(|ingest| ingest.with_batch(batch(max_changes)).with_builds(builds))
    .with_budget(limit, limit + builds * batch_bytes);
    deployment.run_to_end(8).await;
    assert_eq!(
        deployment.recorder.state(),
        db.expected(),
        "seed {seed}, builds {builds}"
    );
}

/// A sink that stops acknowledging holds the budget's bytes, so builds stop
/// once it is full; when the sink resumes, every batch lands and every byte
/// comes back.
#[tokio::test(start_paused = true)]
async fn a_stalled_sink_paces_builds_and_they_resume_when_it_does() {
    let db = Database::new(200, Vec::new());
    let builder = instant(&db);
    let recorder = Recorder::seeded(false);
    recorder.stall();
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Arc::clone(&recorder))
            .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(4))
            .with_budget(16 * KIB, usize::MAX);
    let resumer = {
        let builder = Arc::clone(&builder);
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let stalled_at = builder.grants().len();
            tokio::time::sleep(Duration::from_millis(200)).await;
            let later = builder.grants().len();
            recorder.resume();
            (stalled_at, later)
        })
    };
    deployment.run_to_end(0).await;
    let (stalled_at, later) = resumer.await.unwrap();
    assert_eq!(
        stalled_at, later,
        "no build is granted while the stalled sink holds the budget"
    );
    assert!(
        stalled_at < 50,
        "builds stop well before the 50 batches of the backfill: {stalled_at}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// A backfill holds at most half the budget, so a live change is still built
/// while the backfill's batches wait on a stalled sink.
#[tokio::test(start_paused = true)]
async fn live_keeps_room_beside_a_backfill_holding_its_half() {
    let db = Database::new(200, vec![Change::upsert(1000)]);
    let builder = instant(&db);
    let recorder = Recorder::seeded(false);
    recorder.stall();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(100)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(4))
    .with_budget(16 * KIB, usize::MAX);
    let resumer = {
        let builder = Arc::clone(&builder);
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let live_built = builder.grants().iter().any(|ids| ids.contains(&1000));
            recorder.resume();
            live_built
        })
    };
    deployment.run_to_end(0).await;
    assert!(
        resumer.await.unwrap(),
        "the live change was built while the backfill waited on the sink"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// A batch larger than the whole budget is still built, alone.
#[tokio::test(start_paused = true)]
async fn a_batch_bigger_than_the_budget_still_builds() {
    let db = Database::new(64, (0..20).map(Change::upsert).collect());
    let mut deployment = Deployment::new(Feed::new(&db), instant(&db), Recorder::seeded(false))
        .ingest(|ingest| ingest.with_batch(batch(32)).with_builds(4))
        .with_budget(KIB, usize::MAX);
    deployment.run_to_end(0).await;
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// An engine whose run future is dropped (a supervisor aborting it, a
/// shutdown deadline) leaves no build running: its spawned builds are
/// aborted with it, so nothing keeps reading the source.
#[tokio::test(start_paused = true)]
async fn a_dropped_engine_aborts_its_running_builds() {
    let db = Database::new(0, (0..3).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([0, 1, 2])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(4));
    let sink = deployment.start_sink().await;
    let budget = deployment.budget.clone().unwrap();
    let ingest = deployment.ingest;
    let run = tokio::spawn(async move { ingest.run().await });
    while builder.grants().len() < 3 {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    run.abort();
    let _ = run.await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        builder.cancelled.load(Ordering::SeqCst),
        3,
        "every held build is dropped with the engine"
    );
    assert_eq!(
        budget.held(),
        crate::Held::default(),
        "the aborted builds gave their reservations back"
    );
    sink.abort();
}

/// The reader at its lead behind a held build stops the reading, and a stop
/// is still noticed then: what it read is built and published, in `main`'s
/// batches, once the build before it lands, and nothing more is read. Until
/// ADR 0012 stage 8a's slice 6a a closed batch stopped the reading one change
/// sooner (grants `[[0], [1]]`, two changes read); the lead at
/// `max_changes = 1` is two documents waiting.
#[tokio::test(start_paused = true)]
async fn a_stop_while_the_reader_waits_at_its_lead_ends_the_run() {
    let db = Database::new(0, (0..5).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([0])),
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
            .with_builds(1)
            .with_stop(stop.clone())
    });
    let sink = deployment.start_sink().await;
    let driver = {
        let builder = Arc::clone(&builder);
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            // Changes 1 and 2 wait behind the held build of 0: the reader's
            // lead at `max_changes = 1` (two documents waiting) is reached.
            while db.visible.load(Ordering::SeqCst) < 3 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            stop.request();
            tokio::time::sleep(Duration::from_millis(20)).await;
            builder.release(0);
        })
    };
    let result = deployment.ingest.run().await;
    driver.await.unwrap();
    deployment.drained().await;
    deployment.assert_released().await;
    sink.abort();

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(builder.grants(), vec![vec![0], vec![1], vec![2]]);
    assert_eq!(
        deployment
            .published
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>(),
        [vec![0, 1, 2]],
        "what the reader read is built and published, in `main`'s batches"
    );
    assert_eq!(
        db.visible.load(Ordering::SeqCst),
        3,
        "nothing is read after the stop, nor past the lead before it"
    );
}

/// A write that fails leaves its batch on the lane; the restarted sink
/// engine writes it again, and its bytes come back only then.
#[tokio::test(start_paused = true)]
async fn a_batch_redelivered_after_a_failed_write_gives_its_bytes_back_once_written() {
    let db = Database::new(40, (0..10).map(Change::upsert).collect());
    let recorder = Recorder::seeded(false);
    recorder.fail_next.store(true, Ordering::SeqCst);
    let mut deployment = Deployment::new(Feed::new(&db), instant(&db), Arc::clone(&recorder))
        .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(4))
        .with_budget(64 * KIB, usize::MAX);
    deployment.run_to_end(0).await;
    assert!(!recorder.fail_next.load(Ordering::SeqCst), "a write failed");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// Builds every change as one document in each of two indexes, so a batch
/// is split into a piece per `(index, sink)` lane.
#[derive(Debug)]
struct TwoIndexes;

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

#[async_trait]
impl DocumentBuilder for TwoIndexes {
    async fn resolve(
        &self,
        _table: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok([users(), orders()]
            .into_iter()
            .map(|index| DocumentId {
                index,
                key: key.clone(),
            })
            .collect())
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(BTreeMap::from([(
                "v".to_owned(),
                GenericValue::String("x".repeat(200)),
            )])),
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        Vec::new()
    }

    async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
        Ok(None)
    }

    async fn keys(
        &self,
        _: &IndexName,
        _: Option<&RowKey>,
        _: &RowKey,
        _: usize,
    ) -> source::Result<Vec<RowKey>> {
        Ok(Vec::new())
    }
}

/// One build's pieces travel on four lanes (two indexes into two sinks, the
/// sinks merging ready items into one write); its bytes are counted once
/// and come back exactly when the last lane acknowledged its piece: not
/// while the stalled sink still holds it.
#[tokio::test(start_paused = true)]
async fn a_batch_split_across_lanes_gives_its_bytes_back_once_the_last_lane_lands() {
    let audit = SinkName::try_new("audit").unwrap();
    let sinks = [sink_name(), audit.clone()];
    let stream = Arc::new(ChannelStream::over(64, [users(), orders()], sinks.clone()));
    let db = Database::new(0, (0..12).map(Change::upsert).collect());
    let feed = Arc::new(Feed::new(&db));
    let budget = Arc::new(Budget::new(usize::MAX / 4));
    let ingest = IngestEngine::new(
        Arc::clone(&feed) as Arc<dyn ChangeCapture>,
        Arc::new(TwoIndexes),
        Arc::clone(&stream) as Arc<dyn Stream>,
        sinks.to_vec(),
        Arc::new(crate::Progress::of([users(), orders()], sinks.clone())),
    )
    .with_batch(batch(4))
    .with_builds(4)
    .with_budget(Arc::clone(&budget));
    let primary = Recorder::seeded(true);
    let stalled = Recorder::seeded(true);
    stalled.stall();
    let mut tasks = Vec::new();
    for (name, recorder) in [(sink_name(), &primary), (audit, &stalled)] {
        let mappings = [users(), orders()]
            .into_iter()
            .map(|index| IndexMapping {
                index,
                hash: kernel::ContentHash::new(1),
                fields: Vec::new(),
            })
            .collect();
        let engine = SinkEngine::new(
            name,
            Arc::clone(recorder) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            mappings,
            ingest.progress(),
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        tasks.push(tokio::spawn(async move {
            let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
            engine.run(&mut rx).await
        }));
    }
    ingest.run().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        primary.written_ids().len() >= 2,
        "the healthy sink wrote both indexes' pieces"
    );
    assert!(
        budget.held().total > 0,
        "the stalled sink's pieces keep every batch counted"
    );
    stalled.resume();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !stream.is_idle() {
        assert!(tokio::time::Instant::now() < deadline, "the lanes drain");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    crate::tests::assert_released(&budget).await;
    for task in tasks {
        task.abort();
    }
}

/// A stop in the middle of a backfill abandons the snapshot: the batches it
/// had built are published and written, the rest never built, and every byte
/// the budget counted comes back.
#[tokio::test(start_paused = true)]
async fn a_snapshot_abandoned_at_a_stop_gives_every_byte_back() {
    let db = Database::new(200, Vec::new());
    let builder = Builder::new(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(4)
            .with_stop(stop.clone())
    })
    .with_budget(16 * KIB, usize::MAX);
    let sink = deployment.start_sink().await;
    let driver = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            while builder.grants().len() < 6 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            stop.request();
        })
    };
    let result = deployment.ingest.run().await;
    driver.await.unwrap();
    deployment.drained().await;
    deployment.assert_released().await;
    sink.abort();
    assert!(result.is_ok(), "{result:?}");
    assert!(
        builder.grants().len() < 50,
        "the backfill stopped part way: {} builds",
        builder.grants().len()
    );
}

/// The ingest engine fails while a stalled sink's lane still holds charged
/// batches; it is restarted, the sink resumes, and every byte comes back:
/// a charge outlives the run that built it, and goes when its batch lands.
#[tokio::test(start_paused = true)]
async fn an_ingest_restart_while_lanes_hold_charges_gives_every_byte_back() {
    let db = Database::new(0, (0..12).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([8])),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let mut deployment = Deployment::new(Feed::new(&db), builder, Arc::clone(&recorder))
        .ingest(|ingest| ingest.with_batch(batch(2)).with_builds(4))
        .with_budget(64 * KIB, usize::MAX);
    let resumer = {
        let recorder = Arc::clone(&recorder);
        let budget = deployment.budget.clone().unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let held = budget.held().total;
            recorder.resume();
            held
        })
    };
    let failed = deployment.run_to_end(2).await;
    assert!(
        resumer.await.unwrap() > 0,
        "the stalled lane held charged batches"
    );
    assert_eq!(failed, 1, "the build of 8 failed once");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// Every budget wait, as the engine reports it: what let it go and after how
/// long.
#[derive(Debug, Default)]
struct BudgetWaits(Mutex<Vec<(crate::Work, crate::Wake, Duration)>>);

impl crate::Observer for BudgetWaits {
    fn on_budget_wait(&self, work: crate::Work, wake: crate::Wake, waited: Duration) {
        self.0.lock().unwrap().push((work, wake, waited));
    }
}

impl BudgetWaits {
    fn by_wake(&self) -> BTreeMap<String, (usize, Duration)> {
        let mut counts: BTreeMap<String, (usize, Duration)> = BTreeMap::new();
        for (work, wake, waited) in self.0.lock().unwrap().iter() {
            let entry = counts
                .entry(format!("{}/{}", work.as_str(), wake.as_str()))
                .or_default();
            entry.0 += 1;
            entry.1 = entry.1.max(*waited);
        }
        counts
    }
}

/// Stage 5, the 64 MiB question: every budget wait in a steady pipeline (a
/// backfill and a trickle of live changes through a slow, steadily
/// acknowledging sink and a small budget) ends on the event that freed the
/// bytes, a release or a landing, never on the confirm tick's backstop, and
/// well within it.
#[tokio::test(start_paused = true)]
async fn every_budget_wait_ends_on_a_release_not_the_tick() {
    let db = Database::new(400, (0..300).map(|i| Change::upsert(1000 + i)).collect());
    let builder = instant(&db);
    let recorder = Recorder::seeded(false);
    *recorder.write_delay.lock().unwrap() = Some(Duration::from_millis(5));
    let waits = Arc::new(BudgetWaits::default());
    let observer = Arc::clone(&waits);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(2)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(4)
            .with_observer(observer)
    })
    .with_budget(16 * KIB, usize::MAX);
    deployment.run_to_end(0).await;
    assert_eq!(deployment.recorder.state(), db.expected());

    let by_wake = waits.by_wake();
    assert!(
        by_wake
            .get("backfill/released")
            .is_some_and(|(n, _)| *n > 10),
        "the budget bound, many times: {by_wake:?}"
    );
    for (kind, (count, longest)) in &by_wake {
        assert!(
            !kind.ends_with("/tick") && !kind.ends_with("/settle"),
            "{count} waits of {kind}: a release went unheard: {by_wake:?}"
        );
        assert!(
            *longest < Duration::from_millis(200),
            "{kind} waited {longest:?}, near the tick: {by_wake:?}"
        );
    }
}

/// With three indexes, one bursting holds at most its share of the budget
/// while the sink is stalled; the other two keep building beside it.
#[tokio::test(start_paused = true)]
async fn one_bursting_index_of_three_holds_its_share_and_the_others_build() {
    // `users` (ids divisible by 3) rewrites three documents again and again,
    // then `orders` (1, 4) and `items` (2, 5) change.
    let mut log: Vec<Change> = (0..30).map(|n| Change::upsert(3 * (n % 3))).collect();
    log.extend([1, 2, 4, 5].map(Change::upsert));
    let db = Database::new(0, log);
    // Documents of about 8 KiB: the 96 KiB budget is a dozen of them, so
    // `users` would fill it alone.
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            ..Behaviour::default()
        },
        3,
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Arc::clone(&recorder))
            .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(4))
            .with_budget(96 * KIB, usize::MAX);
    let budget = deployment.budget.clone().unwrap();
    assert_eq!(budget.index_share(), Some(48 * KIB));
    let resumer = {
        let builder = Arc::clone(&builder);
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let built: HashSet<i64> = builder.grants().into_iter().flatten().collect();
            let users_held = budget.held_by(&users());
            recorder.resume();
            (built, users_held)
        })
    };
    deployment.run_to_end(0).await;
    let (built, users_held) = resumer.await.unwrap();
    for id in [1, 2, 4, 5] {
        assert!(
            built.contains(&id),
            "{id} was built while users held the stalled sink: {built:?}"
        );
    }
    assert!(
        users_held <= 48 * KIB + 3 * 16 * KIB,
        "users holds its share and at most its one batch past it: {users_held}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// An index the budget leaves waiting gets nothing from the build that
/// takes the others, not even its step: a sink whose lanes all landed counts
/// none of its changes committed until its documents land.
#[tokio::test(start_paused = true)]
async fn a_deferred_index_is_never_counted_committed_before_its_documents_land() {
    let audit = SinkName::try_new("audit").unwrap();
    let sinks = [sink_name(), audit.clone()];
    let indexes = [users(), orders()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), sinks.clone()));
    // `users` 0, then `users` 2, then `orders` 1, each its own live run.
    let db = Database::new(0, [0, 2, 1].map(Change::upsert).to_vec());
    let feed = Arc::new(Feed::new(&db).with_delay(Duration::from_millis(100)));
    // `users` 0 alone (about 31 KiB) leaves `users` past its 32 KiB share for
    // its next document's estimate; `orders` fits within the 64 KiB limit.
    let budget = Arc::new(Budget::new(64 * KIB).with_index_share(2, 0.5));
    let progress = Arc::new(crate::Progress::of(indexes.clone(), sinks.clone()));
    let builder = Builder::two(
        &db,
        Behaviour {
            padding: 30 * KIB,
            ..Behaviour::default()
        },
    );
    let ingest = IngestEngine::new(
        Arc::clone(&feed) as Arc<dyn ChangeCapture>,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        sinks.to_vec(),
        Arc::clone(&progress),
    )
    .with_batch(batch(4))
    .with_builds(4)
    .with_budget(Arc::clone(&budget));
    let healthy = Recorder::seeded(true);
    let stalled = Recorder::seeded(true);
    stalled.stall();
    let mut tasks = Vec::new();
    for (name, recorder) in [(sink_name(), &healthy), (audit, &stalled)] {
        let mappings = indexes
            .iter()
            .map(|index| IndexMapping {
                index: index.clone(),
                ..mapping()
            })
            .collect();
        let engine = SinkEngine::new(
            name,
            Arc::clone(recorder) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            mappings,
            Arc::clone(&progress),
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        tasks.push(tokio::spawn(async move {
            let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
            engine.run(&mut rx).await
        }));
    }
    let run = tokio::spawn(async move { ingest.run().await });
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(progress.changes_read(), 3);
    assert_eq!(
        progress.waiting_by_index(),
        vec![(users(), 1)],
        "users 2 waits: the stalled sink holds users' share"
    );
    let written: HashSet<i64> = healthy.written_ids().into_iter().flatten().collect();
    assert_eq!(
        written,
        HashSet::from([0, 1]),
        "orders 1 was built beside it"
    );
    assert_eq!(
        progress.committed(&sink_name()),
        1,
        "the healthy sink landed every item routed to it, and still counts only users 0: \
         users 2 is not built"
    );

    stalled.resume();
    run.await.unwrap().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !stream.is_idle() {
        assert!(tokio::time::Instant::now() < deadline, "the lanes drain");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(progress.committed(&sink_name()), 3);
    assert_eq!(progress.committed(&SinkName::try_new("audit").unwrap()), 3);
    crate::tests::assert_released(&budget).await;
    for task in tasks {
        task.abort();
    }
}

/// An index held by the budget past its waiting bound (two documents of
/// `users` behind a stalled sink) is exempt (6b-2): the reading goes on to
/// `orders` 1, which is built, while `users`' documents keep waiting; once
/// the sink resumes everything lands.
#[tokio::test(start_paused = true)]
async fn an_index_held_past_its_waiting_bound_is_exempt_and_the_reading_goes_on() {
    // `users` 0, 2, 4, 6 (distinct documents), then `orders` 1.
    let db = Database::new(0, [0, 2, 4, 6, 1].map(Change::upsert).to_vec());
    let builder = Builder::two(&db, Behaviour::default());
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(20)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(4))
    .with_budget(16 * KIB, usize::MAX);
    let progress = deployment.ingest.progress();
    let resumer = {
        let builder = Arc::clone(&builder);
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let built: HashSet<i64> = builder.grants().into_iter().flatten().collect();
            let read = progress.changes_read();
            recorder.resume();
            (built, read)
        })
    };
    deployment.run_to_end(0).await;
    let (built, read) = resumer.await.unwrap();
    assert!(
        built.contains(&1),
        "users is held with two documents waiting: exempt, so orders 1 was read and built: \
         {built:?}"
    );
    assert_eq!(read, 5, "every change was read");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// A schedule over three indexes, four changes in five on `users`: a
/// backfill of all three, live changes, failures and restarts, the sink
/// stalled for the first second so the budget fills. Documents of about
/// 8 KiB and batches of one to three keep the slack small next to the share:
/// each index's live documents hold at most its share plus one batch at every
/// write (rule 3; the batch is one larger than the share, or a settle past
/// its estimate), which fails with the share off; the whole at most the
/// limit plus a batch per build and per kind. Nothing deadlocks, every
/// change lands, and every byte comes back.
#[test]
fn random_schedules_over_three_indexes_with_one_bursting_stay_within_their_shares() {
    for seed in seeds(60) {
        for builds in [1, 4] {
            crate::tests::paused_runtime().block_on(bursting_schedule(seed, builds));
        }
    }
}

async fn bursting_schedule(seed: u64, builds: usize) {
    let mut random = Rng::new(seed ^ 0x00b0_0057);
    let rows = 3 + random.below(40) as i64;
    let log: Vec<Change> = (0..100 + random.below(100))
        .map(|_| {
            let id = random.below(rows as u64) as i64;
            let id = if random.below(5) == 0 {
                id
            } else {
                id - id.rem_euclid(3)
            };
            Change {
                id,
                delete: random.below(6) == 0,
            }
        })
        .collect();
    let max_changes = 1 + random.below(3) as usize;
    let db = Database::new(rows, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            random: Some(Mutex::new(Rng::new(seed.wrapping_mul(31)))),
            // A failed engine run waits for the lanes to drain before the
            // next, which holds the reading until the sink resumes: odd seeds
            // fail, even ones fill the budget.
            failures_left: AtomicUsize::new(if seed % 2 == 1 {
                random.below(3) as usize
            } else {
                0
            }),
            padding: 8 * KIB,
            ..Behaviour::default()
        },
        3,
    );
    let recorder = Recorder::seeded(false);
    recorder.stall();
    let limit = 512 * KIB;
    let batch_bytes = max_changes * 16 * KIB;
    let mut deployment = Deployment::new(
        Feed::new(&db).with_gaps(seed.wrapping_mul(17)),
        builder,
        Arc::clone(&recorder),
    )
    .ingest(|ingest| ingest.with_batch(batch(max_changes)).with_builds(builds))
    .with_budget(limit, limit + (builds + 3) * batch_bytes);
    *deployment.recorder.watch_index.lock().unwrap() = Some(limit / 2 + batch_bytes);
    let resumer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        recorder.resume();
    });
    deployment.run_to_end(8).await;
    resumer.await.unwrap();
    assert_eq!(
        deployment.recorder.state(),
        db.expected(),
        "seed {seed}, builds {builds}"
    );
}

/// One index's reindex behind a slow sink beside two steady live streams:
/// `orders` (odd ids) backfills 2000 rows of about 8 KiB through a sink
/// writing one item per 200 ms, while `users` and `orders` change every
/// 2 ms, 200 distinct `orders` documents among them, on the shared budget.
/// The requirement: an index being reindexed keeps building its live
/// documents beside its backfill. Checked: `orders`' last live document is
/// built before its backfill completes.
///
/// Until ADR 0012 stage 8a's slice 6a this also compared the run with one on
/// the budget with the share off, as a stand-in for `main`'s rules. With the
/// reader reading ahead, the test sink's fixed cost per item makes that
/// order a matter of how items are cut: both arms land `orders`' live
/// documents about three times sooner than before (21.2 s shared and 17.6 s
/// share off, against 59.2 s and 66.8 s), and the bug the comparison was
/// written for (the share counting the index's backfill bytes) passes it
/// (17.2 s). The budget's own guards catch that bug
/// (`an_index_backfilling_keeps_the_live_room_main_gave_it` among five).
#[tokio::test(start_paused = true)]
async fn an_index_backfilling_behind_a_slow_sink_keeps_both_live_streams_building() {
    let (orders_live, seeded) = reindex_beside_live(Some(2), 8 * KIB).await;
    assert!(
        orders_live < seeded,
        "orders' live documents kept building beside its backfill: built by {orders_live:?}, \
         backfill done at {seeded:?}"
    );
}

/// When the last of `orders`' live documents was built, and when `orders`'
/// backfill completed, from the start.
async fn reindex_beside_live(indexes: Option<usize>, padding: usize) -> (Duration, Duration) {
    let log: Vec<Change> = (0..1500)
        .map(|n| {
            if n % 2 == 1 && n / 2 < 200 {
                Change::upsert(4001 + 2 * (n / 2))
            } else {
                Change::upsert(2 * (n % 20))
            }
        })
        .collect();
    let db = Database::new(4000, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding,
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(false);
    recorder
        .seeded_indexes
        .lock()
        .unwrap()
        .insert("users".to_owned());
    *recorder.write_delay.lock().unwrap() = Some(Duration::from_millis(200));
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(2)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| ingest.with_batch(batch(16)).with_builds(4))
    .with_shared_budget(1024 * KIB, usize::MAX, indexes);
    let started = tokio::time::Instant::now();
    deployment.run_to_end(0).await;
    let built = builder.built_at.lock().unwrap().clone();
    let seeded = recorder.marked_at.lock().unwrap()["orders"] - started;
    let orders_live = (0..200)
        .map(|k| built[&(4001 + 2 * k)] - started)
        .max()
        .unwrap();
    assert_eq!(recorder.state().get(&4001), Some(&2));
    (orders_live, seeded)
}

/// One index held at its share while another builds in several runs is one
/// wait for the budget, measured from the first refusal to the grant that
/// takes the held index: the partial grants beside it neither end that wait
/// nor start another, so `flusso.memory.refused` and `flusso.memory.wait`
/// keep their meaning (a batch of live documents the budget held back).
#[tokio::test(start_paused = true)]
async fn an_index_held_at_its_share_is_one_wait_however_many_builds_pass_it() {
    // `users` (even ids) builds ten documents of about 8 KiB, one per run,
    // past its 64 KiB share while the sink is stalled; then `users` 0 again
    // waits, and `orders` 1..9 build beside it, one per run.
    let mut log: Vec<Change> = (0..10).map(|n| Change::upsert(2 * n)).collect();
    log.push(Change::upsert(0));
    log.extend([1, 3, 5, 7, 9].map(Change::upsert));
    let db = Database::new(0, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let waits = Arc::new(BudgetWaits::default());
    let observer = Arc::clone(&waits);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(100)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(16))
            .with_builds(4)
            .with_observer(observer)
    })
    .with_budget(128 * KIB, usize::MAX);
    let budget = deployment.budget.clone().unwrap();
    let resumer = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3_000)).await;
            recorder.resume();
        })
    };
    deployment.run_to_end(0).await;
    resumer.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());

    assert_eq!(budget.refused(crate::Work::Live), 1, "{:?}", waits.0);
    let live: Vec<Duration> = waits
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(work, _, _)| *work == crate::Work::Live)
        .map(|(_, _, waited)| *waited)
        .collect();
    assert_eq!(live.len(), 1, "one wait: {live:?}");
    assert!(
        live[0] > Duration::from_millis(1_500),
        "from the first refusal to the release: {live:?}"
    );
}

/// The same hold across an engine restart: the build of `orders` 9 fails,
/// the engine run ends while `users` is still held, and the next run's
/// replay is granted at once. One hold is one refusal and one wait: the wait
/// is reported when the run ends, up to then (not lost with the run), and
/// the next run neither counts the hold again nor reports a second wait.
#[tokio::test(start_paused = true)]
async fn a_hold_across_an_engine_restart_is_one_wait() {
    // `users` (even ids) builds ten documents of about 8 KiB, one per run,
    // past its 64 KiB share while the sink is stalled; then `users` 0 again
    // waits, and `orders` 1..9 build beside it, one per run.
    let mut log: Vec<Change> = (0..10).map(|n| Change::upsert(2 * n)).collect();
    log.push(Change::upsert(0));
    log.extend([1, 3, 5, 7, 9].map(Change::upsert));
    let db = Database::new(0, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            resolve_fails_once: Mutex::new(HashSet::from([9])),
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let waits = Arc::new(BudgetWaits::default());
    let observer = Arc::clone(&waits);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(100)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(16))
            .with_builds(4)
            .with_observer(observer)
    })
    .with_budget(128 * KIB, usize::MAX);
    let budget = deployment.budget.clone().unwrap();
    let resumer = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3_000)).await;
            recorder.resume();
        })
    };
    assert_eq!(
        deployment.run_to_end(1).await,
        1,
        "resolving orders 9 failed once: the run restarted"
    );
    resumer.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());

    assert_eq!(budget.refused(crate::Work::Live), 1, "{:?}", waits.0);
    let live: Vec<Duration> = waits
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(work, _, _)| *work == crate::Work::Live)
        .map(|(_, _, waited)| *waited)
        .collect();
    assert_eq!(live.len(), 1, "one wait: {live:?}");
    assert!(
        live[0] > Duration::from_millis(500) && live[0] < Duration::from_secs(3),
        "from the first refusal to the end of the failed run, before the release: {live:?}"
    );
}

/// The same hold, then an outage of an hour before the next engine run (a
/// source that won't prepare, say): the wait is reported up to the end of
/// the run it began in, not stretched over the outage, and the replay
/// granted at once counts no second refusal nor reports a second wait.
#[tokio::test(start_paused = true)]
async fn a_wait_open_when_an_engine_run_ends_is_reported_up_to_that_moment() {
    // `users` (even ids) builds ten documents of about 8 KiB, one per run,
    // past its 64 KiB share while the sink is stalled; then `users` 0 again
    // waits, and `orders` 1..9 build beside it, one per run.
    let mut log: Vec<Change> = (0..10).map(|n| Change::upsert(2 * n)).collect();
    log.push(Change::upsert(0));
    log.extend([1, 3, 5, 7, 9].map(Change::upsert));
    let db = Database::new(0, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            resolve_fails_once: Mutex::new(HashSet::from([9])),
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let waits = Arc::new(BudgetWaits::default());
    let observer = Arc::clone(&waits);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(100)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(16))
            .with_builds(4)
            .with_observer(observer)
    })
    .with_budget(128 * KIB, usize::MAX);
    deployment.restart_delay = Duration::from_secs(3600);
    let budget = deployment.budget.clone().unwrap();
    let resumer = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3_000)).await;
            recorder.resume();
        })
    };
    assert_eq!(
        deployment.run_to_end(1).await,
        1,
        "resolving orders 9 failed once: the run restarted"
    );
    resumer.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());

    assert_eq!(budget.refused(crate::Work::Live), 1, "{:?}", waits.0);
    let live: Vec<Duration> = waits
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(work, _, _)| *work == crate::Work::Live)
        .map(|(_, _, waited)| *waited)
        .collect();
    assert_eq!(live.len(), 1, "one wait: {live:?}");
    assert!(
        live[0] < Duration::from_secs(60),
        "up to the end of the run, not over the outage: {live:?}"
    );
    assert!(
        live[0] > Duration::from_millis(500),
        "from the first refusal to the release: {live:?}"
    );
}

/// The twin across a restart of `orders` alone (review of #456, item 3):
/// `users`' wait is not cut by it (one wait, counted once), and lasts until
/// the release; `orders` is down an hour meanwhile.
#[tokio::test(start_paused = true)]
async fn a_wait_open_when_another_index_restarts_is_not_cut() {
    // `users` (even ids) builds ten documents of about 8 KiB, one per run,
    // past its 64 KiB share while the sink is stalled; then `users` 0 again
    // waits, and `orders` 1..9 build beside it, one per run.
    let mut log: Vec<Change> = (0..10).map(|n| Change::upsert(2 * n)).collect();
    log.push(Change::upsert(0));
    log.extend([1, 3, 5, 7, 9].map(Change::upsert));
    let db = Database::new(0, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            fail_once: Mutex::new(HashSet::from([9])),
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let waits = Arc::new(BudgetWaits::default());
    let observer = Arc::clone(&waits);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(100)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(16))
            .with_builds(4)
            .with_observer(observer)
            .with_backoff(
                crate::Backoff::new(Duration::from_secs(3600), Duration::from_secs(3600)).unwrap(),
            )
    })
    .with_budget(128 * KIB, usize::MAX);
    let budget = deployment.budget.clone().unwrap();
    let resumer = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3_000)).await;
            recorder.resume();
        })
    };
    assert_eq!(
        deployment.run_to_end(0).await,
        0,
        "orders 9's build failed once: orders restarted alone, after an hour"
    );
    resumer.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());

    assert_eq!(budget.refused(crate::Work::Live), 1, "{:?}", waits.0);
    let live: Vec<Duration> = waits
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(work, _, _)| *work == crate::Work::Live)
        .map(|(_, _, waited)| *waited)
        .collect();
    assert_eq!(live.len(), 1, "one wait: {live:?}");
    assert!(
        live[0] < Duration::from_secs(60),
        "up to the release, not cut by orders' restart: {live:?}"
    );
    assert!(
        live[0] > Duration::from_millis(500),
        "from the first refusal to the release: {live:?}"
    );
}

/// Review round 3, case A in the engine: `orders` (odd ids) is reindexed and
/// its lane parks (every document refused under `on_error = "stop"`) for ten
/// simulated minutes, so its backfill climbs to half the budget and its live
/// documents to its share, and neither comes back. Healthy `users` (even
/// ids) changes in bursts of sixteen a second the whole time: each change is
/// read and lands,
/// and the whole never passes the stated bound (the limit, plus one live
/// batch per index, plus one backfill batch per index walking: `orders`).
/// Then `orders` is let through
/// and everything converges.
#[tokio::test(start_paused = true)]
async fn a_parked_index_at_its_share_leaves_a_healthy_index_writing_for_ten_minutes() {
    // A burst of 16 changes a second for ten minutes: in the first minute
    // one `orders` change (three documents rewritten) and 15 `users` ones,
    // then 16 `users` ones, sixteen distinct documents a burst, so a `users`
    // batch is larger than what `orders` leaves below the limit.
    let log: Vec<Change> = (0..9600)
        .map(|n| {
            if n < 960 && n % 16 == 0 {
                Change::upsert(1001 + 2 * ((n / 16) % 3))
            } else {
                Change::upsert(2 * (n % 20))
            }
        })
        .collect();
    let users_changes = log.iter().filter(|change| change.id < 1000).count() as u64;
    let db = Database::new(400, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(false);
    recorder
        .seeded_indexes
        .lock()
        .unwrap()
        .insert("users".to_owned());
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let limit = 512 * KIB;
    // A batch is at most sixteen documents at the first estimate (16 KiB
    // each): one live batch per index and one backfill batch past the limit.
    let batch_bytes = 16 * 16 * KIB;
    // Lanes as large as a deployment's: the budget binds, not a lane.
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_bursts(16, Duration::from_secs(1)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| ingest.with_batch(batch(16)).with_builds(4))
    .with_budget(limit, limit + 2 * batch_bytes + batch_bytes);
    let progress = deployment.ingest.progress();
    let budget = deployment.budget.clone().unwrap();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(605)).await;
            let read = progress.changes_read();
            let orders_held = budget.held_by(&orders());
            let users_landed: HashSet<i64> = recorder
                .written_at
                .lock()
                .unwrap()
                .keys()
                .copied()
                .filter(|id| *id < 40 && id % 2 == 0)
                .collect();
            let users_state: BTreeMap<i64, u64> = recorder
                .state()
                .into_iter()
                .filter(|(id, _)| *id < 40 && id % 2 == 0)
                .collect();
            recorder.refused_indexes.lock().unwrap().clear();
            (read, orders_held, users_landed.len(), users_state)
        })
    };
    deployment.run_to_end(0).await;
    let (read, orders_held, users_landed, users_state) = watcher.await.unwrap();
    assert!(
        orders_held >= limit / 2,
        "orders was held at its share and backfill: {orders_held}"
    );
    assert!(
        read >= users_changes,
        "the reading went on for users: {read} of {users_changes} users changes read"
    );
    assert_eq!(users_landed, 20, "every users document landed");
    let expected: BTreeMap<i64, u64> = db
        .expected()
        .into_iter()
        .filter(|(id, _)| *id < 40 && id % 2 == 0)
        .collect();
    assert_eq!(
        users_state, expected,
        "users as written, while orders was parked"
    );
    let state = deployment.recorder.state();
    for (id, version) in db.expected().iter().filter(|(id, _)| *id % 2 == 1) {
        assert_eq!(state.get(id), Some(version), "orders {id} once let through");
    }
}

/// ADR 0012 stage 8a's acceptance test for 6b-2, turned around from slice
/// 4's: `orders` parked at its share (every document refused under `stop`,
/// so its lane never gives its live bytes back) changes one of fifty
/// documents a second for ten minutes beside `users`' bursts. From the 61st
/// second it has `2 × max_changes` documents waiting (32), where slice 4
/// stopped the reading for every index. Held at its cap, it is exempt: the
/// reading goes on to the end, `orders`' documents keep waiting (fifty, one
/// per document), `users` keeps being written and its lane's fence keeps
/// passing, while confirmation stays pinned before `orders`' first unbuilt
/// change (the slot holds, WAL grows). The budget holds within its rules the
/// whole time. Then `orders` is let through and everything converges.
#[tokio::test(start_paused = true)]
async fn a_parked_index_changing_many_documents_does_not_stop_the_reading() {
    let log: Vec<Change> = (0..9600)
        .map(|n| {
            if n % 16 == 0 {
                Change::upsert(1001 + 2 * ((n / 16) % 50))
            } else {
                Change::upsert(2 * (n % 20))
            }
        })
        .collect();
    let total = log.len() as u64;
    let db = Database::new(400, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(false);
    recorder
        .seeded_indexes
        .lock()
        .unwrap()
        .insert("users".to_owned());
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    // The limit, plus a live batch of each index and a backfill batch, each
    // up to sixteen documents of about 9 KiB, and their settles.
    let most = 512 * KIB + 3 * 16 * 10 * KIB;
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_bursts(16, Duration::from_secs(1)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| ingest.with_batch(batch(16)).with_builds(4))
    .with_budget(512 * KIB, most);
    let progress = deployment.ingest.progress();
    let users_lane = stream::LaneKey::new(users(), sink_name());
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            let mut seen = Vec::new();
            // Half a second off the bursts: a sample on a burst's own
            // instant races the reader's task for it.
            tokio::time::sleep(Duration::from_millis(500)).await;
            for _ in 0..60 {
                tokio::time::sleep(Duration::from_secs(10)).await;
                seen.push((
                    progress.changes_read(),
                    progress.waiting_by_index(),
                    progress
                        .lane(&users_lane, crate::Level::Searchable)
                        .and_then(|landed| landed.covered_seq()),
                    progress.landed().covered_seq(),
                ));
            }
            recorder.refused_indexes.lock().unwrap().clear();
            seen
        })
    };
    deployment.run_to_end(0).await;
    let seen = watcher.await.unwrap();
    let orders_waiting = |waiting: &Vec<(IndexName, usize)>| {
        waiting
            .iter()
            .find(|(index, _)| *index == orders())
            .map_or(0, |(_, count)| *count)
    };
    for (at, (read, waiting, users_through, _)) in seen.iter().enumerate() {
        let second = 10 * (at as u64 + 1);
        assert_eq!(
            *read,
            (16 * second).min(total),
            "the reading goes on at {second} s ({} orders documents waiting)",
            orders_waiting(waiting)
        );
        assert!(
            users_through.is_some_and(|through| through + 32 >= *read),
            "users' fence passes what was read at {second} s: {users_through:?} of {read}"
        );
    }
    let (_, waiting, _, pinned) = seen.last().unwrap();
    assert_eq!(
        orders_waiting(waiting),
        50,
        "each of orders' fifty documents waits once"
    );
    let first_pinned = seen.get(6).unwrap().3;
    assert_eq!(
        *pinned, first_pinned,
        "confirmation stays before orders' first unbuilt change: the slot holds"
    );
    let state = deployment.recorder.state();
    for (id, version) in db
        .expected()
        .iter()
        .filter(|(id, _)| **id < 40 || *id % 2 == 1)
    {
        assert_eq!(
            state.get(id),
            Some(version),
            "{id} once orders is let through"
        );
    }
}
