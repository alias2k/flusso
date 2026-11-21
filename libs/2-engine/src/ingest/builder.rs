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
//! (`main`'s rule, over every index while there is one builder). Live
//! batches are granted meanwhile: on `main` and 6a they waited the
//! coalescing out.
//!
//! **The end and the stop.** Once the reader ended with its stream and
//! nothing is left to serve, the builder reports itself idle and waits for
//! the dispatcher's end, then settles. At the stop (the dispatcher raises it)
//! it drops its snapshot and coalescing set (their requests are served again
//! on the next start), waits for the reader's end (R7), and settles.

use std::collections::HashSet;

use kernel::{IndexName, SinkName};
use std::pin::Pin;
use std::sync::Arc;
use stream::{Delivery, Request};

use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::time::Instant;

use super::builds::Builds;
use super::dispatcher::{Reports, Signal, Signals};
use super::live::Runs;
use super::snapshot::{Snapshot, Step, next_snapshot_step};
use super::{IdleClock, IngestEngine, Lanes, released};
use crate::error::Result;
use crate::observer::{Stage, StageTiming, Wake, Work};

/// How a builder ended without an error: whether the stop ended it.
pub(super) struct BuilderEnd {
    pub(super) stopped: bool,
}

/// Requests gathered for one snapshot, and when it starts at the latest.
struct Coalescing {
    deliveries: Vec<Delivery<Request>>,
    deadline: Instant,
    /// Every sink asked or the deadline passed: it starts once it holds the
    /// walk token, gathering on meanwhile (§3.9 I11).
    ready: bool,
    /// When the first request arrived: the snapshot's start is timed from
    /// here, as `main` timed its gathering.
    started: Instant,
}

/// What the builder borrows for one engine run.
pub(super) struct Building<'a> {
    pub(super) runs: Runs,
    pub(super) inbox: mpsc::UnboundedReceiver<Delivery<Request>>,
    pub(super) signals: &'a Signals,
    pub(super) reports: &'a Reports,
    /// Other builders run beside it: a builder held by the budget folds, so
    /// its index doesn't stop the reading for theirs (§3.9 I7). Alone, it
    /// never folds: it is 6b-1b's builder.
    pub(super) several: bool,
    /// The walk token: one walk at a time over every builder (`main`'s
    /// order), a builder waiting for it coalescing on.
    pub(super) walks: &'a Arc<Semaphore>,
    /// With several builders, the engine's idle clock and this builder's
    /// place on it.
    pub(super) clock: Option<(&'a IdleClock, usize)>,
    pub(super) lanes: &'a Lanes,
    pub(super) scopes: &'a HashSet<IndexName>,
    pub(super) builds: &'a mut Builds,
}

impl IngestEngine {
    /// The builder of one engine run: build until the end or the stop, then
    /// hand the end to the dispatcher over `done`.
    pub(super) async fn build(
        &self,
        building: Building<'_>,
        done: oneshot::Sender<Result<BuilderEnd>>,
    ) {
        let result = self.building(building).await;
        // The dispatcher is gone only once the run ended.
        let _ = done.send(result);
    }

    async fn building(&self, building: Building<'_>) -> Result<BuilderEnd> {
        let Building {
            mut runs,
            mut inbox,
            signals,
            reports,
            several,
            walks,
            clock,
            lanes,
            scopes,
            builds,
        } = building;
        // Out of every line however the builder ends (§3.9 G9′): a waiter
        // left behind would be offered permits nobody asks for again.
        let mut builds = LeavesLines(builds);
        // The walk token while this builder's walk runs, and its wait for it
        // (kept across polls: dropping it would lose its place).
        let mut walk: Option<OwnedSemaphorePermit> = None;
        let mut walk_wait: Option<Pin<Box<WalkWait>>> = None;
        let mut snapshot: Option<Snapshot> = None;
        let mut coalescing: Option<Coalescing> = None;
        // The reader ended: every slice of this builder is handed over (its
        // channel closed after the last one).
        let mut reader_done = false;
        // Where the last arm's work ended: every arm times its wait from here,
        // so the loop reads no clock of its own.
        let mut mark = Instant::now();

        loop {
            // A live stream that ends while a snapshot runs or requests wait
            // still gets those served; the builder settles once the
            // dispatcher saw nothing left to forward either.
            if several {
                self.fold(&mut runs, &mut builds);
            }
            if snapshot.is_none() {
                walk = None;
            }
            if reader_done
                && !self.stop.is_requested()
                && snapshot.is_none()
                && coalescing.is_none()
                && inbox.is_empty()
            {
                if signals.ended() {
                    self.settle(&mut runs, &mut builds, lanes).await?;
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
                // Out of the token's line: a stop walks nothing more.
                drop(walk_wait.take());
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
                self.settle(&mut runs, &mut builds, lanes).await?;
                return Ok(BuilderEnd { stopped: true });
            }
            let building = builds.running() > 0;
            let quiet = building || coalescing.is_some();
            if let Some((clock, slot)) = clock {
                clock.idle(slot, (!quiet && snapshot.is_none()).then_some(mark));
            }
            let deadline = coalescing
                .as_ref()
                .filter(|gathered| !gathered.ready)
                .map(|gathered| gathered.deadline);
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
                    self.grant(&mut runs, &mut snapshot, &mut builds, lanes).await?;
                    mark = Instant::now();
                }
                run = runs.next(), if !reader_done => match run {
                    None => {
                        mark = if runs.lead().ended_waiting() {
                            self.waited(mark, None, &mut snapshot, quiet, clock)
                        } else {
                            Instant::now()
                        };
                        reader_done = true;
                    }
                    Some(run) => {
                        self.waited(mark, run.started(), &mut snapshot, quiet, clock);
                        runs.push(run);
                        builds.woke(Wake::Change);
                        self.grant(&mut runs, &mut snapshot, &mut builds, lanes).await?;
                        mark = Instant::now();
                    }
                },
                () = released(self.budget.as_deref(), builds.refused_since()),
                    if builds.refused().is_some() =>
                {
                    builds.woke(Wake::Released);
                    self.grant(&mut runs, &mut snapshot, &mut builds, lanes).await?;
                    mark = Instant::now();
                }
                landed = builds.next(), if building => {
                    builds.woke(Wake::Landed);
                    self.landed(landed, &mut runs, &mut snapshot, &mut builds, lanes).await?;
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
                    self.grant(&mut runs, &mut snapshot, &mut builds, lanes).await?;
                    mark = Instant::now();
                },
                delivery = inbox.recv(), if snapshot.is_none() => {
                    let Some(delivery) = delivery else { continue };
                    let at = self.waited(mark, None, &mut snapshot, quiet, clock);
                    signals.received();
                    let gathered = coalescing.get_or_insert_with(|| Coalescing {
                        deliveries: Vec::new(),
                        deadline: at + self.batch.max_delay,
                        ready: false,
                        started: at,
                    });
                    gathered.deliveries.push(delivery);
                    while let Ok(more) = inbox.try_recv() {
                        signals.received();
                        gathered.deliveries.push(more);
                    }
                    if everyone_asked(&gathered.deliveries, lanes) {
                        gathered.ready = true;
                        if walk_wait.is_none() {
                            snapshot = self
                                .start_walkable(&mut coalescing, &mut walk, &mut walk_wait, walks, scopes, lanes)
                                .await?;
                        }
                    }
                    mark = Instant::now();
                }
                () = until(deadline), if deadline.is_some() => {
                    if let Some(gathered) = coalescing.as_mut() {
                        gathered.ready = true;
                    }
                    snapshot = self
                        .start_walkable(&mut coalescing, &mut walk, &mut walk_wait, walks, scopes, lanes)
                        .await?;
                    mark = Instant::now();
                }
                // Its turn to walk: the gathered requests start.
                permit = std::future::poll_fn(|cx| match walk_wait.as_mut() {
                    Some(waiting) => waiting.as_mut().poll(cx),
                    None => std::task::Poll::Pending,
                }), if walk_wait.is_some() => {
                    walk_wait = None;
                    walk = permit.ok();
                    snapshot = self.start_coalesced(&mut coalescing, scopes, lanes).await?;
                    mark = Instant::now();
                }
                signal = signals.next() => {
                    mark = self.waited(mark, None, &mut snapshot, quiet, clock);
                    // A backstop, not the mechanism: a batch the budget held
                    // back is granted on a release (the arm above). Should a
                    // release ever go unheard, the next tick still grants it,
                    // so it costs a second, never the run.
                    if signal == Signal::Tick && builds.refused().is_some() {
                        builds.woke(Wake::Tick);
                        self.grant(&mut runs, &mut snapshot, &mut builds, lanes).await?;
                        mark = Instant::now();
                    }
                }
            }
        }
    }

    /// Enter the fold when the budget held the live batch back, and leave it
    /// once nothing waits on the index (§3.9 I7): entering drops the queued
    /// slices (their documents stay waiting; the backlog takes them), and
    /// the reader hands the builder nothing while it folds.
    fn fold(&self, runs: &mut Runs, builds: &mut Builds) {
        let Some(index) = builds.index() else {
            return;
        };
        if !builds.folding() && builds.refused() == Some(Work::Live) {
            enter_fold(runs, builds);
        } else if builds.folding() && self.progress.waiting_on(index) == 0 {
            builds.set_folding(false);
            runs.lead().fold(runs.slot(), false);
        }
    }

    /// Start the gathered requests' snapshot once this builder holds the walk
    /// token: at once when it is free (one builder always finds it so), else
    /// once its wait for it ends (an arm of the loop; the builder gathers on
    /// meanwhile).
    #[allow(clippy::too_many_arguments)]
    async fn start_walkable(
        &self,
        coalescing: &mut Option<Coalescing>,
        walk: &mut Option<OwnedSemaphorePermit>,
        walk_wait: &mut Option<Pin<Box<WalkWait>>>,
        walks: &Arc<Semaphore>,
        scopes: &HashSet<IndexName>,
        lanes: &Lanes,
    ) -> Result<Option<Snapshot>> {
        if walk.is_none() {
            match Arc::clone(walks).try_acquire_owned() {
                Ok(permit) => *walk = Some(permit),
                Err(_) => {
                    *walk_wait = Some(Box::pin(Arc::clone(walks).acquire_owned()));
                    return Ok(None);
                }
            }
        }
        self.start_coalesced(coalescing, scopes, lanes).await
    }

    /// Start the gathered requests' snapshot, timed from the first request
    /// as `main` timed its gathering.
    async fn start_coalesced(
        &self,
        coalescing: &mut Option<Coalescing>,
        scopes: &HashSet<IndexName>,
        lanes: &Lanes,
    ) -> Result<Option<Snapshot>> {
        let Some(gathered) = coalescing.take() else {
            return Ok(None);
        };
        let snapshot = self
            .start_snapshot(gathered.deliveries, scopes, lanes)
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

/// A builder's wait for the walk token.
type WalkWait = dyn Future<Output = std::result::Result<OwnedSemaphorePermit, tokio::sync::AcquireError>>
    + Send;

/// The coalescing deadline; never without one.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
