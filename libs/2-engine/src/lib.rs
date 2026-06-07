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

pub use error::*;

use std::collections::HashSet;
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
    source: Box<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    queue_capacity: usize,
    batch: BatchPolicy,
    skip_backfill: bool,
}

impl Engine {
    /// Assemble an engine from its pluggable parts.
    pub fn new(
        source: Box<dyn ChangeCapture>,
        documents: Arc<dyn DocumentBuilder>,
        sink: Arc<dyn Sink>,
    ) -> Self {
        Self {
            source,
            documents,
            sink,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            batch: BatchPolicy::default(),
            skip_backfill: false,
        }
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
            queue_capacity,
            batch,
            skip_backfill,
        } = self;

        // Enrich the thin config into fully-typed mappings: the source fills the
        // gaps a human config leaves (field types, nullability) from what it
        // knows about its store. This runs by design on every start, before any
        // document flows, so the destination is created from a complete
        // description rather than guessing on first write — idempotent
        // (create-if-absent), so it is safe across resumes and backfills alike.
        let mappings = documents.index_mappings().await?;
        tracing::info!(indexes = mappings.len(), "ensuring target indexes");
        for mapping in &mappings {
            sink.ensure_index(mapping).await?;
        }

        if skip_backfill {
            tracing::info!("skipping backfill (skip_backfill set)");
        } else {
            backfill(
                source.as_ref(),
                documents.as_ref(),
                sink.as_ref(),
                queue_capacity,
                batch,
            )
            .await?;
        }

        let stream = source.live().await?;
        tracing::info!("following live changes");
        let result = pump(
            stream,
            documents.as_ref(),
            sink.as_ref(),
            queue_capacity,
            batch,
            None,
        )
        .await;
        match &result {
            Ok(()) => tracing::info!("pipeline stopped: live stream ended"),
            Err(error) => tracing::error!(%error, "pipeline stopped on error"),
        }
        result
    }
}

/// Seed every index the sink reports as unseeded, then mark them seeded.
///
/// The decision "does this index need a backfill?" is the **sink**'s — the
/// destination is what knows whether it already holds the data. For the indexes
/// that do, the source snapshots their root tables and the snapshot is applied
/// scoped to just those indexes, so an already-seeded index sharing a table is
/// never rewritten.
#[tracing::instrument(name = "backfill", skip_all)]
async fn backfill(
    source: &dyn ChangeCapture,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    queue_capacity: usize,
    batch: BatchPolicy,
) -> Result<()> {
    let mut seeding: HashSet<IndexName> = HashSet::new();
    let mut tables: Vec<SnapshotTable> = Vec::new();
    for scope in documents.backfill_scopes() {
        if sink.is_seeded(&scope.index).await? {
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
    let stream = source.snapshot(&tables).await?;
    pump(
        stream,
        documents,
        sink,
        queue_capacity,
        batch,
        Some(&seeding),
    )
    .await?;

    // The snapshot is fully applied and flushed once `pump` returns; record each
    // index as seeded so a later run skips it.
    for index in &seeding {
        sink.mark_seeded(index).await?;
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
    stream: BoxStream<'static, sources_core::Result<Change>>,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    queue_capacity: usize,
    batch: BatchPolicy,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    let (producer, mut consumer) = channel::<Change>(queue_capacity);

    // Capture runs concurrently with the worker; the worker borrows the shared
    // builder and sink.
    let capture = tokio::spawn(capture(stream, producer));
    let worker = work(&mut consumer, documents, sink, batch, filter).await;

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
) -> Result<()> {
    let mut captured = 0u64;
    while let Some(change) = stream.next().await {
        producer.publish(change?).await?;
        captured += 1;
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
#[tracing::instrument(name = "worker", skip_all, fields(max_changes = batch.max_changes))]
async fn work(
    consumer: &mut ChannelConsumer<Change>,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    batch: BatchPolicy,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    // Source acks and queue handles for changes whose documents are buffered in
    // the sink but not yet flushed. Confirmed/acked only after the flush below.
    let mut pending: Batch = Batch::with_capacity(batch.max_changes);

    'batches: loop {
        // Block for the batch's first change (no busy-wait on an idle stream).
        let Some(delivery) = consumer.recv().await? else {
            break;
        };
        buffer(delivery, documents, filter, &mut pending).await?;

        // Fill the batch until it is full or its time window closes. `recv` is
        // cancel-safe, so dropping it on timeout never drops a queued change.
        let deadline = Instant::now() + batch.max_delay;
        while pending.len() < batch.max_changes {
            match timeout_at(deadline, consumer.recv()).await {
                Err(_elapsed) => break, // window closed: flush a partial batch
                Ok(Ok(Some(delivery))) => {
                    buffer(delivery, documents, filter, &mut pending).await?;
                }
                Ok(Ok(None)) => {
                    // Stream ended: flush what we have, then stop.
                    commit(documents, sink, &mut pending).await?;
                    break 'batches;
                }
                Ok(Err(queue_err)) => return Err(queue_err.into()),
            }
        }
        commit(documents, sink, &mut pending).await?;
    }
    // A batch left buffered when the stream ended mid-fill.
    commit(documents, sink, &mut pending).await
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
#[tracing::instrument(name = "commit", level = "debug", skip_all, fields(changes = pending.len(), documents = pending.ids.len()))]
async fn commit(
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    pending: &mut Batch,
) -> Result<()> {
    if pending.len() == 0 {
        return Ok(());
    }
    for document in documents.build_many(&pending.ids).await? {
        match document {
            Document::Upsert { id, body } => {
                sink.upsert(&id.index, &document_id(&id), &body).await?;
            }
            Document::Delete { id } => {
                sink.delete(&id.index, &document_id(&id)).await?;
            }
        }
    }
    sink.flush().await?;
    for ack in pending.source.drain(..) {
        ack.confirm();
    }
    for handle in pending.handles.drain(..) {
        handle.ack().await?;
    }
    pending.ids.clear();
    pending.seen.clear();
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

        async fn flush(&self) -> sinks_core::Result<()> {
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
            Box::new(MockSource {
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

        async fn flush(&self) -> sinks_core::Result<()> {
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
            Box::new(MockSource {
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
            Box::new(MockSource {
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
        async fn flush(&self) -> sinks_core::Result<()> {
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
            Box::new(MockSource {
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

        async fn flush(&self) -> sinks_core::Result<()> {
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

        Engine::new(Box::new(source), Arc::new(MockDocuments), sink)
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

        Engine::new(Box::new(source), Arc::new(MockDocuments), sink)
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

        Engine::new(Box::new(source), Arc::new(MockDocuments), sink)
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
}
