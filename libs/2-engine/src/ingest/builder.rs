//! The builder: the future of an engine run that builds and publishes (ADR
//! 0012 stage 8a, §3.9 §2). In 6b-1b there is one, over every index: 6a's
//! builder loop without the request lane, confirmation and the wind-down's
//! drain and close, which are the dispatcher's.
//!
//! It owns the runs the reader handed over, the builds, the snapshot, the
//! lanes' producers, its request inbox and the coalescing set. It hands its
//! end (the reader's, with the live stream) or its error to the dispatcher
//! over a one-shot, then returns.
//!
//! **Coalescing on a deadline.** The first request opens a coalescing set
//! with a deadline at `max_delay`; the set starts its snapshot once every
//! sink with a lane asked and the inbox is empty, else at the deadline
//! (`main`'s rule, per index). Live batches are granted meanwhile: on `main`
//! and 6a they waited the coalescing out.
//!
//! **Walks at once** (6b-3). Each builder walks its own index as soon as its
//! requests are gathered, beside every other builder's walk: no walk waits
//! for another (6b-1c's walk token is gone). With two or more builders each
//! key read runs on a task of its own (see the snapshot module), so a builder
//! that stops stepping its walk holds no other walk's key reads.
//!
//! **The end and the stop.** Once the reader ended with its stream and
//! nothing is left to serve, the builder reports itself idle and waits for
//! the dispatcher's end, then settles. At the stop (the dispatcher raises it)
//! it drops its snapshot and coalescing set (their requests are served again
//! on the next start), waits for the reader's end (R7), and settles.
//!
//! **A failure of its own** (slice 8, the stage note's §3.10). With other
//! builders beside it, an error from its index's own work (a build, a
//! document under `on_error = "stop"`, its walk) doesn't end the run: the
//! builder waits for its running builds, gives every unpublished build back
//! to its index's pending set, drops the rest of its run state (its snapshot
//! and coalescing set: their requests come back through the request lane),
//! and, held and folding so its index is exempt while down, waits out its
//! backoff, then starts again from its backlog. A stop cuts the wait short;
//! a failure while stopping, or of a lone builder, ends the run.
//!
//! ```text
//! build ─▶ fail ─▶ wait for builds ─▶ give back ─▶ held + folding ─▶ backoff ─▶ build (backlog first)
//! ```

use std::collections::HashSet;
use std::sync::Arc;

use kernel::{IndexName, SinkName};
use stream::{Delivery, Request};

use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use super::builds::Builds;
use super::dispatcher::{Reports, Signal, Signals};
use super::gate::Gate;
use super::live::Runs;
use super::restart::{Retry, restarts_alone};
use super::snapshot::{Snapshot, Step, next_snapshot_step};
use super::{IdleClock, IngestEngine, Lanes, released};
use crate::error::Result;
use crate::observer::{Stage, StageTiming, Wake, Work};

/// How a builder ended without an error: whether the stop ended it.
pub(super) struct BuilderEnd {
    pub(super) stopped: bool,
}

/// Requests gathered for one snapshot, and when it starts at the latest.
pub(super) struct Coalescing {
    deliveries: Vec<Delivery<Request>>,
    deadline: Instant,
    /// When the first request arrived: the snapshot's start is timed from
    /// here, as `main` timed its gathering.
    started: Instant,
    /// A failed walk's retry (slice 8, fix round 2): it starts at its
    /// deadline, the walk's wait, even once every sink asked; a request
    /// arriving meanwhile joins it.
    retry: bool,
}

/// What the builder borrows for one engine run.
pub(super) struct Building<'a> {
    pub(super) runs: Runs,
    pub(super) inbox: mpsc::UnboundedReceiver<Delivery<Request>>,
    pub(super) signals: &'a Signals,
    pub(super) reports: &'a Reports,
    /// Other builders run beside it: a builder held by the budget folds, so
    /// its index doesn't stop the reading for theirs (§3.9 I7), and its
    /// walk's key reads run on tasks of their own (6b-3). Alone, it never
    /// folds and reads inline: it is 6b-1b's builder.
    pub(super) several: bool,
    /// With several builders, the engine's idle clock and this builder's
    /// place on it.
    pub(super) clock: Option<(&'a IdleClock, usize)>,
    pub(super) lanes: &'a Lanes,
    pub(super) scopes: &'a HashSet<IndexName>,
    pub(super) builds: &'a mut Builds,
    /// The gate its builds ask, for the builds of a restart.
    pub(super) gate: &'a Arc<Gate>,
    /// The snapshot it walks, and the requests it gathers for the next one:
    /// a restart keeps their requests (slice 8).
    pub(super) snapshot: Option<Snapshot>,
    pub(super) coalescing: Option<Coalescing>,
    /// Its waits after a failure, live and walk (slice 8).
    pub(super) retry: Retry,
}

impl IngestEngine {
    /// The builder of one engine run: build until the end or the stop, then
    /// hand the end to the dispatcher over `done`. A failure of its own
    /// index's work, beside other builders, restarts it in place (see the
    /// module docs).
    pub(super) async fn build(
        &self,
        mut building: Building<'_>,
        done: oneshot::Sender<Result<BuilderEnd>>,
    ) {
        let result = loop {
            let error = match self.building(&mut building).await {
                Ok(end) => break Ok(end),
                Err(error) => error,
            };
            let index = match building.builds.index() {
                Some(index)
                    if restarts_alone(building.several, self.stop.is_requested(), &error) =>
                {
                    index.clone()
                }
                _ => break Err(error),
            };
            // Its walk fails alone (in its loop): what reaches here is its
            // live work's. Reported as it happens, before `down` waits for
            // the running builds (review of #457, B1), and restarted `wait`
            // after it, or once `down` returns if that is later.
            let failed_at = Instant::now();
            let wait = building.retry.failed(Work::Live, self.done(&index));
            tracing::error!(
                %index,
                %error,
                kind = "live",
                retry_in = ?wait,
                "the index's builder failed; it restarts alone after the wait while the other \
                 indexes go on: its documents wait, and the replication slot stays before the \
                 oldest of them"
            );
            self.observer
                .on_index_failed(&index, Work::Live, &error.to_string(), wait);
            self.down(&mut building, &index).await;
            // A stop cuts the wait short: the builder tries once more, under
            // the stop's deadline (O3).
            tokio::select! {
                () = tokio::time::sleep_until(failed_at + wait) => {}
                () = self.stop.requested() => {}
            }
            tracing::info!(
                %index,
                kind = "live",
                "the index's builder restarted: building its backlog first"
            );
            self.observer.on_index_restarted(&index, Work::Live);
        };
        // The dispatcher is gone only once the run ended.
        let _ = done.send(result);
    }

    /// The builder failed on its own index's work: wait for its running
    /// builds, give every unpublished build back to the index's pending set,
    /// and leave it down, held and folding (its index exempt at its cap
    /// while another index works, §3.10), with fresh builds for its restart
    /// and its requests gathered for a snapshot that starts with it.
    async fn down(&self, building: &mut Building<'_>, index: &IndexName) {
        // Held and folding from the failure on, before its running builds
        // are waited for: a slow one would otherwise leave the index at its
        // cap, neither held nor exempt, stopping the reading for every index
        // (review of #456, item 7).
        let slot = building.runs.slot();
        building.runs.lead().hold(slot, true);
        building.runs.lead().fold(slot, true);
        building.runs.clear();
        self.index_states.budget(index, false);
        // Its requests are kept, unacknowledged, each lane resumed where it
        // got, and gathered for the restart: at once, or at a failed walk's
        // retry time when it waits for one (a set gathered already keeps its
        // deadline). Dropping the snapshot stops its walk and its key read.
        let mut requests = Vec::new();
        if let Some(snapshot) = building.snapshot.take() {
            requests.extend(snapshot.into_requests());
        }
        // A walk that finished but whose end waits behind unpublished
        // batches answered nobody yet either, nor a walk that failed.
        for finished in building.builds.abandon().await {
            requests.extend(finished.into_requests());
        }
        // A walk that failed in the same pass is a walk failure like any:
        // reported, and retried after the walk's own wait (review of #457,
        // A2), its set joined by what the builder's end left.
        if let Some(failure) = building.builds.take_walk_failure() {
            self.retry_walk(
                failure,
                &mut building.coalescing,
                &mut building.retry,
                Some(index),
            );
        }
        if !requests.is_empty() || building.coalescing.is_some() {
            let now = Instant::now();
            building
                .coalescing
                .get_or_insert_with(|| Coalescing {
                    deliveries: Vec::new(),
                    deadline: now,
                    started: now,
                    retry: false,
                })
                .deliveries
                .extend(requests);
        }
        self.progress.give_back(index);
        building.builds.restarting();
        *building.builds = self.builds_of(building.gate, index.clone(), building.several);
        building.builds.set_folding(true);
    }

    async fn building(&self, building: &mut Building<'_>) -> Result<BuilderEnd> {
        let runs = &mut building.runs;
        let inbox = &mut building.inbox;
        let signals = building.signals;
        let reports = building.reports;
        let several = building.several;
        let clock = building.clock;
        let lanes = building.lanes;
        let scopes = building.scopes;
        // Out of every line however the builder ends (§3.9 G9′): a waiter
        // left behind would be offered permits nobody asks for again.
        let mut builds = LeavesLines(&mut *building.builds);
        let snapshot = &mut building.snapshot;
        let coalescing = &mut building.coalescing;
        let retry = &mut building.retry;
        // The reader ended: every slice of this builder is handed over (its
        // channel closed after the last one).
        let mut reader_done = false;
        // Where the last arm's work ended: every arm times its wait from here,
        // so the loop reads no clock of its own.
        let mut mark = Instant::now();
        // A restart (slice 8) starts folding with its backlog waiting, which
        // no hand-over, landing or release would ask for: it asks once.
        if builds.folding() {
            self.grant(runs, snapshot, &mut builds, lanes).await?;
        }

        loop {
            // A walk failed (a batch, at publish): it waits to retry while
            // the live work goes on (slice 8, fix round 2).
            if let Some(failure) = builds.take_walk_failure() {
                self.retry_walk(failure, coalescing, retry, builds.index());
            }
            // A live stream that ends while a snapshot runs or requests wait
            // still gets those served; the builder settles once the
            // dispatcher saw nothing left to forward either.
            if several {
                self.fold(runs, &mut builds);
            }
            if let Some(index) = builds.index() {
                // What `/status` shows holds the index (slice 7): one load
                // unless the refusal started or ended.
                self.index_states
                    .budget(index, builds.refused() == Some(Work::Live));
            }
            if reader_done
                && !self.stop.is_requested()
                && snapshot.is_none()
                && coalescing.is_none()
                && inbox.is_empty()
            {
                if signals.ended() {
                    self.settle(runs, &mut builds, lanes).await?;
                    // A walk that failed as it settled is retried first.
                    if builds.walk_failure_mut().is_some() {
                        continue;
                    }
                    return Ok(BuilderEnd { stopped: false });
                }
                signals.report_idle(reports);
            }
            if self.stop.is_requested() {
                if let Some(abandoned) = snapshot.take() {
                    tracing::info!(
                        indexes = abandoned.indexes.len(),
                        "stop requested: abandoning the snapshot in progress; its request is \
                         served again on the next start"
                    );
                }
                if let Some(gathered) = coalescing.take() {
                    tracing::info!(
                        requests = gathered.deliveries.len(),
                        "stop requested: dropping the backfill requests gathered; they are served \
                         again on the next start"
                    );
                }
                // The snapshot batch and the requests are gone: so is their
                // place in the backfill line.
                builds.leave_lines();
                // The barrier (R7): the reader hands over what it read and
                // ends (this builder's channel closes) before anything is
                // settled.
                while let Some(run) = runs.next().await {
                    runs.push(run);
                }
                tracing::info!(
                    buffered = runs.changes(),
                    "stop requested: reading no further changes; landing what is in flight"
                );
                self.settle(runs, &mut builds, lanes).await?;
                return Ok(BuilderEnd { stopped: true });
            }
            let building = builds.running() > 0;
            // A failed walk's retry wait is no work: idle unless something
            // else runs (review of #457, A3).
            let quiet = building || coalescing.as_ref().is_some_and(|gathered| !gathered.retry);
            if let Some((clock, slot)) = clock {
                clock.idle(slot, (!quiet && snapshot.is_none()).then_some(mark));
            }
            let deadline = coalescing.as_ref().map(|gathered| gathered.deadline);
            // Built only while it waits in a line: one builder never does.
            let turn = builds.in_line().then(|| builds.turn());
            tokio::select! {
                // A permit or the state of the lines changed while this
                // builder waits in one (G9′): ask again.
                () = async {
                    match turn {
                        Some(turn) => turn.await,
                        None => std::future::pending().await,
                    }
                }, if builds.in_line() => {
                    builds.woke(Wake::Landed);
                    self.grant(runs, snapshot, &mut builds, lanes).await?;
                    mark = Instant::now();
                }
                run = runs.next(), if !reader_done => match run {
                    None => {
                        mark = if runs.lead().ended_waiting() {
                            self.waited(mark, None, snapshot, quiet, clock)
                        } else {
                            Instant::now()
                        };
                        reader_done = true;
                    }
                    Some(run) => {
                        self.waited(mark, run.started(), snapshot, quiet, clock);
                        runs.push(run);
                        builds.woke(Wake::Change);
                        self.grant(runs, snapshot, &mut builds, lanes).await?;
                        mark = Instant::now();
                    }
                },
                () = released(self.budget.as_deref(), builds.refused_since()),
                    if builds.refused().is_some() =>
                {
                    builds.woke(Wake::Released);
                    self.grant(runs, snapshot, &mut builds, lanes).await?;
                    mark = Instant::now();
                }
                landed = builds.next(), if building => {
                    builds.woke(Wake::Landed);
                    self.landed(landed, runs, snapshot, &mut builds, lanes).await?;
                    mark = Instant::now();
                }
                step = next_snapshot_step(snapshot),
                    if snapshot.as_ref().is_some_and(Snapshot::wants_keys) =>
                {
                    let Some(active) = snapshot.as_mut() else { continue };
                    // The wait for a page, if any, is `Page`, unless builds
                    // ran meanwhile: then the loop was waiting on them too.
                    if !building {
                        active.page_wait += mark.elapsed();
                    }
                    match step {
                        // Beside other builders a key read's failure is
                        // its walk's alone (slice 8, fix round 2).
                        Err(error) if several => {
                            if let Some(failed) = snapshot.take() {
                                let reached = Arc::clone(&failed.reached);
                                builds.fail_walk(error.into(), &reached);
                                if let Some(failure) = builds.walk_failure_mut() {
                                    failure.requests.extend(failed.into_requests());
                                }
                            }
                            mark = Instant::now();
                            continue;
                        }
                        Err(error) => return Err(error.into()),
                        Ok(Step::Keys) => active.take_keys(),
                        Ok(Step::Done) => active.done = true,
                    }
                    builds.woke(Wake::Page);
                    self.grant(runs, snapshot, &mut builds, lanes).await?;
                    mark = Instant::now();
                },
                delivery = inbox.recv(), if snapshot.is_none() => {
                    let Some(delivery) = delivery else { continue };
                    let at = self.waited(mark, None, snapshot, quiet, clock);
                    signals.received();
                    let gathered = coalescing.get_or_insert_with(|| Coalescing {
                        deliveries: Vec::new(),
                        deadline: at + self.batch.max_delay,
                        started: at,
                        retry: false,
                    });
                    gathered.deliveries.push(delivery);
                    while let Ok(more) = inbox.try_recv() {
                        signals.received();
                        gathered.deliveries.push(more);
                    }
                    if !gathered.retry && everyone_asked(&gathered.deliveries, lanes) {
                        // Planning awaits (its reads take the key-read
                        // permit): out of every line first (G9′).
                        builds.leave_lines();
                        debug_assert!(snapshot.is_none(), "a walk starts only once none runs");
                        *snapshot = self
                            .start_coalesced(coalescing, scopes, lanes, several)
                            .await?;
                    }
                    mark = Instant::now();
                }
                // A set starts only once no walk runs: a failed walk's retry
                // waits for a newer walk to finish, never drops it (review of
                // #457, A1).
                () = until(deadline), if deadline.is_some() && snapshot.is_none() => {
                    builds.leave_lines();
                    debug_assert!(snapshot.is_none(), "a walk starts only once none runs");
                    let retrying = coalescing.as_ref().is_some_and(|gathered| gathered.retry);
                    *snapshot = self
                        .start_coalesced(coalescing, scopes, lanes, several)
                        .await?;
                    if let (true, Some(index)) = (retrying, builds.index()) {
                        tracing::info!(
                            %index,
                            kind = "backfill",
                            "the index's backfill started again, each sink from where it got"
                        );
                        self.observer.on_index_restarted(index, Work::Backfill);
                    }
                    mark = Instant::now();
                }
                signal = signals.next() => {
                    mark = self.waited(mark, None, snapshot, quiet, clock);
                    // A backstop, not the mechanism: a batch the budget held
                    // back is granted on a release (the arm above). Should a
                    // release ever go unheard, the next tick still grants it,
                    // so it costs a second, never the run.
                    if signal == Signal::Tick && builds.refused().is_some() {
                        builds.woke(Wake::Tick);
                        self.grant(runs, snapshot, &mut builds, lanes).await?;
                        mark = Instant::now();
                    }
                }
            }
        }
    }

    /// A walk failed: its requests (each lane resumed where it got, and any
    /// gathered since) wait for its retry, after the walk's own wait, in a
    /// set nothing starts earlier; the builder's live work goes on (slice
    /// 8, fix round 2).
    fn retry_walk(
        &self,
        failure: super::builds::WalkFailure,
        coalescing: &mut Option<Coalescing>,
        retry: &mut Retry,
        index: Option<&IndexName>,
    ) {
        let super::builds::WalkFailure {
            error, requests, ..
        } = failure;
        let mut deliveries = requests;
        if let Some(gathered) = coalescing.take() {
            deliveries.extend(gathered.deliveries);
        }
        let wait = index.map_or(self.backoff.initial(), |index| {
            retry.failed(Work::Backfill, self.done(index))
        });
        if let Some(index) = index {
            tracing::error!(
                %index,
                %error,
                kind = "backfill",
                retry_in = ?wait,
                "the index's backfill failed; it is walked again after the wait, each sink from \
                 where it got, while the index's live changes keep flowing"
            );
            self.observer
                .on_index_failed(index, Work::Backfill, &error.to_string(), wait);
        }
        let now = Instant::now();
        *coalescing = Some(Coalescing {
            deliveries,
            deadline: now + wait,
            started: now,
            retry: true,
        });
    }

    /// Enter the fold when the budget held the live batch back, and leave it
    /// once nothing waits on the index (§3.9 I7): entering drops the queued
    /// slices (their documents stay waiting; the backlog takes them), and
    /// the reader hands the builder nothing while it folds.
    fn fold(&self, runs: &mut Runs, builds: &mut Builds) {
        let Some(index) = builds.index() else {
            return;
        };
        if builds.refused() == Some(Work::Live) {
            // Held by the budget (§3.9 §5): what the reader's exemption reads.
            runs.lead().hold(runs.slot(), true);
        }
        if !builds.folding() && builds.refused() == Some(Work::Live) {
            enter_fold(runs, builds);
        } else if builds.folding() && self.progress.waiting_on(index) == 0 {
            builds.set_folding(false);
            runs.lead().fold(runs.slot(), false);
            // Nothing waits and nothing is refused: nothing holds it. A
            // restart's hold (slice 8) ends here when the index has nothing
            // to build, since no live grant comes to end it (review of #456).
            if builds.refused() != Some(Work::Live) {
                runs.lead().hold(runs.slot(), false);
            }
        }
    }

    /// Start the gathered requests' snapshot, timed from the first request
    /// as `main` timed its gathering; with `apart`, its key reads run on
    /// tasks of their own.
    async fn start_coalesced(
        &self,
        coalescing: &mut Option<Coalescing>,
        scopes: &HashSet<IndexName>,
        lanes: &Lanes,
        apart: bool,
    ) -> Result<Option<Snapshot>> {
        let Some(gathered) = coalescing.take() else {
            return Ok(None);
        };
        let snapshot = self
            .start_snapshot(gathered.deliveries, scopes, lanes, apart)
            .await?;
        self.observer.on_stage(
            StageTiming::new(Stage::Snapshot, gathered.started.elapsed()).work(Work::Backfill),
        );
        Ok(snapshot)
    }
}

/// Enter the fold (§3.9 I7): drop the queued slices (their documents stay
/// waiting; the backlog takes them) and have the reader hand the builder
/// nothing. A builder alone, or of every index, never folds.
pub(super) fn enter_fold(runs: &mut Runs, builds: &mut Builds) {
    if !builds.several() || builds.index().is_none() || builds.folding() {
        return;
    }
    runs.clear();
    builds.set_folding(true);
    runs.lead().fold(runs.slot(), true);
}

/// Whether every sink with a lane asked (`main`'s rule: requests come one per
/// index, and once every sink asked a snapshot starts at once).
fn everyone_asked(deliveries: &[Delivery<Request>], lanes: &Lanes) -> bool {
    let asked: HashSet<&SinkName> = deliveries
        .iter()
        .map(|delivery| {
            let Request::Backfill { sink, .. } = delivery.item();
            sink
        })
        .collect();
    lanes.keys().all(|lane| asked.contains(&lane.sink))
}

/// A builder's builds, out of every gate line once the builder ends.
struct LeavesLines<'a>(&'a mut Builds);

impl Drop for LeavesLines<'_> {
    fn drop(&mut self) {
        self.0.leave_lines();
    }
}

impl std::ops::Deref for LeavesLines<'_> {
    type Target = Builds;

    fn deref(&self) -> &Builds {
        self.0
    }
}

impl std::ops::DerefMut for LeavesLines<'_> {
    fn deref_mut(&mut self) -> &mut Builds {
        self.0
    }
}

/// The coalescing deadline; never without one.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
