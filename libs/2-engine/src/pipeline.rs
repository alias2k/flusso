//! The pipeline execution: the borrowed run-context ([`Pipeline`]), the
//! top-level orchestration ([`run_inner`] → ensure-index → backfill → live), and
//! the capture/worker machinery (queue draining, batch buffering, flush-then-ack
//! commit). [`Engine`](crate::Engine) constructs a [`Pipeline`] and drives it
//! through [`run_inner`]; the invariants this upholds are documented on the
//! [crate root](crate).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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

use crate::error::{EngineError, Result};
use crate::observer::{BatchStats, Observer};
use crate::policy::{BatchPolicy, FailurePolicies, FailurePolicy};

/// The run-constant borrowed parts threaded through the pipeline's inner
/// functions, so each takes a single context instead of repeating the same five
/// arguments. `Copy` (it is only references plus two small values), so it is
/// passed by value.
#[derive(Clone, Copy)]
pub(crate) struct Pipeline<'a> {
    pub(crate) documents: &'a dyn DocumentBuilder,
    pub(crate) sink: &'a dyn Sink,
    pub(crate) observer: &'a Arc<dyn Observer>,
    pub(crate) queue_capacity: usize,
    pub(crate) batch: BatchPolicy,
    pub(crate) failure_policies: &'a FailurePolicies,
}

/// The body of [`Engine::run`](crate::Engine::run), over borrowed parts, so
/// `run` can report any error to the observer after the borrow ends.
pub(crate) async fn run_inner(
    pipeline: Pipeline<'_>,
    source: &dyn ChangeCapture,
    skip_backfill: bool,
) -> Result<()> {
    // ensure_index runs on every start, before any document flows — idempotent
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

    // The observer's events are `&self`, so capture takes an
    // `Arc` clone it can move into the spawned `'static` task. The guard aborts
    // it if this future is *cancelled* (e.g. dropped for an on-demand reindex
    // restart) — otherwise a capture parked on an idle source stream would
    // linger, holding the source connection / replication slot and blocking the
    // next run from acquiring it. On the normal path the handle is taken out and
    // joined below, so the guard's drop is a no-op.
    let mut capture = CaptureGuard(Some(tokio::spawn(capture(
        stream,
        producer,
        Arc::clone(pipeline.observer),
    ))));
    let worker = work(pipeline, &mut consumer, filter).await;

    let captured = match capture.0.take() {
        Some(handle) => {
            handle.abort();
            handle.await
        }
        None => Ok(Ok(())),
    };
    worker?;
    match captured {
        Ok(result) => result,
        Err(join) if join.is_cancelled() => Ok(()),
        Err(join) => Err(EngineError::Task(join.to_string())),
    }
}

/// Aborts the spawned capture task when dropped, so cancelling [`pump`] (e.g.
/// dropping the engine future for a reindex restart) doesn't leave it running
/// and holding the source's replication slot. On the normal path the handle is
/// `take`n out and joined, leaving the guard empty so its drop is a no-op.
#[derive(Debug)]
struct CaptureGuard(Option<tokio::task::JoinHandle<Result<()>>>);

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        if let Some(handle) = &self.0 {
            handle.abort();
        }
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
pub(crate) async fn work(
    pipeline: Pipeline<'_>,
    consumer: &mut ChannelConsumer<Change>,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    let batch = pipeline.batch;
    // Source acks and queue handles for changes whose documents are buffered in
    // the sink but not yet flushed. Confirmed/acked only after the flush below.
    let mut pending: Batch = Batch::with_capacity(batch.max_changes);

    'batches: loop {
        let Some(delivery) = consumer.recv().await? else {
            break;
        };
        buffer(delivery, pipeline.documents, filter, &mut pending).await?;

        // `recv` is cancel-safe, so dropping it on timeout never drops a queued
        // change.
        let deadline = Instant::now() + batch.max_delay;
        while pending.len() < batch.max_changes {
            match timeout_at(deadline, consumer.recv()).await {
                Err(_elapsed) => break,
                Ok(Ok(Some(delivery))) => {
                    buffer(delivery, pipeline.documents, filter, &mut pending).await?;
                }
                Ok(Ok(None)) => {
                    commit(pipeline, &mut pending, consumer.is_empty()).await?;
                    break 'batches;
                }
                Ok(Err(queue_err)) => return Err(queue_err.into()),
            }
        }
        commit(pipeline, &mut pending, consumer.is_empty()).await?;
    }
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
/// [`Ack`]) — so confirming a batch advances the slot to
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
    let report = pipeline.sink.flush(caught_up).await?;
    let flush = flush_start.elapsed();

    // A single `stop`-policy rejection stops the run
    // — and we must decide that *before* emitting any quarantine event, so a
    // `skip` document in the same batch isn't double-counted when the
    // unconfirmed batch is redelivered on the next run.
    if !report.is_clean() {
        let mut stop_count = 0usize;
        let mut stop_example = String::new();
        for doc in &report.rejected {
            if pipeline.failure_policies.resolve(&doc.index) == FailurePolicy::Stop {
                if stop_count == 0 {
                    stop_example = format!("{}/{}: {}", doc.index, doc.id, doc.reason);
                }
                stop_count += 1;
            }
        }
        if stop_count > 0 {
            return Err(EngineError::DocumentsRejected(stop_count, stop_example));
        }
        for doc in &report.rejected {
            tracing::warn!(
                index = %doc.index,
                id = %doc.id,
                reason = %doc.reason,
                "document rejected by sink; quarantining and continuing",
            );
            pipeline
                .observer
                .on_document_quarantined(&doc.index, &doc.id, &doc.reason);
        }
    }

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
        GenericValue::SmallInt(i) => i.to_string(),
        GenericValue::Int(i) => i.to_string(),
        GenericValue::BigInt(i) => i.to_string(),
        GenericValue::Float(f) => f.to_string(),
        GenericValue::Double(f) => f.to_string(),
        GenericValue::Decimal(d) => d.to_string(),
        GenericValue::String(s) => s.clone(),
        GenericValue::Uuid(u) => u.to_string(),
        GenericValue::Date(d) => d.to_string(),
        GenericValue::Time(t) => t.to_string(),
        GenericValue::Timestamp(ts) => ts.to_string(),
        GenericValue::TimestampTz(ts) => ts.to_rfc3339(),
        // `\x`-prefixed lowercase hex, matching Postgres's `bytea` text output,
        // so a snapshot key and a WAL key for the same row agree.
        GenericValue::Bytes(bytes) => {
            let mut out = String::with_capacity(2 + bytes.len() * 2);
            out.push_str("\\x");
            for byte in bytes {
                out.push_str(&format!("{byte:02x}"));
            }
            out
        }
        GenericValue::Null => "null".to_owned(),
        GenericValue::Array(_) | GenericValue::Map(_) => String::new(),
    }
}
