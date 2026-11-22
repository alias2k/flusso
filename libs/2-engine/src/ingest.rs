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

mod builder;
mod builds;
mod commit;
mod confirm;
mod dispatcher;
mod gate;
mod live;
mod outstanding;
pub(crate) use outstanding::Outstanding;
pub use outstanding::STUCK_AFTER;
mod reader;
mod restart;
mod snapshot;
#[cfg(test)]
mod tests;
mod together;

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::stream::BoxStream;
use kernel::{IndexName, SinkName};
use source::SourceError;
use source::cdc::{ChangeCapture, LiveChange};
use source::document::{DocumentBuilder, DocumentId};
use stream::{Consumer, LaneItem, LaneKey, Producer, Request, Stream};
use tokio::time::{Duration, Instant, timeout_at};

use crate::budget::Budget;
use crate::error::{EngineError, Result};
use crate::index_states::IndexStates;
use crate::observer::{EngineId, NoopObserver, Observer, Stage, StageTiming, StartupStep};
use crate::policy::{Backoff, BatchPolicy, FailurePolicies, MaxWaiting};
use crate::progress::Progress;
use crate::stop::Stop;

use builder::Building;
use builds::Builds;
use confirm::Furthest;
use dispatcher::{Reports, Signals};
use live::Runs;
use reader::{Lead, Reader};
pub use snapshot::KEY_PAGE;
use snapshot::Snapshot;
use together::together;

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

/// How many documents held indexes together may have waiting, by default,
/// before reading stops for every index (`[concurrency]
/// max_pending_documents`).
pub const DEFAULT_MAX_PENDING_DOCUMENTS: usize = 1_000_000;

/// Clears what an engine run alone decides (the exemption, the budget hold)
/// when the run ends by any path.
struct ClearsAtRunEnd<'a>(&'a IndexStates);

impl Drop for ClearsAtRunEnd<'_> {
    fn drop(&mut self) {
        self.0.clear_run();
    }
}

/// What the exemption keeps for the life of the ingest engine (6b-2).
#[derive(Debug)]
struct Exemption {
    outstanding: Arc<outstanding::Outstanding>,
    log: Arc<reader::ExemptionLog>,
}

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
    /// How many documents the exempt indexes together may have waiting
    /// before the reader stops for every index (6b-2).
    max_pending_documents: usize,
    /// With several indexes, what the exemption keeps across engine runs:
    /// what each index has waiting on lanes unacknowledged (as the items on
    /// lanes are kept), and its log lines. `None` with one index, which has
    /// no exemption and pays nothing for it.
    exemption: Option<Exemption>,
    /// What an operator reads of each index (slice 7): kept across engine
    /// runs, read by the daemon's `Status`.
    index_states: Arc<IndexStates>,
    budget: Option<Arc<Budget>>,
    /// The budget waits open across engine runs (see [`Builds::with_waits`]).
    budget_waits: Arc<std::sync::Mutex<builds::Waits>>,
    /// The one key-read permit every walk shares (ADR 0012 stage 8a, K1).
    key_reads: Arc<tokio::sync::Semaphore>,
    failure_policies: FailurePolicies,
    /// How long one index's builder waits before it restarts alone (slice 8).
    backoff: Backoff,
    /// How many walks of each index completed, across runs: what starts a
    /// walk failure's waits over (slice 8).
    walks: BTreeMap<IndexName, AtomicU64>,
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
        let indexes: Vec<IndexName> = progress.indexes().cloned().collect();
        let outstanding = (indexes.len() > 1)
            .then(|| Arc::new(outstanding::Outstanding::new(indexes.iter().cloned())));
        let index_states = IndexStates::new(indexes.iter().cloned());
        let index_states = match &outstanding {
            Some(outstanding) => index_states.with_outstanding(Arc::clone(outstanding)),
            None => index_states,
        };
        let exemption = outstanding.map(|outstanding| Exemption {
            outstanding,
            log: Arc::new(reader::ExemptionLog::new(indexes)),
        });
        Self {
            source,
            resolver: Arc::clone(&documents),
            documents,
            stream,
            observer: Arc::new(NoopObserver),
            batch: BatchPolicy::default(),
            builds: 1,
            max_waiting: None,
            max_pending_documents: DEFAULT_MAX_PENDING_DOCUMENTS,
            exemption,
            index_states: Arc::new(index_states),
            budget: None,
            budget_waits: Arc::default(),
            key_reads: Arc::new(tokio::sync::Semaphore::new(1)),
            failure_policies: FailurePolicies::default(),
            backoff: Backoff::default(),
            walks: progress
                .indexes()
                .map(|index| (index.clone(), AtomicU64::new(0)))
                .collect(),
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

    /// What the engine knows of each index's exemption, holds and walk, for
    /// the daemon's `Status` to read (ADR 0012 stage 8a, slice 7).
    pub fn index_states(&self) -> Arc<IndexStates> {
        Arc::clone(&self.index_states)
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

    /// With two or more indexes, how long an index's builder that failed on
    /// its own work waits before it restarts alone (ADR 0012 stage 8a, slice
    /// 8): `backoff`'s first wait, doubling to its longest, started over once
    /// the index published again. Default: 1 s doubling to 60 s.
    pub fn with_backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
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

    /// With two or more indexes, an index held by the budget or a full lane
    /// with `max_waiting_per_index` documents waiting stops no other index's
    /// reading: its documents keep waiting, up to `max` for every such index
    /// together, where the reading stops for all (at least one; default
    /// [`DEFAULT_MAX_PENDING_DOCUMENTS`]).
    pub fn with_max_pending_documents(mut self, max: usize) -> Self {
        self.max_pending_documents = max.max(1);
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
        // Whatever ends the run (its end, a stop, an error, a drop), nothing
        // is exempt or held by the budget until the next run decides again
        // (slice 7, review of #454).
        let _clears = ClearsAtRunEnd(&self.index_states);
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

        // A builder per index with lanes, each with its own builds and
        // slots, every one asking one gate for its permits (ADR 0012 stage
        // 8a, 6b-1c).
        let gate = gate::Gate::new(
            self.builds,
            self.budget
                .clone()
                .unwrap_or_else(|| Arc::new(Budget::unlimited())),
        );
        // Each index's lanes, moved to its builder.
        let mut by_index: BTreeMap<IndexName, Lanes> = BTreeMap::new();
        for (lane, producer) in lanes {
            by_index
                .entry(lane.index.clone())
                .or_default()
                .insert(lane, producer);
        }
        let by_index: Vec<(IndexName, Lanes)> = by_index.into_iter().collect();
        let several = by_index.len() > 1;
        let mut builds: Vec<Builds> = by_index
            .iter()
            .map(|(index, _)| self.builds_of(&gate, index.clone(), several))
            .collect();
        let lanes: Vec<Lanes> = by_index.into_iter().map(|(_, lanes)| lanes).collect();
        let ended = self
            .follow(&lanes, requests, &scopes, live, &mut builds, &gate)
            .await;
        if ended.is_err() {
            for builds in &mut builds {
                builds.abandon().await;
            }
        }
        ended
    }

    /// A builder's builds of `index`, asking `gate` for their permits, beside
    /// other builders when `several`.
    fn builds_of(&self, gate: &Arc<gate::Gate>, index: IndexName, several: bool) -> Builds {
        Builds::new(self.builds, self.budget.clone())
            .with_gate(Arc::clone(gate))
            .of_index(index)
            .with_observer(Arc::clone(&self.observer))
            .with_progress(Arc::clone(&self.progress))
            .with_waits(Arc::clone(&self.budget_waits))
            .beside_others(several)
    }

    /// What `index` has done so far, by kind (slice 8's waits).
    fn done(&self, index: &IndexName) -> restart::Done {
        restart::Done {
            live: self.progress.numbered(index),
            walks: self
                .walks
                .get(index)
                .map_or(0, |walks| walks.load(Ordering::Relaxed)),
        }
    }

    /// The run, from an open live stream to its end or a stop: the reader,
    /// the dispatcher and the builder, futures of this task (ADR 0012 stage
    /// 8a, §3.8, §3.9).
    async fn follow(
        &self,
        by_index: &[Lanes],
        requests: Box<dyn Consumer<Request>>,
        scopes: &HashSet<IndexName>,
        live: LiveStream,
        builds: &mut [Builds],
        gate: &Arc<gate::Gate>,
    ) -> Result<Ended> {
        // What an earlier run marked and never published is forgotten
        // before this run's reader marks anything (R10): confirmation stayed
        // before all of it, so this run's stream replays it.
        self.progress.forget_all();
        // With two or more builders a held index at its cap is exempt
        // (6b-2); one builder keeps 6b-1b's lead.
        let lead = if let (true, Some(exemption)) = (builds.len() > 1, &self.exemption) {
            Lead::exempting(
                self.max_waiting(),
                self.progress.changes_read(),
                builds
                    .iter()
                    .filter_map(|builds| builds.index().cloned())
                    .collect(),
                self.max_pending_documents,
                Arc::clone(&exemption.outstanding),
                Arc::clone(&exemption.log),
                Arc::clone(&self.index_states),
            )
        } else {
            Lead::over(
                self.max_waiting(),
                self.progress.changes_read(),
                builds.len(),
            )
        };
        // Each builder's slice channel, request inbox, signals and result,
        // in the lead's queue order.
        let signals: Vec<Signals> = builds.iter().map(|_| Signals::default()).collect();
        let reports = Reports::default();
        let several = builds.len() > 1;
        let clock = IdleClock::new(builds.len());
        let mut senders = Vec::new();
        let mut futures = Vec::new();
        let mut dispatched = Vec::new();
        for (slot, ((builds, lanes), signals)) in builds
            .iter_mut()
            .zip(by_index.iter())
            .zip(signals.iter())
            .enumerate()
        {
            let Some(index) = builds.index().cloned() else {
                continue;
            };
            let (sender, handed) = tokio::sync::mpsc::unbounded_channel();
            senders.push((index.clone(), sender));
            let runs = Runs::new(
                self.batch.max_changes,
                self.batch.max_delay,
                Arc::clone(&lead),
                handed,
                Arc::clone(&self.observer),
            )
            .of_queue(slot);
            let (inbox, delivered) = tokio::sync::mpsc::unbounded_channel();
            let (done, result) = tokio::sync::oneshot::channel();
            futures.push(self.build(
                Building {
                    runs,
                    inbox: delivered,
                    signals,
                    reports: &reports,
                    several,
                    clock: several.then_some((&clock, slot)),
                    lanes,
                    scopes,
                    builds,
                    gate,
                    snapshot: None,
                    coalescing: None,
                    walk_failed: false,
                },
                done,
            ));
            dispatched.push(dispatcher::Builder {
                index,
                inbox,
                signals,
                result,
            });
        }
        let lead_for_ticks = Arc::clone(&lead);
        let (reading, reader) = reader::reading(
            Reader {
                resolver: Arc::clone(&self.resolver),
                progress: Arc::clone(&self.progress),
                observer: Arc::clone(&self.observer),
                stop: self.stop.clone(),
                batch: self.batch,
                lead,
                runs: senders,
                states: Some(Arc::clone(&self.index_states)),
                #[cfg(test)]
                handing: None,
                #[cfg(test)]
                checked: None,
            },
            live,
        );
        let dispatching = self.dispatch(requests, dispatched, &reports, reader, lead_for_ticks);
        // One task: the reader, the dispatcher and a builder per index
        // (§3.9 I1). The run ends with the dispatcher; returning drops the
        // reader and the builders in place (R9), so nothing of them outlives
        // the run, the next run's `forget_all` included.
        together(reading, dispatching, futures, self.stop.requested()).await
    }

    /// Close the wait since `since` as the loop wakes, and return the moment,
    /// so the work that follows times from it. While builds run (`busy`), the
    /// loop was waiting on them, which their [`Stage::Build`] covers; while
    /// requests are gathered, on them, which the snapshot's
    /// [`Stage::Snapshot`] covers. Otherwise, with
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
        busy: bool,
        clock: Option<(&IdleClock, usize)>,
    ) -> Instant {
        let now = Instant::now();
        if busy {
            return now;
        }
        let end = until.map_or(now, |until| until.min(now));
        let waited = end.saturating_duration_since(since);
        match (snapshot, clock) {
            (Some(active), _) => active.page_wait += waited,
            (None, None) => self
                .observer
                .on_stage(StageTiming::new(Stage::Idle, waited)),
            (None, Some((clock, slot))) => {
                if let Some(idle) = clock.woke(slot, end) {
                    self.observer.on_stage(StageTiming::new(Stage::Idle, idle));
                }
            }
        }
        now
    }

    /// The end of a stop, once every lane acknowledged what was built: close
    /// the live stream so the final position reaches the source.
    async fn close(&self, mut live: LiveStream, live_ended: bool) -> Result<()> {
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

/// The engine's idle time over several builders (ADR 0012 stage 8a, 6b-1c):
/// [`Stage::Idle`] is the wall time no builder is busy. Each builder marks
/// itself idle from the end of its last work, when the wait it starts is
/// idle (nothing building, no snapshot), and tells the clock when it wakes;
/// the first to wake while every builder was idle reports the span since
/// the last of them went idle. One builder reports its own waits, as 6b-1b
/// did, with no clock.
#[derive(Debug)]
pub(crate) struct IdleClock(Mutex<Vec<Option<Instant>>>);

impl IdleClock {
    fn new(builders: usize) -> Self {
        Self(Mutex::new(vec![None; builders]))
    }

    /// Builder `slot` is idle from `since` (`None`: its wait isn't idle).
    pub(crate) fn idle(&self, slot: usize, since: Option<Instant>) {
        let mut idle = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(at) = idle.get_mut(slot) {
            *at = since;
        }
    }

    /// Builder `slot` woke at `end`: the engine's idle span that ends here,
    /// if every builder was idle.
    fn woke(&self, slot: usize, end: Instant) -> Option<Duration> {
        let mut idle = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let all = idle.iter().all(Option::is_some);
        let start = idle.iter().flatten().max().copied();
        if let Some(at) = idle.get_mut(slot) {
            *at = None;
        }
        all.then(|| start.map(|start| end.saturating_duration_since(start)))
            .flatten()
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
