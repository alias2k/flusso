//! The sink engine: one per sink. Stage the sink's indexes, request whatever
//! backfill the sink needs, then receive → write → acknowledge, with up to
//! `writes` lane items written at once.
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
//! The loop ([`SinkEngine::run`], in `writes`) writes each lane item with
//! one [`Sink::write`], several at once but never two holding the same
//! document, decides item-level rejections by the failure policies, and
//! acknowledges: write-then-ack is what makes delivery at-least-once, because
//! the acknowledgement is what moves this lane's watermark, and the lane's
//! watermark never passes an item still being written. A `SnapshotComplete`
//! marker records its indexes as seeded once everything before it is written.
//! A reindex arrives as a control message and is staged once the writes in
//! flight returned: `reindex` + `ensure_index` on the sink, then a fresh
//! `Backfill` request.
//!
//! The loop also reports the sink's two **fence levels** to the observer, in
//! position order whatever order the writes finish in (`fences`). *Applied* is
//! the last position before the oldest one still being written, reported
//! before the acknowledgement that lets the watermark pass it. *Searchable*
//! trails it by what each write reported ([`Visibility`](sink::Visibility)), so a searchable
//! position always means "this and everything before it". The engine only
//! observes; nothing here asks the sink to refresh because a reader waits.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use kernel::{IndexMapping, IndexName, SinkName};
use sink::{Sink, SinkOptions};
use source::cdc::Continuity;
use stream::{Request, Stream};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, sleep_until};

use self::fences::Fences;

use crate::error::{EngineError, Result, StagingFailures};
use crate::observer::{EngineId, NoopObserver, Observer, Stage, StageTiming, StartupStep};
use crate::policy::FailurePolicies;
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

mod fences;
mod writes;

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
    /// The fence levels over the positions received, in position order.
    /// Survives a restart of this engine's run, so a redelivered item keeps
    /// its place and a position written before a failure is still reported
    /// once its time comes.
    fences: Mutex<Fences>,
    stop: Stop,
    /// Indexes whose snapshot batches were acknowledged unapplied during a
    /// stop: their `SnapshotComplete` must not mark them seeded.
    skipped: Mutex<BTreeSet<IndexName>>,
    /// Lane items acknowledged over this engine's life, across restarts.
    acknowledged: AtomicU64,
    /// Lane items written at once.
    writes: usize,
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
            fences: Mutex::new(Fences::default()),
            stop: Stop::new(),
            skipped: Mutex::new(BTreeSet::new()),
            acknowledged: AtomicU64::new(0),
            writes: 1,
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

    /// How many lane items this engine writes at once: at least one, which
    /// is the default. The daemon sets `[concurrency] writes`.
    pub fn with_writes(mut self, writes: usize) -> Self {
        self.writes = writes.max(1);
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

    /// Follow the lane until it closes and everything taken from it is
    /// written (`Ok`), or until a sink, stream, or `stop` policy error stops
    /// the engine (`Err`), once the writes in flight beside it returned.
    /// Control messages arriving on `control` are handled between writes.
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
        let lane = self.stream.lane(&self.name)?.consumer;
        writes::Run::new(self, lane).run(control).await
    }

    /// Close an idle window (nothing in flight, nothing taken) and report it;
    /// return the moment it closed.
    pub(super) fn idle_until(&self, since: Instant) -> Instant {
        let now = Instant::now();
        self.observer
            .on_stage(StageTiming::new(Stage::Idle, now - since).sink(&self.name));
        now
    }

    fn lock_fences(&self) -> std::sync::MutexGuard<'_, Fences> {
        self.fences.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_skipped(&self) -> std::sync::MutexGuard<'_, BTreeSet<IndexName>> {
        self.skipped.lock().unwrap_or_else(PoisonError::into_inner)
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
