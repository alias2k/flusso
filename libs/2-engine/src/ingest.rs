//! The ingest engine: capture → batch → resolve → build once → publish to every
//! lane, plus the request lane it serves snapshots from.
//!
//! One task, one build path. Live changes are buffered per [`BatchPolicy`] —
//! every change the stream has ready, committed the moment the stream would
//! block or [`BatchPolicy::max_changes`] is reached, so a lone change on a
//! quiet stream never waits out [`BatchPolicy::max_delay`] — resolved to the
//! documents they touch (deduplicated), built once with
//! [`DocumentBuilder::build_many`], and published as one [`Batch`](stream::Batch) to every
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
//! After every commit, and once a second, the engine reads how far every lane
//! has landed ([`Progress`]) and hands it to the source as confirmation. That
//! is the at-least-once guarantee: the resume point advances only past what
//! *every* sink has made durable.
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

mod builds;
mod commit;
mod confirm;
mod gate;
mod live;
mod snapshot;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::stream::BoxStream;
use kernel::{IndexName, SinkName};
use source::SourceError;
use source::cdc::{ChangeCapture, Landed, LiveChange};
use source::document::{DocumentBuilder, DocumentId};
use stream::{Consumer, LaneItem, LaneKey, Producer, Request, Stream};
use tokio::time::{Duration, Instant, timeout_at};

use crate::budget::Budget;
use crate::error::{EngineError, Result};
use crate::observer::{
    EngineId, NoopObserver, Observer, Stage, StageTiming, StartupStep, Wake, Work,
};
use crate::policy::{BatchPolicy, FailurePolicies};
use crate::progress::Progress;
use crate::stop::{Stop, drain_lanes};

use builds::Builds;
use confirm::{CONFIRM_TICK, Furthest};
use live::{LiveRun, Next, next_live};
pub use snapshot::KEY_PAGE;
use snapshot::{ClosedConsumer, Snapshot, Step, next_snapshot_step};

/// The longest a stopping engine polls the live stream for its end after
/// asking the source to close it. Past it the stream is dropped. Longer than
/// the source's own bounds added up (the Postgres stream allows 5 s to send
/// the final position and 5 s to close), so a close that just fits is not
/// reported as one that failed.
const CLOSE_WAIT: Duration = Duration::from_secs(12);

/// The producing end of every `(index, sink)` lane.
type Lanes = BTreeMap<LaneKey, Box<dyn Producer<LaneItem>>>;

/// The source's live change stream.
type LiveStream = BoxStream<'static, source::Result<LiveChange>>;

/// The ingest engine over one source, one document builder, and one stream.
#[derive(Debug)]
pub struct IngestEngine {
    source: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    /// Resolves live changes; `documents` when none was given.
    resolver: Arc<dyn DocumentBuilder>,
    stream: Arc<dyn Stream>,
    sinks: Vec<SinkName>,
    observer: Arc<dyn Observer>,
    batch: BatchPolicy,
    builds: usize,
    budget: Option<Arc<Budget>>,
    failure_policies: FailurePolicies,
    stop: Stop,
    /// How many times this engine moved the source forward (see
    /// [`advances`](Self::advances)), across restarts of its run.
    advances: AtomicU64,
    /// The furthest this engine has confirmed, and the furthest its source
    /// reported reaching, across restarts of its run.
    furthest: Mutex<Furthest>,
    /// How far each lane has landed what this engine routed: registered at
    /// publish, read to confirm.
    progress: Arc<Progress>,
}

/// How the engine's run ended without an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ended {
    LiveEnded,
    Stopped,
}

impl IngestEngine {
    /// An engine publishing to the lanes of `sinks` on `stream`, registering
    /// what it routes in `progress`: the one every sink engine over `stream`
    /// reports to, which is what confirmation and fences read.
    pub fn new(
        source: Arc<dyn ChangeCapture>,
        documents: Arc<dyn DocumentBuilder>,
        stream: Arc<dyn Stream>,
        sinks: Vec<SinkName>,
        progress: Arc<Progress>,
    ) -> Self {
        Self {
            source,
            resolver: Arc::clone(&documents),
            documents,
            stream,
            observer: Arc::new(NoopObserver),
            batch: BatchPolicy::default(),
            builds: 1,
            budget: None,
            failure_policies: FailurePolicies::default(),
            stop: Stop::new(),
            advances: AtomicU64::new(0),
            furthest: Mutex::new(Furthest::default()),
            progress,
            sinks,
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

    /// How far each lane has landed what this engine routed.
    pub fn progress(&self) -> Arc<Progress> {
        Arc::clone(&self.progress)
    }

    /// Resolve live changes with `resolver` (a builder on a connection of its
    /// own, so resolving never waits for a build's connection); `None` keeps
    /// the document builder.
    pub fn with_resolver(mut self, resolver: Option<Arc<dyn DocumentBuilder>>) -> Self {
        if let Some(resolver) = resolver {
            self.resolver = resolver;
        }
        self
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

    /// Hold built documents within `budget` (see [`Budget`]): a build is
    /// granted only when the budget has room, and its bytes are held until
    /// every lane acknowledged its batch. Without one nothing is counted.
    pub fn with_budget(mut self, budget: Arc<Budget>) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Build up to `builds` batches at once (at least one). Each lane still
    /// gets its items in the order they were granted.
    pub fn with_builds(mut self, builds: usize) -> Self {
        self.builds = builds.max(1);
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
            .flat_map(|sink| self.stream.lanes(sink))
            .map(|key| {
                let producer = self.stream.lane(&key)?.producer;
                Ok((key, producer))
            })
            .collect::<Result<_>>()?;
        self.progress.check_lanes(lanes.keys())?;
        let requests = self.stream.requests()?.consumer;
        let scopes: HashSet<IndexName> = self.documents.backfill_indexes().into_iter().collect();

        let started = Instant::now();
        let live: LiveStream = self.source.live().await?;
        self.observer
            .on_startup_step(StartupStep::OpenLive, None, started.elapsed());
        tracing::info!("following live changes");
        self.observer.on_live_started();

        let mut builds =
            Builds::new(self.builds, self.budget.clone()).with_observer(Arc::clone(&self.observer));
        let ended = self
            .follow(&lanes, requests, &scopes, live, &mut builds)
            .await;
        if ended.is_err() {
            builds.abandon().await;
        }
        ended
    }

    /// The run's loop, from an open live stream to its end or a stop.
    async fn follow(
        &self,
        lanes: &Lanes,
        mut requests: Box<dyn Consumer<Request>>,
        scopes: &HashSet<IndexName>,
        mut live: LiveStream,
        builds: &mut Builds,
    ) -> Result<Ended> {
        let mut pending = LiveRun::new(self.batch.max_changes);
        let mut snapshot: Option<Snapshot> = None;
        let mut last_confirmed: Option<Landed> = None;
        let mut confirm_tick = tokio::time::interval(CONFIRM_TICK);
        let mut live_ended = false;
        // Where the last arm's work ended: every arm times its wait from here,
        // so the loop reads no clock of its own.
        let mut mark = Instant::now();
        // What an earlier run took and never published waits again, and is
        // built with the first change this run reads: confirmation stayed
        // before it, so that change is a replay of what it waits for.
        self.progress.give_back_all();

        loop {
            // A live stream that ends while a snapshot runs or requests wait
            // (a sink staged its backfill before this engine started) still
            // gets those served; the run ends once nothing is left to do.
            if live_ended && snapshot.is_none() && requests.is_empty() {
                self.settle(&mut pending, builds, lanes).await?;
                self.confirm(&mut last_confirmed, false);
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
                self.wind_down(
                    &mut pending,
                    builds,
                    lanes,
                    live,
                    live_ended,
                    &mut last_confirmed,
                )
                .await?;
                return Ok(Ended::Stopped);
            }
            // A batch that can't take more changes (full, or past its
            // deadline) and waits for a build stops the reading: the stream
            // waits, as it did while a build ran.
            let open = pending.is_open();
            let building = builds.running() > 0;
            tokio::select! {
                // While the live stream is read, `next_live` watches the stop.
                () = self.stop.requested(), if live_ended || !open => {
                    mark = self.waited(mark, &mut snapshot, building);
                }
                change = next_live(&mut live, &self.stop), if !live_ended && open => match change {
                    Next::Stopped => {
                        mark = self.waited(mark, &mut snapshot, building);
                    }
                    Next::Change(None) => {
                        mark = self.waited(mark, &mut snapshot, building);
                        live_ended = true;
                    }
                    Next::Change(Some(Err(error))) => return Err(error.into()),
                    Next::Change(Some(Ok((position, event)))) => {
                        let started = self.waited(mark, &mut snapshot, building);
                        self.buffer_live(&mut pending, position, event).await?;
                        live_ended = self.drain_ready(&mut live, &mut pending).await?;
                        self.observer.on_stage(
                            StageTiming::new(Stage::Resolve, started.elapsed()).work(Work::Live),
                        );
                        builds.woke(Wake::Change);
                        self.grant(&mut pending, &mut snapshot, builds, lanes).await?;
                        self.confirm(&mut last_confirmed, false);
                        mark = Instant::now();
                    }
                },
                () = released(self.budget.as_deref()), if builds.refused().is_some() => {
                    builds.woke(Wake::Released);
                    self.grant(&mut pending, &mut snapshot, builds, lanes).await?;
                    self.confirm(&mut last_confirmed, false);
                    mark = Instant::now();
                }
                landed = builds.next(), if building => {
                    builds.woke(Wake::Landed);
                    self.landed(landed, &mut pending, &mut snapshot, builds, lanes).await?;
                    self.confirm(&mut last_confirmed, false);
                    mark = Instant::now();
                }
                step = next_snapshot_step(&mut snapshot),
                    if snapshot.as_ref().is_some_and(Snapshot::wants_keys) =>
                {
                    let Some(active) = snapshot.as_mut() else { continue };
                    // The wait for a page, if any, is `Page`, unless builds
                    // ran meanwhile: then the loop was waiting on them too.
                    if !building {
                        active.page_wait += mark.elapsed();
                    }
                    match step {
                        Err(error) => return Err(error.into()),
                        Ok(Step::Keys) => active.take_keys(),
                        Ok(Step::Done) => active.done = true,
                    }
                    builds.woke(Wake::Page);
                    self.grant(&mut pending, &mut snapshot, builds, lanes).await?;
                    mark = Instant::now();
                },
                request = requests.recv(), if snapshot.is_none() => match request? {
                    None => {
                        mark = self.waited(mark, &mut snapshot, building);
                        tracing::warn!("request lane closed; no further backfills can be served");
                        requests = Box::new(ClosedConsumer);
                    }
                    Some(first) => {
                        let started = self.waited(mark, &mut snapshot, building);
                        let deliveries = self.collect_requests(first, &mut requests, lanes).await?;
                        snapshot = self.start_snapshot(deliveries, scopes, lanes).await?;
                        mark = Instant::now();
                        self.observer.on_stage(
                            StageTiming::new(Stage::Snapshot, mark - started).work(Work::Backfill),
                        );
                    }
                },
                _ = confirm_tick.tick() => {
                    mark = self.waited(mark, &mut snapshot, building);
                    // A backstop, not the mechanism: a batch the budget held
                    // back is granted on a release (the arm above). Should a
                    // release ever go unheard, the next tick still grants it,
                    // so it costs a second, never the run.
                    if builds.refused().is_some() {
                        builds.woke(Wake::Tick);
                        self.grant(&mut pending, &mut snapshot, builds, lanes).await?;
                        mark = Instant::now();
                    }
                    self.confirm(&mut last_confirmed, true);
                }
            }
        }
    }

    /// Close the wait since `since` as the loop wakes, and return the moment,
    /// so the work that follows times from it. While builds run, the loop was
    /// waiting on them, which their [`Stage::Build`] covers. Otherwise, with
    /// no snapshot running the wait is [`Stage::Idle`]; with one, the loop
    /// was waiting on its rows too, so it is the snapshot's [`Stage::Page`]
    /// (reported with its next batch).
    fn waited(&self, since: Instant, snapshot: &mut Option<Snapshot>, building: bool) -> Instant {
        let now = Instant::now();
        if building {
            return now;
        }
        match snapshot {
            Some(active) => active.page_wait += now - since,
            None => self
                .observer
                .on_stage(StageTiming::new(Stage::Idle, now - since)),
        }
        now
    }

    /// The rest of a stop: finish the builds running, build and publish the
    /// buffered batch, wait until every lane
    /// acknowledged it (confirming what landed as it moves), then close the
    /// live stream so the final position reaches the source.
    async fn wind_down(
        &self,
        pending: &mut LiveRun,
        builds: &mut Builds,
        lanes: &Lanes,
        mut live: LiveStream,
        live_ended: bool,
        last_confirmed: &mut Option<Landed>,
    ) -> Result<()> {
        tracing::info!(
            buffered = pending.count,
            "stop requested: reading no further changes; landing what is in flight"
        );
        self.settle(pending, builds, lanes).await?;
        drain_lanes(&*self.stream, &*self.source, &self.progress, last_confirmed).await;
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
}

/// The next release of `budget`'s bytes; never, without a budget.
async fn released(budget: Option<&Budget>) {
    match budget {
        Some(budget) => budget.released().await,
        None => std::future::pending().await,
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
