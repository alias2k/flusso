//! The ingest engine: capture → batch → resolve → build once → publish to every
//! lane, plus the request lane it serves snapshots from.
//!
//! One task, one build path. Live changes are buffered per [`BatchPolicy`] —
//! every change the stream has ready, committed the moment the stream would
//! block or [`BatchPolicy::max_changes`] is reached, so a lone change on a
//! quiet stream never waits out [`BatchPolicy::max_delay`] — resolved to the
//! documents they touch (deduplicated), built once with
//! [`DocumentBuilder::build_many`], and published as one [`Batch`] to every
//! lane, carrying the position of the last change. A backfill (a *snapshot*)
//! walks each requested index's root keys in pages
//! ([`DocumentBuilder::keys`], [`KEY_PAGE`] at a time, the engine holding the
//! last key as the cursor), and each key is its own document, built through
//! the same path but never resolved: resolving it like a live change would
//! cost a reverse query per related path and rebuild the parents of related
//! indexes over and over (issue #385). A page is read when it is needed, with
//! no snapshot shared across pages: the source's resume point exists before
//! any backfill, so a row that gets a key behind the walk reaches the live
//! stream. Snapshot batches are published only to the lanes that requested
//! them, without a position, followed by a [`LaneItem::SnapshotComplete`].
//! Both kinds interleave on this single task, so lane order is build order and
//! a document's later message is always the newer state: a page only names
//! ids, and its documents are read when they are built.
//!
//! After every commit the engine reads the stream's watermark — the lowest
//! position every lane has acknowledged — and hands it to the source as
//! confirmation. That is the at-least-once guarantee: the resume point advances
//! only past what *every* sink has made durable.
//!
//! Requests are at-least-once too: a `Backfill` is acknowledged only after its
//! `SnapshotComplete` is published, and concurrent requests for the same index
//! coalesce into one snapshot fanned to every requesting lane. Coalescing costs
//! no wait when every lane has asked (the startup case: the daemon stages every
//! sink before this engine runs, so their requests are already queued); only
//! while some lane is still silent does the engine hold the snapshot for up to
//! [`BatchPolicy::max_delay`] so a reindex fanned to every sink, each answering
//! between its own batches, is one pass over the table.
//!
//! A [`Stop`] ends the engine gracefully instead of where it stands: no further
//! read, the buffered batch published, every lane drained and confirmed, the
//! live stream closed once the source has the final position. See
//! [`Stop`] for the sequence.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::Utc;
use futures::future::{BoxFuture, try_join_all};
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use kernel::{Envelope, IndexName, Position, SinkName};
use source::cdc::{ChangeCapture, ChangeEvent, LiveChange, SourcePosition};
use source::document::{BuildFailure, Document, DocumentBuilder, DocumentId};
use source::{RowKey, SourceError};
use stream::{AckHandle, Batch, Consumer, LaneItem, Producer, Request, Stream};
use tokio::time::{Duration, Instant, sleep_until, timeout_at};

use crate::error::{EngineError, Result};
use crate::observer::{
    BuildStats, EngineId, NoopObserver, Observer, Stage, StageTiming, StartupStep, Work,
};
use crate::policy::{BatchPolicy, FailurePolicies, FailurePolicy};
use crate::stop::{Stop, drain_lanes};

/// How often the watermark is confirmed to the source while no commit happens,
/// so acknowledgements that arrive during a quiet period still advance it.
const CONFIRM_TICK: Duration = Duration::from_secs(1);

/// The longest a stopping engine polls the live stream for its end after
/// asking the source to close it. Past it the stream is dropped. Longer than
/// the source's own bounds added up (the Postgres stream allows 5 s to send
/// the final position and 5 s to close), so a close that just fits is not
/// reported as one that failed.
const CLOSE_WAIT: Duration = Duration::from_secs(12);

/// How many root keys a backfill reads per page. The keys are built in
/// batches of [`BatchPolicy::max_changes`].
pub const KEY_PAGE: usize = 1024;

/// The producing end of every sink's lane, by sink name.
type Lanes = BTreeMap<SinkName, Box<dyn Producer<LaneItem>>>;

/// The source's live change stream.
type LiveStream = BoxStream<'static, source::Result<LiveChange>>;

/// The ingest engine over one source, one document builder, and one stream.
#[derive(Debug)]
pub struct IngestEngine {
    source: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    stream: Arc<dyn Stream>,
    sinks: Vec<SinkName>,
    observer: Arc<dyn Observer>,
    batch: BatchPolicy,
    failure_policies: FailurePolicies,
    stop: Stop,
    /// How many times this engine moved the source forward (see
    /// [`advances`](Self::advances)), across restarts of its run.
    advances: AtomicU64,
    /// The furthest this engine has confirmed, and the furthest its source
    /// reported reaching, across restarts of its run.
    furthest: Mutex<Furthest>,
}

/// The furthest an ingest engine has moved its source, over its life.
#[derive(Debug, Default)]
struct Furthest {
    confirmed: Option<Position>,
    reached: Option<SourcePosition>,
}

/// How the engine's run ended without an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ended {
    LiveEnded,
    Stopped,
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
            failure_policies: FailurePolicies::default(),
            stop: Stop::new(),
            advances: AtomicU64::new(0),
            furthest: Mutex::new(Furthest::default()),
        }
    }

    /// End gracefully once `stop` is requested: no further read, the batch in
    /// flight published, every lane drained and confirmed, the stream closed
    /// (see [`Stop`]).
    pub fn with_stop(mut self, stop: Stop) -> Self {
        self.stop = stop;
        self
    }

    /// How many times this engine has moved its source forward since it was
    /// built, across restarts of its run: the source reported reading further
    /// than ever (keepalives on a quiet stream included), or, for a source that
    /// reports no such position, it confirmed a position further than any
    /// before, or a backfill finished. Publishing a batch
    /// is not enough: a restart publishes a replayed change or backfill page
    /// again. A supervisor reads it to tell a run that made progress from one
    /// that failed where the last one did.
    pub fn advances(&self) -> u64 {
        self.advances.load(Ordering::Relaxed)
    }

    /// Report lifecycle and progress to `observer`.
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// How a document that could not be built is handled, per index: `stop`
    /// errors the engine out (the change is retried after a restart), `skip`
    /// reports it and publishes nothing for it.
    pub fn with_failure_policies(mut self, policies: FailurePolicies) -> Self {
        self.failure_policies = policies;
        self
    }

    /// How live changes are grouped into batches (at least one per batch).
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
            Ok(ended) => {
                match ended {
                    Ended::LiveEnded => tracing::info!("ingest engine stopped: live stream ended"),
                    Ended::Stopped => tracing::info!("ingest engine stopped: stop requested"),
                }
                self.observer.on_engine_stopped(&EngineId::Ingest);
            }
            Err(error) => {
                tracing::error!(%error, "ingest engine stopped on error");
                self.observer
                    .on_engine_error(&EngineId::Ingest, &error.to_string());
            }
        }
        result.map(|_| ())
    }

    async fn run_inner(&self) -> Result<Ended> {
        let started = Instant::now();
        self.source.prepare().await?;
        self.observer
            .on_startup_step(StartupStep::Prepare, None, started.elapsed());
        let lanes: Lanes = self
            .sinks
            .iter()
            .map(|sink| Ok((sink.clone(), self.stream.lane(sink)?.producer)))
            .collect::<Result<_>>()?;
        let mut requests = self.stream.requests()?.consumer;
        let scopes: HashSet<IndexName> = self.documents.backfill_indexes().into_iter().collect();

        let started = Instant::now();
        let mut live: LiveStream = self.source.live().await?;
        self.observer
            .on_startup_step(StartupStep::OpenLive, None, started.elapsed());
        tracing::info!("following live changes");
        self.observer.on_live_started();

        let mut pending = PendingBatch::new(self.batch.max_changes);
        let mut snapshot: Option<Snapshot> = None;
        let mut last_confirmed: Option<Position> = None;
        let mut confirm_tick = tokio::time::interval(CONFIRM_TICK);
        let mut live_ended = false;
        // Where the last arm's work ended: every arm times its wait from here,
        // so the loop reads no clock of its own.
        let mut mark = Instant::now();

        loop {
            // A live stream that ends while a snapshot runs or requests wait
            // (a sink staged its backfill before this engine started) still
            // gets those served; the run ends once nothing is left to do.
            if live_ended && snapshot.is_none() && requests.is_empty() {
                self.commit_live(&mut pending, &lanes).await?;
                self.confirm(&mut last_confirmed);
                return Ok(Ended::LiveEnded);
            }
            if self.stop.is_requested() {
                if let Some(abandoned) = snapshot.take() {
                    tracing::info!(
                        indexes = abandoned.indexes.len(),
                        "stop requested: abandoning the snapshot in progress; its request is \
                         served again on the next start"
                    );
                }
                self.wind_down(&mut pending, &lanes, live, live_ended, &mut last_confirmed)
                    .await?;
                return Ok(Ended::Stopped);
            }
            let deadline = pending.deadline.unwrap_or_else(far_future);
            tokio::select! {
                // While the live stream is open, `next_live` watches the stop.
                () = self.stop.requested(), if live_ended => {
                    mark = self.waited(mark, &mut snapshot);
                }
                change = next_live(&mut live, &self.stop), if !live_ended => match change {
                    Next::Stopped => {
                        mark = self.waited(mark, &mut snapshot);
                    }
                    Next::Change(None) => {
                        mark = self.waited(mark, &mut snapshot);
                        live_ended = true;
                    }
                    Next::Change(Some(Err(error))) => return Err(error.into()),
                    Next::Change(Some(Ok((position, event)))) => {
                        let started = self.waited(mark, &mut snapshot);
                        self.buffer_live(&mut pending, position, event).await?;
                        live_ended = self.drain_ready(&mut live, &mut pending).await?;
                        self.observer.on_stage(
                            StageTiming::new(Stage::Resolve, started.elapsed()).work(Work::Live),
                        );
                        self.commit_live(&mut pending, &lanes).await?;
                        self.confirm(&mut last_confirmed);
                        mark = Instant::now();
                    }
                },
                () = sleep_until(deadline), if pending.deadline.is_some() => {
                    self.waited(mark, &mut snapshot);
                    self.commit_live(&mut pending, &lanes).await?;
                    self.confirm(&mut last_confirmed);
                    mark = Instant::now();
                }
                step = next_snapshot_step(&mut snapshot), if snapshot.is_some() => {
                    let Some(active) = snapshot.as_mut() else { continue };
                    // The wait for a page, if any, is `Page`.
                    let now = Instant::now();
                    active.page_wait += now - mark;
                    mark = now;
                    match step {
                        Err(error) => return Err(error.into()),
                        Ok(Step::Keys) => {
                            active.take_keys();
                            if active.batch.is_full() {
                                self.commit_snapshot(active, &lanes).await?;
                                mark = Instant::now();
                            }
                        }
                        Ok(Step::Done) => {
                            self.commit_snapshot(active, &lanes).await?;
                            let started = Instant::now();
                            self.finish_snapshot(active, &lanes).await?;
                            self.observer.on_stage(
                                StageTiming::new(Stage::Snapshot, started.elapsed())
                                    .work(Work::Backfill),
                            );
                            snapshot = None;
                            mark = Instant::now();
                        }
                    }
                },
                request = requests.recv(), if snapshot.is_none() => match request? {
                    None => {
                        mark = self.waited(mark, &mut snapshot);
                        tracing::warn!("request lane closed; no further backfills can be served");
                        requests = Box::new(ClosedConsumer);
                    }
                    Some(first) => {
                        let started = self.waited(mark, &mut snapshot);
                        let deliveries = self.collect_requests(first, &mut requests, &lanes).await?;
                        snapshot = self.start_snapshot(deliveries, &scopes, &lanes).await?;
                        mark = Instant::now();
                        self.observer.on_stage(
                            StageTiming::new(Stage::Snapshot, mark - started).work(Work::Backfill),
                        );
                    }
                },
                _ = confirm_tick.tick() => {
                    mark = self.waited(mark, &mut snapshot);
                    self.confirm(&mut last_confirmed);
                }
            }
        }
    }

    /// Close the wait since `since` as the loop wakes, and return the moment,
    /// so the work that follows times from it. With no snapshot running the
    /// wait is [`Stage::Idle`]; with one, the loop was waiting on its rows
    /// too, so it is the snapshot's [`Stage::Page`] (reported with its next
    /// batch).
    fn waited(&self, since: Instant, snapshot: &mut Option<Snapshot>) -> Instant {
        let now = Instant::now();
        match snapshot {
            Some(active) => active.page_wait += now - since,
            None => self
                .observer
                .on_stage(StageTiming::new(Stage::Idle, now - since)),
        }
        now
    }

    /// The rest of a stop: publish the buffered batch, wait until every lane
    /// acknowledged it (confirming as the watermark moves), then close the
    /// live stream so the final position reaches the source.
    async fn wind_down(
        &self,
        pending: &mut PendingBatch,
        lanes: &Lanes,
        mut live: LiveStream,
        live_ended: bool,
        last_confirmed: &mut Option<Position>,
    ) -> Result<()> {
        tracing::info!(
            buffered = pending.count,
            "stop requested: reading no further changes; landing what is in flight"
        );
        self.commit_live(pending, lanes).await?;
        drain_lanes(&*self.stream, &*self.source, last_confirmed).await;
        tracing::info!(position = ?last_confirmed, "every sink acknowledged; closing the live stream");
        if live_ended || !self.source.close_live() {
            return Ok(());
        }
        let closed = timeout_at(Instant::now() + CLOSE_WAIT, async {
            let mut unread = 0usize;
            let mut failed = None;
            while let Some(item) = live.next().await {
                match item {
                    Ok(_) => unread += 1,
                    Err(error) => failed = Some(error),
                }
            }
            (unread, failed)
        })
        .await;
        match closed {
            Ok((unread, failed)) => {
                if unread > 0 {
                    tracing::warn!(
                        unread,
                        "the live stream yielded changes while closing; dropped unconfirmed, so \
                         the next start reads them again"
                    );
                }
                match failed {
                    Some(error) => Err(error.into()),
                    None => Ok(()),
                }
            }
            Err(_) => Err(SourceError::Connection(format!(
                "the final position was not confirmed to the source: the live stream did not \
                 close within {CLOSE_WAIT:?}"
            ))
            .into()),
        }
    }

    /// Buffer every change the live stream has ready *right now*, stopping at
    /// a full batch, the batch's deadline, or the moment the stream would
    /// block. Returns whether the stream ended. The caller commits on return
    /// either way: a change on a quiet stream has nothing to batch with, so
    /// waiting out `max_delay` would only add latency, while a burst keeps the
    /// stream ready and fills the batch — for at most `max_delay`.
    async fn drain_ready(&self, live: &mut LiveStream, pending: &mut PendingBatch) -> Result<bool> {
        while pending.is_open() && !self.stop.is_requested() {
            match live.next().now_or_never() {
                None => return Ok(false),
                Some(None) => return Ok(true),
                Some(Some(Err(error))) => return Err(error.into()),
                Some(Some(Ok((position, event)))) => {
                    self.buffer_live(pending, position, event).await?;
                }
            }
        }
        Ok(false)
    }

    /// Resolve one live change into the batch: the documents it touches,
    /// deduplicated, and its position.
    async fn buffer_live(
        &self,
        pending: &mut PendingBatch,
        position: Position,
        event: ChangeEvent,
    ) -> Result<()> {
        self.observer.on_change_captured();
        let affected = self
            .documents
            .resolve(event.table(), event.key(), event.before(), event.after())
            .await?;
        tracing::trace!(documents = affected.len(), "change resolved to documents");
        pending.add(affected);
        pending.position = Some(position);
        if pending.deadline.is_none() {
            pending.deadline = Some(Instant::now() + self.batch.max_delay);
        }
        Ok(())
    }

    /// Build the batch's documents once and publish them to every lane with
    /// the batch's position, concurrently, so a full lane delays only its own
    /// copy. A batch that resolved to no document is still published (empty),
    /// so the lanes acknowledge its position.
    #[tracing::instrument(name = "ingest.commit", level = "debug", skip_all, fields(changes = pending.count, documents = pending.ids.len()))]
    async fn commit_live(&self, pending: &mut PendingBatch, lanes: &Lanes) -> Result<()> {
        if pending.count == 0 {
            return Ok(());
        }
        let started = Instant::now();
        let (envelopes, by_index) = self.build(&pending.ids, pending.position).await?;
        let built = Instant::now();
        let build = built - started;
        self.observer
            .on_stage(StageTiming::new(Stage::Build, build).work(Work::Live));
        let stats = BuildStats {
            changes: pending.count,
            documents: envelopes.len(),
            documents_by_index: by_index.into_iter().collect(),
            build,
        };
        let item = LaneItem::Batch(Batch {
            position: pending.position,
            changes: pending.count,
            envelopes: envelopes.into(),
        });
        self.publish(
            lanes
                .iter()
                .map(|(sink, producer)| (sink, producer, item.clone())),
            Work::Live,
            built,
        )
        .await?;
        pending.clear();
        self.observer.on_batch_built(stats);
        tracing::debug!("batch built and published");
        Ok(())
    }

    /// Publish each item to its lane, all at once, so a full lane delays only
    /// its own copy. Times each lane ([`Stage::LaneWait`]) and the whole
    /// ([`Stage::Publish`]) from `started`, the end of the build, so cutting
    /// the items is publishing too.
    async fn publish<'a>(
        &self,
        items: impl IntoIterator<Item = (&'a SinkName, &'a Box<dyn Producer<LaneItem>>, LaneItem)>,
        work: Work,
        started: Instant,
    ) -> Result<()> {
        try_join_all(items.into_iter().map(|(sink, producer, item)| async move {
            producer.publish(item).await?;
            self.observer.on_stage(
                StageTiming::new(Stage::LaneWait, started.elapsed())
                    .work(work)
                    .sink(sink),
            );
            Ok::<_, EngineError>(())
        }))
        .await?;
        self.observer
            .on_stage(StageTiming::new(Stage::Publish, started.elapsed()).work(work));
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
            let envelope = match document {
                Document::Upsert { id, body } => {
                    Envelope::upsert(id.index.clone(), document_id(&id), body, position, ts)
                }
                Document::Delete { id } => {
                    Envelope::delete(id.index.clone(), document_id(&id), position, ts)
                }
                Document::Failed { id, failure } => {
                    self.document_failed(&id, failure)?;
                    continue;
                }
            };
            *by_index.entry(envelope.index.clone()).or_insert(0) += 1;
            envelopes.push(envelope);
        }
        Ok((envelopes, by_index))
    }

    /// Report a document that could not be built, then apply its policy: `stop`
    /// fails the batch (nothing in it is published, so the change is retried),
    /// `skip` leaves every sink's indexed version alone.
    fn document_failed(&self, id: &DocumentId, failure: BuildFailure) -> Result<()> {
        let rendered = document_id(id);
        let policy = self.failure_policies.resolve(id.index.as_ref());
        self.observer
            .on_document_failed(&id.index, &rendered, &failure, policy);
        match policy {
            FailurePolicy::Stop => Err(EngineError::DocumentFailed {
                index: id.index.to_string(),
                id: rendered,
                failure: Box::new(failure),
            }),
            FailurePolicy::Skip => Ok(()),
        }
    }

    /// Gather the requests one snapshot will serve: everything already queued
    /// behind `first`, then, only while some lane has not asked yet, stragglers
    /// for up to `max_delay`. Returns as soon as every lane has asked, so a
    /// single-sink backfill starts at once; a lane that never asks (a seeded
    /// sibling) costs one `max_delay`. A straggler past the window simply gets
    /// its own snapshot.
    async fn collect_requests(
        &self,
        first: stream::Delivery<Request>,
        requests: &mut Box<dyn Consumer<Request>>,
        lanes: &Lanes,
    ) -> Result<Vec<stream::Delivery<Request>>> {
        let mut deliveries = vec![first];
        let deadline = Instant::now() + self.batch.max_delay;
        loop {
            let asked: HashSet<&SinkName> = deliveries
                .iter()
                .map(|delivery| {
                    let Request::Backfill { sink, .. } = delivery.item();
                    sink
                })
                .collect();
            if lanes.keys().all(|lane| asked.contains(lane)) {
                break;
            }
            let next = if requests.is_empty() {
                match timeout_at(deadline, requests.recv()).await {
                    Ok(received) => received?,
                    Err(_) => break,
                }
            } else {
                requests.recv().await?
            };
            match next {
                Some(more) => deliveries.push(more),
                None => break,
            }
        }
        Ok(deliveries)
    }

    /// Hand the stream's watermark to the source when it moved, and count an
    /// advance past the furthest this engine ever got.
    fn confirm(&self, last_confirmed: &mut Option<Position>) {
        let watermark = self.stream.watermark();
        if let Some(watermark) = watermark
            && *last_confirmed != Some(watermark)
        {
            self.source.confirm(watermark);
            *last_confirmed = Some(watermark);
        }
        let reached = self.source.reached(watermark);
        let mut furthest = self.furthest.lock().unwrap_or_else(PoisonError::into_inner);
        // A reopened stream numbers a replayed change afresh, so the stream's
        // own position moves on every replay: the source's answer decides
        // whenever it gives one.
        let moved = match reached {
            Some(_) => reached > furthest.reached,
            None => watermark > furthest.confirmed,
        };
        furthest.confirmed = furthest.confirmed.max(watermark);
        furthest.reached = furthest.reached.max(reached);
        if moved {
            self.advances.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Coalesce the delivered requests into one snapshot: the union of their
    /// indexes, each walked once, remembering which lane asked for which.
    /// Requests for indexes the builder knows nothing about are answered with
    /// an immediate `SnapshotComplete` for whatever *is* known.
    async fn start_snapshot(
        &self,
        deliveries: Vec<stream::Delivery<Request>>,
        scopes: &HashSet<IndexName>,
        lanes: &Lanes,
    ) -> Result<Option<Snapshot>> {
        let mut requested: BTreeMap<SinkName, BTreeSet<IndexName>> = BTreeMap::new();
        let mut handles = Vec::with_capacity(deliveries.len());
        for delivery in deliveries {
            let (request, handle) = delivery.into_parts();
            let Request::Backfill { sink, indexes } = request;
            let known: BTreeSet<IndexName> = indexes
                .into_iter()
                .filter(|index| {
                    let known = scopes.contains(index);
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
        let mut snapshot = Snapshot {
            walk: Walk::new(Arc::clone(&self.documents), indexes.iter().cloned()),
            requested,
            indexes: indexes.iter().cloned().collect(),
            handles,
            batch: PendingBatch::new(self.batch.max_changes),
            page_wait: Duration::ZERO,
        };
        if indexes.is_empty() {
            self.finish_snapshot(&mut snapshot, lanes).await?;
            return Ok(None);
        }
        let ordered: Vec<IndexName> = indexes.into_iter().collect();
        tracing::info!(
            indexes = ordered.len(),
            lanes = snapshot.requested.len(),
            "starting snapshot"
        );
        self.observer.on_snapshot_started(&ordered);
        Ok(Some(snapshot))
    }

    /// Build the snapshot batch once and publish each lane the slice of it that
    /// lane requested, with no position.
    async fn commit_snapshot(&self, snapshot: &mut Snapshot, lanes: &Lanes) -> Result<()> {
        let page = std::mem::take(&mut snapshot.page_wait);
        if snapshot.batch.count == 0 && page.is_zero() {
            return Ok(());
        }
        self.observer
            .on_stage(StageTiming::new(Stage::Page, page).work(Work::Backfill));
        if snapshot.batch.count == 0 {
            return Ok(());
        }
        let started = Instant::now();
        let (envelopes, by_index) = self.build(&snapshot.batch.ids, None).await?;
        let built = Instant::now();
        let build = built - started;
        self.observer
            .on_stage(StageTiming::new(Stage::Build, build).work(Work::Backfill));
        // Cutting each lane's slice copies its documents: part of publishing.
        let mut publishes = Vec::with_capacity(snapshot.requested.len());
        for (sink, indexes) in &snapshot.requested {
            let slice: Vec<Envelope> = envelopes
                .iter()
                .filter(|envelope| indexes.contains(&envelope.index))
                .cloned()
                .collect();
            if slice.is_empty() {
                continue;
            }
            let Some(producer) = lanes.get(sink) else {
                tracing::warn!(%sink, "snapshot requested by a sink with no lane; dropping its slice");
                continue;
            };
            publishes.push((
                sink,
                producer,
                LaneItem::Batch(Batch {
                    position: None,
                    changes: 0,
                    envelopes: slice.into(),
                }),
            ));
        }
        self.publish(publishes, Work::Backfill, built).await?;
        self.observer.on_batch_built(BuildStats {
            changes: 0,
            documents: envelopes.len(),
            documents_by_index: by_index.into_iter().collect(),
            build,
        });
        snapshot.batch.clear();
        Ok(())
    }

    /// Publish `SnapshotComplete` to every requesting lane, then acknowledge
    /// the requests: a crash before this point redelivers them.
    async fn finish_snapshot(&self, snapshot: &mut Snapshot, lanes: &Lanes) -> Result<()> {
        let markers = snapshot.requested.iter().filter_map(|(sink, indexes)| {
            lanes.get(sink).map(|producer| {
                producer.publish(LaneItem::SnapshotComplete {
                    indexes: indexes.iter().cloned().collect(),
                })
            })
        });
        try_join_all(markers).await?;
        for handle in snapshot.handles.drain(..) {
            handle.ack().await?;
        }
        let indexes: Vec<IndexName> = snapshot.indexes.iter().cloned().collect();
        tracing::info!(indexes = indexes.len(), "snapshot complete");
        self.advances.fetch_add(1, Ordering::Relaxed);
        self.observer.on_snapshot_completed(&indexes);
        Ok(())
    }
}

/// What the live arm of the loop got.
enum Next {
    /// The stop was requested first; nothing was read.
    Stopped,
    Change(Option<source::Result<LiveChange>>),
}

/// The next live change, unless the stop is requested first: checked before
/// the stream on every poll, so a stop is never raced by a ready change. The
/// check is an atomic load; the stop's waker is registered only when the
/// stream would wait, so a busy stream pays nothing more.
async fn next_live(live: &mut LiveStream, stop: &Stop) -> Next {
    let notified = stop.notified();
    tokio::pin!(notified);
    std::future::poll_fn(|cx| {
        if stop.is_requested() {
            return std::task::Poll::Ready(Next::Stopped);
        }
        if let std::task::Poll::Ready(change) = live.poll_next_unpin(cx) {
            return std::task::Poll::Ready(Next::Change(change));
        }
        match notified.as_mut().poll(cx) {
            std::task::Poll::Ready(()) => std::task::Poll::Ready(Next::Stopped),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    })
    .await
}

/// A batch in the making: the deduplicated document ids of the buffered changes
/// (or snapshot rows), how many were buffered, and — for a live
/// batch — the last position and the flush deadline.
#[derive(Debug)]
struct PendingBatch {
    ids: Vec<DocumentId>,
    seen: HashSet<DocumentId>,
    count: usize,
    position: Option<Position>,
    deadline: Option<Instant>,
    capacity: usize,
}

impl PendingBatch {
    fn new(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
            count: 0,
            position: None,
            deadline: None,
            capacity,
        }
    }

    /// Buffer one change's affected documents, deduplicated.
    fn add(&mut self, affected: impl IntoIterator<Item = DocumentId>) {
        for id in affected {
            if self.seen.insert(id.clone()) {
                self.ids.push(id);
            }
        }
        self.count += 1;
    }

    fn is_full(&self) -> bool {
        self.count >= self.capacity
    }

    /// Still accepting changes: neither full nor past its deadline.
    fn is_open(&self) -> bool {
        !self.is_full()
            && self
                .deadline
                .is_none_or(|deadline| Instant::now() < deadline)
    }

    fn clear(&mut self) {
        self.ids.clear();
        self.seen.clear();
        self.count = 0;
        self.position = None;
        self.deadline = None;
    }
}

/// A running snapshot: the walk over its indexes' keys, who asked for what,
/// the request handles to acknowledge at the end, and the batch being filled.
struct Snapshot {
    walk: Walk,
    requested: BTreeMap<SinkName, BTreeSet<IndexName>>,
    indexes: HashSet<IndexName>,
    handles: Vec<Box<dyn AckHandle>>,
    batch: PendingBatch,
    /// Time spent getting keys since the last batch: waiting for pages and
    /// buffering keys.
    page_wait: Duration,
}

impl Snapshot {
    /// Buffer the keys in hand as their index's documents, until the batch is
    /// full. A key with a null part addresses no document.
    fn take_keys(&mut self) {
        let Some(index) = self.walk.indexes.front() else {
            return;
        };
        while !self.batch.is_full() {
            let Some(key) = self.walk.keys.pop_front() else {
                break;
            };
            if key.has_null() {
                self.batch.count += 1;
                continue;
            }
            self.batch.add([DocumentId {
                index: index.clone(),
                key,
            }]);
        }
    }
}

/// A snapshot's walk over its indexes' root keys, one index after another,
/// one page at a time ([`DocumentBuilder::keys`]). The page being read is
/// kept here, not in the `select!` that polls it, so a live change winning
/// the `select!` doesn't drop a page half read.
struct Walk {
    documents: Arc<dyn DocumentBuilder>,
    /// The indexes still to walk, the one being walked first.
    indexes: VecDeque<IndexName>,
    /// The last key read of the index being walked: the next page follows it.
    after: Option<RowKey>,
    /// Keys read and not yet buffered.
    keys: VecDeque<RowKey>,
    /// Whether the keys in hand are the index's last.
    last: bool,
    page: Option<BoxFuture<'static, source::Result<Vec<RowKey>>>>,
}

/// What the walk has for the loop.
enum Step {
    /// Keys in hand, from the front index.
    Keys,
    /// Every index walked.
    Done,
}

impl Walk {
    fn new(
        documents: Arc<dyn DocumentBuilder>,
        indexes: impl IntoIterator<Item = IndexName>,
    ) -> Self {
        Self {
            documents,
            indexes: indexes.into_iter().collect(),
            after: None,
            keys: VecDeque::new(),
            last: false,
            page: None,
        }
    }

    /// The next step: keys in hand at once, else the next page read.
    async fn step(&mut self) -> source::Result<Step> {
        loop {
            if !self.keys.is_empty() {
                return Ok(Step::Keys);
            }
            if self.last {
                self.indexes.pop_front();
                self.after = None;
                self.last = false;
            }
            let Some(index) = self.indexes.front() else {
                return Ok(Step::Done);
            };
            let page = self.page.get_or_insert_with(|| {
                let documents = Arc::clone(&self.documents);
                let index = index.clone();
                let after = self.after.clone();
                Box::pin(async move { documents.keys(&index, after.as_ref(), KEY_PAGE).await })
            });
            let keys = page.await;
            self.page = None;
            let keys = keys?;
            self.last = keys.len() < KEY_PAGE;
            self.after = keys.last().cloned().or(self.after.take());
            self.keys.extend(keys);
        }
    }
}

async fn next_snapshot_step(snapshot: &mut Option<Snapshot>) -> source::Result<Step> {
    match snapshot {
        Some(active) => active.walk.step().await,
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
    id.rendered()
}

impl From<SourceError> for EngineError {
    fn from(error: SourceError) -> Self {
        EngineError::Source(error)
    }
}
