//! A builder per index (ADR 0012 stage 8a, slice 6b-1c), run as the daemon
//! runs it: the document cap, a change split by it, a change touching two
//! indexes, and a stop with one index stalled. Every wait is bounded, so a
//! broken rule fails instead of hanging.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ColumnName, Envelope, GenericValue, IndexMapping, IndexName, Position, TableName};
use sink::{Sink, WriteReport};
use source::RowKey;
use source::cdc::{ChangeCapture, ChangeEvent, Landed};
use source::document::{Document, DocumentBuilder, DocumentId};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::sync::{Notify, mpsc};

use super::{MockSource, sink_name, users, wait_for};
use crate::{BatchPolicy, IngestEngine, SinkControl, SinkEngine, Stop};

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

fn row(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

/// A change to `table` (`users`, `orders`, or `both`) of row `id`.
fn change(table: &str, id: i64) -> ChangeEvent {
    ChangeEvent::Upsert {
        table: TableName::try_new(table).unwrap(),
        key: row(id),
        before: None,
        after: None,
    }
}

fn index_mapping(index: IndexName) -> IndexMapping {
    IndexMapping {
        index,
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// A change to `users` row `k` resolves to `fan` `users` documents
/// (`k × 100 + i`); one to `orders` to that `orders` document; one to `both`
/// to the `users` and the `orders` document `k`. Records each build's size.
#[derive(Debug)]
struct Fanning {
    fan: i64,
    builds: Mutex<Vec<usize>>,
    /// A build holding this document fails, once.
    fails_once: Mutex<Option<i64>>,
    /// How long each build takes, and how many ran at once at most.
    delay: Duration,
    now: std::sync::atomic::AtomicUsize,
    most: std::sync::atomic::AtomicUsize,
}

impl Fanning {
    fn new(fan: i64) -> Arc<Self> {
        Arc::new(Self {
            fan,
            builds: Mutex::default(),
            fails_once: Mutex::default(),
            delay: Duration::ZERO,
            now: Default::default(),
            most: Default::default(),
        })
    }

    /// Each build takes `delay`.
    fn slow(fan: i64, delay: Duration) -> Arc<Self> {
        let mut fanning = Arc::into_inner(Self::new(fan)).unwrap();
        fanning.delay = delay;
        Arc::new(fanning)
    }

    fn failing_once_on(fan: i64, document: i64) -> Arc<Self> {
        let fanning = Self::new(fan);
        *fanning.fails_once.lock().unwrap() = Some(document);
        fanning
    }
}

#[async_trait]
impl DocumentBuilder for Fanning {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        _: Option<&source::RowImage>,
        _: Option<&source::RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        let Some((_, GenericValue::BigInt(k))) = key.0.first() else {
            return Ok(Vec::new());
        };
        let id = |index: IndexName, id: i64| DocumentId {
            index,
            key: row(id),
        };
        Ok(match table.as_ref() {
            "users" => (0..self.fan).map(|i| id(users(), k * 100 + i)).collect(),
            "orders" => vec![id(orders(), *k)],
            _ => vec![id(users(), *k), id(orders(), *k)],
        })
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Null,
        })
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        self.builds.lock().unwrap().push(ids.len());
        {
            let mut fails = self.fails_once.lock().unwrap();
            if fails.is_some_and(|document| ids.iter().any(|id| id.key == row(document))) {
                *fails = None;
                return Err(source::SourceError::Connection(
                    "the build failed".to_owned(),
                ));
            }
        }
        if !self.delay.is_zero() {
            use std::sync::atomic::Ordering;
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            self.now.fetch_sub(1, Ordering::SeqCst);
        }
        let mut documents = Vec::with_capacity(ids.len());
        for id in ids {
            documents.push(self.build(id).await?);
        }
        Ok(documents)
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

/// A sink holding every write that carries a held index's document, or a
/// held document, until released; records what it wrote.
#[derive(Debug, Default)]
struct Gated {
    held_index: Mutex<Option<IndexName>>,
    held_id: Mutex<Option<String>>,
    released: Notify,
    written: Mutex<Vec<(String, String)>>,
}

impl Gated {
    fn holding_index(index: IndexName) -> Arc<Self> {
        let gated = Self::default();
        *gated.held_index.lock().unwrap() = Some(index);
        Arc::new(gated)
    }

    fn holding_id(id: &str) -> Arc<Self> {
        let gated = Self::default();
        *gated.held_id.lock().unwrap() = Some(id.to_owned());
        Arc::new(gated)
    }

    fn release(&self) {
        *self.held_index.lock().unwrap() = None;
        *self.held_id.lock().unwrap() = None;
        self.released.notify_waiters();
    }

    fn held(&self, envelopes: &[Envelope]) -> bool {
        let index = self.held_index.lock().unwrap();
        let id = self.held_id.lock().unwrap();
        envelopes.iter().any(|envelope| {
            index.as_ref() == Some(&envelope.index) || id.as_deref() == Some(envelope.id.as_str())
        })
    }

    fn written(&self, index: &str) -> Vec<String> {
        self.written
            .lock()
            .unwrap()
            .iter()
            .filter(|(of, _)| of == index)
            .map(|(_, id)| id.clone())
            .collect()
    }
}

#[async_trait]
impl Sink for Gated {
    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        loop {
            let released = self.released.notified();
            tokio::pin!(released);
            released.as_mut().enable();
            if !self.held(envelopes) {
                break;
            }
            released.await;
        }
        self.written.lock().unwrap().extend(
            envelopes
                .iter()
                .map(|envelope| (envelope.index.to_string(), envelope.id.clone())),
        );
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// The ingest engine over `indexes`, `documents` and one sink, with a stop.
struct Running {
    progress: Arc<crate::Progress>,
    /// The live stream stays open while it is held: the run ends at the stop.
    _feed: mpsc::Sender<ChangeEvent>,
    run: tokio::task::JoinHandle<crate::Result<()>>,
    sink: tokio::task::JoinHandle<crate::Result<()>>,
}

async fn start(
    indexes: Vec<IndexName>,
    changes: Vec<ChangeEvent>,
    max_changes: usize,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    stop: &Stop,
) -> Running {
    start_observed(
        indexes,
        changes,
        max_changes,
        documents,
        sink,
        stop,
        Arc::new(crate::NoopObserver),
        1,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn start_observed(
    indexes: Vec<IndexName>,
    changes: Vec<ChangeEvent>,
    max_changes: usize,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    stop: &Stop,
    observer: Arc<dyn crate::Observer>,
    builds: usize,
) -> Running {
    let primary = sink_name("primary");
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(crate::Progress::of(indexes.clone(), [primary.clone()]));
    let engine = SinkEngine::new(
        primary.clone(),
        sink,
        Arc::clone(&stream) as Arc<dyn Stream>,
        indexes.iter().cloned().map(index_mapping).collect(),
        Arc::clone(&progress),
    )
    // Lanes write at once: a held lane doesn't hold the other's writes.
    .with_writes(4);
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let sink = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    let (source, feed) = MockSource::fed();
    for change in changes {
        feed.send(change).await.unwrap();
    }
    let ingest = IngestEngine::new(
        Arc::new(source) as Arc<dyn ChangeCapture>,
        documents,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary],
        Arc::clone(&progress),
    )
    .with_budget(Arc::new(crate::Budget::new(usize::MAX / 4)))
    .with_batch(BatchPolicy {
        max_changes,
        max_delay: Duration::from_millis(10),
    })
    .with_stop(stop.clone())
    .with_builds(builds)
    .with_observer(observer);
    let run = tokio::spawn(async move { ingest.run().await });
    Running {
        progress,
        _feed: feed,
        run,
        sink,
    }
}

fn primary() -> kernel::SinkName {
    sink_name("primary")
}

/// O2: one index's build takes at most `max_changes` documents. A change
/// touching ten documents with `max_changes` 4 is built in three builds of
/// at most four, every document once.
#[tokio::test(start_paused = true)]
async fn a_build_never_takes_more_than_max_changes_documents() {
    let documents = Fanning::new(10);
    let sink = Gated::holding_index(orders());
    sink.release();
    let stop = Stop::new();
    let running = start(
        vec![users()],
        vec![change("users", 1)],
        4,
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::clone(&sink) as Arc<dyn Sink>,
        &stop,
    )
    .await;
    wait_for(|| sink.written("users").len() == 10).await;
    let builds = documents.builds.lock().unwrap().clone();
    assert!(builds.iter().all(|size| *size <= 4), "{builds:?}");
    assert_eq!(builds.iter().sum::<usize>(), 10, "{builds:?}");
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), running.run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    running.sink.abort();
}

/// A change split by the document cap (F2): confirmation, a per-sink fence
/// and the committed count wait for its last part, landed by its own build.
#[tokio::test(start_paused = true)]
async fn a_change_split_by_the_document_cap_counts_only_once_its_last_part_lands() {
    let documents = Fanning::new(10);
    // The last part of `users` 1 holds documents 108 and 109.
    let sink = Gated::holding_id("109");
    let stop = Stop::new();
    let running = start(
        vec![users()],
        vec![change("users", 1)],
        4,
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::clone(&sink) as Arc<dyn Sink>,
        &stop,
    )
    .await;
    wait_for(|| sink.written("users").len() == 8).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        running.progress.committed(&primary()),
        0,
        "two parts of three landed: the change isn't committed"
    );
    assert_eq!(
        running.progress.landed(),
        Landed::Before(Position::new(0)),
        "confirmation and fences wait for its last part"
    );
    sink.release();
    wait_for(|| running.progress.committed(&primary()) == 1).await;
    assert_eq!(running.progress.landed(), Landed::Through(Position::new(0)));
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), running.run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    running.sink.abort();
}

/// A change touching two indexes is built and published by two builders:
/// it counts, and a per-sink fence passes it, only once both landed it.
#[tokio::test(start_paused = true)]
async fn a_change_touching_two_indexes_counts_only_once_both_builders_landed_it() {
    let sink = Gated::holding_index(orders());
    let stop = Stop::new();
    let running = start(
        vec![orders(), users()],
        vec![change("both", 1)],
        4,
        Fanning::new(1) as Arc<dyn DocumentBuilder>,
        Arc::clone(&sink) as Arc<dyn Sink>,
        &stop,
    )
    .await;
    wait_for(|| sink.written("users").len() == 1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        running.progress.committed(&primary()),
        0,
        "users landed it, orders not yet"
    );
    assert_eq!(running.progress.landed(), Landed::Before(Position::new(0)));
    sink.release();
    wait_for(|| running.progress.committed(&primary()) == 1).await;
    assert_eq!(running.progress.landed(), Landed::Through(Position::new(0)));
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), running.run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    running.sink.abort();
}

/// A stop with one index stalled at its sink: the other index's builder
/// builds and lands what was read; the stop's drain waits only on the
/// stalled lane, and ends once it lands.
#[tokio::test(start_paused = true)]
async fn a_stop_with_one_index_stalled_lands_the_others() {
    let sink = Gated::holding_index(orders());
    let stop = Stop::new();
    let mut running = start(
        vec![orders(), users()],
        vec![change("users", 1), change("orders", 2), change("users", 3)],
        4,
        Fanning::new(1) as Arc<dyn DocumentBuilder>,
        Arc::clone(&sink) as Arc<dyn Sink>,
        &stop,
    )
    .await;
    wait_for(|| running.progress.changes_read() == 3).await;
    stop.request();
    wait_for(|| sink.written("users").len() == 2).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(2), &mut running.run)
            .await
            .is_err(),
        "the drain waits on the stalled orders lane"
    );
    sink.release();
    tokio::time::timeout(Duration::from_secs(5), running.run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(sink.written("orders"), ["2"]);
    running.sink.abort();
}

/// Walks no key page comes back for, of every index: how many are in
/// flight at once, and the most ever.
#[derive(Debug, Default)]
struct HangingWalks {
    in_flight: std::sync::atomic::AtomicUsize,
    most: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl DocumentBuilder for HangingWalks {
    async fn resolve(
        &self,
        _: &TableName,
        _: &RowKey,
        _: Option<&source::RowImage>,
        _: Option<&source::RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(Vec::new())
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Null,
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        vec![orders(), users()]
    }

    async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
        use std::sync::atomic::Ordering;
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        std::future::pending().await
    }

    async fn keys(
        &self,
        _: &IndexName,
        _: Option<&RowKey>,
        _: &RowKey,
        _: usize,
    ) -> source::Result<Vec<RowKey>> {
        std::future::pending().await
    }
}

/// An unseeded sink of both indexes.
#[derive(Debug)]
struct Unseeded;

#[async_trait]
impl Sink for Unseeded {
    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(false)
    }
}

/// Counts the snapshots started.
#[derive(Debug, Default)]
struct Snapshots(std::sync::atomic::AtomicUsize);

impl crate::Observer for Snapshots {
    fn on_snapshot_started(&self, _: &[IndexName]) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// The walk token (§3.9 I11, 6b-1c): both indexes ask for a backfill and
/// neither walk ever gets a page; walks are one at a time, as `main`'s one
/// snapshot walked its indexes, so only one ever starts (the key-read
/// permit alone would let the second start and wait at its first read).
/// The stop ends the run, the walk abandoned.
#[tokio::test(start_paused = true)]
async fn indexes_walk_one_at_a_time() {
    let walks = Arc::new(HangingWalks::default());
    let started = Arc::new(Snapshots::default());
    let stop = Stop::new();
    let running = start_observed(
        vec![orders(), users()],
        Vec::new(),
        4,
        Arc::clone(&walks) as Arc<dyn DocumentBuilder>,
        Arc::new(Unseeded) as Arc<dyn Sink>,
        &stop,
        Arc::clone(&started) as Arc<dyn crate::Observer>,
        1,
    )
    .await;
    wait_for(|| walks.in_flight.load(std::sync::atomic::Ordering::SeqCst) == 1).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        started.0.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a second walk started while the first ran"
    );
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), running.run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    running.sink.abort();
}

/// A walk of `orders` takes two seconds to find its bound; `users` has
/// none. Records the indexes marked seeded.
#[derive(Debug, Default)]
struct SlowOrdersWalk;

#[async_trait]
impl DocumentBuilder for SlowOrdersWalk {
    async fn resolve(
        &self,
        _: &TableName,
        _: &RowKey,
        _: Option<&source::RowImage>,
        _: Option<&source::RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(Vec::new())
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Null,
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        vec![orders(), users()]
    }

    async fn last_key(&self, index: &IndexName) -> source::Result<Option<RowKey>> {
        if *index == orders() {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
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

/// `orders` unseeded, `users` seeded; records what is marked seeded.
#[derive(Debug, Default)]
struct OrdersUnseeded(Mutex<Vec<String>>);

#[async_trait]
impl Sink for OrdersUnseeded {
    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(*index != orders())
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.0.lock().unwrap().push(index.to_string());
        Ok(())
    }
}

/// The end waits for every builder (§3.9 §8): the live stream ended at
/// once, `users`' builder is idle while `orders` still walks, and a request
/// for `users` reaches the lane meanwhile. The dispatcher still forwards it
/// and `users` is served before the run ends ("a live stream that ends still
/// drains queued requests"), not left for the next start.
#[tokio::test(start_paused = true)]
async fn a_request_queued_for_one_index_after_live_ends_is_served() {
    let primary = primary();
    let indexes = vec![orders(), users()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(crate::Progress::of(indexes.clone(), [primary.clone()]));
    let sink = Arc::new(OrdersUnseeded::default());
    let engine = SinkEngine::new(
        primary.clone(),
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        indexes.iter().cloned().map(index_mapping).collect(),
        Arc::clone(&progress),
    )
    .with_writes(4);
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let sink_task = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    let ingest = IngestEngine::new(
        Arc::new(MockSource::new(Vec::new())) as Arc<dyn ChangeCapture>,
        Arc::new(SlowOrdersWalk) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary.clone()],
        Arc::clone(&progress),
    );
    let run = tokio::spawn(async move { ingest.run().await });
    tokio::time::sleep(Duration::from_millis(500)).await;
    stream
        .requests()
        .unwrap()
        .producer
        .publish(stream::Request::Backfill {
            index: users(),
            sink: primary,
            after: None,
            ticket: stream::Ticket::default(),
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    wait_for(|| sink.0.lock().unwrap().len() == 2).await;
    let mut seeded = sink.0.lock().unwrap().clone();
    seeded.sort();
    assert_eq!(seeded, ["orders", "users"]);
    sink_task.abort();
}

/// Blocker 1 of the 6b-1c review: a change split by the document cap whose
/// last part failed. Run 1 lands documents 100..=107 of `users` 1 and fails
/// building 108 and 109; run 2, on the same progress, has read nothing yet
/// (its replay is still to come). Nothing may confirm change 0, nor a fence
/// pass it: its last two documents were never written, and a second failure
/// before the replay would lose them.
#[tokio::test(start_paused = true)]
async fn after_a_failed_part_no_run_confirms_the_change_before_its_replay() {
    let primary = primary();
    let stream = Arc::new(ChannelStream::over(64, vec![users()], [primary.clone()]));
    let progress = Arc::new(crate::Progress::of(vec![users()], [primary.clone()]));
    let sink = Gated::holding_id("none");
    let engine = SinkEngine::new(
        primary.clone(),
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![index_mapping(users())],
        Arc::clone(&progress),
    );
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let sink_task = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    let documents = Fanning::failing_once_on(10, 108);
    let engine_over = |source: MockSource, stop: &Stop| {
        IngestEngine::new(
            Arc::new(source) as Arc<dyn ChangeCapture>,
            Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![primary.clone()],
            Arc::clone(&progress),
        )
        .with_batch(BatchPolicy {
            max_changes: 4,
            max_delay: Duration::from_millis(10),
        })
        .with_stop(stop.clone())
    };

    let (first, feed) = MockSource::fed();
    feed.send(change("users", 1)).await.unwrap();
    let first = engine_over(first, &Stop::new());
    let failed = tokio::time::timeout(Duration::from_secs(5), first.run())
        .await
        .unwrap();
    assert!(failed.is_err(), "the part holding 108 failed");
    wait_for(|| sink.written("users").len() == 8).await;

    let (second, _replay) = MockSource::fed();
    let confirmed = Arc::clone(&second.confirmed);
    let stop = Stop::new();
    let second = engine_over(second, &stop);
    let run = tokio::spawn(async move { second.run().await });
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        *confirmed.lock().unwrap(),
        Vec::<Position>::new(),
        "change 0 was confirmed with two of its documents never written"
    );
    for level in [crate::Level::Applied, crate::Level::Searchable] {
        let lane = stream::LaneKey::new(users(), primary.clone());
        assert!(
            !matches!(progress.lane(&lane, level), Some(Landed::Through(_))),
            "a {level:?} fence passes change 0"
        );
    }
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    sink_task.abort();
}

/// `users` backfills its rows (walked from a hundred keys), `orders` builds
/// live; every build takes 10 ms.
#[derive(Debug)]
struct BackfillBesideLive;

#[async_trait]
impl DocumentBuilder for BackfillBesideLive {
    async fn resolve(
        &self,
        _: &TableName,
        key: &RowKey,
        _: Option<&source::RowImage>,
        _: Option<&source::RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: orders(),
            key: key.clone(),
        }])
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Null,
        })
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let mut documents = Vec::with_capacity(ids.len());
        for id in ids {
            documents.push(self.build(id).await?);
        }
        Ok(documents)
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        vec![users()]
    }

    async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
        Ok(Some(row(99)))
    }

    async fn keys(
        &self,
        _: &IndexName,
        after: Option<&RowKey>,
        _: &RowKey,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        let from = match after.and_then(|key| key.0.first()) {
            Some((_, GenericValue::BigInt(after))) => after + 1,
            _ => 0,
        };
        Ok((from..100).take(limit).map(row).collect())
    }
}

/// `users` unseeded, `orders` seeded.
#[derive(Debug)]
struct UsersUnseeded;

#[async_trait]
impl Sink for UsersUnseeded {
    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(*index != users())
    }
}

/// Blocker 2 of the 6b-1c review: one permit, `users`' backfill batch
/// waiting for it in the backfill line, `orders` building live changes one
/// at a time. A stop drops `users`' snapshot: its waiter must leave the line
/// as its builder ends, or G4's every fourth turn hands the permit to a
/// waiter nobody will ever ask again and `orders`' last batch waits for it
/// forever. At each of forty stop times the stop ends.
#[test]
fn a_stop_beside_a_backfill_waiting_for_a_permit_ends() {
    for at in 0..40u64 {
        crate::tests::paused_runtime().block_on(async move {
            let stop = Stop::new();
            let running = start(
                vec![orders(), users()],
                (0..15).map(|id| change("orders", id)).collect(),
                1,
                Arc::new(BackfillBesideLive) as Arc<dyn DocumentBuilder>,
                Arc::new(UsersUnseeded) as Arc<dyn Sink>,
                &stop,
            )
            .await;
            tokio::time::sleep(Duration::from_millis(5 * at)).await;
            stop.request();
            let ended = tokio::time::timeout(Duration::from_secs(30), running.run).await;
            assert!(ended.is_ok(), "the stop at {} ms never ended", 5 * at);
            ended.unwrap().unwrap().unwrap();
            running.sink.abort();
        });
    }
}

/// Finding 5 of the 6b-1c review (F2): what a capped take leaves is a full
/// candidate while `max_changes` or more of its documents are left, so its
/// parts build at once, up to `builds`. A change touching sixteen documents
/// with `max_changes` 4 and four permits builds as four builds running
/// together, not one after another.
#[tokio::test(start_paused = true)]
async fn a_capped_takes_leftovers_build_at_once_up_to_builds() {
    let documents = Fanning::slow(16, Duration::from_millis(50));
    let sink = Gated::holding_id("none");
    let stop = Stop::new();
    let running = start_observed(
        vec![users()],
        vec![change("users", 1)],
        4,
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::clone(&sink) as Arc<dyn Sink>,
        &stop,
        Arc::new(crate::NoopObserver),
        4,
    )
    .await;
    wait_for(|| sink.written("users").len() == 16).await;
    assert_eq!(
        documents.most.load(std::sync::atomic::Ordering::SeqCst),
        4,
        "the parts of one change built one at a time: {:?}",
        documents.builds.lock().unwrap()
    );
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), running.run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    running.sink.abort();
}

/// I13 (until slice 8): a build error in one builder ends the run for every
/// index at once, while another builder still has work outstanding (its
/// sink holds `orders`' write) and its stream is open; the run's future
/// drops that builder in place rather than waiting for it.
#[tokio::test(start_paused = true)]
async fn a_build_error_ends_the_run_and_drops_every_builder_in_place() {
    let sink = Gated::holding_index(orders());
    let stop = Stop::new();
    let running = start(
        vec![orders(), users()],
        vec![change("orders", 2), change("users", 1)],
        1,
        Fanning::failing_once_on(1, 100) as Arc<dyn DocumentBuilder>,
        Arc::clone(&sink) as Arc<dyn Sink>,
        &stop,
    )
    .await;
    let ended = tokio::time::timeout(Duration::from_secs(5), running.run)
        .await
        .expect("the error ended the run beside the orders builder")
        .unwrap();
    assert!(ended.is_err(), "users' build failed");
    sink.release();
    running.sink.abort();
}
