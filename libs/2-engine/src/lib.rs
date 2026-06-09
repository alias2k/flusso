//! The `flusso` sync engine.
//!
//! Wires the pluggable edges together and runs the pipeline:
//!
//! ```text
//! ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
//! ```
//!
//! A **capture** task drains the source's change stream into a bounded
//! in-process [`queue`](queue_channel) (back-pressure: capture blocks when the
//! queue is full). A **worker** pulls changes and, for the row each names,
//! resolves the affected document ids, assembles each one, and writes it to the
//! [`Sink`]'s buffer.
//!
//! Writes are **batched**: the worker groups up to [`BatchPolicy::max_changes`]
//! changes (or whatever arrives within [`BatchPolicy::max_delay`], whichever
//! comes first) into a single [`flush`](Sink::flush), turning N changes into
//! ⌈N / max_changes⌉ bulk round-trips instead of N. The source acks for a batch
//! are confirmed **only after** the flush that persisted their documents, so the
//! replication slot advances past a change exactly when its documents are
//! durable downstream — preserving at-least-once delivery: a crash before the
//! flush leaves the whole batch unconfirmed, so it is redelivered on restart and
//! re-applied idempotently (documents are rebuilt from the current row and
//! written by deterministic id).
//!
//! Stopping on any error is therefore safe: unconfirmed changes are redelivered
//! when the run restarts.
//!
//! Before anything else, the engine asks the [`DocumentBuilder`] for each
//! index's resolved mapping and tells the sink to create it
//! ([`ensure_index`](Sink::ensure_index)) — so the destination uses the
//! configured field types instead of guessing. This is idempotent, so it runs
//! on every start, including resumes.
//!
//! Before live capture, the engine runs an optional **backfill** phase. It asks
//! the [`DocumentBuilder`] which indexes exist and the sink whether each is
//! already seeded; for those that aren't, it asks the source to
//! [`snapshot`](ChangeCapture::snapshot) their root tables and drives that
//! finite stream through the same queue → resolve → build → sink path (scoped to
//! just the unseeded indexes), then records each as seeded. So "is a backfill
//! needed?" is the destination's call, not the source's.
//!
//! The queue, source, sink, and document builder are all trait objects, so the
//! backend choices (WAL vs polling, stdout vs OpenSearch, channel vs a durable
//! broker) are swappable without touching this loop.

// The pipeline benchmark (in `benches/`) pulls a concrete source and sink as
// dev-dependencies the unit-test build doesn't touch; allow that only under
// `cfg(test)` — the normal build still enforces unused dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod error;
mod observer;

pub use error::*;
pub use observer::*;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use queue_channel::{ChannelConsumer, channel};
use queue_core::{AckHandle, Consumer, Delivery, Producer};
use schema_core::{GenericValue, IndexName};
use sinks_core::Sink;
use sources_core::SnapshotTable;
use sources_core::cdc::{Ack, Change, ChangeCapture, ChangeEvent};
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use tokio::time::{Instant, timeout_at};

/// Pending changes buffered between capture and the worker.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// How the worker groups changes into one sink flush.
///
/// Batching trades a little latency for far fewer round-trips: up to
/// `max_changes` changes (or whatever has arrived after `max_delay`, whichever
/// comes first) are buffered and flushed together. `max_changes: 1` reproduces
/// the original flush-per-change behavior.
///
/// Acks respect the batch boundary — see the [module docs](crate). The source
/// ack for a change is confirmed only after the flush that made its documents
/// durable, so at-least-once delivery holds regardless of batch size.
#[derive(Debug, Clone, Copy)]
pub struct BatchPolicy {
    /// Flush once this many changes have accumulated. Clamped to at least 1.
    pub max_changes: usize,
    /// Flush a partial batch this long after its first change, so a trickle of
    /// changes still lands promptly instead of waiting for a full batch.
    pub max_delay: Duration,
}

impl Default for BatchPolicy {
    fn default() -> Self {
        Self {
            max_changes: 256,
            max_delay: Duration::from_millis(50),
        }
    }
}

/// Drives changes from a source through to a sink.
#[derive(Debug)]
pub struct Engine {
    source: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    observer: Arc<dyn Observer>,
    queue_capacity: usize,
    batch: BatchPolicy,
    skip_backfill: bool,
}

impl Engine {
    /// Assemble an engine from its pluggable parts.
    pub fn new(
        source: Arc<dyn ChangeCapture>,
        documents: Arc<dyn DocumentBuilder>,
        sink: Arc<dyn Sink>,
    ) -> Self {
        Self {
            source,
            documents,
            sink,
            observer: Arc::new(NoopObserver),
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            batch: BatchPolicy::default(),
            skip_backfill: false,
        }
    }

    /// Report lifecycle and progress events to `observer` (metrics, a live
    /// status surface, …). Defaults to [`NoopObserver`]. See [`Observer`].
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// Set how many changes may buffer between capture and the worker.
    pub fn with_queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity.max(1);
        self
    }

    /// Set how the worker groups changes into one sink flush (see
    /// [`BatchPolicy`]). `max_changes` is clamped to at least 1.
    pub fn with_batch(mut self, batch: BatchPolicy) -> Self {
        self.batch = BatchPolicy {
            max_changes: batch.max_changes.max(1),
            ..batch
        };
        self
    }

    /// Force-skip the backfill phase entirely, regardless of what the sink
    /// reports. An escape hatch for sinks that can't persist seeded-state (so
    /// they would otherwise re-seed every run) or to resume without re-checking.
    pub fn skip_backfill(mut self, skip: bool) -> Self {
        self.skip_backfill = skip;
        self
    }

    /// Run until the live change stream ends or an error stops the pipeline.
    ///
    /// First seeds any unseeded index (unless [`skip_backfill`](Self::skip_backfill)
    /// is set), then follows live changes.
    #[tracing::instrument(
        name = "engine.run",
        skip_all,
        fields(
            skip_backfill = self.skip_backfill,
            queue_capacity = self.queue_capacity,
            max_changes = self.batch.max_changes,
            max_delay_ms = self.batch.max_delay.as_millis() as u64,
        ),
    )]
    pub async fn run(self) -> Result<()> {
        let Engine {
            source,
            documents,
            sink,
            observer,
            queue_capacity,
            batch,
            skip_backfill,
        } = self;
        let pipeline = Pipeline {
            documents: documents.as_ref(),
            sink: sink.as_ref(),
            observer: &observer,
            queue_capacity,
            batch,
        };
        let result = run_inner(pipeline, source.as_ref(), skip_backfill).await;
        if let Err(error) = &result {
            observer.on_error(&error.to_string());
        }
        result
    }
}

/// The run-constant borrowed parts threaded through the pipeline's inner
/// functions, so each takes a single context instead of repeating the same five
/// arguments. `Copy` (it is only references plus two small values), so it is
/// passed by value.
#[derive(Clone, Copy)]
struct Pipeline<'a> {
    documents: &'a dyn DocumentBuilder,
    sink: &'a dyn Sink,
    observer: &'a Arc<dyn Observer>,
    queue_capacity: usize,
    batch: BatchPolicy,
}

/// The body of [`Engine::run`], over borrowed parts, so [`run`](Engine::run) can
/// report any error to the observer after the borrow ends.
async fn run_inner(
    pipeline: Pipeline<'_>,
    source: &dyn ChangeCapture,
    skip_backfill: bool,
) -> Result<()> {
    // Enrich the thin config into fully-typed mappings: the source fills the
    // gaps a human config leaves (field types, nullability) from what it
    // knows about its store. This runs by design on every start, before any
    // document flows, so the destination is created from a complete
    // description rather than guessing on first write — idempotent
    // (create-if-absent), so it is safe across resumes and backfills alike.
    let mappings = pipeline.documents.index_mappings().await?;
    tracing::info!(indexes = mappings.len(), "ensuring target indexes");
    for mapping in &mappings {
        pipeline.sink.ensure_index(mapping).await?;
    }
    pipeline.observer.on_indexes_ensured(mappings.len());

    if skip_backfill {
        tracing::info!("skipping backfill (skip_backfill set)");
    } else {
        backfill(pipeline, source).await?;
    }
    pipeline.observer.on_backfill_completed();

    let stream = source.live().await?;
    tracing::info!("following live changes");
    pipeline.observer.on_live_started();
    let result = pump(pipeline, stream, None).await;
    match &result {
        Ok(()) => tracing::info!("pipeline stopped: live stream ended"),
        Err(error) => tracing::error!(%error, "pipeline stopped on error"),
    }
    result
}

/// Seed every index the sink reports as unseeded, then mark them seeded.
///
/// The decision "does this index need a backfill?" is the **sink**'s — the
/// destination is what knows whether it already holds the data. For the indexes
/// that do, the source snapshots their root tables and the snapshot is applied
/// scoped to just those indexes, so an already-seeded index sharing a table is
/// never rewritten.
#[tracing::instrument(name = "backfill", skip_all)]
async fn backfill(pipeline: Pipeline<'_>, source: &dyn ChangeCapture) -> Result<()> {
    let mut seeding: HashSet<IndexName> = HashSet::new();
    let mut tables: Vec<SnapshotTable> = Vec::new();
    for scope in pipeline.documents.backfill_scopes() {
        if pipeline.sink.is_seeded(&scope.index).await? {
            continue;
        }
        if !tables.contains(&scope.root) {
            tables.push(scope.root);
        }
        seeding.insert(scope.index);
    }
    if seeding.is_empty() {
        tracing::info!("no unseeded indexes; skipping backfill");
        return Ok(());
    }

    tracing::info!(
        indexes = seeding.len(),
        tables = tables.len(),
        "seeding indexes"
    );
    pipeline
        .observer
        .on_backfill_started(&seeding.iter().cloned().collect::<Vec<_>>());
    let stream = source.snapshot(&tables).await?;
    pump(pipeline, stream, Some(&seeding)).await?;

    // The snapshot is fully applied and flushed once `pump` returns; record each
    // index as seeded so a later run skips it.
    for index in &seeding {
        pipeline.sink.mark_seeded(index).await?;
        pipeline.observer.on_index_seeded(index);
    }
    tracing::info!(indexes = seeding.len(), "backfill complete");
    Ok(())
}

/// Drain one change stream through the queue to the sink: spawn a capture task,
/// run the worker, then fold the outcomes (a worker failure takes priority).
///
/// `filter`, when set, restricts which indexes a change may write to — used by
/// the backfill so a snapshot only seeds the indexes being backfilled.
#[tracing::instrument(name = "pump", skip_all)]
async fn pump(
    pipeline: Pipeline<'_>,
    stream: BoxStream<'static, sources_core::Result<Change>>,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    let (producer, mut consumer) = channel::<Change>(pipeline.queue_capacity);

    // Capture runs concurrently with the worker; the worker borrows the shared
    // builder and sink. The observer's events are `&self`, so capture takes an
    // `Arc` clone it can move into the spawned `'static` task.
    let capture = tokio::spawn(capture(stream, producer, Arc::clone(pipeline.observer)));
    let worker = work(pipeline, &mut consumer, filter).await;

    capture.abort();
    let captured = capture.await;
    worker?;
    match captured {
        Ok(result) => result,
        Err(join) if join.is_cancelled() => Ok(()),
        Err(join) => Err(EngineError::Task(join.to_string())),
    }
}

/// Drain the change stream into the queue. Ends (closing the queue) when the
/// stream is exhausted.
#[tracing::instrument(name = "capture", skip_all)]
async fn capture(
    mut stream: BoxStream<'static, sources_core::Result<Change>>,
    producer: queue_channel::ChannelProducer<Change>,
    observer: Arc<dyn Observer>,
) -> Result<()> {
    let mut captured = 0u64;
    while let Some(change) = stream.next().await {
        producer.publish(change?).await?;
        captured += 1;
        observer.on_change_captured();
    }
    tracing::debug!(captured, "capture stream ended");
    Ok(())
}

/// Pull changes, buffer a batch of them into the sink, flush once, then confirm
/// the whole batch — see [`BatchPolicy`] and the [module docs](crate).
///
/// A batch starts when the first change arrives (the worker blocks for it, so an
/// idle stream costs nothing) and closes when `max_changes` are buffered, when
/// `max_delay` elapses since that first change, or when the stream ends.
///
/// At each flush the worker reads whether the queue is now empty — its
/// `caught_up` signal: the batch drained everything captured so far, with no
/// backlog behind it. It is forwarded through [`commit`] to [`Sink::flush`]
/// (the OpenSearch sink uses it to force a refresh only when idle). It is a
/// point-in-time snapshot, which is all the sink needs: a caught-up flush that
/// races a just-arrived change simply does its idle-time work slightly early.
#[tracing::instrument(name = "worker", skip_all, fields(max_changes = pipeline.batch.max_changes))]
async fn work(
    pipeline: Pipeline<'_>,
    consumer: &mut ChannelConsumer<Change>,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    let batch = pipeline.batch;
    // Source acks and queue handles for changes whose documents are buffered in
    // the sink but not yet flushed. Confirmed/acked only after the flush below.
    let mut pending: Batch = Batch::with_capacity(batch.max_changes);

    'batches: loop {
        // Block for the batch's first change (no busy-wait on an idle stream).
        let Some(delivery) = consumer.recv().await? else {
            break;
        };
        buffer(delivery, pipeline.documents, filter, &mut pending).await?;

        // Fill the batch until it is full or its time window closes. `recv` is
        // cancel-safe, so dropping it on timeout never drops a queued change.
        let deadline = Instant::now() + batch.max_delay;
        while pending.len() < batch.max_changes {
            match timeout_at(deadline, consumer.recv()).await {
                Err(_elapsed) => break, // window closed: flush a partial batch
                Ok(Ok(Some(delivery))) => {
                    buffer(delivery, pipeline.documents, filter, &mut pending).await?;
                }
                Ok(Ok(None)) => {
                    // Stream ended: flush what we have, then stop.
                    commit(pipeline, &mut pending, consumer.is_empty()).await?;
                    break 'batches;
                }
                Ok(Err(queue_err)) => return Err(queue_err.into()),
            }
        }
        commit(pipeline, &mut pending, consumer.is_empty()).await?;
    }
    // A batch left buffered when the stream ended mid-fill.
    commit(pipeline, &mut pending, consumer.is_empty()).await
}

/// A batch in the making: the acks owed once its documents are durable (the
/// source acks that advance the replication slot, plus the queue handles), and
/// the deduplicated ids of the documents the buffered changes resolved to —
/// built together in one [`build_many`](DocumentBuilder::build_many) at commit.
#[derive(Debug)]
struct Batch {
    source: Vec<Ack>,
    handles: Vec<Box<dyn AckHandle>>,
    /// Ids to (re)build for this batch, in first-seen order.
    ids: Vec<DocumentId>,
    /// Membership of `ids`, so a document touched by several changes in the
    /// batch is built once — the dedup the two-step resolve/build is designed
    /// for (see [`DocumentBuilder`]).
    seen: HashSet<DocumentId>,
}

impl Batch {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            source: Vec::with_capacity(capacity),
            handles: Vec::with_capacity(capacity),
            ids: Vec::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
        }
    }

    /// The number of changes buffered — what the batch policy caps.
    fn len(&self) -> usize {
        self.source.len()
    }
}

/// Resolve one change to the documents it affects and fold their ids into the
/// batch (deduplicated) — no build, no sink write, no ack yet. The actual
/// assembly happens once per batch in [`commit`]. The change's acks are
/// retained until then.
async fn buffer(
    delivery: Delivery<Change>,
    documents: &dyn DocumentBuilder,
    filter: Option<&HashSet<IndexName>>,
    pending: &mut Batch,
) -> Result<()> {
    let (change, handle) = delivery.into_parts();
    match &change.event {
        ChangeEvent::Upsert { table, key } | ChangeEvent::Delete { table, key } => {
            let affected = documents.resolve(table, key).await?;
            tracing::trace!(documents = affected.len(), "change resolved to documents");
            for id in affected {
                if filter.is_some_and(|filter| !filter.contains(&id.index)) {
                    continue;
                }
                if pending.seen.insert(id.clone()) {
                    pending.ids.push(id);
                }
            }
        }
    }
    pending.source.push(change.ack);
    pending.handles.push(handle);
    Ok(())
}

/// Close a batch: assemble its deduplicated documents in one
/// [`build_many`](DocumentBuilder::build_many), write each to the sink, then one
/// [`flush`](Sink::flush) makes them durable and every ack is confirmed.
///
/// The flush-then-confirm ordering is the at-least-once guarantee — a crash
/// before the flush leaves the whole batch unconfirmed and redelivered;
/// confirming after it means the slot only advances over durable changes.
/// Building the batch's ids as a set rather than per change reorders writes
/// within the batch, which is safe: documents are keyed and rebuilt from the
/// current row, so the resulting sink state is identical either way.
///
/// Source acks confirm out of order safely — the mechanism advances its resume
/// point only to the highest *contiguous* confirmed sequence (see
/// [`Ack`](sources_core::cdc::Ack)) — so confirming a batch advances the slot to
/// the batch's last change and no further.
///
/// `caught_up` is forwarded to [`Sink::flush`]: it tells the sink no backlog is
/// waiting behind this batch, so a sink with a cost to making writes *visible*
/// can pay it now (while idle) instead of under load. See [`work`] for how it is
/// derived from the queue.
#[tracing::instrument(name = "commit", level = "debug", skip_all, fields(changes = pending.len(), documents = pending.ids.len(), caught_up))]
async fn commit(pipeline: Pipeline<'_>, pending: &mut Batch, caught_up: bool) -> Result<()> {
    if pending.len() == 0 {
        return Ok(());
    }
    let changes = pending.len();
    let documents_built = pending.ids.len();
    // Tally documents per target index for per-index metrics, before the ids
    // are cleared below.
    let mut by_index: HashMap<IndexName, usize> = HashMap::new();
    for id in &pending.ids {
        *by_index.entry(id.index.clone()).or_insert(0) += 1;
    }

    for document in pipeline.documents.build_many(&pending.ids).await? {
        match document {
            Document::Upsert { id, body } => {
                pipeline
                    .sink
                    .upsert(&id.index, &document_id(&id), &body)
                    .await?;
            }
            Document::Delete { id } => {
                pipeline.sink.delete(&id.index, &document_id(&id)).await?;
            }
        }
    }
    let flush_start = Instant::now();
    pipeline.sink.flush(caught_up).await?;
    let flush = flush_start.elapsed();
    for ack in pending.source.drain(..) {
        ack.confirm();
    }
    for handle in pending.handles.drain(..) {
        handle.ack().await?;
    }
    pending.ids.clear();
    pending.seen.clear();
    pipeline.observer.on_batch_committed(BatchStats {
        changes,
        documents: documents_built,
        documents_by_index: by_index.into_iter().collect(),
        flush,
    });
    tracing::debug!("batch built, flushed, and acked");
    Ok(())
}

/// The sink's document `_id`, derived from the document key (the root primary
/// key); composite keys join their parts with `:`.
fn document_id(id: &DocumentId) -> String {
    id.key
        .0
        .iter()
        .map(|(_, value)| value_to_string(value))
        .collect::<Vec<_>>()
        .join(":")
}

fn value_to_string(value: &GenericValue) -> String {
    match value {
        GenericValue::Bool(b) => b.to_string(),
        GenericValue::Int(i) => i.to_string(),
        GenericValue::Decimal(d) => d.to_string(),
        GenericValue::String(s) => s.clone(),
        GenericValue::Null => "null".to_owned(),
        GenericValue::Array(_) | GenericValue::Map(_) => String::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use async_trait::async_trait;
    use futures::stream;
    use schema_core::{ColumnName, IndexName, TableName};
    use sources_core::RowKey;
    use sources_core::cdc::{Ack, AckSink};

    /// A source that replays a fixed list of changes once.
    #[derive(Debug)]
    struct MockSource {
        changes: Mutex<Option<Vec<Change>>>,
    }

    #[async_trait]
    impl ChangeCapture for MockSource {
        async fn live(
            &self,
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            let changes = self.changes.lock().unwrap().take().unwrap_or_default();
            Ok(Box::pin(stream::iter(
                changes
                    .into_iter()
                    .map(Ok::<Change, sources_core::SourceError>),
            )))
        }
    }

    /// Counts how many changes were confirmed.
    #[derive(Debug)]
    struct CountingAck(Arc<AtomicU64>);

    impl AckSink for CountingAck {
        fn confirm(&self, _seq: u64) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Resolves each change to one document; key value `2` builds a tombstone.
    #[derive(Debug)]
    struct MockDocuments;

    #[async_trait]
    impl DocumentBuilder for MockDocuments {
        async fn resolve(
            &self,
            _table: &TableName,
            key: &RowKey,
        ) -> sources_core::Result<Vec<DocumentId>> {
            Ok(vec![DocumentId {
                index: IndexName::try_new("users").unwrap(),
                key: key.clone(),
            }])
        }

        async fn build(&self, id: &DocumentId) -> sources_core::Result<Document> {
            let deleted = matches!(id.key.0.first(), Some((_, GenericValue::Int(2))));
            Ok(if deleted {
                Document::Delete { id: id.clone() }
            } else {
                Document::Upsert {
                    id: id.clone(),
                    body: GenericValue::Map(Default::default()),
                }
            })
        }

        fn backfill_scopes(&self) -> Vec<sources_core::document::IndexScope> {
            vec![sources_core::document::IndexScope {
                index: IndexName::try_new("users").unwrap(),
                root: SnapshotTable {
                    db_schema: schema_core::DatabaseSchema::try_new("public").unwrap(),
                    table: TableName::try_new("users").unwrap(),
                },
            }]
        }
    }

    /// Records the sink operations it receives.
    #[derive(Debug, Default)]
    struct RecordingSink {
        ops: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Sink for RecordingSink {
        async fn upsert(
            &self,
            index: &IndexName,
            id: &str,
            _document: &GenericValue,
        ) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("upsert {} {id}", index.as_ref()));
            Ok(())
        }

        async fn delete(&self, index: &IndexName, id: &str) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("delete {} {id}", index.as_ref()));
            Ok(())
        }

        async fn flush(&self, _caught_up: bool) -> sinks_core::Result<()> {
            Ok(())
        }
    }

    fn upsert_change(id: i64, seq: u64, acks: &Arc<AtomicU64>) -> Change {
        row_change(false, id, seq, acks)
    }

    fn delete_change(id: i64, seq: u64, acks: &Arc<AtomicU64>) -> Change {
        row_change(true, id, seq, acks)
    }

    fn row_change(delete: bool, id: i64, seq: u64, acks: &Arc<AtomicU64>) -> Change {
        let table = TableName::try_new("users").unwrap();
        let key = RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::Int(id),
        )]);
        let event = if delete {
            ChangeEvent::Delete { table, key }
        } else {
            ChangeEvent::Upsert { table, key }
        };
        Change {
            event,
            ack: Ack::new(seq, Arc::new(CountingAck(Arc::clone(acks)))),
        }
    }

    #[tokio::test]
    async fn drives_changes_to_the_sink_and_acks_each() {
        let acks = Arc::new(AtomicU64::new(0));
        let ops = Arc::new(Mutex::new(Vec::new()));

        let changes = vec![upsert_change(1, 0, &acks), delete_change(2, 1, &acks)];

        let engine = Engine::new(
            Arc::new(MockSource {
                changes: Mutex::new(Some(changes)),
            }),
            Arc::new(MockDocuments),
            Arc::new(RecordingSink {
                ops: Arc::clone(&ops),
            }),
        );
        engine.run().await.unwrap();

        assert_eq!(
            *ops.lock().unwrap(),
            vec!["upsert users 1".to_owned(), "delete users 2".to_owned()]
        );
        // Every change is confirmed.
        assert_eq!(acks.load(Ordering::SeqCst), 2);
    }

    /// Records every upsert/delete and each flush boundary in one ordered log,
    /// so a test can see how changes group into flushes.
    #[derive(Debug, Default)]
    struct FlushLogSink {
        ops: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Sink for FlushLogSink {
        async fn upsert(
            &self,
            index: &IndexName,
            id: &str,
            _document: &GenericValue,
        ) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("upsert {} {id}", index.as_ref()));
            Ok(())
        }

        async fn delete(&self, index: &IndexName, id: &str) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("delete {} {id}", index.as_ref()));
            Ok(())
        }

        async fn flush(&self, _caught_up: bool) -> sinks_core::Result<()> {
            self.ops.lock().unwrap().push("flush".to_owned());
            Ok(())
        }
    }

    #[tokio::test]
    async fn batches_changes_into_a_single_flush() {
        let acks = Arc::new(AtomicU64::new(0));
        let ops = Arc::new(Mutex::new(Vec::new()));
        let changes = (0..5)
            .map(|i| upsert_change(10 + i as i64, i, &acks))
            .collect::<Vec<_>>();

        Engine::new(
            Arc::new(MockSource {
                changes: Mutex::new(Some(changes)),
            }),
            Arc::new(MockDocuments),
            Arc::new(FlushLogSink {
                ops: Arc::clone(&ops),
            }),
        )
        // A wide window and a high cap so all five buffer into one batch; the
        // finite stream ends long before the delay could fire.
        .with_batch(BatchPolicy {
            max_changes: 256,
            max_delay: Duration::from_secs(10),
        })
        .skip_backfill(true)
        .run()
        .await
        .unwrap();

        assert_eq!(
            *ops.lock().unwrap(),
            vec![
                "upsert users 10".to_owned(),
                "upsert users 11".to_owned(),
                "upsert users 12".to_owned(),
                "upsert users 13".to_owned(),
                "upsert users 14".to_owned(),
                "flush".to_owned(),
            ],
            "all five changes batch into exactly one flush, after every upsert",
        );
        assert_eq!(
            acks.load(Ordering::SeqCst),
            5,
            "the whole batch is confirmed"
        );
    }

    /// Resolves every change to the *same* document id and counts how many
    /// times that document is assembled — so a test can show a batch builds a
    /// repeatedly-touched document once, not once per change.
    #[derive(Debug)]
    struct CountingBuilder {
        builds: Arc<AtomicU64>,
    }

    #[async_trait]
    impl DocumentBuilder for CountingBuilder {
        async fn resolve(
            &self,
            _table: &TableName,
            _key: &RowKey,
        ) -> sources_core::Result<Vec<DocumentId>> {
            Ok(vec![DocumentId {
                index: IndexName::try_new("users").unwrap(),
                key: RowKey(vec![(
                    ColumnName::try_new("id").unwrap(),
                    GenericValue::Int(1),
                )]),
            }])
        }

        async fn build(&self, id: &DocumentId) -> sources_core::Result<Document> {
            self.builds.fetch_add(1, Ordering::SeqCst);
            Ok(Document::Upsert {
                id: id.clone(),
                body: GenericValue::Map(Default::default()),
            })
        }
    }

    #[tokio::test]
    async fn builds_a_repeatedly_touched_document_once_per_batch() {
        let acks = Arc::new(AtomicU64::new(0));
        let builds = Arc::new(AtomicU64::new(0));
        let ops = Arc::new(Mutex::new(Vec::new()));
        // Three changes that all resolve to the same document id.
        let changes = (0..3)
            .map(|i| upsert_change(100 + i as i64, i, &acks))
            .collect::<Vec<_>>();

        Engine::new(
            Arc::new(MockSource {
                changes: Mutex::new(Some(changes)),
            }),
            Arc::new(CountingBuilder {
                builds: Arc::clone(&builds),
            }),
            Arc::new(FlushLogSink {
                ops: Arc::clone(&ops),
            }),
        )
        // One batch holds all three changes.
        .with_batch(BatchPolicy {
            max_changes: 256,
            max_delay: Duration::from_secs(10),
        })
        .skip_backfill(true)
        .run()
        .await
        .unwrap();

        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "the document is assembled once despite three changes touching it"
        );
        assert_eq!(
            *ops.lock().unwrap(),
            vec!["upsert users 1".to_owned(), "flush".to_owned()],
            "one upsert, one flush",
        );
        // Every change is still confirmed — dedup is on the build, not the ack.
        assert_eq!(acks.load(Ordering::SeqCst), 3);
    }

    /// Counts flushes; shares its counter with [`OrderingAck`] so a test can
    /// observe how many flushes had happened at the moment a seq was confirmed.
    #[derive(Debug)]
    struct FlushCountSink {
        flushes: Arc<AtomicU64>,
    }

    #[async_trait]
    impl Sink for FlushCountSink {
        async fn upsert(
            &self,
            _index: &IndexName,
            _id: &str,
            _document: &GenericValue,
        ) -> sinks_core::Result<()> {
            Ok(())
        }
        async fn delete(&self, _index: &IndexName, _id: &str) -> sinks_core::Result<()> {
            Ok(())
        }
        async fn flush(&self, _caught_up: bool) -> sinks_core::Result<()> {
            self.flushes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// On confirm, records the flush count observed at that instant — so a test
    /// can assert no change is confirmed before the flush that persisted it.
    #[derive(Debug)]
    struct OrderingAck {
        flushes: Arc<AtomicU64>,
        observed: Arc<Mutex<BTreeMap<u64, u64>>>,
    }

    impl AckSink for OrderingAck {
        fn confirm(&self, seq: u64) {
            let flushes_so_far = self.flushes.load(Ordering::SeqCst);
            self.observed.lock().unwrap().insert(seq, flushes_so_far);
        }
    }

    fn ordering_change(
        seq: u64,
        flushes: &Arc<AtomicU64>,
        observed: &Arc<Mutex<BTreeMap<u64, u64>>>,
    ) -> Change {
        let table = TableName::try_new("users").unwrap();
        let key = RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::Int(seq as i64 + 100),
        )]);
        Change {
            event: ChangeEvent::Upsert { table, key },
            ack: Ack::new(
                seq,
                Arc::new(OrderingAck {
                    flushes: Arc::clone(flushes),
                    observed: Arc::clone(observed),
                }),
            ),
        }
    }

    #[tokio::test]
    async fn confirms_no_ack_before_its_flush() {
        let flushes = Arc::new(AtomicU64::new(0));
        let observed = Arc::new(Mutex::new(BTreeMap::new()));
        let changes = (0..4)
            .map(|seq| ordering_change(seq, &flushes, &observed))
            .collect::<Vec<_>>();

        Engine::new(
            Arc::new(MockSource {
                changes: Mutex::new(Some(changes)),
            }),
            Arc::new(MockDocuments),
            Arc::new(FlushCountSink {
                flushes: Arc::clone(&flushes),
            }),
        )
        // Two per flush → two batches over four changes; the wide delay never
        // fires, so the split is deterministic.
        .with_batch(BatchPolicy {
            max_changes: 2,
            max_delay: Duration::from_secs(10),
        })
        .skip_backfill(true)
        .run()
        .await
        .unwrap();

        assert_eq!(
            flushes.load(Ordering::SeqCst),
            2,
            "four changes → two flushes of two"
        );
        let observed = observed.lock().unwrap();
        // A change in batch k (0-indexed) is confirmed only after k+1 flushes —
        // i.e. never before the flush that made its own documents durable.
        assert_eq!(observed.get(&0), Some(&1), "seq 0 confirmed after flush 1");
        assert_eq!(observed.get(&1), Some(&1), "seq 1 confirmed after flush 1");
        assert_eq!(observed.get(&2), Some(&2), "seq 2 confirmed after flush 2");
        assert_eq!(observed.get(&3), Some(&2), "seq 3 confirmed after flush 2");
    }

    /// A source whose `live` stream is empty (so `run` returns) and whose
    /// `snapshot` records that it was called, with what tables, and replays a
    /// fixed set of rows.
    #[derive(Debug)]
    struct SeedSource {
        rows: Mutex<Option<Vec<Change>>>,
        called: Arc<AtomicBool>,
        tables: Arc<Mutex<Vec<SnapshotTable>>>,
    }

    impl SeedSource {
        fn new(rows: Vec<Change>) -> Self {
            Self {
                rows: Mutex::new(Some(rows)),
                called: Arc::new(AtomicBool::new(false)),
                tables: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl ChangeCapture for SeedSource {
        async fn live(
            &self,
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            Ok(Box::pin(stream::empty()))
        }

        async fn snapshot(
            &self,
            tables: &[SnapshotTable],
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            self.called.store(true, Ordering::SeqCst);
            *self.tables.lock().unwrap() = tables.to_vec();
            let rows = self.rows.lock().unwrap().take().unwrap_or_default();
            Ok(Box::pin(stream::iter(
                rows.into_iter()
                    .map(Ok::<Change, sources_core::SourceError>),
            )))
        }
    }

    /// A sink that reports a fixed seeded-state, records `mark_seeded` calls, and
    /// records the upserts it receives.
    #[derive(Debug)]
    struct SeedSink {
        seeded: bool,
        marked: Arc<Mutex<Vec<String>>>,
        ops: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Sink for SeedSink {
        async fn upsert(
            &self,
            index: &IndexName,
            id: &str,
            _document: &GenericValue,
        ) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("upsert {} {id}", index.as_ref()));
            Ok(())
        }

        async fn delete(&self, index: &IndexName, id: &str) -> sinks_core::Result<()> {
            self.ops
                .lock()
                .unwrap()
                .push(format!("delete {} {id}", index.as_ref()));
            Ok(())
        }

        async fn flush(&self, _caught_up: bool) -> sinks_core::Result<()> {
            Ok(())
        }

        async fn is_seeded(&self, _index: &IndexName) -> sinks_core::Result<bool> {
            Ok(self.seeded)
        }

        async fn mark_seeded(&self, index: &IndexName) -> sinks_core::Result<()> {
            self.marked.lock().unwrap().push(index.as_ref().to_owned());
            Ok(())
        }
    }

    #[tokio::test]
    async fn seeds_an_unseeded_index_then_marks_it() {
        let acks = Arc::new(AtomicU64::new(0));
        let source = SeedSource::new(vec![upsert_change(1, 0, &acks), upsert_change(3, 1, &acks)]);
        let called = Arc::clone(&source.called);
        let tables = Arc::clone(&source.tables);
        let ops = Arc::new(Mutex::new(Vec::new()));
        let marked = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::new(SeedSink {
            seeded: false,
            marked: Arc::clone(&marked),
            ops: Arc::clone(&ops),
        });

        Engine::new(Arc::new(source), Arc::new(MockDocuments), sink)
            .run()
            .await
            .unwrap();

        assert!(
            called.load(Ordering::SeqCst),
            "snapshot should be requested"
        );
        // The index's root table is what gets snapshotted.
        let tables = tables.lock().unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables.first().unwrap().table.as_ref(), "users");
        // Snapshot rows are applied, and the index is marked seeded afterwards.
        assert_eq!(
            *ops.lock().unwrap(),
            vec!["upsert users 1".to_owned(), "upsert users 3".to_owned()]
        );
        assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
    }

    #[tokio::test]
    async fn skips_backfill_when_the_sink_reports_seeded() {
        let acks = Arc::new(AtomicU64::new(0));
        let source = SeedSource::new(vec![upsert_change(1, 0, &acks)]);
        let called = Arc::clone(&source.called);
        let ops = Arc::new(Mutex::new(Vec::new()));
        let marked = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::new(SeedSink {
            seeded: true,
            marked: Arc::clone(&marked),
            ops: Arc::clone(&ops),
        });

        Engine::new(Arc::new(source), Arc::new(MockDocuments), sink)
            .run()
            .await
            .unwrap();

        assert!(
            !called.load(Ordering::SeqCst),
            "a seeded index is not snapshotted"
        );
        assert!(ops.lock().unwrap().is_empty());
        assert!(marked.lock().unwrap().is_empty());
    }

    /// Records the observer events a run emits, so a test can assert the engine
    /// reports its lifecycle and per-batch progress.
    #[derive(Debug, Default)]
    struct RecordingObserver {
        indexes_ensured: AtomicU64,
        captured: AtomicU64,
        committed_changes: AtomicU64,
        committed_documents: AtomicU64,
        batches: AtomicU64,
        live: AtomicBool,
    }

    impl Observer for RecordingObserver {
        fn on_indexes_ensured(&self, count: usize) {
            self.indexes_ensured.store(count as u64, Ordering::SeqCst);
        }
        fn on_live_started(&self) {
            self.live.store(true, Ordering::SeqCst);
        }
        fn on_change_captured(&self) {
            self.captured.fetch_add(1, Ordering::SeqCst);
        }
        fn on_batch_committed(&self, stats: BatchStats) {
            self.committed_changes
                .fetch_add(stats.changes as u64, Ordering::SeqCst);
            self.committed_documents
                .fetch_add(stats.documents as u64, Ordering::SeqCst);
            self.batches.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn reports_lifecycle_and_progress_to_the_observer() {
        let acks = Arc::new(AtomicU64::new(0));
        let observer = Arc::new(RecordingObserver::default());
        // Five changes resolving to five distinct documents, one batch.
        let changes = (0..5)
            .map(|i| upsert_change(10 + i as i64, i, &acks))
            .collect::<Vec<_>>();

        Engine::new(
            Arc::new(MockSource {
                changes: Mutex::new(Some(changes)),
            }),
            Arc::new(MockDocuments),
            Arc::new(RecordingSink::default()),
        )
        .with_observer(Arc::clone(&observer) as Arc<dyn Observer>)
        .with_batch(BatchPolicy {
            max_changes: 256,
            max_delay: Duration::from_secs(10),
        })
        .skip_backfill(true)
        .run()
        .await
        .unwrap();

        assert!(observer.live.load(Ordering::SeqCst), "live phase reported");
        assert_eq!(observer.captured.load(Ordering::SeqCst), 5, "all captured");
        assert_eq!(observer.committed_changes.load(Ordering::SeqCst), 5);
        assert_eq!(observer.committed_documents.load(Ordering::SeqCst), 5);
        assert_eq!(observer.batches.load(Ordering::SeqCst), 1, "one batch");
    }

    #[tokio::test]
    async fn skip_backfill_flag_overrides_an_unseeded_index() {
        let acks = Arc::new(AtomicU64::new(0));
        let source = SeedSource::new(vec![upsert_change(1, 0, &acks)]);
        let called = Arc::clone(&source.called);
        let ops = Arc::new(Mutex::new(Vec::new()));
        let marked = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::new(SeedSink {
            seeded: false,
            marked: Arc::clone(&marked),
            ops: Arc::clone(&ops),
        });

        Engine::new(Arc::new(source), Arc::new(MockDocuments), sink)
            .skip_backfill(true)
            .run()
            .await
            .unwrap();

        assert!(
            !called.load(Ordering::SeqCst),
            "skip_backfill suppresses the snapshot"
        );
        assert!(ops.lock().unwrap().is_empty());
        assert!(marked.lock().unwrap().is_empty());
    }

    /// One write staged in the sink between flushes.
    #[derive(Debug)]
    enum CrashOp {
        Upsert(String, GenericValue),
        Delete(String),
    }

    /// A durable store behind a staging buffer — OpenSearch behind its bulk
    /// buffer. `upsert`/`delete` stage an op; `flush` applies the staged ops to
    /// the durable `store` atomically. A sink built to fail returns an error
    /// from its first `flush` *without* touching the store, reproducing a crash
    /// in the window after writes are buffered but before they are durable —
    /// exactly what at-least-once delivery must survive.
    ///
    /// `store` is shared across runs on purpose: a flusso restart points at the
    /// same destination, so what survived the crash is what the next run sees.
    #[derive(Debug)]
    struct CrashSink {
        store: Arc<Mutex<BTreeMap<String, GenericValue>>>,
        staging: Mutex<Vec<CrashOp>>,
        fail_next_flush: AtomicBool,
    }

    impl CrashSink {
        fn new(store: Arc<Mutex<BTreeMap<String, GenericValue>>>, fail_first_flush: bool) -> Self {
            Self {
                store,
                staging: Mutex::new(Vec::new()),
                fail_next_flush: AtomicBool::new(fail_first_flush),
            }
        }
    }

    /// The store key the engine's deterministic `_id` maps to within an index.
    fn doc_key(index: &IndexName, id: &str) -> String {
        format!("{}/{id}", index.as_ref())
    }

    #[async_trait]
    impl Sink for CrashSink {
        async fn upsert(
            &self,
            index: &IndexName,
            id: &str,
            document: &GenericValue,
        ) -> sinks_core::Result<()> {
            self.staging
                .lock()
                .unwrap()
                .push(CrashOp::Upsert(doc_key(index, id), document.clone()));
            Ok(())
        }

        async fn delete(&self, index: &IndexName, id: &str) -> sinks_core::Result<()> {
            self.staging
                .lock()
                .unwrap()
                .push(CrashOp::Delete(doc_key(index, id)));
            Ok(())
        }

        async fn flush(&self, _caught_up: bool) -> sinks_core::Result<()> {
            if self.fail_next_flush.swap(false, Ordering::SeqCst) {
                // Crash before durability: the staged ops never reach the store.
                return Err(sinks_core::SinkError::Write(
                    "simulated crash before flush completed".to_owned(),
                ));
            }
            let mut store = self.store.lock().unwrap();
            for op in self.staging.lock().unwrap().drain(..) {
                match op {
                    CrashOp::Upsert(key, body) => {
                        store.insert(key, body);
                    }
                    CrashOp::Delete(key) => {
                        store.remove(&key);
                    }
                }
            }
            Ok(())
        }
    }

    /// The at-least-once guarantee end to end: a crash in the window *after* a
    /// batch's documents are buffered but *before* the flush that makes them
    /// durable must lose nothing. The slot never advances over an unconfirmed
    /// change, so the source redelivers the whole batch on restart, and rebuilding
    /// from the current row by deterministic id re-applies it idempotently — the
    /// durable state ends identical to a single clean run. This is the durability
    /// counterpart to `confirms_no_ack_before_its_flush`, which guards the
    /// ack-ordering half of the same invariant.
    #[tokio::test]
    async fn redelivers_and_reapplies_idempotently_after_a_crash_before_flush() {
        let store: Arc<Mutex<BTreeMap<String, GenericValue>>> =
            Arc::new(Mutex::new(BTreeMap::new()));
        let acks = Arc::new(AtomicU64::new(0));
        // A wide window and a high cap so both changes buffer into one batch and
        // commit in a single flush — the one the first run crashes on.
        let batch = BatchPolicy {
            max_changes: 256,
            max_delay: Duration::from_secs(10),
        };

        // Run 1: two changes are delivered and buffered, but the sole flush
        // crashes — so nothing lands durably and nothing is confirmed.
        let run1 = Engine::new(
            Arc::new(MockSource {
                changes: Mutex::new(Some(vec![
                    upsert_change(1, 0, &acks),
                    upsert_change(3, 1, &acks),
                ])),
            }),
            Arc::new(MockDocuments),
            Arc::new(CrashSink::new(Arc::clone(&store), true)),
        )
        .with_batch(batch)
        .skip_backfill(true)
        .run()
        .await;

        assert!(run1.is_err(), "the crashing flush stops the run");
        assert!(
            store.lock().unwrap().is_empty(),
            "a crash before the flush completes leaves nothing durable"
        );
        assert_eq!(
            acks.load(Ordering::SeqCst),
            0,
            "no change is confirmed when the flush that would persist it never completed"
        );

        // Run 2: nothing was confirmed, so the slot never advanced and the source
        // redelivers the same changes. This run's flush succeeds.
        Engine::new(
            Arc::new(MockSource {
                changes: Mutex::new(Some(vec![
                    upsert_change(1, 0, &acks),
                    upsert_change(3, 1, &acks),
                ])),
            }),
            Arc::new(MockDocuments),
            Arc::new(CrashSink::new(Arc::clone(&store), false)),
        )
        .with_batch(batch)
        .skip_backfill(true)
        .run()
        .await
        .unwrap();

        // The redelivered batch lands exactly once, by deterministic id —
        // identical to what a single clean run would have produced.
        let store = store.lock().unwrap();
        assert_eq!(
            store.keys().cloned().collect::<Vec<_>>(),
            vec!["users/1".to_owned(), "users/3".to_owned()],
            "both documents are durable exactly once after replay — no loss, no duplicate"
        );
        assert_eq!(
            acks.load(Ordering::SeqCst),
            2,
            "every redelivered change is confirmed once its flush completes"
        );
    }

    /// Records the `caught_up` flag of every flush, so a test can assert the
    /// engine derives it from the queue and forwards it to the sink.
    #[derive(Debug, Default)]
    struct CaughtUpSink {
        flushes: Arc<Mutex<Vec<bool>>>,
    }

    #[async_trait]
    impl Sink for CaughtUpSink {
        async fn upsert(
            &self,
            _index: &IndexName,
            _id: &str,
            _document: &GenericValue,
        ) -> sinks_core::Result<()> {
            Ok(())
        }
        async fn delete(&self, _index: &IndexName, _id: &str) -> sinks_core::Result<()> {
            Ok(())
        }
        async fn flush(&self, caught_up: bool) -> sinks_core::Result<()> {
            self.flushes.lock().unwrap().push(caught_up);
            Ok(())
        }
    }

    #[tokio::test]
    async fn caught_up_is_false_while_a_backlog_drains_then_true_on_the_last_batch() {
        let acks = Arc::new(AtomicU64::new(0));
        let flushes = Arc::new(Mutex::new(Vec::new()));
        let documents = MockDocuments;
        let sink = CaughtUpSink {
            flushes: Arc::clone(&flushes),
        };
        let observer: Arc<dyn Observer> = Arc::new(NoopObserver);

        // Pre-fill the queue and close it, so the worker sees a fixed backlog
        // with no concurrent capture racing — making the caught-up sequence
        // deterministic. Five changes in batches of two drain as [2, 2, 1]; only
        // the final batch empties the queue.
        let (producer, mut consumer) = channel::<Change>(16);
        for seq in 0..5 {
            producer
                .publish(upsert_change(seq as i64, seq, &acks))
                .await
                .unwrap();
        }
        drop(producer);

        let pipeline = Pipeline {
            documents: &documents,
            sink: &sink,
            observer: &observer,
            queue_capacity: 16,
            batch: BatchPolicy {
                max_changes: 2,
                // Wide window: only the closed-and-drained queue ends a batch
                // early, never the timer — so the split is purely backlog-driven.
                max_delay: Duration::from_secs(10),
            },
        };
        work(pipeline, &mut consumer, None).await.unwrap();

        assert_eq!(
            flushes.lock().unwrap().as_slice(),
            &[false, false, true],
            "a flush is caught up only once it has drained the queue behind it",
        );
    }
}
