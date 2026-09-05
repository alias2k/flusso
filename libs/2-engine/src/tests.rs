//! Engine unit tests: the two engines over an in-process stream with mock
//! source, builder, and sink, asserting the invariants the crate docs name.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{ColumnName, Envelope, IndexMapping, IndexName, Op, Position, SinkName, TableName};
use sink::{FlushReport, Sink, SinkOptions};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId, IndexScope};
use source::{RowKey, SnapshotTable};
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::sync::Notify;

use super::*;

fn users() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn sink_name(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

fn key(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        kernel::GenericValue::BigInt(id),
    )])
}

fn upsert(id: i64) -> ChangeEvent {
    ChangeEvent::Upsert {
        table: TableName::try_new("users").unwrap(),
        key: key(id),
    }
}

fn delete(id: i64) -> ChangeEvent {
    ChangeEvent::Delete {
        table: TableName::try_new("users").unwrap(),
        key: key(id),
    }
}

fn mapping() -> IndexMapping {
    IndexMapping {
        index: users(),
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// A source that replays live changes (positions 0..n) once and snapshots a
/// fixed set of rows, recording what it was asked and what was confirmed.
#[derive(Debug)]
struct MockSource {
    live: Mutex<Option<Vec<ChangeEvent>>>,
    snapshot_rows: Vec<ChangeEvent>,
    continuity: Continuity,
    events: Arc<Mutex<Vec<String>>>,
    confirmed: Arc<Mutex<Vec<Position>>>,
}

impl MockSource {
    fn new(live: Vec<ChangeEvent>) -> Self {
        Self {
            live: Mutex::new(Some(live)),
            snapshot_rows: Vec::new(),
            continuity: Continuity::Resumed,
            events: Arc::new(Mutex::new(Vec::new())),
            confirmed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_snapshot(mut self, rows: Vec<ChangeEvent>) -> Self {
        self.snapshot_rows = rows;
        self
    }

    fn with_continuity(mut self, continuity: Continuity) -> Self {
        self.continuity = continuity;
        self
    }
}

#[async_trait]
impl ChangeCapture for MockSource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(self.continuity)
    }

    async fn prepare(&self) -> source::Result<()> {
        self.events.lock().unwrap().push("prepare".to_owned());
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let changes = self.live.lock().unwrap().take().unwrap_or_default();
        Ok(Box::pin(futures::stream::iter(
            changes
                .into_iter()
                .enumerate()
                .map(|(i, event)| Ok((Position(i as u64), event))),
        )))
    }

    fn confirm(&self, position: Position) {
        self.confirmed.lock().unwrap().push(position);
    }

    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> source::Result<BoxStream<'static, source::Result<ChangeEvent>>> {
        self.events
            .lock()
            .unwrap()
            .push(format!("snapshot {}", tables.len()));
        Ok(Box::pin(futures::stream::iter(
            self.snapshot_rows.clone().into_iter().map(Ok),
        )))
    }
}

/// Resolves every change to one `users` document; a delete builds a tombstone.
/// Counts `build_many` calls and the ids handed to each.
#[derive(Debug, Default)]
struct MockDocuments {
    builds: Arc<Mutex<Vec<usize>>>,
    deleted: Mutex<Vec<i64>>,
}

#[async_trait]
impl DocumentBuilder for MockDocuments {
    async fn resolve(&self, _table: &TableName, key: &RowKey) -> source::Result<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: users(),
            key: key.clone(),
        }])
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        let row = match id.key.0.first() {
            Some((_, kernel::GenericValue::BigInt(v))) => *v,
            _ => 0,
        };
        if self.deleted.lock().unwrap().contains(&row) {
            return Ok(Document::Delete { id: id.clone() });
        }
        Ok(Document::Upsert {
            id: id.clone(),
            body: kernel::GenericValue::Map(BTreeMap::new()),
        })
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        self.builds.lock().unwrap().push(ids.len());
        let mut out = Vec::new();
        for id in ids {
            out.push(self.build(id).await?);
        }
        Ok(out)
    }

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        vec![IndexScope {
            index: users(),
            root: SnapshotTable {
                db_schema: kernel::DatabaseSchema::try_new("public").unwrap(),
                table: TableName::try_new("users").unwrap(),
            },
        }]
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok(vec![mapping()])
    }
}

/// Records applied envelopes as `"<op> <index> <id>"`, flushes, and the seed
/// hooks; `seeded` is what `is_seeded` answers.
#[derive(Debug, Default)]
struct RecordingSink {
    ops: Arc<Mutex<Vec<String>>>,
    flushes: Arc<Mutex<Vec<(usize, bool)>>>,
    seeded: AtomicBool,
    marked: Arc<Mutex<Vec<String>>>,
    reindexed: Arc<Mutex<Vec<String>>>,
    events: Arc<Mutex<Vec<String>>>,
    pending: AtomicUsize,
}

impl RecordingSink {
    fn seeded(seeded: bool) -> Self {
        let sink = Self::default();
        sink.seeded.store(seeded, Ordering::SeqCst);
        sink
    }

    fn with_events(mut self, events: &Arc<Mutex<Vec<String>>>) -> Self {
        self.events = Arc::clone(events);
        self
    }
}

#[async_trait]
impl Sink for RecordingSink {
    async fn apply(&self, envelope: &Envelope) -> sink::Result<()> {
        self.ops.lock().unwrap().push(format!(
            "{} {} {}",
            envelope.op,
            envelope.index.as_ref(),
            envelope.id
        ));
        self.pending.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn flush(&self, caught_up: bool) -> sink::Result<FlushReport> {
        let n = self.pending.swap(0, Ordering::SeqCst);
        self.flushes.lock().unwrap().push((n, caught_up));
        Ok(FlushReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded.load(Ordering::SeqCst))
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.marked.lock().unwrap().push(index.as_ref().to_owned());
        self.seeded.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        let index = mapping.index.as_ref().to_owned();
        self.events.lock().unwrap().push(format!("reindex {index}"));
        self.reindexed.lock().unwrap().push(index);
        self.seeded.store(false, Ordering::SeqCst);
        Ok(())
    }
}

/// One deployment in a test: a stream with one lane per sink, the ingest
/// engine, and one sink engine per sink, run the way the daemon runs them.
struct Harness {
    stream: Arc<ChannelStream>,
    source: Arc<MockSource>,
    documents: Arc<MockDocuments>,
    ingest: IngestEngine,
    sinks: Vec<SinkEngine>,
}

impl Harness {
    fn new(source: MockSource, sinks: Vec<(SinkName, Arc<dyn Sink>)>) -> Self {
        let documents = Arc::new(MockDocuments::default());
        let names: Vec<SinkName> = sinks.iter().map(|(n, _)| n.clone()).collect();
        let stream = Arc::new(ChannelStream::new(64, names.clone()));
        let source = Arc::new(source);
        let ingest = IngestEngine::new(
            Arc::clone(&source) as Arc<dyn ChangeCapture>,
            Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            names,
        );
        let sinks = sinks
            .into_iter()
            .map(|(name, sink)| {
                SinkEngine::new(
                    name,
                    sink,
                    Arc::clone(&stream) as Arc<dyn Stream>,
                    vec![mapping()],
                )
            })
            .collect();
        Self {
            stream,
            source,
            documents,
            ingest,
            sinks,
        }
    }

    fn map_sinks(mut self, f: impl Fn(SinkEngine) -> SinkEngine) -> Self {
        self.sinks = self.sinks.into_iter().map(f).collect();
        self
    }

    fn with_batch(mut self, batch: BatchPolicy) -> Self {
        self.ingest = self.ingest.with_batch(batch);
        self
    }

    /// Stage every sink with `continuity`, run everything until the live
    /// stream ends and the lanes drain, then stop the sink engines. Returns the
    /// sink engines' outcomes (an error stops that sink engine, as it would in
    /// the daemon).
    async fn run(self, continuity: Continuity) -> Vec<Result<()>> {
        for sink in &self.sinks {
            sink.stage(continuity).await.unwrap();
        }
        let stream = Arc::clone(&self.stream);
        let sinks = self.sinks;
        let mut controls = Vec::new();
        let mut tasks = Vec::new();
        for engine in sinks {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(4);
            controls.push(tx);
            tasks.push(tokio::spawn(async move { engine.run(&mut rx).await }));
        }
        self.ingest.run().await.unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !stream.is_idle() && tokio::time::Instant::now() < deadline {
            if tasks.iter().all(|t| t.is_finished()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let mut outcomes = Vec::new();
        for task in tasks {
            task.abort();
            outcomes.push(match task.await {
                Ok(result) => result,
                Err(_) => Ok(()),
            });
        }
        if let Some(watermark) = stream.watermark() {
            self.source.confirm(watermark);
        }
        drop(controls);
        outcomes
    }
}

#[tokio::test]
async fn drives_live_changes_to_the_sink_and_confirms_the_watermark() {
    let sink = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&sink.ops);
    let harness = Harness::new(
        MockSource::new(vec![upsert(1), delete(2), upsert(3)]),
        vec![(sink_name("primary"), sink)],
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(10),
    });
    let source = Arc::clone(&harness.source);
    harness.run(Continuity::Resumed).await;

    let recorded = ops.lock().unwrap();
    assert!(recorded.contains(&"upsert users 1".to_owned()));
    assert!(recorded.contains(&"upsert users 3".to_owned()));
    assert_eq!(recorded.len(), 3);
    let confirmed = source.confirmed.lock().unwrap();
    assert_eq!(
        confirmed.last(),
        Some(&Position(2)),
        "the last change's position reaches the source once every lane acked it"
    );
}

#[tokio::test]
async fn batches_changes_into_a_single_build_and_flush() {
    let sink = Arc::new(RecordingSink::seeded(true));
    let flushes = Arc::clone(&sink.flushes);
    let harness = Harness::new(
        MockSource::new(vec![upsert(1), upsert(2), upsert(3), upsert(4)]),
        vec![(sink_name("primary"), sink)],
    )
    .with_batch(BatchPolicy {
        max_changes: 256,
        max_delay: Duration::from_secs(5),
    });
    let builds = Arc::clone(&harness.documents.builds);
    harness.run(Continuity::Resumed).await;

    assert_eq!(
        *builds.lock().unwrap(),
        vec![4],
        "one build_many for the batch"
    );
    assert_eq!(
        *flushes.lock().unwrap(),
        vec![(4, true)],
        "one flush of four envelopes, caught up"
    );
}

#[tokio::test]
async fn builds_a_repeatedly_touched_document_once_per_batch() {
    let sink = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&sink.ops);
    let harness = Harness::new(
        MockSource::new(vec![upsert(1), upsert(1), upsert(1), upsert(2)]),
        vec![(sink_name("primary"), sink)],
    )
    .with_batch(BatchPolicy {
        max_changes: 256,
        max_delay: Duration::from_secs(5),
    });
    let builds = Arc::clone(&harness.documents.builds);
    harness.run(Continuity::Resumed).await;

    assert_eq!(
        *builds.lock().unwrap(),
        vec![2],
        "ids are deduplicated before the build"
    );
    assert_eq!(ops.lock().unwrap().len(), 2);
}

/// A sink whose flush blocks until released, so a test can look at the
/// watermark while a batch is applied-but-not-flushed.
#[derive(Debug)]
struct GatedSink {
    release: Arc<Notify>,
    released: AtomicBool,
}

#[async_trait]
impl Sink for GatedSink {
    async fn apply(&self, _: &Envelope) -> sink::Result<()> {
        Ok(())
    }

    async fn flush(&self, _: bool) -> sink::Result<FlushReport> {
        if !self.released.swap(true, Ordering::SeqCst) {
            self.release.notified().await;
        }
        Ok(FlushReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn confirms_no_position_before_its_flush() {
    let release = Arc::new(Notify::new());
    let sink = Arc::new(GatedSink {
        release: Arc::clone(&release),
        released: AtomicBool::new(false),
    });
    let harness = Harness::new(
        MockSource::new(vec![upsert(1)]),
        vec![(sink_name("primary"), sink)],
    );
    let stream = Arc::clone(&harness.stream);
    let source = Arc::clone(&harness.source);

    let run = tokio::spawn(harness.run(Continuity::Resumed));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        stream.watermark(),
        None,
        "the batch is applied but its flush has not returned: nothing is confirmed"
    );
    assert!(source.confirmed.lock().unwrap().is_empty());
    release.notify_one();
    run.await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(0)));
    assert_eq!(*source.confirmed.lock().unwrap(), vec![Position(0)]);
}

#[tokio::test]
async fn an_unseeded_sink_requests_a_snapshot_then_marks_seeded() {
    let sink = Arc::new(RecordingSink::seeded(false));
    let ops = Arc::clone(&sink.ops);
    let marked = Arc::clone(&sink.marked);
    let harness = Harness::new(
        MockSource::new(vec![]).with_snapshot(vec![upsert(10), upsert(11)]),
        vec![(sink_name("primary"), sink)],
    );
    let events = Arc::clone(&harness.source.events);
    harness.run(Continuity::Resumed).await;

    assert_eq!(*events.lock().unwrap(), vec!["prepare", "snapshot 1"]);
    assert_eq!(
        *ops.lock().unwrap(),
        vec!["upsert users 10".to_owned(), "upsert users 11".to_owned()]
    );
    assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
}

#[tokio::test]
async fn a_seeded_sink_requests_nothing() {
    let sink = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&sink.ops);
    let harness = Harness::new(
        MockSource::new(vec![]).with_snapshot(vec![upsert(10)]),
        vec![(sink_name("primary"), sink)],
    );
    let events = Arc::clone(&harness.source.events);
    harness.run(Continuity::Resumed).await;
    assert_eq!(*events.lock().unwrap(), vec!["prepare"]);
    assert!(ops.lock().unwrap().is_empty());
}

#[tokio::test]
async fn snapshots_go_only_to_the_requesting_lane() {
    let seeded = Arc::new(RecordingSink::seeded(true));
    let unseeded = Arc::new(RecordingSink::seeded(false));
    let seeded_ops = Arc::clone(&seeded.ops);
    let unseeded_ops = Arc::clone(&unseeded.ops);
    let harness = Harness::new(
        MockSource::new(vec![upsert(1)]).with_snapshot(vec![upsert(10)]),
        vec![(sink_name("warm"), seeded), (sink_name("cold"), unseeded)],
    );
    harness.run(Continuity::Resumed).await;

    assert_eq!(
        *seeded_ops.lock().unwrap(),
        vec!["upsert users 1".to_owned()],
        "the seeded sink sees live changes only"
    );
    let cold = unseeded_ops.lock().unwrap();
    assert!(cold.contains(&"upsert users 10".to_owned()), "{cold:?}");
    assert!(cold.contains(&"upsert users 1".to_owned()), "{cold:?}");
}

#[tokio::test]
async fn concurrent_requests_for_the_same_index_coalesce_into_one_snapshot() {
    let a = Arc::new(RecordingSink::seeded(false));
    let b = Arc::new(RecordingSink::seeded(false));
    let a_marked = Arc::clone(&a.marked);
    let b_marked = Arc::clone(&b.marked);
    let harness = Harness::new(
        MockSource::new(vec![]).with_snapshot(vec![upsert(10)]),
        vec![(sink_name("a"), a), (sink_name("b"), b)],
    );
    let events = Arc::clone(&harness.source.events);
    harness.run(Continuity::Resumed).await;

    assert_eq!(
        *events.lock().unwrap(),
        vec!["prepare", "snapshot 1"],
        "two requests, one pass over the table"
    );
    assert_eq!(*a_marked.lock().unwrap(), vec!["users".to_owned()]);
    assert_eq!(*b_marked.lock().unwrap(), vec!["users".to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn a_backfill_with_every_lane_asking_starts_without_waiting() {
    let sink = Arc::new(RecordingSink::seeded(false));
    let marked = Arc::clone(&sink.marked);
    let harness = Harness::new(
        MockSource::new(vec![]).with_snapshot(vec![upsert(10)]),
        vec![(sink_name("primary"), sink)],
    )
    .with_batch(BatchPolicy {
        max_changes: 256,
        max_delay: Duration::from_secs(60),
    });
    let started = tokio::time::Instant::now();
    harness.run(Continuity::Resumed).await;

    assert!(
        started.elapsed() < Duration::from_secs(60),
        "the snapshot waited {:?} although every lane had already asked",
        started.elapsed()
    );
    assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn a_straggling_reindex_request_within_max_delay_joins_the_snapshot() {
    let a = Arc::new(RecordingSink::seeded(true));
    let b = Arc::new(RecordingSink::seeded(true));
    let a_marked = Arc::clone(&a.marked);
    let b_marked = Arc::clone(&b.marked);
    let stream = Arc::new(ChannelStream::new(64, [sink_name("a"), sink_name("b")]));
    let source = Arc::new(MockSource::new(vec![]).with_snapshot(vec![upsert(10)]));
    let events = Arc::clone(&source.events);
    let mut controls = Vec::new();
    let mut sink_tasks = Vec::new();
    for (name, sink) in [
        (sink_name("a"), a as Arc<dyn Sink>),
        (sink_name("b"), b as Arc<dyn Sink>),
    ] {
        let engine = SinkEngine::new(
            name,
            sink,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![mapping()],
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
        controls.push(tx);
        sink_tasks.push(tokio::spawn(async move { engine.run(&mut rx).await }));
    }
    let reindex = || SinkControl::Reindex {
        indexes: vec![users()],
    };
    // `a` asks before the ingest engine starts (so the engine has a request to
    // serve); `b` asks a second later, inside the straggler window.
    controls[0].send(reindex()).await.unwrap();
    let ingest = IngestEngine::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::new(MockDocuments::default()),
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![sink_name("a"), sink_name("b")],
    )
    .with_batch(BatchPolicy {
        max_changes: 256,
        max_delay: Duration::from_secs(60),
    });
    let ingest_task = tokio::spawn(async move { ingest.run().await });
    tokio::time::sleep(Duration::from_secs(1)).await;
    controls[1].send(reindex()).await.unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        while a_marked.lock().unwrap().is_empty() || b_marked.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    ingest_task.abort();
    for task in sink_tasks {
        task.abort();
    }

    assert_eq!(
        *events.lock().unwrap(),
        vec!["prepare", "snapshot 1"],
        "the straggler joined the first request's snapshot"
    );
    assert_eq!(*a_marked.lock().unwrap(), vec!["users".to_owned()]);
    assert_eq!(*b_marked.lock().unwrap(), vec!["users".to_owned()]);
}

#[tokio::test]
async fn fresh_source_rebuilds_seeded_indexes_before_prepare_then_snapshots() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink::seeded(true).with_events(&events));
    let reindexed = Arc::clone(&sink.reindexed);
    let marked = Arc::clone(&sink.marked);
    let mut source = MockSource::new(vec![])
        .with_snapshot(vec![upsert(1)])
        .with_continuity(Continuity::Fresh);
    source.events = Arc::clone(&events);
    let harness = Harness::new(source, vec![(sink_name("primary"), sink)]);
    harness.run(Continuity::Fresh).await;

    assert_eq!(*reindexed.lock().unwrap(), vec!["users".to_owned()]);
    assert_eq!(
        *events.lock().unwrap(),
        vec!["reindex users", "prepare", "snapshot 1"],
        "stage the rebuild, then establish the resume point, then snapshot"
    );
    assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
}

#[tokio::test]
async fn fresh_source_leaves_an_unseeded_index_to_the_normal_backfill() {
    let sink = Arc::new(RecordingSink::seeded(false));
    let reindexed = Arc::clone(&sink.reindexed);
    let marked = Arc::clone(&sink.marked);
    let harness = Harness::new(
        MockSource::new(vec![])
            .with_snapshot(vec![upsert(1)])
            .with_continuity(Continuity::Fresh),
        vec![(sink_name("primary"), sink)],
    );
    harness.run(Continuity::Fresh).await;
    assert!(reindexed.lock().unwrap().is_empty());
    assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
}

#[tokio::test]
async fn skip_backfill_with_a_fresh_source_stages_nothing() {
    let sink = Arc::new(RecordingSink::seeded(true));
    let reindexed = Arc::clone(&sink.reindexed);
    let ops = Arc::clone(&sink.ops);
    let harness = Harness::new(
        MockSource::new(vec![])
            .with_snapshot(vec![upsert(1)])
            .with_continuity(Continuity::Fresh),
        vec![(sink_name("primary"), sink)],
    )
    .map_sinks(|engine| engine.skip_backfill(true));
    let events = Arc::clone(&harness.source.events);
    harness.run(Continuity::Fresh).await;
    assert!(reindexed.lock().unwrap().is_empty());
    assert!(ops.lock().unwrap().is_empty());
    assert_eq!(*events.lock().unwrap(), vec!["prepare"]);
}

#[tokio::test]
async fn backfill_false_makes_a_stateless_sink_live_only() {
    let sink = Arc::new(RecordingSink::seeded(false));
    let ops = Arc::clone(&sink.ops);
    let marked = Arc::clone(&sink.marked);
    let harness = Harness::new(
        MockSource::new(vec![upsert(1)]).with_snapshot(vec![upsert(10)]),
        vec![(sink_name("audit"), sink)],
    )
    .map_sinks(|engine| engine.with_options(SinkOptions { backfill: false }));
    let events = Arc::clone(&harness.source.events);
    harness.run(Continuity::Resumed).await;
    assert_eq!(
        *events.lock().unwrap(),
        vec!["prepare"],
        "no snapshot requested"
    );
    assert_eq!(*ops.lock().unwrap(), vec!["upsert users 1".to_owned()]);
    assert!(marked.lock().unwrap().is_empty());
}

/// A sink whose first flush fails, then stores durably; the store survives
/// across sink instances so a redelivered batch can be checked for duplicates.
#[derive(Debug)]
struct CrashSink {
    store: Arc<Mutex<BTreeMap<String, Op>>>,
    staging: Mutex<Vec<(String, Op)>>,
    fail_next_flush: AtomicBool,
}

#[async_trait]
impl Sink for CrashSink {
    async fn apply(&self, envelope: &Envelope) -> sink::Result<()> {
        self.staging.lock().unwrap().push((
            format!("{}/{}", envelope.index.as_ref(), envelope.id),
            envelope.op,
        ));
        Ok(())
    }

    async fn flush(&self, _: bool) -> sink::Result<FlushReport> {
        if self.fail_next_flush.swap(false, Ordering::SeqCst) {
            self.staging.lock().unwrap().clear();
            return Err(sink::SinkError::Write(
                "simulated crash before flush completed".to_owned(),
            ));
        }
        let mut store = self.store.lock().unwrap();
        for (key, op) in self.staging.lock().unwrap().drain(..) {
            store.insert(key, op);
        }
        Ok(FlushReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn redelivers_the_unacked_batch_to_a_restarted_sink_engine() {
    let store: Arc<Mutex<BTreeMap<String, Op>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let crashing = Arc::new(CrashSink {
        store: Arc::clone(&store),
        staging: Mutex::new(Vec::new()),
        fail_next_flush: AtomicBool::new(true),
    });
    let harness = Harness::new(
        MockSource::new(vec![upsert(1), upsert(3)]),
        vec![(sink_name("primary"), Arc::clone(&crashing) as Arc<dyn Sink>)],
    )
    .with_batch(BatchPolicy {
        max_changes: 256,
        max_delay: Duration::from_secs(5),
    });
    let stream = Arc::clone(&harness.stream);
    let source = Arc::clone(&harness.source);
    let outcomes = harness.run(Continuity::Resumed).await;

    assert!(
        outcomes[0].is_err(),
        "the crashing flush stops the sink engine"
    );
    assert!(store.lock().unwrap().is_empty());
    assert_eq!(
        stream.watermark(),
        None,
        "nothing acknowledged, nothing confirmed"
    );
    assert!(source.confirmed.lock().unwrap().is_empty());

    // The daemon restarts the sink engine over the same lane: the batch it
    // left unacknowledged is redelivered and lands exactly once.
    let restarted = SinkEngine::new(
        sink_name("primary"),
        Arc::clone(&crashing) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
    );
    let (_tx, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    let task = tokio::spawn(async move { restarted.run(&mut rx).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while !stream.is_idle() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert_eq!(
        store.lock().unwrap().keys().cloned().collect::<Vec<_>>(),
        vec!["users/1".to_owned(), "users/3".to_owned()]
    );
    assert_eq!(stream.watermark(), Some(Position(1)));
}

#[tokio::test]
async fn caught_up_is_false_while_a_backlog_drains_then_true_on_the_last_batch() {
    let sink = Arc::new(RecordingSink::seeded(true));
    let flushes = Arc::clone(&sink.flushes);
    let harness = Harness::new(
        MockSource::new(vec![upsert(1), upsert(2), upsert(3)]),
        vec![(sink_name("primary"), sink)],
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(5),
    });
    harness.run(Continuity::Resumed).await;
    let flushes = flushes.lock().unwrap();
    assert_eq!(flushes.len(), 3);
    assert!(flushes.last().unwrap().1, "the final flush is caught up");
}

/// A sink that rejects every document at the item level; the flush succeeds.
#[derive(Debug, Default)]
struct RejectingSink {
    staged: Mutex<Vec<(String, String)>>,
}

#[async_trait]
impl Sink for RejectingSink {
    async fn apply(&self, envelope: &Envelope) -> sink::Result<()> {
        self.staged
            .lock()
            .unwrap()
            .push((envelope.index.as_ref().to_owned(), envelope.id.clone()));
        Ok(())
    }

    async fn flush(&self, _: bool) -> sink::Result<FlushReport> {
        let rejected = self
            .staged
            .lock()
            .unwrap()
            .drain(..)
            .map(|(index, id)| sink::RejectedDocument {
                index,
                id,
                reason: "simulated item-level rejection".to_owned(),
            })
            .collect();
        Ok(FlushReport { rejected })
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

#[derive(Debug, Default)]
struct QuarantineObserver {
    quarantined: Mutex<Vec<(String, String, String)>>,
}

impl Observer for QuarantineObserver {
    fn on_document_quarantined(&self, sink: &SinkName, index: &str, id: &str, _reason: &str) {
        self.quarantined
            .lock()
            .unwrap()
            .push((sink.to_string(), index.to_owned(), id.to_owned()));
    }
}

#[test]
fn failure_policies_resolve_override_then_default() {
    let policies =
        FailurePolicies::new(FailurePolicy::Stop).with_override("analytics", FailurePolicy::Skip);
    assert_eq!(policies.resolve("analytics"), FailurePolicy::Skip);
    assert_eq!(policies.resolve("users"), FailurePolicy::Stop);
}

#[tokio::test]
async fn skip_policy_quarantines_rejected_documents_and_acks_the_batch() {
    let observer = Arc::new(QuarantineObserver::default());
    let harness = Harness::new(
        MockSource::new(vec![upsert(1), upsert(3)]),
        vec![(sink_name("primary"), Arc::new(RejectingSink::default()))],
    )
    .with_batch(BatchPolicy {
        max_changes: 256,
        max_delay: Duration::from_secs(5),
    });
    let observer_dyn: Arc<dyn Observer> = Arc::clone(&observer) as Arc<dyn Observer>;
    let harness = harness.map_sinks(move |engine| {
        engine
            .with_observer(Arc::clone(&observer_dyn))
            .with_failure_policies(FailurePolicies::new(FailurePolicy::Skip))
    });
    let stream = Arc::clone(&harness.stream);
    let outcomes = harness.run(Continuity::Resumed).await;
    assert!(outcomes[0].is_ok());
    let quarantined = observer.quarantined.lock().unwrap();
    assert_eq!(quarantined.len(), 2);
    assert!(
        quarantined
            .iter()
            .all(|(sink, index, _)| sink == "primary" && index == "users")
    );
    assert_eq!(
        stream.watermark(),
        Some(Position(1)),
        "the batch is acked despite rejections"
    );
}

#[tokio::test]
async fn stop_policy_errors_and_leaves_the_batch_unacked() {
    let harness = Harness::new(
        MockSource::new(vec![upsert(1)]),
        vec![(sink_name("primary"), Arc::new(RejectingSink::default()))],
    );
    let stream = Arc::clone(&harness.stream);
    let outcomes = harness.run(Continuity::Resumed).await;
    assert!(matches!(
        outcomes[0],
        Err(EngineError::DocumentsRejected(1, _))
    ));
    assert_eq!(
        stream.watermark(),
        None,
        "nothing acked when the engine stops"
    );
}

#[tokio::test]
async fn per_index_stop_override_halts_even_when_global_is_skip() {
    let harness = Harness::new(
        MockSource::new(vec![upsert(1)]),
        vec![(sink_name("primary"), Arc::new(RejectingSink::default()))],
    )
    .map_sinks(|engine| {
        engine.with_failure_policies(
            FailurePolicies::new(FailurePolicy::Skip).with_override("users", FailurePolicy::Stop),
        )
    });
    let outcomes = harness.run(Continuity::Resumed).await;
    assert!(matches!(
        outcomes[0],
        Err(EngineError::DocumentsRejected(..))
    ));
}

#[tokio::test]
async fn reindex_control_stages_and_requests_a_snapshot_without_restarting() {
    let sink = Arc::new(RecordingSink::seeded(true));
    let reindexed = Arc::clone(&sink.reindexed);
    let marked = Arc::clone(&sink.marked);
    let ops = Arc::clone(&sink.ops);
    let stream = Arc::new(ChannelStream::new(64, [sink_name("primary")]));
    let source = Arc::new(MockSource::new(vec![]).with_snapshot(vec![upsert(10)]));
    let documents = Arc::new(MockDocuments::default());
    let engine = SinkEngine::new(
        sink_name("primary"),
        sink,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
    );
    engine.stage(Continuity::Resumed).await.unwrap();
    let (control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    let sink_task = tokio::spawn(async move { engine.run(&mut rx).await });

    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    // A request is now on the request lane; the ingest engine serves it.
    let ingest = IngestEngine::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        documents,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![sink_name("primary")],
    );
    let ingest_task = tokio::spawn(async move { ingest.run().await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while marked.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    ingest_task.abort();
    sink_task.abort();

    assert_eq!(*reindexed.lock().unwrap(), vec!["users".to_owned()]);
    assert_eq!(*ops.lock().unwrap(), vec!["upsert users 10".to_owned()]);
    assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
    let _: HashMap<String, String> = HashMap::new();
}

/// Records the observer events a run emits, so a test can assert the engines
/// report their lifecycle and per-batch progress with the sink dimension.
#[derive(Debug, Default)]
struct RecordingObserver {
    events: Mutex<Vec<String>>,
}

impl Observer for RecordingObserver {
    fn on_live_started(&self) {
        self.events.lock().unwrap().push("live".to_owned());
    }

    fn on_batch_built(&self, stats: BuildStats) {
        self.events
            .lock()
            .unwrap()
            .push(format!("built {}", stats.documents));
    }

    fn on_sink_started(&self, sink: &SinkName) {
        self.events
            .lock()
            .unwrap()
            .push(format!("sink started {sink}"));
    }

    fn on_batch_committed(&self, sink: &SinkName, stats: CommitStats) {
        self.events
            .lock()
            .unwrap()
            .push(format!("committed {sink} {}", stats.envelopes));
    }
}

#[tokio::test]
async fn reports_lifecycle_and_progress_to_the_observer() {
    let observer = Arc::new(RecordingObserver::default());
    let observer_dyn: Arc<dyn Observer> = Arc::clone(&observer) as Arc<dyn Observer>;
    let mut harness = Harness::new(
        MockSource::new(vec![upsert(1), upsert(2)]),
        vec![(sink_name("primary"), Arc::new(RecordingSink::seeded(true)))],
    )
    .with_batch(BatchPolicy {
        max_changes: 256,
        max_delay: Duration::from_secs(5),
    });
    harness.ingest = harness.ingest.with_observer(Arc::clone(&observer_dyn));
    let harness = harness.map_sinks(move |engine| engine.with_observer(Arc::clone(&observer_dyn)));
    harness.run(Continuity::Resumed).await;
    let events = observer.events.lock().unwrap();
    assert_eq!(
        *events,
        vec![
            "sink started primary".to_owned(),
            "live".to_owned(),
            "built 2".to_owned(),
            "committed primary 2".to_owned(),
        ]
    );
}
