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
//! an item counts as landed in the engine's [`Progress`] only once its write
//! returned, and an item not acknowledged is delivered again. A `SnapshotComplete`
//! marker records its indexes as seeded once everything before it is written.
//! A reindex arrives as a control message and is staged once the writes in
//! flight returned: `reindex` + `ensure_index` on the sink, then a fresh
//! `Backfill` request.
//!
//! **A backfill cut short by a new target** (issue #428). Each request
//! carries a [`Ticket`], increasing, and every snapshot item names the ticket
//! it answers. The engine keeps, beside each request, the target
//! ([`sink::Ensured::target`]) it was asked for. Every staging of an index
//! whose backfill is outstanding compares, right after `ensure_index` and
//! before any other step that can fail, the target it finds with that one:
//! another target (or a restage, which only runs after a write found the
//! target missing) means the pages that backfill wrote are gone, and the
//! backfill is **cut**. A cut is kept on the request until a fresh one is
//! published, so a staging that fails after finding it loses nothing: from
//! the cut on, no item of the old request marks or seeds the index, and every
//! later staging asks again. Asking again forgets the checkpoint and asks
//! from the first key under a new ticket. The old snapshot's remaining pages
//! still land: each is a build of its row, delivered in lane order with the
//! live changes, so writing it is never a step back, only work the fresh
//! backfill repeats. But they record no walk mark, and the old
//! `SnapshotComplete`, naming an older ticket, seeds nothing: only the fresh
//! backfill's does. A seed the sink refuses because its target changed
//! ([`sink::SinkError::TargetChanged`]) stages the lane again the same way.
//! Other lanes of the same snapshot (another sink, another index) answer
//! their own tickets and are untouched.
//!
//! ```text
//! lane     p₁ p₂ │delete│ p₃ p₄ Complete(1) q₁ q₂ q₃ q₄ Complete(2)
//! target   A  A        B  B   (not seeded)  B  B  B  B  seeded
//! ```
//!
//! The loop also reports the sink's two **fence levels**, per lane, in item
//! key order whatever order the writes finish in (`fences`): to the progress
//! by the item's [`ItemKey`](stream::ItemKey), to the observer by its position.
//! *Applied* is the last item before the oldest one still being written,
//! reported before the item's acknowledgement. *Searchable* trails it by what
//! each write reported ([`Visibility`](sink::Visibility)), so a searchable item
//! always means "this and everything before it on its lane". The engine only
//! observes; nothing here asks the sink to refresh because a reader waits.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use kernel::{IndexMapping, IndexName, SinkName};
use sink::{Sink, SinkOptions, TargetId};
use source::cdc::Continuity;
use stream::{LaneKey, Request, Stream, Ticket, WalkMark};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, sleep_until};

use self::fences::Fences;
use self::health::LaneState;
pub use self::health::{LaneHealth, ParkReason, ProbeOutcome};

use crate::error::{EngineError, Result, StagingFailures};
use crate::observer::{EngineId, NoopObserver, Observer, Stage, StageTiming, StartupStep};
use crate::policy::FailurePolicies;
use crate::progress::Progress;
use crate::stop::Stop;

/// How many of a sink's indexes are staged at once.
///
/// Staging is a few round trips per index, so overlapping them is most of the
/// gain. A destination such as OpenSearch applies index creations one
/// cluster-state update at a time, so more in flight only queues on its
/// master; the bound keeps a deployment with hundreds of indexes from
/// flooding it, or the client's connection pool, at start.
pub(crate) const STAGE_CONCURRENCY: usize = 8;

/// Why a reindex staged as the stop came is left owed (X19).
const STAGING_AT_THE_STOP: &str =
    "the reindex was staging as the stop came; the next start picks it up";

/// The longest an idle window stays open before it is reported, so a quiet
/// sink reads as idle in every scrape, not as one spike when work resumes.
const IDLE_TICK: Duration = Duration::from_secs(1);

mod fences;
pub(crate) mod flights;
mod health;
mod walked;
mod writes;

/// Which lanes a failed run is blamed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Blame {
    /// The last run ended cleanly: no barrier, every landing is progress.
    Nobody,
    /// A fresh engine, or a failure no one lane caused: every lane has a
    /// barrier, every landing is progress.
    Everyone,
    /// A failure on these lanes: only they have a barrier, and only a landing
    /// on them is progress.
    Lanes(BTreeSet<LaneKey>),
}

impl Blame {
    /// Whether `lane` is blamed (or nothing is, and so everyone counts).
    pub(crate) fn covers(&self, lane: &LaneKey) -> bool {
        match self {
            Blame::Nobody | Blame::Everyone => true,
            Blame::Lanes(lanes) => lanes.contains(lane),
        }
    }
}

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
    /// `ensure_index` after a write found the target missing, then
    /// `is_seeded` when the sink is backfilled.
    Restage,
}

/// A backfill an index was asked for and not yet seeded by.
#[derive(Debug, Clone)]
struct Asked {
    /// The latest request's ticket.
    ticket: Ticket,
    /// The target the request was asked for, if the sink names it.
    target: Option<TargetId>,
    /// Staging found the index in another target since: no item of this
    /// request marks or seeds it, and the next staging asks again.
    cut: bool,
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
    /// Indexes with a `Backfill` request published and not seeded yet, each
    /// with its latest request. Survives a restart of this engine's run, so
    /// re-staging doesn't ask for a snapshot the ingest engine is already
    /// serving.
    outstanding: Mutex<BTreeMap<IndexName, Asked>>,
    /// The last ticket a request of this engine carried.
    pub(super) tickets: AtomicU64,
    /// The fence levels over the live items received, per lane in key order.
    /// Survives a restart of this engine's run, so a redelivered item keeps
    /// its place and an item written before a failure is still reported once
    /// its time comes.
    fences: Mutex<Fences>,
    /// Where landings and searchable positions are reported, for
    /// confirmation and fences.
    progress: Arc<Progress>,
    stop: Stop,
    /// Indexes whose snapshot batches were acknowledged unapplied during a
    /// stop: their `SnapshotComplete` must not mark them seeded.
    skipped: Mutex<BTreeSet<IndexName>>,
    /// How often the oldest unfinished lane item landed, over this engine's
    /// life, across restarts.
    advanced: AtomicU64,
    /// Lane items written at once.
    writes: usize,
    /// The most documents one write holds when ready items are merged.
    write_documents: usize,
    /// The longest a ready live item waits for the writes out, to go out
    /// merged with what else is ready.
    pub(crate) merge_wait: std::time::Duration,
    /// A reindex taken off the control channel and not yet staged. It
    /// outlives a run, so a run failing with writes in flight doesn't lose a
    /// reindex the daemon already accepted; the next run stages it once its
    /// writes in flight returned. A reindex still pending when a stop ends
    /// the deployment is dropped with it (the daemon refuses new ones once
    /// stopping).
    pending_reindex: Mutex<Option<Vec<IndexName>>>,
    /// Whether `pending_reindex` holds one: the run asks every turn, so
    /// the answer is read without the lock.
    reindex_waiting: AtomicBool,
    /// Which lanes the last run's failure is blamed on: their next run writes
    /// its oldest item alone before anything else (see `writes`). A fresh
    /// engine blames every lane, since a previous process may have stopped
    /// on any.
    blame: Mutex<Blame>,
    /// Each lane's health (open, parked, restaging) and its probe backoff.
    /// Outlives a run: a run failing for another reason leaves a parked lane
    /// parked, on its schedule. A lane not in it is open.
    health: Mutex<std::collections::BTreeMap<LaneKey, LaneState>>,
    /// How long a parked lane waits between two probes, and a re-staged
    /// lane between two stagings.
    backoff: crate::Backoff,
}

impl SinkEngine {
    /// An engine for `sink`, named `name` in the config, over its lane on
    /// `stream`, maintaining `mappings`, reporting what lands to `progress`:
    /// the one the ingest engine over `stream` registers in.
    pub fn new(
        name: SinkName,
        sink: Arc<dyn Sink>,
        stream: Arc<dyn Stream>,
        mappings: Vec<IndexMapping>,
        progress: Arc<Progress>,
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
            outstanding: Mutex::new(BTreeMap::new()),
            tickets: AtomicU64::new(0),
            fences: Mutex::new(Fences::default()),
            progress,
            stop: Stop::new(),
            skipped: Mutex::new(BTreeSet::new()),
            advanced: AtomicU64::new(0),
            writes: 1,
            write_documents: crate::BatchPolicy::default().max_changes,
            merge_wait: crate::BatchPolicy::default().max_delay,
            blame: Mutex::new(Blame::Everyone),
            pending_reindex: Mutex::new(None),
            reindex_waiting: AtomicBool::new(false),
            health: Mutex::new(std::collections::BTreeMap::new()),
            backoff: crate::Backoff::default(),
        }
    }

    /// How long a parked lane waits between two probes of its front item, and
    /// a re-staged lane between two stagings: the first wait, doubling up to
    /// the longest. The daemon sets `[backoff]`; the default is 1 s doubling
    /// to 60 s.
    pub fn with_backoff(mut self, backoff: crate::Backoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Where `lane` is: open, parked, or staged again after its target went
    /// missing.
    pub fn lane_health(&self, lane: &LaneKey) -> LaneHealth {
        self.lock_health()
            .get(lane)
            .map_or(LaneHealth::Open, |state| state.health.clone())
    }

    /// What the last staging of `index` read of its detach records, when the
    /// sink keeps any (ADR 0012 stage 10): `None` for nothing kept.
    pub fn detachment(&self, _index: &IndexName) -> Option<sink::Detachment> {
        None
    }

    fn lock_health(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::BTreeMap<LaneKey, LaneState>> {
        self.health.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `lane`'s F9 map, if the engine keeps state for the lane (tests).
    #[cfg(test)]
    pub(crate) fn superseded_map(&self, lane: &LaneKey) -> Option<BTreeMap<String, u64>> {
        self.lock_health()
            .get(lane)
            .map(|state| state.superseded.clone())
    }

    /// `lane`'s state as the last run left it; open when never stored.
    pub(super) fn lane_state(&self, lane: &LaneKey) -> LaneState {
        self.lock_health()
            .get(lane)
            .cloned()
            .unwrap_or_else(|| LaneState::new(self.backoff))
    }

    /// Keep `lane`'s state for the runs to come, and tell the observers when
    /// its health `changed`.
    pub(super) fn store_lane(&self, lane: &LaneKey, state: &LaneState, changed: bool) {
        {
            let mut health = self.lock_health();
            if !state.is_kept() {
                health.remove(lane);
            } else {
                health.insert(lane.clone(), state.clone());
            }
        }
        if changed {
            self.observer.on_lane_health(lane, &state.health);
        }
    }

    /// Stage `index` again after its target went missing: `ensure_index`
    /// (which retracts a seed whose target is gone and creates a fresh one),
    /// then, when it is unseeded, a backfill. One already outstanding is cut
    /// (what it wrote went with the missing target), so a fresh one is asked
    /// for. Runs beside writes of the other indexes, never beside one of
    /// `index`.
    pub(super) async fn restage(&self, index: &IndexName) -> Result<()> {
        let Some(mapping) = self.mappings.iter().find(|m| &m.index == index) else {
            return Ok(());
        };
        let staged = self
            .step(Step::Restage, mapping)
            .await
            .map_err(|error| staging_failed(index, error))?;
        let Staged::Unseeded { walked, target } = staged else {
            return Ok(());
        };
        if !self.options.backfill || self.skip_backfill {
            tracing::warn!(%index, sink = %self.name, "staged again with no backfill (backfill off for this sink, or skip_backfill): it holds only what is written from now on");
            return Ok(());
        }
        if self.stop.is_requested() {
            tracing::info!(%index, sink = %self.name, "stopping: staged again, its backfill is asked for at the next start");
            return Ok(());
        }
        if self.outstanding().contains(index) {
            return self.ask_again(vec![(index.clone(), target)]).await;
        }
        self.request_backfill(vec![(index.clone(), walked, target)])
            .await
    }

    /// Stage again `index`, which a staging left out (#471): with ensure's
    /// rules, as [`stage`](Self::stage) treats one index. An intact target
    /// keeps its outstanding backfill and its walk; a target that changed
    /// meanwhile has its backfill cut and asked again; an unseeded index not
    /// asked for yet is asked for.
    pub(super) async fn ensure_again(&self, index: &IndexName) -> Result<()> {
        let Some(mapping) = self.mappings.iter().find(|m| &m.index == index) else {
            return Ok(());
        };
        let backfill = self.options.backfill;
        let staged = self
            .step(
                Step::Ensure {
                    ask_seeded: backfill,
                },
                mapping,
            )
            .await
            .map_err(|error| staging_failed(index, error))?;
        let Staged::Unseeded { walked, target } = staged else {
            return Ok(());
        };
        if !backfill || self.skip_backfill || self.stop.is_requested() {
            return Ok(());
        }
        if self.outstanding().contains(index) {
            if self.is_cut(index) {
                return self.ask_again(vec![(index.clone(), target)]).await;
            }
            return Ok(());
        }
        if self
            .sink
            .is_rebuilding(index)
            .await
            .map_err(|error| staging_failed(index, error))?
        {
            self.observer.on_index_rebuilding(&self.name, index);
        }
        self.request_backfill(vec![(index.clone(), walked, target)])
            .await
    }

    /// `indexes` were cut: staged into a target their outstanding backfill
    /// was not asked for, so it can't seed them. Forget their checkpoints, so
    /// a restart never resumes the old walk, and ask for each again from the
    /// first key, for `target`; the new ticket is the one their seed now
    /// waits for. A failure leaves them cut, for the next staging.
    async fn ask_again(&self, indexes: Vec<(IndexName, Option<TargetId>)>) -> Result<()> {
        for (index, _) in &indexes {
            self.sink
                .forget_walked(index)
                .await
                .map_err(|error| staging_failed(index, error))?;
        }
        self.request_backfill(
            indexes
                .into_iter()
                .map(|(index, target)| (index, None, target))
                .collect(),
        )
        .await
    }

    /// `index` was just staged into `ensured` (`restaged`: after a write
    /// found its target missing). Cut its outstanding backfill when the
    /// target is not the one it was asked for: named otherwise, or, when
    /// either is unnamed, created anew or staged again after going missing.
    /// The cut stays until a fresh request replaces it.
    fn check_target(&self, index: &IndexName, ensured: &sink::Ensured, restaged: bool) {
        let mut outstanding = self.lock_outstanding();
        let Some(asked) = outstanding.get_mut(index) else {
            return;
        };
        let changed = restaged
            || match (&asked.target, &ensured.target) {
                (Some(asked), Some(now)) => asked != now,
                _ => ensured.created,
            };
        if changed && !asked.cut {
            asked.cut = true;
            tracing::warn!(%index, sink = %self.name, ticket = %asked.ticket, "staged into a new target while its backfill was running: the rows that backfill wrote are not there, so it can't seed this one; asking for a fresh backfill");
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

    /// The most documents one write holds: items ready at once (across lanes,
    /// and several of one lane) are merged into one write up to it, never
    /// splitting an item, and below the sink's own
    /// [`write_limit`](sink::Sink::write_limit). The default is
    /// [`BatchPolicy`](crate::BatchPolicy)'s `max_changes`; the daemon sets
    /// `[batch] max_changes`, so a write holds about one batch, as with one
    /// lane per sink.
    pub fn with_write_documents(mut self, documents: usize) -> Self {
        self.write_documents = documents.max(1);
        self
    }

    /// The longest a live item ready while a write is out waits for the writes
    /// out to return, so it goes out in one write with what else is ready
    /// (fewer, larger requests, as one lane per sink wrote one batch over
    /// every index). Bounded from when the item became ready; a write out
    /// longer than this is not waited for, so a stalled or slow lane holds no
    /// other index's writes past it. Never for backfill, an ordered sink, a
    /// sink with one lane, or without merging. The default is
    /// [`BatchPolicy`](crate::BatchPolicy)'s `max_delay`, which the daemon
    /// sets from `[batch] max_delay_ms`; zero turns the wait off.
    pub fn with_merge_wait(mut self, wait: std::time::Duration) -> Self {
        self.merge_wait = wait;
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

    /// How often this engine's oldest unfinished lane item landed since it
    /// was built, across restarts of its run: a supervisor reads it to tell a
    /// run that made progress from one that failed where the last one did.
    /// A later item landing beside an older one that then fails is not
    /// progress: the sink is still stuck at the older one.
    pub fn advanced(&self) -> u64 {
        self.advanced.load(Ordering::Relaxed)
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
    /// index's own steps in order. Under `Fresh` no further index is started
    /// after a failure and the error names every failure seen; under
    /// `Resumed` an index that can't be ensured is left restaging and the rest
    /// go on (#471, `ensure_per_index`).
    #[tracing::instrument(name = "sink.stage", skip_all, fields(sink = %self.name, ?continuity))]
    pub async fn stage(&self, continuity: Continuity) -> Result<()> {
        let started = Instant::now();
        if self.stop.is_requested() {
            self.stage_while_stopping(started).await;
            return Ok(());
        }
        let backfill = self.options.backfill;
        let step = Step::Ensure {
            ask_seeded: backfill,
        };
        // A `Fresh` source needs every seeded index's rebuild staged before it
        // is prepared (#120), and an index not ensured can't be told seeded:
        // that staging waits for every index (#471, R4).
        let (mut seeded, unensured) = if continuity == Continuity::Fresh {
            (
                self.each_index(self.mappings.iter(), step).await?,
                BTreeSet::new(),
            )
        } else {
            self.ensure_per_index(step).await?
        };
        self.observer.on_indexes_ensured(&self.name, seeded.len());
        // A stop that came while ensuring makes this staging ensure only too
        // (X21): nothing asked for, no restaging lane reopened.
        if self.stop.is_requested() {
            self.staged(started);
            return Ok(());
        }

        if !backfill {
            tracing::info!("backfill disabled for this sink; live changes only");
            self.reopen_restaged(&unensured);
            self.staged(started);
            return Ok(());
        }

        if continuity == Continuity::Fresh {
            self.rebuild_stale_seeds(&mut seeded).await?;
            self.forget_walks(&mut seeded).await?;
        }

        // An index already asked for is not asked again (a restart of this
        // engine), unless its backfill was cut: then what it wrote is gone.
        let already_requested = self.outstanding();
        let mut unseeded: Vec<(IndexName, Option<WalkMark>, Option<TargetId>)> = Vec::new();
        let mut cut: Vec<(IndexName, Option<TargetId>)> = Vec::new();
        for (mapping, staged) in seeded {
            let index = mapping.index.clone();
            match staged {
                Staged::Seeded => {}
                Staged::Unseeded { target, .. } if already_requested.contains(&index) => {
                    if self.is_cut(&index) {
                        cut.push((index, target));
                    }
                }
                Staged::Unseeded { walked, target } => unseeded.push((index, walked, target)),
            }
        }
        if !cut.is_empty() {
            self.ask_again(cut).await?;
        }
        if self.skip_backfill {
            if !unseeded.is_empty() {
                tracing::warn!(
                    indexes = ?unseeded.iter().map(|(i, _, _)| i.as_ref()).collect::<Vec<_>>(),
                    "skipping backfill (skip_backfill set); these indexes stay unseeded"
                );
            }
        } else if !unseeded.is_empty() {
            // A start in the middle of a rebuild: reads still see the old copy,
            // so a fence must vouch for nothing of it (#379).
            for (index, _, _) in &unseeded {
                if self.sink.is_rebuilding(index).await? {
                    self.observer.on_index_rebuilding(&self.name, index);
                }
            }
            self.request_backfill(unseeded).await?;
        } else {
            tracing::info!("every index is seeded; no backfill needed");
        }
        self.reopen_restaged(&unensured);
        self.staged(started);
        Ok(())
    }

    /// Staging for a run restarted during the stop (X21): every index is
    /// ensured and nothing more. No backfill is asked for and no restaging
    /// lane reopens; an index that can't be ensured, or wasn't tried after
    /// another failed, is left restaging with why, for the stop to release
    /// like any other, rather than failing the staging and holding the stop
    /// until its deadline.
    async fn stage_while_stopping(&self, started: Instant) {
        tracing::info!(sink = %self.name, "staging during the stop: the indexes are ensured only");
        let tried = self
            .try_each_index(
                self.mappings.iter(),
                Step::Ensure { ask_seeded: false },
                Spread::StopAtFirstFailure,
            )
            .await;
        self.observer
            .on_indexes_ensured(&self.name, tried.done.len());
        self.leave_restaging(tried.failed, tried.not_started, true);
        self.staged(started);
    }

    /// Ensure every index on its own (#471, R1): an index that can't be
    /// ensured, or wasn't tried after another failed, is left restaging with
    /// why, for its lane's restage path to stage again, and the rest go on.
    /// When none could be ensured the destination is down, not an index: the
    /// staging fails as a whole, for the daemon to back off and retry. What
    /// each ensured index staged, and the indexes left restaging.
    async fn ensure_per_index(
        &self,
        step: Step,
    ) -> Result<(Vec<(&IndexMapping, Staged)>, BTreeSet<IndexName>)> {
        let tried = self
            .try_each_index(self.mappings.iter(), step, Spread::OnceAnswered)
            .await;
        if tried.done.is_empty() && !tried.failed.is_empty() {
            return Err(EngineError::Staging(StagingFailures(tried.failed)));
        }
        let stopping = self.stop.is_requested();
        let unensured = self.leave_restaging(tried.failed, tried.not_started, stopping);
        Ok((tried.done, unensured))
    }

    /// Leave each index of `failed` and `not_started` restaging, with why,
    /// marked to be staged again with ensure's rules: the lanes the staging
    /// couldn't ensure. `stopping`: a staging during the stop, whose lanes the
    /// stop releases.
    fn leave_restaging(
        &self,
        failed: Vec<(IndexName, sink::SinkError)>,
        not_started: Vec<IndexName>,
        stopping: bool,
    ) -> BTreeSet<IndexName> {
        let when = if stopping { " during the stop" } else { "" };
        let unstaged = failed
            .into_iter()
            .map(|(index, error)| (index, format!("could not be ensured{when}: {error}")))
            .chain(not_started.into_iter().map(|index| {
                let why = if stopping {
                    "not ensured during the stop: not tried".to_owned()
                } else {
                    format!(
                        "not tried: {STAGE_CONCURRENCY} indexes in a row could not be ensured before it"
                    )
                };
                (index, why)
            }));
        let now = Instant::now();
        let mut left = BTreeSet::new();
        for (index, reason) in unstaged {
            let lane = LaneKey::new(index.clone(), self.name.clone());
            let mut state = self.lane_state(&lane);
            if !state.restage(reason.clone(), now) {
                state.left_out_again(reason.clone());
            } else {
                state.unensured = true;
                if stopping {
                    tracing::warn!(%lane, %reason, "not ensured during the stop; its items are released unapplied");
                } else {
                    tracing::warn!(%lane, %reason, "the index was not ensured; its lane is staged again on its own while the sink's other indexes go on");
                }
            }
            self.store_lane(&lane, &state, true);
            left.insert(index);
        }
        left
    }

    /// Every index but `unensured` was just ensured: a lane waiting to be
    /// staged again after its target went missing is staged. A lane owed a
    /// reindex is not: its retry stages the reindex (X17). Nor is one whose
    /// index wasn't ensured: its restage path stages it (#471, R3).
    fn reopen_restaged(&self, unensured: &BTreeSet<IndexName>) {
        let restaged: Vec<LaneKey> = self
            .lock_health()
            .iter()
            .filter(|(lane, state)| {
                matches!(state.health, LaneHealth::Restaging { .. })
                    && state.reindex.is_none()
                    && !unensured.contains(&lane.index)
            })
            .map(|(lane, _)| lane.clone())
            .collect();
        for lane in restaged {
            let mut state = self.lane_state(&lane);
            state.reopen(Instant::now());
            tracing::info!(%lane, "staged again with the sink; the lane is open");
            self.store_lane(&lane, &state, true);
        }
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
    async fn rebuild_stale_seeds(&self, seeded: &mut [(&IndexMapping, Staged)]) -> Result<()> {
        let stale: Vec<&IndexMapping> = seeded
            .iter()
            .filter(|(_, staged)| *staged == Staged::Seeded)
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

    /// The source has no resume point, so where an unseeded index's last
    /// backfill got means nothing any more: the changes since are lost. Forget
    /// it before the source prepares (the daemon's order), so that a crash
    /// before the new walk records anything never resumes the old one.
    async fn forget_walks(&self, staged: &mut [(&IndexMapping, Staged)]) -> Result<()> {
        for (mapping, state) in staged.iter_mut() {
            if let Staged::Unseeded {
                walked: walked @ Some(_),
                ..
            } = state
            {
                tracing::warn!(index = %mapping.index, "the source has no resume point, so this index's unfinished backfill starts again from the first key");
                self.sink
                    .forget_walked(&mapping.index)
                    .await
                    .map_err(|error| staging_failed(&mapping.index, error))?;
                *walked = None;
            }
        }
        Ok(())
    }

    /// Run `step` for each of `mappings`, at most [`STAGE_CONCURRENCY`] at
    /// once, and return each mapping staged with what staging learned of it
    /// (unseeded with no checkpoint when not asked), in the order given. Every
    /// mapping, unless a stop asked meanwhile left some not started: those
    /// are missing from the answer.
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
    ) -> Result<Vec<(&'a IndexMapping, Staged)>> {
        let tried = self
            .try_each_index(mappings, step, Spread::StopAtFirstFailure)
            .await;
        if tried.failed.is_empty() {
            Ok(tried.done)
        } else {
            Err(EngineError::Staging(StagingFailures(tried.failed)))
        }
    }

    /// [`each_index`](Self::each_index), every outcome kept: what staged,
    /// what failed, and what was not started after a failure.
    async fn try_each_index<'a>(
        &self,
        mappings: impl IntoIterator<Item = &'a IndexMapping>,
        step: Step,
        spread: Spread,
    ) -> Tried<'a> {
        let mut waiting = mappings.into_iter().enumerate();
        let mut running = FuturesUnordered::new();
        let mut outcomes = Vec::new();
        let mut failed_once = false;
        let mut answered = false;
        let mut probes = 0;
        let mut in_a_row = 0;
        // A staging that began under the stop ensures every index (X21); one
        // under way starts no index once the stop is asked: what is left is
        // the stop's to release.
        let began_stopping = self.stop.is_requested();
        loop {
            loop {
                if !began_stopping && self.stop.is_requested() {
                    break;
                }
                // A wave's worth of failures in a row, even after a success:
                // the destination stopped answering (#471, second review).
                let open = (!failed_once || (spread == Spread::OnceAnswered && answered))
                    && in_a_row < STAGE_CONCURRENCY;
                let room = if open {
                    running.len() < STAGE_CONCURRENCY
                } else if answered {
                    false
                } else {
                    // Nothing ensured yet: one more index at a time, up to one
                    // more wave's worth, before reading the destination as down.
                    spread == Spread::OnceAnswered
                        && running.is_empty()
                        && probes < STAGE_CONCURRENCY
                };
                if !room {
                    break;
                }
                let Some((order, mapping)) = waiting.next() else {
                    break;
                };
                if !open {
                    probes += 1;
                }
                running.push(async move { (order, mapping, self.step(step, mapping).await) });
            }
            let Some(outcome) = running.next().await else {
                break;
            };
            if outcome.2.is_ok() {
                answered = true;
                in_a_row = 0;
            } else {
                failed_once = true;
                in_a_row += 1;
            }
            outcomes.push(outcome);
        }
        outcomes.sort_unstable_by_key(|(order, _, _)| *order);
        let mut tried = Tried {
            done: Vec::with_capacity(outcomes.len()),
            failed: Vec::new(),
            not_started: waiting.map(|(_, mapping)| mapping.index.clone()).collect(),
        };
        for (_, mapping, outcome) in outcomes {
            match outcome {
                Ok(seeded) => tried.done.push((mapping, seeded)),
                Err(error) => tried.failed.push((mapping.index.clone(), error)),
            }
        }
        tried
    }

    /// One index's staging steps, in order. Whether an outstanding backfill
    /// is cut is decided right after `ensure_index`, before any step that can
    /// fail.
    async fn step(&self, step: Step, mapping: &IndexMapping) -> sink::Result<Staged> {
        let (ask_seeded, restaged) = match step {
            Step::Ensure { ask_seeded } => (ask_seeded, false),
            Step::Restage => (self.options.backfill, true),
            Step::Rebuild { ask_seeded } => {
                // Before the sink moves its writes: a fence must stop vouching
                // for this index first (#379).
                self.observer
                    .on_index_rebuilding(&self.name, &mapping.index);
                self.sink.reindex(mapping).await?;
                (ask_seeded, false)
            }
        };
        let ensured = self.sink.ensure_index(mapping).await?;
        self.check_target(&mapping.index, &ensured, restaged);
        let target = ensured.target;
        if !ask_seeded {
            return Ok(Staged::Unseeded {
                walked: None,
                target,
            });
        }
        Ok(match self.sink.seeding(&mapping.index).await? {
            sink::Seeding::Seeded => Staged::Seeded,
            sink::Seeding::Unseeded { walked } => Staged::Unseeded {
                walked: walked.map(WalkMark::new),
                target,
            },
            _ => Staged::Unseeded {
                walked: None,
                target,
            },
        })
    }

    /// Ask for a backfill of each index, after where its last one durably
    /// got, if anywhere, for the target staging found, each under a new
    /// ticket that its seed then waits for. An index counts as asked for
    /// once its request is published, so a failure part way leaves the rest
    /// to be asked for again.
    async fn request_backfill(
        &self,
        requests: Vec<(IndexName, Option<WalkMark>, Option<TargetId>)>,
    ) -> Result<()> {
        tracing::info!(indexes = requests.len(), "requesting backfill");
        let producer = self.stream.requests()?.producer;
        let mut indexes = Vec::with_capacity(requests.len());
        for (index, after, target) in requests {
            if after.is_some() {
                tracing::info!(%index, sink = %self.name, "requesting a resumed backfill: its last walk recorded how far it got");
            }
            let ticket = Ticket::new(self.tickets.fetch_add(1, Ordering::Relaxed) + 1);
            producer
                .publish(Request::Backfill {
                    index: index.clone(),
                    sink: self.name.clone(),
                    after,
                    ticket,
                })
                .await?;
            self.lock_outstanding().insert(
                index.clone(),
                Asked {
                    ticket,
                    target,
                    cut: false,
                },
            );
            indexes.push(index);
        }
        self.observer.on_backfill_requested(&self.name, &indexes);
        Ok(())
    }

    fn outstanding(&self) -> BTreeSet<IndexName> {
        self.lock_outstanding().keys().cloned().collect()
    }

    /// Whether an item naming `ticket` answers `index`'s latest request: no
    /// request is outstanding, or it is not cut and the item's ticket is its
    /// own or later. A snapshot batch naming none answers nothing
    /// outstanding.
    pub(super) fn answers(&self, index: &IndexName, ticket: Option<Ticket>) -> bool {
        match self.lock_outstanding().get(index) {
            None => true,
            Some(asked) => !asked.cut && ticket.is_some_and(|ticket| ticket >= asked.ticket),
        }
    }

    /// Whether `index`'s outstanding backfill is cut.
    fn is_cut(&self, index: &IndexName) -> bool {
        self.lock_outstanding()
            .get(index)
            .is_some_and(|asked| asked.cut)
    }

    /// The outstanding request a seed answering `ticket` ends: `index`'s, when
    /// not cut and `ticket` is its own or later.
    pub(super) fn settle(&self, index: &IndexName, ticket: Ticket) {
        let mut outstanding = self.lock_outstanding();
        if outstanding
            .get(index)
            .is_some_and(|asked| !asked.cut && asked.ticket <= ticket)
        {
            outstanding.remove(index);
        }
    }

    fn lock_outstanding(&self) -> std::sync::MutexGuard<'_, BTreeMap<IndexName, Asked>> {
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
        match &result {
            Ok(()) => self.set_blame(Blame::Nobody),
            Err(_) if matches!(self.blame(), Blame::Nobody) => self.set_blame(Blame::Everyone),
            Err(_) => {}
        }
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
        self.progress.check_lanes(self.lane_keys().iter())?;
        let lanes = self
            .lane_keys()
            .into_iter()
            .map(|key| Ok((key.clone(), self.stream.lane(&key)?.consumer)))
            .collect::<Result<Vec<_>>>()?;
        writes::Run::new(self, lanes).run(control).await
    }

    /// The lanes this engine reads: one per index it maintains.
    fn lane_keys(&self) -> Vec<LaneKey> {
        self.mappings
            .iter()
            .map(|mapping| LaneKey::new(mapping.index.clone(), self.name.clone()))
            .collect()
    }

    pub(super) fn blame(&self) -> Blame {
        self.blame
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn set_blame(&self, blame: Blame) {
        *self.blame.lock().unwrap_or_else(PoisonError::into_inner) = blame;
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

    fn lock_pending_reindex(&self) -> std::sync::MutexGuard<'_, Option<Vec<IndexName>>> {
        self.pending_reindex
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn set_pending_reindex(&self, indexes: Vec<IndexName>) {
        let mut pending = self.lock_pending_reindex();
        *pending = Some(indexes);
        self.reindex_waiting.store(true, Ordering::Release);
    }

    fn take_pending_reindex(&self) -> Option<Vec<IndexName>> {
        if !self.reindex_pending() {
            return None;
        }
        let mut pending = self.lock_pending_reindex();
        self.reindex_waiting.store(false, Ordering::Release);
        pending.take()
    }

    fn reindex_pending(&self) -> bool {
        self.reindex_waiting.load(Ordering::Acquire)
    }

    fn lock_skipped(&self) -> std::sync::MutexGuard<'_, BTreeSet<IndexName>> {
        self.skipped.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stage a rebuild of `indexes` and request their snapshot: the indexes
    /// staged, and each that could not be with why (§15.3, X14, X15). The
    /// current generation keeps serving until the snapshot completes and the
    /// sink swaps. A sink with `backfill = false` has nothing to rebuild. An
    /// index whose backfill is outstanding is left to that backfill while its
    /// lane is open; with its lane parked, blocked or restaging the reindex is
    /// the way out, so the backfill is cut and asked for again from the first
    /// key (stage 9, X6). Nothing is lost on a failure: see
    /// [`stage_reindex`](Self::stage_reindex).
    async fn reindex(&self, indexes: Vec<IndexName>) -> Reindexed {
        if !self.options.backfill {
            tracing::warn!("reindex requested, but backfill is disabled for this sink; ignoring");
            return Reindexed::default();
        }
        let outstanding = self.outstanding();
        let mut chosen = Vec::new();
        for index in indexes {
            let Some(mapping) = self.mappings.iter().find(|m| m.index == index) else {
                tracing::warn!(%index, "reindex requested for an index this sink does not maintain; ignoring");
                continue;
            };
            // Two chains for one index at once are what the `Sink` port forbids.
            if chosen.iter().any(|m: &&IndexMapping| m.index == index) {
                continue;
            }
            let open = self
                .lane_health(&LaneKey::new(index.clone(), self.name.clone()))
                .is_open();
            if outstanding.contains(&index) && open {
                tracing::warn!(%index, "reindex requested while its snapshot is still outstanding; ignoring");
                continue;
            }
            tracing::info!(%index, "reindex requested; staging a fresh generation");
            chosen.push(mapping);
        }
        self.stage_reindex(chosen, &outstanding).await
    }

    /// Retry the reindex `index` is owed (X15): staged, its backfill is
    /// asked for as the reindex would have.
    pub(super) async fn reindex_again(&self, index: &IndexName) -> Reindexed {
        let Some(mapping) = self.mappings.iter().find(|m| &m.index == index) else {
            return Reindexed::default();
        };
        tracing::info!(%index, sink = %self.name, "retrying the reindex its staging failed");
        let outstanding = self.outstanding();
        self.stage_reindex(vec![mapping], &outstanding).await
    }

    /// Stage `chosen` for a reindex and ask for each one's backfill. Every
    /// index ends in `staged` or in `failed`, whatever fails and in whatever
    /// order: a failed staging, an index not started after another failed,
    /// and an index whose ask for its backfill failed (a sink write to forget
    /// its walk, or the request lane) are each owed, never dropped (§15.3).
    async fn stage_reindex(
        &self,
        chosen: Vec<&IndexMapping>,
        outstanding: &BTreeSet<IndexName>,
    ) -> Reindexed {
        let tried = self
            .try_each_index(
                chosen,
                Step::Rebuild { ask_seeded: false },
                Spread::StopAtFirstFailure,
            )
            .await;
        let mut reindexed = Reindexed {
            staged: Vec::new(),
            failed: tried
                .failed
                .into_iter()
                .map(|(index, error)| (index, format!("the reindex could not stage: {error}")))
                .collect(),
        };
        let not_started = if self.stop.is_requested() {
            "the reindex could not stage: not started, the stop came"
        } else {
            "the reindex could not stage: not started after another index failed"
        };
        reindexed.failed.extend(
            tried
                .not_started
                .into_iter()
                .map(|index| (index, not_started.to_owned())),
        );
        for (mapping, staged) in tried.done {
            let index = mapping.index.clone();
            if self.stop.is_requested() {
                reindexed
                    .failed
                    .push((index, STAGING_AT_THE_STOP.to_owned()));
                continue;
            }
            let target = match staged {
                Staged::Unseeded { target, .. } => target,
                Staged::Seeded => None,
            };
            let asked = if outstanding.contains(&index) {
                tracing::warn!(%index, sink = %self.name, "reindex during a backfill: the backfill is cut and asked for again");
                self.ask_again(vec![(index.clone(), target)]).await
            } else {
                self.request_backfill(vec![(index.clone(), None, target)])
                    .await
            };
            match asked {
                Ok(()) => reindexed.staged.push(index),
                Err(error) => reindexed.failed.push((
                    index,
                    format!("the reindex could not ask for its backfill: {error}"),
                )),
            }
        }
        reindexed
    }

    /// Whether `index`'s backfill is still outstanding (not seeded yet).
    pub(super) fn is_outstanding(&self, index: &IndexName) -> bool {
        self.lock_outstanding().contains_key(index)
    }
}

/// How a staging goes on after an index fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spread {
    /// Start no further index: a destination that is down fails every index
    /// alike, and each wave would cost one more request timeout.
    StopAtFirstFailure,
    /// Per index (#471): once one index was ensured the destination answers,
    /// so every index is still started; while none is, further indexes are
    /// tried one at a time, at most [`STAGE_CONCURRENCY`] more, before the
    /// destination is read as down.
    OnceAnswered,
}

/// Every index a staging tried: what it learned of each that staged, each
/// that failed with its error, and each not started after a failure.
struct Tried<'a> {
    done: Vec<(&'a IndexMapping, Staged)>,
    failed: Vec<(IndexName, sink::SinkError)>,
    not_started: Vec<IndexName>,
}

/// What a reindex staged, and each index it could not stage with why.
#[derive(Debug, Default)]
pub(super) struct Reindexed {
    pub(super) staged: Vec<IndexName>,
    pub(super) failed: Vec<(IndexName, String)>,
}

/// What staging learned of an index.
#[derive(Debug, Clone, PartialEq)]
enum Staged {
    Seeded,
    /// Unseeded, with where its last backfill durably got, if recorded, and
    /// the target staging found, if the sink names it.
    Unseeded {
        walked: Option<WalkMark>,
        target: Option<TargetId>,
    },
}

/// A staging step of `index` failed with `error`.
fn staging_failed(index: &IndexName, error: sink::SinkError) -> EngineError {
    EngineError::Staging(StagingFailures(vec![(index.clone(), error)]))
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
