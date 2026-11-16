//! A requested [`Stop`]: the ingest engine reads nothing more, publishes what
//! it was building, waits until every sink flushed and acknowledged it, hands
//! the final watermark to the source, and only then closes the live stream.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{IndexName, Position, SinkName};
use sink::{FlushReport, Sink};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId, IndexScope};
use source::{RowImage, RowKey, SnapshotTable};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::sync::{Notify, mpsc};

use super::{MockDocuments, RecordingSink, mapping, sink_name, upsert};
use crate::{IngestEngine, SinkControl, SinkEngine, Stop};

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
    snapshot: Mutex<Option<BoxStream<'static, source::Result<ChangeEvent>>>>,
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
                snapshot: Mutex::new(None),
            },
            tx,
        )
    }

    /// The next snapshot is `rows`.
    fn with_snapshot(self, rows: BoxStream<'static, source::Result<ChangeEvent>>) -> Self {
        *self.snapshot.lock().unwrap() = Some(rows);
        self
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
                            Some((Ok((Position(next), event)), (feed, next + 1)))
                        }
                        () = closing.notified() => None,
                    }
                }
            },
        )))
    }

    fn confirm(&self, position: Position) {
        self.log
            .lock()
            .unwrap()
            .push(format!("confirm {}", position.0));
    }

    fn close_live(&self) -> bool {
        self.log.lock().unwrap().push("close".to_owned());
        self.closed.store(true, Ordering::SeqCst);
        self.closing.notify_one();
        true
    }

    async fn snapshot(
        &self,
        _tables: &[SnapshotTable],
    ) -> source::Result<BoxStream<'static, source::Result<ChangeEvent>>> {
        Ok(self
            .snapshot
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Box::pin(futures::stream::empty())))
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

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        self.inner.backfill_scopes()
    }

    async fn index_mappings(&self) -> source::Result<Vec<kernel::IndexMapping>> {
        self.inner.index_mappings().await
    }
}

/// A sink whose flush never returns: down, or stalled.
#[derive(Debug, Default)]
struct StalledSink;

#[async_trait]
impl Sink for StalledSink {
    async fn apply(&self, _: &kernel::Envelope) -> sink::Result<()> {
        Ok(())
    }

    async fn flush(&self, _: bool) -> sink::Result<FlushReport> {
        std::future::pending().await
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// Stage and spawn one sink engine per sink; returns their tasks.
async fn spawn_sinks(
    stream: &Arc<ChannelStream>,
    sinks: Vec<(SinkName, Arc<dyn Sink>)>,
) -> Vec<tokio::task::JoinHandle<crate::Result<()>>> {
    let mut tasks = Vec::new();
    for (name, sink) in sinks {
        let engine = SinkEngine::new(
            name,
            sink,
            Arc::clone(stream) as Arc<dyn Stream>,
            vec![mapping()],
        );
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
    sinks: &[&str],
    stop: &Stop,
) -> IngestEngine {
    IngestEngine::new(
        Arc::clone(source) as Arc<dyn ChangeCapture>,
        documents,
        Arc::clone(stream) as Arc<dyn Stream>,
        sinks.iter().map(|sink| sink_name(sink)).collect(),
    )
    .with_stop(stop.clone())
}

#[tokio::test]
async fn a_stop_during_a_build_publishes_the_batch_and_every_sink_lands_it_before_the_run_ends() {
    let (source, feed) = ClosableSource::new();
    let source = Arc::new(source);
    let documents = Arc::new(GatedDocuments::default());
    documents.gated.store(true, Ordering::SeqCst);
    let stream = Arc::new(ChannelStream::new(64, [sink_name("a"), sink_name("b")]));
    let a = Arc::new(RecordingSink::seeded(true));
    let b = Arc::new(RecordingSink::seeded(true));
    let (a_ops, b_ops) = (Arc::clone(&a.ops), Arc::clone(&b.ops));
    let (a_flushes, b_flushes) = (Arc::clone(&a.flushes), Arc::clone(&b.flushes));
    let sinks = spawn_sinks(
        &stream,
        vec![(sink_name("a"), a as Arc<dyn Sink>), (sink_name("b"), b)],
    )
    .await;
    let stop = Stop::new();
    let engine = ingest(
        &source,
        Arc::clone(&documents) as _,
        &stream,
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
        "the final watermark reached the source before the live stream was closed"
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
    let sink = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&sink.ops);
    let sinks = spawn_sinks(&stream, vec![(sink_name("primary"), sink as Arc<dyn Sink>)]).await;
    let stop = Stop::new();
    let engine = ingest(
        &source,
        Arc::new(MockDocuments::default()),
        &stream,
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
async fn a_snapshot_in_progress_is_abandoned_and_its_request_served_again_on_the_next_run() {
    let (rows_tx, rows_rx) = mpsc::channel::<source::Result<ChangeEvent>>(4);
    rows_tx.send(Ok(upsert(10))).await.unwrap();
    let slow_rows: BoxStream<'static, source::Result<ChangeEvent>> =
        Box::pin(futures::stream::unfold(rows_rx, |mut rx| async move {
            rx.recv().await.map(|row| (row, rx))
        }));
    let (source, _feed) = ClosableSource::new();
    let source = Arc::new(source.with_snapshot(slow_rows));
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let sink = Arc::new(RecordingSink::seeded(false));
    let marked = Arc::clone(&sink.marked);
    let sinks = spawn_sinks(&stream, vec![(sink_name("primary"), sink as Arc<dyn Sink>)]).await;
    let stop = Stop::new();
    let engine = ingest(
        &source,
        Arc::new(MockDocuments::default()),
        &stream,
        &["primary"],
        &stop,
    );
    let run = tokio::spawn(async move { engine.run().await });

    tokio::time::sleep(Duration::from_millis(100)).await;
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
    let next_source =
        Arc::new(next_source.with_snapshot(Box::pin(futures::stream::iter([Ok(upsert(10))]))));
    let next = ingest(
        &next_source,
        Arc::new(MockDocuments::default()),
        &stream,
        &["primary"],
        &Stop::new(),
    );
    let next_run = tokio::spawn(async move { next.run().await });
    super::wait_for(|| !marked.lock().unwrap().is_empty()).await;
    assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
    next_run.abort();
    drop(rows_tx);
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
    let healthy = Arc::new(RecordingSink::seeded(true));
    let flushes = Arc::clone(&healthy.flushes);
    let sinks = spawn_sinks(
        &stream,
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
        "the watermark is the slowest lane's: nothing the stalled sink holds is confirmed"
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
    feed.send(upsert(1)).await.unwrap();
    let stop = Stop::new();
    stop.request();
    let engine = ingest(
        &source,
        Arc::new(MockDocuments::default()),
        &stream,
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
    let source = Arc::new(source.with_snapshot(Box::pin(futures::stream::iter(
        (1..=600).map(|id| Ok(upsert(id))),
    ))));
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let sink = Arc::new(RecordingSink::seeded(false));
    let (ops, marked) = (Arc::clone(&sink.ops), Arc::clone(&sink.marked));
    let engine = SinkEngine::new(
        sink_name("primary"),
        sink,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
    );
    engine.stage(Continuity::Resumed).await.unwrap();
    let stop = Stop::new();
    let ingest_engine = ingest(
        &source,
        Arc::new(MockDocuments::default()),
        &stream,
        &["primary"],
        &stop,
    );
    let ingest_run = tokio::spawn(async move { ingest_engine.run().await });
    super::wait_for(|| !stream.is_idle() && stream.watermark().is_none()).await;
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
