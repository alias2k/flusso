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
mod reader;
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
use crate::policy::{BatchPolicy, FailurePolicies, MaxWaiting};
use crate::progress::Progress;
use crate::stop::{Stop, drain_lanes};

use builds::Builds;
use confirm::{CONFIRM_TICK, Furthest};
use live::Runs;
use reader::{Lead, Reader, ReaderTask};
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
    /// How far the reader may read ahead; derived from `batch` and `builds`
    /// when unset.
    max_waiting: Option<MaxWaiting>,
    budget: Option<Arc<Budget>>,
    /// The budget waits open across engine runs (see [`Builds::with_waits`]).
    budget_waits: Arc<std::sync::Mutex<builds::Waits>>,
    /// The one key-read permit every walk shares (ADR 0012 stage 8a, K1).
    key_reads: Arc<tokio::sync::Semaphore>,
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
            max_waiting: None,
            budget: None,
            budget_waits: Arc::default(),
            key_reads: Arc::new(tokio::sync::Semaphore::new(1)),
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

    /// Let the reader read ahead of the builds up to `max` (each bound at
    /// least one); unset, [`MaxWaiting::derived`] from the batch policy and
    /// the builds.
    pub fn with_max_waiting(mut self, max: MaxWaiting) -> Self {
        self.max_waiting = Some(MaxWaiting {
            per_index: max.per_index.max(1),
            total: max.total.max(1),
        });
        self
    }

    /// The reader's lead this engine runs with.
    pub fn max_waiting(&self) -> MaxWaiting {
        self.max_waiting
            .unwrap_or_else(|| MaxWaiting::derived(self.batch, self.builds))
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

        // An index the shared budget leaves out may wait while the others
        // build, up to the lead per index (ADR 0012 stage 8a, slices 4 and
        // 6a); past that the live batch waits whole, and the reader stops at
        // the same number, as with one index.
        let defer_below = self
            .budget
            .as_ref()
            .and_then(|budget| budget.index_share())
            .map(|_| self.max_waiting().per_index);
        let mut builds = Builds::new(self.builds, self.budget.clone())
            .with_observer(Arc::clone(&self.observer))
            .with_progress(Arc::clone(&self.progress))
            .with_deferral(defer_below)
            .with_waits(Arc::clone(&self.budget_waits));
        let ended = self
            .follow(&lanes, requests, &scopes, live, &mut builds)
            .await;
        if ended.is_err() {
            builds.abandon().await;
        }
        ended
    }

    /// The run's loop, from an open live stream to its end or a stop: the
    /// builder, with the reader on its own task (ADR 0012 stage 8a, §3.8).
    async fn follow(
        &self,
        lanes: &Lanes,
        mut requests: Box<dyn Consumer<Request>>,
        scopes: &HashSet<IndexName>,
        live: LiveStream,
        builds: &mut Builds,
    ) -> Result<Ended> {
        // What an earlier run marked and never published is forgotten
        // before this run's reader marks anything (R10): confirmation stayed
        // before all of it, so this run's stream replays it.
        self.progress.forget_all();
        let lead = Lead::new(self.max_waiting(), self.progress.changes_read());
        let (sender, mut handed) = tokio::sync::mpsc::unbounded_channel();
        let mut reader = ReaderTask::spawn(
            Reader {
                resolver: Arc::clone(&self.resolver),
                progress: Arc::clone(&self.progress),
                observer: Arc::clone(&self.observer),
                stop: self.stop.clone(),
                batch: self.batch,
                lead: Arc::clone(&lead),
                runs: sender,
                #[cfg(test)]
                handing: None,
                #[cfg(test)]
                checked: None,
            },
            live,
        );
        let mut runs = Runs::new(self.batch.max_changes, self.batch.max_delay, lead);
        // On an error the reader is aborted and awaited before the run returns
        // (R9): nothing it does, a mark, a count, its stream, outlives the
        // engine run, so the next run's `forget_all` never meets it.
        let result: Result<Ended> = async {
            let mut snapshot: Option<Snapshot> = None;
            let mut last_confirmed: Option<Landed> = None;
            let mut confirm_tick = tokio::time::interval(CONFIRM_TICK);
            // The reader's end, once joined: the stream to close, and whether
            // it ended by itself.
            let mut ended: Option<reader::Ended> = None;
            // Where the last arm's work ended: every arm times its wait from here,
            // so the loop reads no clock of its own.
            let mut mark = Instant::now();

            loop {
                // A live stream that ends while a snapshot runs or requests wait
                // (a sink staged its backfill before this engine started) still
                // gets those served; the run ends once nothing is left to do.
                if ended.as_ref().is_some_and(|ended| ended.stream_ended)
                    && snapshot.is_none()
                    && requests.is_empty()
                {
                    self.settle(&mut runs, builds, lanes).await?;
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
                    // The barrier (R7): the reader hands over what it read and
                    // ends before anything is settled.
                    let ended = match ended.take() {
                        Some(ended) => ended,
                        None => self.reader_end(&mut reader, &mut handed, &mut runs).await?,
                    };
                    self.wind_down(
                        &mut runs,
                        builds,
                        lanes,
                        ended.live,
                        ended.stream_ended,
                        &mut last_confirmed,
                    )
                    .await?;
                    return Ok(Ended::Stopped);
                }
                let building = builds.running() > 0;
                tokio::select! {
                    // While the reader runs, its end (the channel closing) is what
                    // a stop wakes this loop with.
                    () = self.stop.requested(), if ended.is_some() => {
                        mark = self.waited(mark, None, &mut snapshot, building);
                    }
                    run = handed.recv(), if ended.is_none() => match run {
                        None => {
                            let end = reader.join().await?;
                            mark = if end.waiting {
                                self.waited(mark, None, &mut snapshot, building)
                            } else {
                                Instant::now()
                            };
                            ended = Some(end);
                        }
                        Some(run) => {
                            self.waited(mark, run.started(), &mut snapshot, building);
                            self.observer.on_stage(
                                StageTiming::new(Stage::Resolve, run.resolved).work(Work::Live),
                            );
                            runs.push(run);
                            self.take_handed(&mut handed, &mut runs);
                            builds.woke(Wake::Change);
                            self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                            self.confirm(&mut last_confirmed, false);
                            mark = Instant::now();
                        }
                    },
                    () = released(self.budget.as_deref(), builds.refused_since()),
                        if builds.refused().is_some() =>
                    {
                        builds.woke(Wake::Released);
                        self.take_handed(&mut handed, &mut runs);
                        self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                        self.confirm(&mut last_confirmed, false);
                        mark = Instant::now();
                    }
                    landed = builds.next(), if building => {
                        builds.woke(Wake::Landed);
                        self.take_handed(&mut handed, &mut runs);
                        self.landed(landed, &mut runs, &mut snapshot, builds, lanes).await?;
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
                        self.take_handed(&mut handed, &mut runs);
                        self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                        mark = Instant::now();
                    },
                    request = requests.recv(), if snapshot.is_none() => match request? {
                        None => {
                            mark = self.waited(mark, None, &mut snapshot, building);
                            tracing::warn!("request lane closed; no further backfills can be served");
                            requests = Box::new(ClosedConsumer);
                        }
                        Some(first) => {
                            let started = self.waited(mark, None, &mut snapshot, building);
                            let deliveries = self.collect_requests(first, &mut requests, lanes).await?;
                            snapshot = self.start_snapshot(deliveries, scopes, lanes).await?;
                            mark = Instant::now();
                            self.observer.on_stage(
                                StageTiming::new(Stage::Snapshot, mark - started).work(Work::Backfill),
                            );
                        }
                    },
                    _ = confirm_tick.tick() => {
                        mark = self.waited(mark, None, &mut snapshot, building);
                        // A backstop, not the mechanism: a batch the budget held
                        // back is granted on a release (the arm above). Should a
                        // release ever go unheard, the next tick still grants it,
                        // so it costs a second, never the run.
                        if builds.refused().is_some() {
                            builds.woke(Wake::Tick);
                            self.take_handed(&mut handed, &mut runs);
                            self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                            mark = Instant::now();
                        }
                        self.confirm(&mut last_confirmed, true);
                    }
                }
            }
        }
        .await;
        if result.is_err() {
            reader.abort_and_wait().await;
        }
        result
    }

    /// Queue every run the reader handed over and this loop hasn't taken
    /// yet. Every arm that grants takes them first: `select!` picks among
    /// ready arms at random, and a grant must see every change read before
    /// it (R3), not only those an earlier arm happened to take.
    fn take_handed(
        &self,
        handed: &mut tokio::sync::mpsc::UnboundedReceiver<reader::Run>,
        runs: &mut Runs,
    ) {
        while let Ok(run) = handed.try_recv() {
            self.observer
                .on_stage(StageTiming::new(Stage::Resolve, run.resolved).work(Work::Live));
            runs.push(run);
        }
    }

    /// The reader's end once the stop is requested: every run it hands over
    /// on the way queued, then its join (R7).
    async fn reader_end(
        &self,
        reader: &mut ReaderTask,
        handed: &mut tokio::sync::mpsc::UnboundedReceiver<reader::Run>,
        runs: &mut Runs,
    ) -> Result<reader::Ended> {
        while let Some(run) = handed.recv().await {
            self.observer
                .on_stage(StageTiming::new(Stage::Resolve, run.resolved).work(Work::Live));
            runs.push(run);
        }
        reader.join().await
    }

    /// Close the wait since `since` as the loop wakes, and return the moment,
    /// so the work that follows times from it. While builds run, the loop was
    /// waiting on them, which their [`Stage::Build`] covers. Otherwise, with
    /// no snapshot running the wait is [`Stage::Idle`]; with one, the loop
    /// was waiting on its rows too, so it is the snapshot's [`Stage::Page`]
    /// (reported with its next batch). A wait that a run ended ends `until`
    /// the run's first change arrived: from there the reader was resolving
    /// it, which its [`Stage::Resolve`] covers.
    fn waited(
        &self,
        since: Instant,
        until: Option<Instant>,
        snapshot: &mut Option<Snapshot>,
        building: bool,
    ) -> Instant {
        let now = Instant::now();
        if building {
            return now;
        }
        let waited = until
            .map_or(now, |until| until.min(now))
            .saturating_duration_since(since);
        match snapshot {
            Some(active) => active.page_wait += waited,
            None => self
                .observer
                .on_stage(StageTiming::new(Stage::Idle, waited)),
        }
        now
    }

    /// The rest of a stop, once the reader ended: finish the builds running,
    /// build and publish what the reader read, wait until every lane
    /// acknowledged it (confirming what landed as it moves), then close the
    /// live stream so the final position reaches the source.
    async fn wind_down(
        &self,
        runs: &mut Runs,
        builds: &mut Builds,
        lanes: &Lanes,
        mut live: LiveStream,
        live_ended: bool,
        last_confirmed: &mut Option<Landed>,
    ) -> Result<()> {
        tracing::info!(
            buffered = runs.changes(),
            "stop requested: reading no further changes; landing what is in flight"
        );
        self.settle(runs, builds, lanes).await?;
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

/// A release of `budget`'s bytes past generation `since`; never, without a
/// budget.
async fn released(budget: Option<&Budget>, since: u64) {
    match budget {
        Some(budget) => budget.released_since(since).await,
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
