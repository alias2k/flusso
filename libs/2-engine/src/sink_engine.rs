//! The sink engine: one per sink. Stage the sink's indexes, request whatever
//! backfill the sink needs, then receive → apply → flush → acknowledge.
//!
//! Staging ([`SinkEngine::stage`]) is the sink's own seeding decision: ensure
//! every index, retire seeds the source can no longer honor (a `Fresh`
//! continuity), and send one `Backfill` request for the indexes still
//! unseeded. The daemon runs every sink engine's staging before it lets the
//! ingest engine `prepare` the source, which preserves the #120 ordering
//! without a shared engine: rebuilds are staged before the resume point exists,
//! and the resume point exists before any snapshot runs.
//!
//! The indexes are staged concurrently, a few at a time
//! ([`STAGE_CONCURRENCY`]): each index's own steps stay in order, and nothing
//! orders one index against another, since each has its own target on the
//! destination. Staging is all or nothing per pass: once an index fails no
//! further index is started, the ones in flight finish, the error names every
//! failure seen, and the daemon retries the whole stage (each step is
//! idempotent), which starts the rest.
//!
//! The loop ([`SinkEngine::run`]) applies each batch's envelopes, flushes once
//! at the batch boundary, decides item-level rejections by the failure
//! policies, and acknowledges: flush-then-ack is what makes delivery
//! at-least-once, because the acknowledgement is what moves this lane's
//! watermark. A `SnapshotComplete` marker records its indexes as seeded. A
//! reindex arrives as a control message and is staged between two batches:
//! `reindex` + `ensure_index` on the sink, then a fresh `Backfill` request.
//!
//! The loop also reports the sink's two **fence levels** to the observer.
//! *Applied* is the position of the batch just flushed, reported before its
//! acknowledgement (see the loop for why the order matters). *Searchable*
//! trails it by what each flush reported ([`Visibility`]): the engine queues
//! every positioned flush with the moment it becomes searchable and reports the
//! position of the last one that has, in order, so a searchable position always
//! means "this and everything before it". The engine only observes; nothing
//! here asks the sink to refresh.

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use kernel::{IndexMapping, IndexName, Position, SinkName};
use sink::{Sink, SinkOptions, Visibility};
use source::cdc::Continuity;
use stream::{LaneItem, Request, Stream};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, sleep_until};

use crate::error::{EngineError, Result, StagingFailures};
use crate::observer::{
    CommitStats, EngineId, NoopObserver, Observer, Stage, StageTiming, StartupStep, Work,
};
use crate::policy::{FailurePolicies, FailurePolicy};
use crate::stop::Stop;

/// How many of a sink's indexes are staged at once.
///
/// Staging is a few round trips per index, so overlapping them is most of the
/// gain. A destination such as OpenSearch applies index creations one
/// cluster-state update at a time, so more in flight only queues on its
/// master; the bound keeps a deployment with hundreds of indexes from
/// flooding it, or the client's connection pool, at start.
pub(crate) const STAGE_CONCURRENCY: usize = 8;

/// The longest an idle window stays open before it is reported, so a quiet
/// sink reads as idle in every scrape, not as one spike when work resumes.
const IDLE_TICK: Duration = Duration::from_secs(1);

/// An operation the daemon hands a running sink engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkControl {
    /// Rebuild `indexes` into fresh generations: stage them and request a
    /// snapshot. Handled between two batches; nothing restarts.
    Reindex { indexes: Vec<IndexName> },
}

/// What staging does to one index.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// `ensure_index`, then `is_seeded` when asked.
    Ensure { ask_seeded: bool },
    /// `reindex` and `ensure_index`, then `is_seeded` when asked.
    Rebuild { ask_seeded: bool },
}

/// One sink's engine over its lane.
#[derive(Debug)]
pub struct SinkEngine {
    name: SinkName,
    sink: Arc<dyn Sink>,
    stream: Arc<dyn Stream>,
    mappings: Vec<IndexMapping>,
    options: SinkOptions,
    observer: Arc<dyn Observer>,
    failure_policies: FailurePolicies,
    skip_backfill: bool,
    /// Indexes with a `Backfill` request published and no `SnapshotComplete`
    /// received yet. Survives a restart of this engine's run, so re-staging
    /// doesn't ask for a snapshot the ingest engine is already serving.
    outstanding: Mutex<BTreeSet<IndexName>>,
    /// Flushed positions not yet reported as searchable,
    /// oldest first. Survives a restart of this engine's run, so a position
    /// flushed before a failure is still reported once its time comes.
    unsearchable: Mutex<VecDeque<(Position, Searchable)>>,
    stop: Stop,
    /// Indexes whose snapshot batches were acknowledged unapplied during a
    /// stop: their `SnapshotComplete` must not mark them seeded.
    skipped: Mutex<BTreeSet<IndexName>>,
    /// Lane items acknowledged over this engine's life, across restarts.
    acknowledged: AtomicU64,
}

/// When a flushed position becomes searchable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Searchable {
    /// At this instant (already passed for an immediate flush).
    At(Instant),
    /// When a later flush of this sink reports [`Visibility::Immediate`].
    WithNextImmediate,
}

impl SinkEngine {
    /// An engine for `sink`, named `name` in the config, over its lane on
    /// `stream`, maintaining `mappings`.
    pub fn new(
        name: SinkName,
        sink: Arc<dyn Sink>,
        stream: Arc<dyn Stream>,
        mappings: Vec<IndexMapping>,
    ) -> Self {
        Self {
            name,
            sink,
            stream,
            mappings,
            options: SinkOptions::default(),
            observer: Arc::new(NoopObserver),
            failure_policies: FailurePolicies::default(),
            skip_backfill: false,
            outstanding: Mutex::new(BTreeSet::new()),
            unsearchable: Mutex::new(VecDeque::new()),
            stop: Stop::new(),
            skipped: Mutex::new(BTreeSet::new()),
            acknowledged: AtomicU64::new(0),
        }
    }

    /// The deployment's stop: once requested, a snapshot batch still on the
    /// lane is acknowledged without being applied, and its index left
    /// unseeded. The backfill was abandoned with the stop and runs again on
    /// the next start, so writing it now would only hold the drain up.
    pub fn with_stop(mut self, stop: Stop) -> Self {
        self.stop = stop;
        self
    }

    /// The universal sink keys (`backfill`).
    pub fn with_options(mut self, options: SinkOptions) -> Self {
        self.options = options;
        self
    }

    /// Report lifecycle and progress to `observer`.
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// How item-level rejections are decided, per index.
    pub fn with_failure_policies(mut self, policies: FailurePolicies) -> Self {
        self.failure_policies = policies;
        self
    }

    /// Never request a backfill (`--skip-backfill`): unseeded indexes stay so.
    pub fn skip_backfill(mut self, skip: bool) -> Self {
        self.skip_backfill = skip;
        self
    }

    /// How many lane items this engine has acknowledged since it was built,
    /// across restarts of its run: a supervisor reads it to tell a run that
    /// made progress from one that failed where the last one did.
    pub fn acknowledged(&self) -> u64 {
        self.acknowledged.load(Ordering::Relaxed)
    }

    /// The sink this engine drives.
    pub fn name(&self) -> &SinkName {
        &self.name
    }

    /// Ensure every index, retire stale seeds under a `Fresh` source, and
    /// request a backfill for what is still unseeded. Idempotent; the daemon
    /// runs it before the ingest engine prepares the source, and again before
    /// each restart of this engine (with `Resumed`).
    ///
    /// The indexes are staged concurrently, at most eight at a time, each
    /// index's own steps in order. After a failure no further index is
    /// started; the error names every failure seen.
    #[tracing::instrument(name = "sink.stage", skip_all, fields(sink = %self.name, ?continuity))]
    pub async fn stage(&self, continuity: Continuity) -> Result<()> {
        let started = Instant::now();
        let backfill = self.options.backfill;
        let mut seeded = self
            .each_index(
                self.mappings.iter(),
                Step::Ensure {
                    ask_seeded: backfill,
                },
            )
            .await?;
        self.observer
            .on_indexes_ensured(&self.name, self.mappings.len());

        if !backfill {
            tracing::info!("backfill disabled for this sink; live changes only");
            self.staged(started);
            return Ok(());
        }

        if continuity == Continuity::Fresh {
            self.rebuild_stale_seeds(&mut seeded).await?;
        }

        let already_requested = self.outstanding();
        let unseeded: Vec<IndexName> = seeded
            .into_iter()
            .filter(|(mapping, seeded)| !seeded && !already_requested.contains(&mapping.index))
            .map(|(mapping, _)| mapping.index.clone())
            .collect();
        if self.skip_backfill {
            if !unseeded.is_empty() {
                tracing::warn!(
                    indexes = ?unseeded.iter().map(|i| i.as_ref()).collect::<Vec<_>>(),
                    "skipping backfill (skip_backfill set); these indexes stay unseeded"
                );
            }
        } else if !unseeded.is_empty() {
            self.request_backfill(unseeded).await?;
        } else {
            tracing::info!("every index is seeded; no backfill needed");
        }
        self.staged(started);
        Ok(())
    }

    fn staged(&self, started: Instant) {
        self.observer
            .on_startup_step(StartupStep::Stage, Some(&self.name), started.elapsed());
        self.observer.on_sink_started(&self.name);
    }

    /// The source has no resume point, so every seed this sink recorded is
    /// stale: the changes between that seed and now are unobservable. Each
    /// index still reported as seeded gets a from-scratch rebuild staged (a
    /// fresh target, so rows gone from the source are dropped on the swap) and
    /// re-announced, so the backfill that follows reseeds it while the old copy
    /// keeps serving. Indexes already unseeded are left alone: they are about
    /// to be rebuilt anyway, and staging again would orphan the target already
    /// in flight. Under `skip_backfill` nothing is staged: a staged target
    /// nobody backfills would take the live writes while the old copy keeps
    /// serving reads.
    ///
    /// `seeded` is each index with whether it is seeded; a rebuilt index's
    /// answer is asked again.
    async fn rebuild_stale_seeds(&self, seeded: &mut [(&IndexMapping, bool)]) -> Result<()> {
        let stale: Vec<&IndexMapping> = seeded
            .iter()
            .filter(|(_, seeded)| *seeded)
            .map(|(mapping, _)| *mapping)
            .collect();
        if stale.is_empty() {
            return Ok(());
        }
        let names: Vec<&str> = stale.iter().map(|m| m.index.as_ref()).collect();
        tracing::warn!(
            indexes = ?names,
            skip_backfill = self.skip_backfill,
            "the source has no resume point, so the changes since these indexes were seeded are \
             lost; rebuilding them from scratch (with skip_backfill they are served as-is)",
        );
        if self.skip_backfill {
            return Ok(());
        }
        let rebuilt = self
            .each_index(stale, Step::Rebuild { ask_seeded: true })
            .await?;
        for (mapping, now) in rebuilt {
            if let Some((_, seeded)) = seeded.iter_mut().find(|(m, _)| m.index == mapping.index) {
                *seeded = now;
            }
        }
        Ok(())
    }

    /// Run `step` for each of `mappings`, at most [`STAGE_CONCURRENCY`] at
    /// once, and return each mapping with whether it is seeded (`false` when
    /// not asked), in the order given.
    ///
    /// After a failure no further index is started: a destination that is
    /// down or never answers fails every index alike, and each wave would cost
    /// one more request timeout. The ones in flight run to their end, and the
    /// error names every failure seen; the daemon retries the whole stage,
    /// which starts the rest.
    async fn each_index<'a>(
        &self,
        mappings: impl IntoIterator<Item = &'a IndexMapping>,
        step: Step,
    ) -> Result<Vec<(&'a IndexMapping, bool)>> {
        let mut waiting = mappings.into_iter().enumerate();
        let mut running = FuturesUnordered::new();
        let mut outcomes = Vec::new();
        let mut failed_once = false;
        loop {
            while !failed_once && running.len() < STAGE_CONCURRENCY {
                let Some((order, mapping)) = waiting.next() else {
                    break;
                };
                running.push(async move { (order, mapping, self.step(step, mapping).await) });
            }
            let Some(outcome) = running.next().await else {
                break;
            };
            failed_once |= outcome.2.is_err();
            outcomes.push(outcome);
        }
        outcomes.sort_unstable_by_key(|(order, _, _)| *order);
        let mut done = Vec::with_capacity(outcomes.len());
        let mut failed = Vec::new();
        for (_, mapping, outcome) in outcomes {
            match outcome {
                Ok(seeded) => done.push((mapping, seeded)),
                Err(error) => failed.push((mapping.index.clone(), error)),
            }
        }
        if failed.is_empty() {
            Ok(done)
        } else {
            Err(EngineError::Staging(StagingFailures(failed)))
        }
    }

    /// One index's staging steps, in order.
    async fn step(&self, step: Step, mapping: &IndexMapping) -> sink::Result<bool> {
        let ask_seeded = match step {
            Step::Ensure { ask_seeded } => ask_seeded,
            Step::Rebuild { ask_seeded } => {
                self.sink.reindex(mapping).await?;
                ask_seeded
            }
        };
        self.sink.ensure_index(mapping).await?;
        if ask_seeded {
            self.sink.is_seeded(&mapping.index).await
        } else {
            Ok(false)
        }
    }

    async fn request_backfill(&self, indexes: Vec<IndexName>) -> Result<()> {
        tracing::info!(indexes = indexes.len(), "requesting backfill");
        let producer = self.stream.requests()?.producer;
        producer
            .publish(Request::Backfill {
                sink: self.name.clone(),
                indexes: indexes.clone(),
            })
            .await?;
        self.lock_outstanding().extend(indexes.iter().cloned());
        self.observer.on_backfill_requested(&self.name, &indexes);
        Ok(())
    }

    fn outstanding(&self) -> BTreeSet<IndexName> {
        self.lock_outstanding().clone()
    }

    fn lock_outstanding(&self) -> std::sync::MutexGuard<'_, BTreeSet<IndexName>> {
        self.outstanding
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Follow the lane until it closes (`Ok`) or a sink, stream, or `stop`
    /// policy error stops the engine (`Err`). Control messages arriving on
    /// `control` are handled between batches.
    #[tracing::instrument(name = "sink.run", skip_all, fields(sink = %self.name))]
    pub async fn run(&self, control: &mut mpsc::Receiver<SinkControl>) -> Result<()> {
        let result = self.run_inner(control).await;
        let id = EngineId::Sink(self.name.clone());
        match &result {
            Ok(()) => {
                tracing::info!("sink engine stopped: lane closed");
                self.observer.on_engine_stopped(&id);
            }
            Err(error) => {
                tracing::error!(%error, "sink engine stopped on error");
                self.observer.on_engine_error(&id, &error.to_string());
            }
        }
        result
    }

    async fn run_inner(&self, control: &mut mpsc::Receiver<SinkControl>) -> Result<()> {
        let mut lane = self.stream.lane(&self.name)?.consumer;
        let mut control_open = true;
        let mut idle_since = Instant::now();
        let mut idle_tick = tokio::time::interval_at(idle_since + IDLE_TICK, IDLE_TICK);
        idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let next_searchable = self.next_searchable_at();
            tokio::select! {
                delivery = lane.recv() => match delivery? {
                    None => return Ok(()),
                    Some(delivery) => {
                        self.idle_until(idle_since);
                        let (item, handle) = delivery.into_parts();
                        let flushed = match item {
                            LaneItem::Batch(batch)
                                if batch.position.is_none() && self.stop.is_requested() =>
                            {
                                self.lock_skipped()
                                    .extend(batch.envelopes.iter().map(|e| e.index.clone()));
                                None
                            }
                            LaneItem::Batch(batch) => {
                                let visibility = self.commit(&batch, lane.is_empty()).await?;
                                Some((batch.position, visibility))
                            }
                            LaneItem::SnapshotComplete { indexes } => {
                                let skipped = self.lock_skipped().clone();
                                for index in indexes.iter().filter(|index| skipped.contains(*index)) {
                                    tracing::info!(%index, "stopping: its backfill was cut short, so it stays unseeded and is backfilled on the next start");
                                }
                                let indexes: Vec<IndexName> = indexes
                                    .into_iter()
                                    .filter(|index| !skipped.contains(index))
                                    .collect();
                                for index in &indexes {
                                    let started = Instant::now();
                                    self.sink.mark_seeded(index).await?;
                                    self.observer.on_stage(
                                        StageTiming::new(Stage::Seed, started.elapsed())
                                            .work(Work::Backfill)
                                            .sink(&self.name)
                                            .index(index),
                                    );
                                    self.observer.on_index_seeded(&self.name, index);
                                }
                                let mut outstanding = self.lock_outstanding();
                                for index in &indexes {
                                    outstanding.remove(index);
                                }
                                if !indexes.is_empty() {
                                    tracing::info!(indexes = indexes.len(), "backfill complete");
                                }
                                None
                            }
                        };
                        // A stop skips snapshot batches unflushed, so the last
                        // live flush may have run while they were still queued
                        // (not caught up) and left its refresh to a flush that
                        // never comes.
                        let flushed = match flushed {
                            None if self.stop.is_requested()
                                && lane.is_empty()
                                && self.waits_for_an_immediate_flush() =>
                            {
                                let started = Instant::now();
                                let report = self.sink.flush(true).await?;
                                self.observer.on_stage(
                                    StageTiming::new(Stage::Write, started.elapsed())
                                        .work(Work::Live)
                                        .sink(&self.name),
                                );
                                Some((None, report.visibility))
                            }
                            flushed => flushed,
                        };
                        // Before the acknowledgement: applied is a fact about
                        // the destination, and a listener must hear of it
                        // before the source can be told the position is
                        // confirmed and forget it.
                        if let Some((position, visibility)) = flushed {
                            self.report_positions(position, visibility);
                        }
                        handle.ack().await?;
                        self.acknowledged.fetch_add(1, Ordering::Relaxed);
                        idle_since = Instant::now();
                    }
                },
                message = control.recv(), if control_open => {
                    let started = self.idle_until(idle_since);
                    match message {
                        None => control_open = false,
                        Some(SinkControl::Reindex { indexes }) => {
                            self.reindex(indexes).await?;
                            self.observer.on_stage(
                                StageTiming::new(Stage::Reindex, started.elapsed()).sink(&self.name),
                            );
                        }
                    }
                    idle_since = Instant::now();
                }
                () = until(next_searchable) => {
                    self.idle_until(idle_since);
                    self.report_searchable();
                    idle_since = Instant::now();
                }
                _ = idle_tick.tick() => idle_since = self.idle_until(idle_since),
            }
        }
    }

    /// Close an idle window (nothing in flight, nothing taken) and report it;
    /// return the moment it closed.
    fn idle_until(&self, since: Instant) -> Instant {
        let now = Instant::now();
        self.observer
            .on_stage(StageTiming::new(Stage::Idle, now - since).sink(&self.name));
        now
    }

    /// Report a flush that returned: its position is applied, and it joins the
    /// queue of positions waiting to become searchable. A snapshot batch has no
    /// position, but an immediate flush of one still releases what earlier
    /// flushes deferred.
    fn report_positions(&self, position: Option<Position>, visibility: Visibility) {
        // The caught-up path, and the hot one: nothing is waiting and this
        // flush is searchable already, so there is nothing to queue.
        if visibility == Visibility::Immediate && self.lock_unsearchable().is_empty() {
            if let Some(position) = position {
                self.observer.on_position_applied(&self.name, position);
                self.observer.on_position_searchable(&self.name, position);
            }
            return;
        }
        let now = Instant::now();
        {
            let mut queue = self.lock_unsearchable();
            if visibility == Visibility::Immediate {
                for (_, due) in queue.iter_mut() {
                    if *due == Searchable::WithNextImmediate {
                        *due = Searchable::At(now);
                    }
                }
            }
            if let Some(position) = position {
                queue.push_back((
                    position,
                    match visibility {
                        Visibility::Immediate => Searchable::At(now),
                        Visibility::Within(delay) => Searchable::At(now + delay),
                        Visibility::Deferred => Searchable::WithNextImmediate,
                    },
                ));
            }
        }
        if let Some(position) = position {
            self.observer.on_position_applied(&self.name, position);
        }
        self.report_searchable();
    }

    /// Report the last queued position whose time has come, and drop it and
    /// everything before it. In order: a position that is not searchable yet
    /// holds back the ones behind it.
    fn report_searchable(&self) {
        let now = Instant::now();
        let mut reached = None;
        {
            let mut queue = self.lock_unsearchable();
            while let Some((position, Searchable::At(due))) = queue.front().copied() {
                if due > now {
                    break;
                }
                reached = Some(position);
                queue.pop_front();
            }
        }
        if let Some(position) = reached {
            self.observer.on_position_searchable(&self.name, position);
        }
    }

    /// Whether a flushed position waits for the next immediately visible flush
    /// to become searchable.
    fn waits_for_an_immediate_flush(&self) -> bool {
        self.lock_unsearchable()
            .iter()
            .any(|(_, due)| *due == Searchable::WithNextImmediate)
    }

    /// When the oldest queued position becomes searchable, if that is a matter
    /// of time.
    fn next_searchable_at(&self) -> Option<Instant> {
        match self.lock_unsearchable().front() {
            Some((_, Searchable::At(due))) => Some(*due),
            _ => None,
        }
    }

    fn lock_skipped(&self) -> std::sync::MutexGuard<'_, BTreeSet<IndexName>> {
        self.skipped.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_unsearchable(&self) -> std::sync::MutexGuard<'_, VecDeque<(Position, Searchable)>> {
        self.unsearchable
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Apply every envelope, flush once, decide rejections by policy. The
    /// caller acknowledges only after this returns `Ok`: a crash before the
    /// flush leaves the batch unacknowledged and redelivered, re-applied
    /// idempotently; a `stop` rejection returns `Err` with the batch still
    /// unacknowledged. Returns when the flush's documents become searchable.
    #[tracing::instrument(name = "sink.commit", level = "debug", skip_all, fields(envelopes = batch.envelopes.len(), caught_up))]
    async fn commit(&self, batch: &stream::Batch, caught_up: bool) -> Result<Visibility> {
        let work = if batch.position.is_some() {
            Work::Live
        } else {
            Work::Backfill
        };
        let apply_start = Instant::now();
        for envelope in batch.envelopes.iter() {
            self.sink.apply(envelope).await?;
        }
        let flush_start = Instant::now();
        let report = self.sink.flush(caught_up).await?;
        let written = Instant::now();
        let flush = written - flush_start;
        self.observer.on_stage(
            StageTiming::new(Stage::Write, written - apply_start)
                .work(work)
                .sink(&self.name),
        );

        // A single `stop`-policy rejection stops the run, decided *before* any
        // quarantine event is emitted, so a `skip` document in the same batch
        // isn't double-counted when the unacknowledged batch is redelivered.
        if !report.is_clean() {
            let mut stop_count = 0usize;
            let mut stop_example = String::new();
            for doc in &report.rejected {
                if self.failure_policies.resolve(&doc.index) == FailurePolicy::Stop {
                    if stop_count == 0 {
                        stop_example =
                            format!("{}/{}: {}", doc.index, shown_id(&doc.id), doc.reason);
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
                    id = %shown_id(&doc.id),
                    reason = %doc.reason,
                    "document rejected by sink; quarantining and continuing",
                );
                self.observer
                    .on_document_quarantined(&self.name, &doc.index, &doc.id, &doc.reason);
            }
        }

        self.observer.on_batch_committed(
            &self.name,
            CommitStats::new(batch.envelopes.len(), batch.changes, flush, work).sent(report.bytes),
        );
        Ok(report.visibility)
    }

    /// Stage a rebuild of `indexes` and request their snapshot. The current
    /// generation keeps serving until the snapshot completes and the sink swaps.
    /// A sink with `backfill = false` has nothing to rebuild; an index whose
    /// snapshot is still outstanding is left to that snapshot.
    async fn reindex(&self, indexes: Vec<IndexName>) -> Result<()> {
        if !self.options.backfill {
            tracing::warn!("reindex requested, but backfill is disabled for this sink; ignoring");
            return Ok(());
        }
        let outstanding = self.outstanding();
        let mut chosen = Vec::new();
        for index in indexes {
            let Some(mapping) = self.mappings.iter().find(|m| m.index == index) else {
                tracing::warn!(%index, "reindex requested for an index this sink does not maintain; ignoring");
                continue;
            };
            if outstanding.contains(&index) {
                tracing::warn!(%index, "reindex requested while its snapshot is still outstanding; ignoring");
                continue;
            }
            // Two chains for one index at once are what the `Sink` port forbids.
            if chosen.iter().any(|m: &&IndexMapping| m.index == index) {
                continue;
            }
            tracing::info!(%index, "reindex requested; staging a fresh generation");
            chosen.push(mapping);
        }
        let staged: Vec<IndexName> = self
            .each_index(chosen, Step::Rebuild { ask_seeded: false })
            .await?
            .into_iter()
            .map(|(mapping, _)| mapping.index.clone())
            .collect();
        if !staged.is_empty() {
            self.request_backfill(staged).await?;
        }
        Ok(())
    }
}

/// Sleep until `at`; never, when nothing is waiting on time. Keeps the loop
/// from building a timer on every pass just to leave it unpolled.
async fn until(at: Option<Instant>) {
    match at {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// The longest id a log line or an error message carries whole, in characters.
const SHOWN_ID_CHARS: usize = 64;

/// A document id as a log line or an error message shows it: whole when
/// short, else its first [`SHOWN_ID_CHARS`] characters and its length. An id
/// has no upper bound (a `bytea` key renders two characters per byte), and a
/// `stop` rejection is logged again on every restart.
fn shown_id(id: &str) -> std::borrow::Cow<'_, str> {
    match id.char_indices().nth(SHOWN_ID_CHARS) {
        None => id.into(),
        Some((cut, _)) => {
            let head = id.get(..cut).unwrap_or(id);
            format!("{head}… ({} bytes)", id.len()).into()
        }
    }
}

#[cfg(test)]
mod shown_id_tests {
    use super::shown_id;

    #[test]
    fn a_short_id_is_shown_whole() {
        assert_eq!(shown_id("42"), "42");
        assert_eq!(shown_id(&"é".repeat(64)), "é".repeat(64));
    }

    #[test]
    fn a_long_id_is_cut_at_a_character_and_says_its_length() {
        assert_eq!(
            shown_id(&"k".repeat(600)),
            format!("{}… (600 bytes)", "k".repeat(64))
        );
        assert_eq!(
            shown_id(&"é".repeat(65)),
            format!("{}… (130 bytes)", "é".repeat(64))
        );
    }
}
