//! The ingest engine: capture → batch → resolve → build once → publish to every
//! lane, plus the request lane it serves snapshots from.
//!
//! One task, one build path. Live changes are buffered per [`BatchPolicy`],
//! resolved to the documents they touch (deduplicated), built once with
//! [`DocumentBuilder::build_many`], and published as one [`Batch`] to every
//! lane, carrying the position of the last change. Snapshot rows for a
//! backfill flow through the same resolve → build path but are published only
//! to the lanes that requested them, without a position, followed by a
//! [`LaneItem::SnapshotComplete`]. Both kinds interleave on this single task,
//! so lane order is build order and a document's later message is always the
//! newer state.
//!
//! After every commit the engine reads the stream's watermark — the lowest
//! position every lane has acknowledged — and hands it to the source as
//! confirmation. That is the at-least-once guarantee: the resume point advances
//! only past what *every* sink has made durable.
//!
//! Requests are at-least-once too: a `Backfill` is acknowledged only after its
//! `SnapshotComplete` is published, and concurrent requests for the same index
//! coalesce into one snapshot fanned to every requesting lane.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use futures::StreamExt;
use futures::stream::BoxStream;
use kernel::{Envelope, GenericValue, IndexName, Position, SinkName};
use source::cdc::{ChangeCapture, ChangeEvent, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{SnapshotTable, SourceError};
use stream::{AckHandle, Batch, Consumer, LaneItem, Producer, Request, Stream};
use tokio::time::{Duration, Instant, sleep_until, timeout};

use crate::error::{EngineError, Result};
use crate::observer::{BuildStats, EngineId, NoopObserver, Observer};
use crate::policy::BatchPolicy;

/// How often the watermark is confirmed to the source while no commit happens,
/// so acknowledgements that arrive during a quiet period still advance it.
const CONFIRM_TICK: Duration = Duration::from_secs(1);

/// The ingest engine over one source, one document builder, and one stream.
#[derive(Debug)]
pub struct IngestEngine {
    source: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    stream: Arc<dyn Stream>,
    sinks: Vec<SinkName>,
    observer: Arc<dyn Observer>,
    batch: BatchPolicy,
}

impl IngestEngine {
    /// An engine publishing to the lanes of `sinks` on `stream`.
    pub fn new(
        source: Arc<dyn ChangeCapture>,
        documents: Arc<dyn DocumentBuilder>,
        stream: Arc<dyn Stream>,
        sinks: Vec<SinkName>,
    ) -> Self {
        Self {
            source,
            documents,
            stream,
            sinks,
            observer: Arc::new(NoopObserver),
            batch: BatchPolicy::default(),
        }
    }

    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    pub fn with_batch(mut self, batch: BatchPolicy) -> Self {
        self.batch = BatchPolicy {
            max_changes: batch.max_changes.max(1),
            ..batch
        };
        self
    }

    /// Prepare the source, open the live stream, and run until it ends (`Ok`)
    /// or a source, builder, or stream error stops the engine (`Err`).
    ///
    /// The daemon calls this only after every sink engine has staged, so
    /// [`ChangeCapture::prepare`] runs after any stale-seed rebuild was staged
    /// and before any snapshot (the #120 ordering).
    #[tracing::instrument(name = "ingest.run", skip_all, fields(max_changes = self.batch.max_changes))]
    pub async fn run(&self) -> Result<()> {
        let result = self.run_inner().await;
        match &result {
            Ok(()) => {
                tracing::info!("ingest engine stopped: live stream ended");
                self.observer.on_engine_stopped(&EngineId::Ingest);
            }
            Err(error) => {
                tracing::error!(%error, "ingest engine stopped on error");
                self.observer
                    .on_engine_error(&EngineId::Ingest, &error.to_string());
            }
        }
        result
    }

    async fn run_inner(&self) -> Result<()> {
        self.source.prepare().await?;
        let lanes: Vec<(SinkName, Box<dyn Producer<LaneItem>>)> = self
            .sinks
            .iter()
            .map(|sink| Ok((sink.clone(), self.stream.lane(sink)?.producer)))
            .collect::<Result<_>>()?;
        let mut requests = self.stream.requests()?.consumer;
        let scopes: HashMap<IndexName, SnapshotTable> = self
            .documents
            .backfill_scopes()
            .into_iter()
            .map(|scope| (scope.index, scope.root))
            .collect();

        let mut live = self.source.live().await?;
        tracing::info!("following live changes");
        self.observer.on_live_started();

        let mut pending = LiveBatch::new(self.batch.max_changes);
        let mut snapshot: Option<Snapshot> = None;
        let mut last_confirmed: Option<Position> = None;
        let mut confirm_tick = tokio::time::interval(CONFIRM_TICK);
        let mut live_ended = false;

        loop {
            // A live stream that ends while a snapshot runs or requests wait
            // (a sink staged its backfill before this engine started) still
            // gets those served; the run ends once nothing is left to do.
            if live_ended && snapshot.is_none() && requests.is_empty() {
                self.commit_live(&mut pending, &lanes).await?;
                self.confirm(&mut last_confirmed);
                return Ok(());
            }
            let deadline = pending.deadline.unwrap_or_else(far_future);
            tokio::select! {
                change = live.next(), if !live_ended => match change {
                    None => live_ended = true,
                    Some(Err(error)) => return Err(error.into()),
                    Some(Ok((position, event))) => {
                        self.buffer_live(&mut pending, position, event).await?;
                        if pending.is_full() {
                            self.commit_live(&mut pending, &lanes).await?;
                            self.confirm(&mut last_confirmed);
                        }
                    }
                },
                () = sleep_until(deadline), if pending.deadline.is_some() => {
                    self.commit_live(&mut pending, &lanes).await?;
                    self.confirm(&mut last_confirmed);
                }
                row = next_snapshot_row(&mut snapshot), if snapshot.is_some() => {
                    let Some(active) = snapshot.as_mut() else { continue };
                    match row {
                        Some(Err(error)) => return Err(error.into()),
                        Some(Ok(event)) => {
                            self.buffer_snapshot(active, event).await?;
                            if active.batch.is_full() {
                                self.commit_snapshot(active, &lanes).await?;
                            }
                        }
                        None => {
                            self.commit_snapshot(active, &lanes).await?;
                            self.finish_snapshot(active, &lanes).await?;
                            snapshot = None;
                        }
                    }
                },
                request = requests.recv(), if snapshot.is_none() => match request? {
                    None => {
                        tracing::warn!("request lane closed; no further backfills can be served");
                        requests = Box::new(ClosedConsumer);
                    }
                    Some(first) => {
                        let mut deliveries = vec![first];
                        while let Ok(Ok(Some(more))) =
                            timeout(self.batch.max_delay, requests.recv()).await
                        {
                            deliveries.push(more);
                        }
                        snapshot = self.start_snapshot(deliveries, &scopes, &lanes).await?;
                    }
                },
                _ = confirm_tick.tick() => self.confirm(&mut last_confirmed),
            }
        }
    }

    /// Resolve one live change into the batch: the documents it touches,
    /// deduplicated, and its position.
    async fn buffer_live(
        &self,
        pending: &mut LiveBatch,
        position: Position,
        event: ChangeEvent,
    ) -> Result<()> {
        self.observer.on_change_captured();
        let affected = self.documents.resolve(event.table(), event.key()).await?;
        tracing::trace!(documents = affected.len(), "change resolved to documents");
        for id in affected {
            if pending.seen.insert(id.clone()) {
                pending.ids.push(id);
            }
        }
        pending.changes += 1;
        pending.position = Some(position);
        if pending.deadline.is_none() {
            pending.deadline = Some(Instant::now() + self.batch.max_delay);
        }
        Ok(())
    }

    /// Build the batch's documents once and publish them to every lane with
    /// the batch's position. A batch that resolved to no document is still
    /// published (empty), so the lanes acknowledge its position.
    #[tracing::instrument(name = "ingest.commit", level = "debug", skip_all, fields(changes = pending.changes, documents = pending.ids.len()))]
    async fn commit_live(
        &self,
        pending: &mut LiveBatch,
        lanes: &[(SinkName, Box<dyn Producer<LaneItem>>)],
    ) -> Result<()> {
        if pending.changes == 0 {
            return Ok(());
        }
        let started = Instant::now();
        let (envelopes, by_index) = self.build(&pending.ids, pending.position).await?;
        let stats = BuildStats {
            changes: pending.changes,
            documents: envelopes.len(),
            documents_by_index: by_index.into_iter().collect(),
            build: started.elapsed(),
        };
        let item = LaneItem::Batch(Batch {
            position: pending.position,
            changes: pending.changes,
            envelopes,
        });
        for (_, producer) in lanes {
            producer.publish(item.clone()).await?;
        }
        pending.clear();
        self.observer.on_batch_built(stats);
        tracing::debug!("batch built and published");
        Ok(())
    }

    /// Build `ids` into envelopes tagged with `position`.
    async fn build(
        &self,
        ids: &[DocumentId],
        position: Option<Position>,
    ) -> Result<(Vec<Envelope>, BTreeMap<IndexName, usize>)> {
        let mut by_index: BTreeMap<IndexName, usize> = BTreeMap::new();
        let mut envelopes = Vec::with_capacity(ids.len());
        let ts = Utc::now();
        for document in self.documents.build_many(ids).await? {
            let id = document.id().clone();
            *by_index.entry(id.index.clone()).or_insert(0) += 1;
            envelopes.push(match document {
                Document::Upsert { id, body } => {
                    Envelope::upsert(id.index.clone(), document_id(&id), body, position, ts)
                }
                Document::Delete { id } => {
                    Envelope::delete(id.index.clone(), document_id(&id), position, ts)
                }
            });
        }
        Ok((envelopes, by_index))
    }

    /// Hand the stream's watermark to the source when it moved.
    fn confirm(&self, last_confirmed: &mut Option<Position>) {
        if let Some(watermark) = self.stream.watermark()
            && *last_confirmed != Some(watermark)
        {
            self.source.confirm(watermark);
            *last_confirmed = Some(watermark);
        }
    }

    /// Coalesce the delivered requests into one snapshot: the union of their
    /// indexes' root tables, remembering which lane asked for which indexes.
    /// Requests for indexes the builder knows nothing about are answered with
    /// an immediate `SnapshotComplete` for whatever *is* known.
    async fn start_snapshot(
        &self,
        deliveries: Vec<stream::Delivery<Request>>,
        scopes: &HashMap<IndexName, SnapshotTable>,
        lanes: &[(SinkName, Box<dyn Producer<LaneItem>>)],
    ) -> Result<Option<Snapshot>> {
        let mut requested: BTreeMap<SinkName, BTreeSet<IndexName>> = BTreeMap::new();
        let mut handles = Vec::with_capacity(deliveries.len());
        for delivery in deliveries {
            let (request, handle) = delivery.into_parts();
            let Request::Backfill { sink, indexes } = request;
            let known: BTreeSet<IndexName> = indexes
                .into_iter()
                .filter(|index| {
                    let known = scopes.contains_key(index);
                    if !known {
                        tracing::warn!(index = %index, %sink, "backfill requested for an index the source does not build; ignoring");
                    }
                    known
                })
                .collect();
            requested.entry(sink).or_default().extend(known);
            handles.push(handle);
        }
        let indexes: BTreeSet<IndexName> = requested.values().flatten().cloned().collect();
        let mut tables: Vec<SnapshotTable> = Vec::new();
        for index in &indexes {
            if let Some(root) = scopes.get(index)
                && !tables.contains(root)
            {
                tables.push(root.clone());
            }
        }
        let mut snapshot = Snapshot {
            stream: Box::pin(futures::stream::empty()),
            requested,
            indexes: indexes.iter().cloned().collect(),
            handles,
            batch: SnapshotBatch::new(self.batch.max_changes),
        };
        if tables.is_empty() {
            self.finish_snapshot(&mut snapshot, lanes).await?;
            return Ok(None);
        }
        let ordered: Vec<IndexName> = indexes.into_iter().collect();
        tracing::info!(
            indexes = ordered.len(),
            tables = tables.len(),
            lanes = snapshot.requested.len(),
            "starting snapshot"
        );
        self.observer.on_snapshot_started(&ordered);
        snapshot.stream = self.source.snapshot(&tables).await?;
        Ok(Some(snapshot))
    }

    async fn buffer_snapshot(&self, snapshot: &mut Snapshot, event: ChangeEvent) -> Result<()> {
        let affected = self.documents.resolve(event.table(), event.key()).await?;
        for id in affected {
            if snapshot.indexes.contains(&id.index) && snapshot.batch.seen.insert(id.clone()) {
                snapshot.batch.ids.push(id);
            }
        }
        snapshot.batch.rows += 1;
        Ok(())
    }

    /// Build the snapshot batch once and publish each lane the slice of it that
    /// lane requested, with no position.
    async fn commit_snapshot(
        &self,
        snapshot: &mut Snapshot,
        lanes: &[(SinkName, Box<dyn Producer<LaneItem>>)],
    ) -> Result<()> {
        if snapshot.batch.rows == 0 {
            return Ok(());
        }
        let started = Instant::now();
        let (envelopes, by_index) = self.build(&snapshot.batch.ids, None).await?;
        for (sink, indexes) in &snapshot.requested {
            let slice: Vec<Envelope> = envelopes
                .iter()
                .filter(|envelope| indexes.contains(&envelope.index))
                .cloned()
                .collect();
            if slice.is_empty() {
                continue;
            }
            let Some((_, producer)) = lanes.iter().find(|(name, _)| name == sink) else {
                tracing::warn!(%sink, "snapshot requested by a sink with no lane; dropping its slice");
                continue;
            };
            producer
                .publish(LaneItem::Batch(Batch {
                    position: None,
                    changes: 0,
                    envelopes: slice,
                }))
                .await?;
        }
        self.observer.on_batch_built(BuildStats {
            changes: 0,
            documents: envelopes.len(),
            documents_by_index: by_index.into_iter().collect(),
            build: started.elapsed(),
        });
        snapshot.batch.clear();
        Ok(())
    }

    /// Publish `SnapshotComplete` to every requesting lane, then acknowledge
    /// the requests: a crash before this point redelivers them.
    async fn finish_snapshot(
        &self,
        snapshot: &mut Snapshot,
        lanes: &[(SinkName, Box<dyn Producer<LaneItem>>)],
    ) -> Result<()> {
        for (sink, indexes) in &snapshot.requested {
            let Some((_, producer)) = lanes.iter().find(|(name, _)| name == sink) else {
                continue;
            };
            producer
                .publish(LaneItem::SnapshotComplete {
                    indexes: indexes.iter().cloned().collect(),
                })
                .await?;
        }
        for handle in snapshot.handles.drain(..) {
            handle.ack().await?;
        }
        let indexes: Vec<IndexName> = snapshot.indexes.iter().cloned().collect();
        tracing::info!(indexes = indexes.len(), "snapshot complete");
        self.observer.on_snapshot_completed(&indexes);
        Ok(())
    }
}

/// A live batch in the making: the deduplicated document ids the buffered
/// changes resolved to, how many changes, the last position, and the deadline.
#[derive(Debug)]
struct LiveBatch {
    ids: Vec<DocumentId>,
    seen: HashSet<DocumentId>,
    changes: usize,
    position: Option<Position>,
    deadline: Option<Instant>,
    capacity: usize,
}

impl LiveBatch {
    fn new(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
            changes: 0,
            position: None,
            deadline: None,
            capacity,
        }
    }

    fn is_full(&self) -> bool {
        self.changes >= self.capacity
    }

    fn clear(&mut self) {
        self.ids.clear();
        self.seen.clear();
        self.changes = 0;
        self.position = None;
        self.deadline = None;
    }
}

/// A running snapshot: its row stream, who asked for what, the request handles
/// to acknowledge at the end, and the batch being filled.
struct Snapshot {
    stream: BoxStream<'static, source::Result<ChangeEvent>>,
    requested: BTreeMap<SinkName, BTreeSet<IndexName>>,
    indexes: HashSet<IndexName>,
    handles: Vec<Box<dyn AckHandle>>,
    batch: SnapshotBatch,
}

#[derive(Debug)]
struct SnapshotBatch {
    ids: Vec<DocumentId>,
    seen: HashSet<DocumentId>,
    rows: usize,
    capacity: usize,
}

impl SnapshotBatch {
    fn new(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
            rows: 0,
            capacity,
        }
    }

    fn is_full(&self) -> bool {
        self.rows >= self.capacity
    }

    fn clear(&mut self) {
        self.ids.clear();
        self.seen.clear();
        self.rows = 0;
    }
}

async fn next_snapshot_row(
    snapshot: &mut Option<Snapshot>,
) -> Option<std::result::Result<ChangeEvent, SourceError>> {
    match snapshot {
        Some(active) => active.stream.next().await,
        None => std::future::pending().await,
    }
}

fn far_future() -> Instant {
    Instant::now() + Duration::from_secs(60 * 60 * 24 * 365)
}

/// Stands in for the request consumer once the request lane has closed, so the
/// select loop keeps serving live changes instead of spinning on `None`.
#[derive(Debug)]
struct ClosedConsumer;

#[async_trait::async_trait]
impl Consumer<Request> for ClosedConsumer {
    async fn recv(&mut self) -> stream::Result<Option<stream::Delivery<Request>>> {
        std::future::pending().await
    }

    fn is_empty(&self) -> bool {
        true
    }
}

/// The sink's document `_id`, derived from the document key (the root primary
/// key); composite keys join their parts with `:`.
pub fn document_id(id: &DocumentId) -> String {
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

impl From<SourceError> for EngineError {
    fn from(error: SourceError) -> Self {
        EngineError::Source(error)
    }
}

#[allow(dead_code)]
fn _assert_live_change_shape(change: LiveChange) -> (Position, ChangeEvent) {
    change
}
