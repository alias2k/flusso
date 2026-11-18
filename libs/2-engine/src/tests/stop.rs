//! A requested [`Stop`]: the ingest engine reads nothing more, publishes what
//! it was building, waits until every sink flushed and acknowledged it, hands
//! how far every lane landed to the source, and only then closes the live
//! stream.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{IndexName, Position, SinkName};
use sink::{Sink, WriteReport};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::sync::{Notify, mpsc};

use super::{MockDocuments, RecordingSink, mapping, sink_name, upsert};
use crate::{IngestEngine, Progress, SinkControl, SinkEngine, Stop};

/// A live stream fed through a channel that counts every change it hands
/// out, and can be closed: `close_live` ends the stream the way the Postgres
/// capture does once the final position reached the server. Records confirms
/// and the close, in order, in `log`.
struct ClosableSource {
    feed: Mutex<Option<mpsc::Receiver<ChangeEvent>>>,
    pulled: Arc<AtomicUsize>,
    closing: Arc<Notify>,
    closed: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<String>>>,
}

impl std::fmt::Debug for ClosableSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClosableSource").finish_non_exhaustive()
    }
}

impl ClosableSource {
    fn new() -> (Self, mpsc::Sender<ChangeEvent>) {
        let (tx, rx) = mpsc::channel(64);
        (
            Self {
                feed: Mutex::new(Some(rx)),
                pulled: Arc::default(),
                closing: Arc::default(),
                closed: Arc::default(),
                log: Arc::default(),
            },
            tx,
        )
    }

    fn confirmed(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|line| line.starts_with("confirm"))
            .cloned()
            .collect()
    }
}

#[async_trait]
impl ChangeCapture for ClosableSource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let feed = self.feed.lock().unwrap().take().unwrap();
        let pulled = Arc::clone(&self.pulled);
        let closing = Arc::clone(&self.closing);
        let closed = Arc::clone(&self.closed);
        Ok(Box::pin(futures::stream::unfold(
            (feed, 0u64),
            move |(mut feed, next)| {
                let pulled = Arc::clone(&pulled);
                let closing = Arc::clone(&closing);
                let closed = Arc::clone(&closed);
                async move {
                    if closed.load(Ordering::SeqCst) {
                        return None;
                    }
                    tokio::select! {
                        event = feed.recv() => {
                            // A read in flight when the close came yields nothing,
                            // as the Postgres stream checks for a close before
                            // emitting.
                            let event = event?;
                            if closed.load(Ordering::SeqCst) {
                                return None;
                            }
                            pulled.fetch_add(1, Ordering::SeqCst);
                            Some((Ok((Position::new(next), event)), (feed, next + 1)))
                        }
                        () = closing.notified() => None,
                    }
                }
            },
        )))
    }

    fn confirm(&self, landed: source::cdc::Landed) {
        let Some(seq) = landed.covered_seq() else {
            return;
        };
        let line = format!("confirm {seq}");
        let mut log = self.log.lock().unwrap();
        let last = log.iter().rev().find(|line| line.starts_with("confirm "));
        if last != Some(&line) {
            log.push(line);
        }
    }

    fn close_live(&self) -> bool {
        self.log.lock().unwrap().push("close".to_owned());
        self.closed.store(true, Ordering::SeqCst);
        self.closing.notify_one();
        true
    }
}

/// [`MockDocuments`] whose `build_many` waits for `gate` once, so a test can
/// request the stop while a batch is being built.
#[derive(Debug, Default)]
struct GatedDocuments {
    inner: MockDocuments,
    entered: Notify,
    gate: Notify,
    gated: AtomicBool,
}

#[async_trait]
impl DocumentBuilder for GatedDocuments {
    async fn resolve(
        &self,
        table: &kernel::TableName,
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
        if self.gated.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.gate.notified().await;
        }
        self.inner.build_many(ids).await
    }

    fn backfill_indexes(&self) -> Vec<kernel::IndexName> {
        self.inner.backfill_indexes()
    }

    async fn keys(
        &self,
        index: &kernel::IndexName,
        after: Option<&RowKey>,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        self.inner.keys(index, after, limit).await
    }

    async fn index_mappings(&self) -> source::Result<Vec<kernel::IndexMapping>> {
        self.inner.index_mappings().await
    }
}

/// [`MockDocuments`] whose key pages never come back: a stop finds the
/// snapshot waiting on a page.
#[derive(Debug, Default)]
struct HangingKeys {
    inner: MockDocuments,
    asked: AtomicBool,
}

#[async_trait]
impl DocumentBuilder for HangingKeys {
    async fn resolve(
        &self,
        table: &kernel::TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        self.inner.resolve(table, key, before, after).await
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        self.inner.build(id).await
    }

    fn backfill_indexes(&self) -> Vec<kernel::IndexName> {
        self.inner.backfill_indexes()
    }

    async fn keys(
        &self,
        _index: &kernel::IndexName,
        _after: Option<&RowKey>,
        _limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        self.asked.store(true, Ordering::SeqCst);
        std::future::pending().await
    }

    async fn index_mappings(&self) -> source::Result<Vec<kernel::IndexMapping>> {
        self.inner.index_mappings().await
    }
}

/// [`MockDocuments`] paging `rows`' keys.
fn paging(rows: impl IntoIterator<Item = i64>) -> Arc<MockDocuments> {
    Arc::new(MockDocuments {
        rows: rows
            .into_iter()
            .map(|id| upsert(id).key().clone())
            .collect(),
        ..MockDocuments::default()
    })
}

/// A sink whose flush never returns: down, or stalled.
#[derive(Debug, Default)]
struct StalledSink;

#[async_trait]
impl Sink for StalledSink {
    async fn write(
        &self,
        _envelopes: &[kernel::Envelope],
        _caught_up: bool,
    ) -> sink::Result<WriteReport> {
        std::future::pending().await
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// The lanes of `stream`, for a `Progress` over them.
fn stream_sinks(stream: &ChannelStream) -> Vec<SinkName> {
    stream.sinks().cloned().collect()
}

/// Stage and spawn one sink engine per sink; returns their tasks.
async fn spawn_sinks(
    stream: &Arc<ChannelStream>,
    progress: &Arc<Progress>,
    sinks: Vec<(SinkName, Arc<dyn Sink>)>,
) -> Vec<tokio::task::JoinHandle<crate::Result<()>>> {
    let mut tasks = Vec::new();
    for (name, sink) in sinks {
        let engine = SinkEngine::new(
            name,
            sink,
            Arc::clone(stream) as Arc<dyn Stream>,
            vec![mapping()],
        )
        .with_progress(Arc::clone(progress));
        engine.stage(Continuity::Resumed).await.unwrap();
        tasks.push(tokio::spawn(async move {
            let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
            engine.run(&mut control).await
        }));
    }
    tasks
}

fn ingest(
    source: &Arc<ClosableSource>,
    documents: Arc<dyn DocumentBuilder>,
    stream: &Arc<ChannelStream>,
    progress: &Arc<Progress>,
    sinks: &[&str],
    stop: &Stop,
) -> IngestEngine {
    IngestEngine::new(
        Arc::clone(source) as Arc<dyn ChangeCapture>,
        documents,
        Arc::clone(stream) as Arc<dyn Stream>,
        sinks.iter().map(|sink| sink_name(sink)).collect(),
    )
    .with_progress(Arc::clone(progress))
    .with_stop(stop.clone())
}

#[tokio::test]
async fn a_stop_during_a_build_publishes_the_batch_and_every_sink_lands_it_before_the_run_ends() {
    let (source, feed) = ClosableSource::new();
    let source = Arc::new(source);
    let documents = Arc::new(GatedDocuments::default());
    documents.gated.store(true, Ordering::SeqCst);
    let stream = Arc::new(ChannelStream::new(64, [sink_name("a"), sink_name("b")]));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    let a = Arc::new(RecordingSink::seeded(true));
    let b = Arc::new(RecordingSink::seeded(true));
    let (a_ops, b_ops) = (Arc::clone(&a.ops), Arc::clone(&b.ops));
    let (a_flushes, b_flushes) = (Arc::clone(&a.flushes), Arc::clone(&b.flushes));
    let sinks = spawn_sinks(
        &stream,
        &progress,
        vec![(sink_name("a"), a as Arc<dyn Sink>), (sink_name("b"), b)],
    )
    .await;
    let stop = Stop::new();
    let engine = ingest(
        &source,
        Arc::clone(&documents) as _,
        &stream,
        &progress,
        &["a", "b"],
        &stop,
    );
    let run = tokio::spawn(async move { engine.run().await });

    feed.send(upsert(1)).await.unwrap();
    documents.entered.notified().await;
    stop.request();
    feed.send(upsert(2)).await.unwrap();
    documents.gate.notify_one();
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("the stop ends the run once the batch landed")
        .unwrap()
        .unwrap();

    for (ops, flushes) in [(&a_ops, &a_flushes), (&b_ops, &b_flushes)] {
        assert_eq!(*ops.lock().unwrap(), vec!["upsert users 1".to_owned()]);
        assert_eq!(*flushes.lock().unwrap(), vec![(1, true)]);
    }
    assert!(stream.is_idle(), "every lane acknowledged the batch");
    assert_eq!(
        source.pulled.load(Ordering::SeqCst),
        1,
        "the change sent after the stop was never read"
    );
    assert_eq!(
        *source.log.lock().unwrap(),
        vec!["confirm 0".to_owned(), "close".to_owned()],
        "the final confirmation reached the source before the live stream was closed"
    );
    for task in sinks {
        task.abort();
    }
}

#[tokio::test]
async fn nothing_is_read_from_the_live_stream_once_the_stop_is_requested() {
    let (source, feed) = ClosableSource::new();
    let source = Arc::new(source);
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    let sink = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&sink.ops);
    let sinks = spawn_sinks(
        &stream,
        &progress,
        vec![(sink_name("primary"), sink as Arc<dyn Sink>)],
    )
    .await;
    let stop = Stop::new();
    let engine = ingest(
        &source,
        Arc::new(MockDocuments::default()),
        &stream,
        &progress,
        &["primary"],
        &stop,
    );
    let run = tokio::spawn(async move { engine.run().await });

    feed.send(upsert(1)).await.unwrap();
    super::wait_for(|| ops.lock().unwrap().len() == 1).await;
    stop.request();
    for id in 2..10 {
        feed.send(upsert(id)).await.unwrap();
    }
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(source.pulled.load(Ordering::SeqCst), 1);
    assert_eq!(*ops.lock().unwrap(), vec!["upsert users 1".to_owned()]);
    assert_eq!(source.confirmed(), vec!["confirm 0".to_owned()]);
    for task in sinks {
        task.abort();
    }
}

#[tokio::test]
async fn a_snapshot_waiting_on_a_page_is_abandoned_and_its_request_served_again_on_the_next_run() {
    let (source, _feed) = ClosableSource::new();
    let source = Arc::new(source);
    let hanging = Arc::new(HangingKeys::default());
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    let sink = Arc::new(RecordingSink::seeded(false));
    let marked = Arc::clone(&sink.marked);
    let sinks = spawn_sinks(
        &stream,
        &progress,
        vec![(sink_name("primary"), sink as Arc<dyn Sink>)],
    )
    .await;
    let stop = Stop::new();
    let engine = ingest(
        &source,
        Arc::clone(&hanging) as Arc<dyn DocumentBuilder>,
        &stream,
        &progress,
        &["primary"],
        &stop,
    );
    let run = tokio::spawn(async move { engine.run().await });

    super::wait_for(|| hanging.asked.load(Ordering::SeqCst)).await;
    stop.request();
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("an unfinished snapshot does not hold the stop")
        .unwrap()
        .unwrap();
    assert!(
        marked.lock().unwrap().is_empty(),
        "an abandoned snapshot does not mark the index seeded"
    );

    let (next_source, _next_feed) = ClosableSource::new();
    let next_source = Arc::new(next_source);
    let next = ingest(
        &next_source,
        paging([10]),
        &stream,
        &progress,
        &["primary"],
        &Stop::new(),
    );
    let next_run = tokio::spawn(async move { next.run().await });
    super::wait_for(|| !marked.lock().unwrap().is_empty()).await;
    assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
    next_run.abort();
    for task in sinks {
        task.abort();
    }
}

#[tokio::test(start_paused = true)]
async fn a_stalled_sink_does_not_keep_the_other_from_landing_its_lane_and_holds_only_the_end() {
    let (source, feed) = ClosableSource::new();
    let source = Arc::new(source);
    let stream = Arc::new(ChannelStream::new(
        64,
        [sink_name("healthy"), sink_name("stalled")],
    ));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    let healthy = Arc::new(RecordingSink::seeded(true));
    let flushes = Arc::clone(&healthy.flushes);
    let sinks = spawn_sinks(
        &stream,
        &progress,
        vec![
            (sink_name("healthy"), healthy as Arc<dyn Sink>),
            (sink_name("stalled"), Arc::new(StalledSink)),
        ],
    )
    .await;
    let stop = Stop::new();
    let engine = ingest(
        &source,
        Arc::new(MockDocuments::default()),
        &stream,
        &progress,
        &["healthy", "stalled"],
        &stop,
    );
    let mut run = tokio::spawn(async move { engine.run().await });

    feed.send(upsert(1)).await.unwrap();
    super::wait_for(|| !flushes.lock().unwrap().is_empty()).await;
    stop.request();
    let deadline = tokio::time::timeout(Duration::from_secs(30), &mut run).await;

    assert!(
        deadline.is_err(),
        "the stalled sink holds the end of the stop until the caller's deadline"
    );
    assert_eq!(*flushes.lock().unwrap(), vec![(1, true)]);
    assert!(
        source.confirmed().is_empty(),
        "confirmation follows the slowest lane: nothing the stalled sink holds is confirmed"
    );
    assert!(!source.closed.load(Ordering::SeqCst));
    run.abort();
    for task in sinks {
        task.abort();
    }
}

#[tokio::test]
async fn a_stop_requested_before_the_run_reads_nothing() {
    let (source, feed) = ClosableSource::new();
    let source = Arc::new(source);
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    feed.send(upsert(1)).await.unwrap();
    let stop = Stop::new();
    stop.request();
    let engine = ingest(
        &source,
        Arc::new(MockDocuments::default()),
        &stream,
        &progress,
        &["primary"],
        &stop,
    );

    tokio::time::timeout(Duration::from_secs(5), engine.run())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(source.pulled.load(Ordering::SeqCst), 0);
    assert_eq!(*source.log.lock().unwrap(), vec!["close".to_owned()]);
}

#[tokio::test]
async fn a_stopping_sink_skips_queued_snapshot_batches_and_leaves_their_index_unseeded() {
    let (source, _feed) = ClosableSource::new();
    let source = Arc::new(source);
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    let sink = Arc::new(RecordingSink::seeded(false));
    let (ops, marked) = (Arc::clone(&sink.ops), Arc::clone(&sink.marked));
    let engine = SinkEngine::new(
        sink_name("primary"),
        sink,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
    )
    .with_progress(Arc::clone(&progress));
    engine.stage(Continuity::Resumed).await.unwrap();
    let stop = Stop::new();
    let ingest_engine = ingest(
        &source,
        paging(1..=600),
        &stream,
        &progress,
        &["primary"],
        &stop,
    );
    let ingest_run = tokio::spawn(async move { ingest_engine.run().await });
    super::wait_for(|| !stream.is_idle() && super::covered(&progress).is_none()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    stop.request();
    let engine = engine.with_stop(stop.clone());
    let sink_run = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    tokio::time::timeout(Duration::from_secs(5), ingest_run)
        .await
        .expect("the drain ends once the queued snapshot is acknowledged")
        .unwrap()
        .unwrap();

    assert!(
        ops.lock().unwrap().is_empty(),
        "a snapshot batch queued when the stop came is acknowledged, not applied"
    );
    assert!(
        marked.lock().unwrap().is_empty(),
        "an index whose snapshot was skipped is not marked seeded: the next start backfills it"
    );
    sink_run.abort();
}

/// [`ClosableSource`] whose stream, once closed, ends with an error: the
/// final position never reached the source.
#[derive(Debug)]
struct UnconfirmedClose(ClosableSource);

#[async_trait]
impl ChangeCapture for UnconfirmedClose {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let live = self.0.live().await?;
        Ok(Box::pin(futures::StreamExt::chain(
            live,
            futures::stream::iter([Err(source::SourceError::Connection(
                "the final position was not confirmed to the source".to_owned(),
            ))]),
        )))
    }

    fn confirm(&self, landed: source::cdc::Landed) {
        self.0.confirm(landed);
    }

    fn close_live(&self) -> bool {
        self.0.close_live()
    }
}

#[tokio::test]
async fn a_stop_whose_final_position_never_reaches_the_source_fails_the_run() {
    let (source, feed) = ClosableSource::new();
    let source = Arc::new(UnconfirmedClose(source));
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    let sink = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&sink.ops);
    let sinks = spawn_sinks(
        &stream,
        &progress,
        vec![(sink_name("primary"), sink as Arc<dyn Sink>)],
    )
    .await;
    let stop = Stop::new();
    let engine = IngestEngine::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::new(MockDocuments::default()),
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![sink_name("primary")],
    )
    .with_progress(Arc::clone(&progress))
    .with_stop(stop.clone());
    let run = tokio::spawn(async move { engine.run().await });

    feed.send(upsert(1)).await.unwrap();
    super::wait_for(|| ops.lock().unwrap().len() == 1).await;
    stop.request();
    let outcome = tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .unwrap()
        .unwrap();

    let Err(error) = outcome else {
        panic!("a stop that couldn't confirm its final position is not a clean stop");
    };
    assert!(
        error
            .to_string()
            .contains("the final position was not confirmed to the source"),
        "{error}"
    );
    for task in sinks {
        task.abort();
    }
}

/// A sink that refreshes only on a caught-up flush, as the OpenSearch sink
/// does under `refresh_interval = "-1"`: `Deferred` otherwise.
#[derive(Debug, Default)]
struct RefreshesWhenCaughtUp {
    flushes: Mutex<Vec<bool>>,
}

#[async_trait]
impl Sink for RefreshesWhenCaughtUp {
    async fn write(
        &self,
        _envelopes: &[kernel::Envelope],
        caught_up: bool,
    ) -> sink::Result<WriteReport> {
        self.flushes.lock().unwrap().push(caught_up);
        Ok(WriteReport::clean().visible(if caught_up {
            sink::Visibility::Immediate
        } else {
            sink::Visibility::Deferred
        }))
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_deferred_sink_still_refreshes_when_skipped_snapshot_batches_follow_the_last_live_one() {
    let name = sink_name("primary");
    let stream = Arc::new(ChannelStream::new(64, [name.clone()]));
    let lane = stream.lane(&name).unwrap().producer;
    let live = |position: u64| {
        stream::LaneItem::Batch(stream::Batch {
            position: Some(Position::new(position)),
            changes: 1,
            envelopes: Vec::new().into(),
        })
    };
    let snapshot = stream::LaneItem::Batch(stream::Batch {
        position: None,
        changes: 0,
        envelopes: Vec::new().into(),
    });
    lane.publish(live(5)).await.unwrap();
    lane.publish(snapshot.clone()).await.unwrap();
    lane.publish(snapshot).await.unwrap();
    let sink = Arc::new(RefreshesWhenCaughtUp::default());
    let observer = Arc::new(super::PositionObserver::default());
    let stop = Stop::new();
    stop.request();
    let engine = SinkEngine::new(
        name,
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
    )
    .with_observer(Arc::clone(&observer) as Arc<dyn crate::Observer>)
    .with_stop(stop);
    let run = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });

    super::wait_for(|| stream.is_idle()).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    run.abort();

    assert!(
        observer
            .events()
            .contains(&"searchable primary 5".to_owned()),
        "the last live write is made searchable before the stop ends: {:?} after flushes {:?}",
        observer.events(),
        sink.flushes.lock().unwrap()
    );
}

/// [`MockDocuments`] that requests `stop` while resolving the change of row
/// `at`, as a signal landing in the middle of a ready burst would.
#[derive(Debug)]
struct StopsWhileResolving {
    inner: MockDocuments,
    stop: Stop,
    at: i64,
}

#[async_trait]
impl DocumentBuilder for StopsWhileResolving {
    async fn resolve(
        &self,
        table: &kernel::TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        if key.0.first().map(|(_, value)| value) == Some(&kernel::GenericValue::BigInt(self.at)) {
            self.stop.request();
        }
        self.inner.resolve(table, key, before, after).await
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        self.inner.build(id).await
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        self.inner.build_many(ids).await
    }

    fn backfill_indexes(&self) -> Vec<kernel::IndexName> {
        self.inner.backfill_indexes()
    }

    async fn keys(
        &self,
        index: &kernel::IndexName,
        after: Option<&RowKey>,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        self.inner.keys(index, after, limit).await
    }

    async fn index_mappings(&self) -> source::Result<Vec<kernel::IndexMapping>> {
        self.inner.index_mappings().await
    }
}

#[tokio::test]
async fn a_stop_raised_mid_burst_ends_the_batch_there_and_lands_what_was_read() {
    let (source, feed) = ClosableSource::new();
    let source = Arc::new(source);
    for id in 1..=50 {
        feed.send(upsert(id)).await.unwrap();
    }
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let progress = Arc::new(Progress::new(stream_sinks(&stream)));
    let sink = Arc::new(RecordingSink::seeded(true));
    let (ops, flushes) = (Arc::clone(&sink.ops), Arc::clone(&sink.flushes));
    let sinks = spawn_sinks(
        &stream,
        &progress,
        vec![(sink_name("primary"), sink as Arc<dyn Sink>)],
    )
    .await;
    let stop = Stop::new();
    let documents = Arc::new(StopsWhileResolving {
        inner: MockDocuments::default(),
        stop: stop.clone(),
        at: 3,
    });
    let engine = ingest(&source, documents, &stream, &progress, &["primary"], &stop);

    tokio::time::timeout(Duration::from_secs(5), engine.run())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        source.pulled.load(Ordering::SeqCst),
        3,
        "the ready burst is read no further than the stop"
    );
    assert_eq!(*flushes.lock().unwrap(), vec![(3, true)]);
    assert_eq!(ops.lock().unwrap().len(), 3);
    assert_eq!(source.confirmed(), vec!["confirm 2".to_owned()]);
    for task in sinks {
        task.abort();
    }
}
