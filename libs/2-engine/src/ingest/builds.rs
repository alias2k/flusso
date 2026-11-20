//! The builds the ingest engine has granted and not yet published: up to
//! `builds` running at once, published in the order they were granted
//! (ADR 0012 stage 4).
//!
//! **Slots.** Every grant takes the next slot, in a queue whose front is the
//! oldest unpublished one. A slot is published once it and every slot before
//! it are ready, so a lane gets its items in grant order whatever finishes
//! first. Live batches are granted in the order they formed, so live positions
//! only grow along a lane. A snapshot's `SnapshotComplete` takes a slot of its
//! own, ready at once, so it follows the snapshot's last batch.
//!
//! **Bound.** At most `builds` builds run, and the queue holds at most
//! `2 × builds` slots, running or built, plus a snapshot's completion (queued
//! without a free slot; it holds no documents). A slow build at the front
//! holds back at most `2 × builds − 1` built batches behind it.
//!
//! **Admission.** A batch is granted only when no running build holds any of
//! its documents: a live take leaves waiting what a running build holds (it
//! takes all or none), and a backfill batch is checked here. A later build
//! of a document therefore starts after the earlier one returned, reads newer
//! state, and lands later (a larger slot).
//! Documents are compared by a 64-bit hash of their id, kept sorted per build;
//! a collision only holds a batch back. With `builds = 1` nothing is hashed:
//! no grant happens while a build runs.
//!
//! **The gate.** When a live and a backfill batch could both take a free slot,
//! live goes first except every fourth such grant, which goes to backfill. A
//! grant only one kind could take doesn't move the count.
//!
//! **Running a build.** A build is polled once where it is granted. If that
//! finishes it, nothing is spawned; otherwise it is spawned on its own task,
//! so its CPU work runs beside the engine's and it makes progress while the
//! engine publishes. A spawned build is awaited through [`Flights`], and
//! aborted when the run's future is dropped, so none outlives the engine.
//!
//! ```text
//! slots    [L1 ready] [B1 running] [L2 ready] [Complete]
//! publish  L1; L2 and Complete wait for B1
//! ```

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::task::Poll;

use futures::future::BoxFuture;
use kernel::{IndexName, Position, SinkName};
use source::document::{Document, DocumentBuilder, DocumentId};
use tokio::task::AbortHandle;
use tokio::time::{Duration, Instant};

use super::gate::{Gate, Permit};
use super::snapshot::Snapshot;
use crate::budget::{Budget, Charge, Reservation, document_bytes};
use crate::error::{EngineError, Result};
use crate::observer::{Observer, Wake, Work};
use crate::progress::{Progress, Step, Token};
use crate::sink_engine::flights::Flights;

/// The bytes per document a build is first estimated at, before any build
/// of its kind landed: ADR 0012's measured `users` document.
const FIRST_ESTIMATE: usize = 16 * 1024;

/// What a granted slot publishes.
pub(super) enum Job {
    /// A live batch, to every lane, with the position of its live run's last
    /// change and, per index, the pending build it took (released once
    /// published).
    Live {
        taken: Vec<(IndexName, Token, Position)>,
        position: Position,
        changes: usize,
        step: Step,
    },
    /// A snapshot batch, to each lane the slice it asked for, each index's
    /// slice carrying how far its walk got (the batch's greatest key of it).
    Backfill {
        requested: BTreeMap<SinkName, BTreeMap<IndexName, stream::Ticket>>,
        marks: BTreeMap<IndexName, stream::WalkMark>,
    },
    /// A snapshot's end: `SnapshotComplete` to every lane that asked.
    Complete(Box<Snapshot>),
}

/// A build's result and how long it ran.
pub(super) struct Built {
    pub(super) documents: Result<Vec<Document>>,
    pub(super) took: Duration,
}

/// What [`Builds::start`] hands a build: the ids, given back when it ends,
/// and the heap its documents hold when the budget counts them.
pub(super) struct Output {
    ids: Vec<DocumentId>,
    built: Built,
    bytes: usize,
}

struct Slot {
    job: Job,
    built: Option<Built>,
    /// The budget's bytes for this build: reserved at grant, settled when it
    /// lands.
    reservation: Option<Reservation>,
    charge: Option<Arc<Charge>>,
}

impl Slot {
    fn is_ready(&self) -> bool {
        self.built.is_some() || matches!(self.job, Job::Complete(_))
    }
}

/// Bytes per document of each kind, a running average of what landed.
struct PerDocument {
    live: usize,
    backfill: usize,
}

impl PerDocument {
    fn of(&self, work: Work) -> usize {
        match work {
            Work::Live => self.live,
            Work::Backfill => self.backfill,
        }
    }

    fn of_mut(&mut self, work: Work) -> &mut usize {
        match work {
            Work::Live => &mut self.live,
            Work::Backfill => &mut self.backfill,
        }
    }
}

/// Which kind of batch a free slot goes to; a live grant carries back what
/// the live candidate was offered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Grant<L> {
    Live(L),
    Backfill,
}

/// The granted, unpublished slots and the builds running for them.
pub(super) struct Builds {
    limit: usize,
    slots: VecDeque<Slot>,
    /// The slot number of the front slot.
    front: u64,
    flights: Flights<'static, (u64, Option<Output>)>,
    /// The sorted id hashes of each running build, by slot number.
    holding: Vec<(u64, Vec<u64>)>,
    /// The slot numbers of the live builds running on their own tasks.
    live_running: Vec<u64>,
    /// The permits every build holds while it runs (ADR 0012 §5).
    gate: Arc<Gate>,
    /// The permit the last choice took, for the build it granted.
    granted: Option<Permit>,
    /// Each running build's permit, by slot number.
    permits: Vec<(u64, Permit)>,
    /// A build failed: nothing more is granted.
    halted: bool,
    spare: Vec<Vec<DocumentId>>,
    budget: Option<Arc<Budget>>,
    /// Bytes per document, live and backfill: a running average of what
    /// landed, which a grant reserves per id.
    per_document: PerDocument,
    /// The last choice found a batch the budget alone refused.
    refused: Option<Work>,
    /// The kinds counted as waiting for the budget since their last grant.
    waiting: (bool, bool),
    /// When each kind (live, backfill) was first held back by the budget
    /// since its last grant.
    refused_at: (Option<Instant>, Option<Instant>),
    /// What woke the loop before this grant, for [`Observer::on_budget_wait`].
    wake: Wake,
    observer: Arc<dyn Observer>,
    /// The spawned builds, aborted when the run's future is dropped (a
    /// supervisor's abort, a shutdown deadline) so none outlives it.
    spawned: Vec<(u64, AbortHandle)>,
    /// Told when a live build lands, so its documents leave admission there
    /// too (the pending take reads it).
    progress: Option<Arc<Progress>>,
}

impl Drop for Builds {
    fn drop(&mut self) {
        for (_, build) in self.spawned.drain(..) {
            build.abort();
        }
    }
}

impl std::fmt::Debug for Builds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builds")
            .field("limit", &self.limit)
            .field("slots", &self.slots.len())
            .field("running", &self.flights.len())
            .finish_non_exhaustive()
    }
}

impl Builds {
    pub(super) fn new(limit: usize, budget: Option<Arc<Budget>>) -> Self {
        Self {
            limit: limit.max(1),
            slots: VecDeque::with_capacity(2 * limit.max(1)),
            front: 0,
            flights: Flights::new(),
            holding: Vec::new(),
            live_running: Vec::new(),
            gate: Gate::new(limit),
            granted: None,
            permits: Vec::new(),
            halted: false,
            spare: Vec::new(),
            budget,
            per_document: PerDocument {
                live: FIRST_ESTIMATE,
                backfill: FIRST_ESTIMATE,
            },
            refused: None,
            waiting: (false, false),
            refused_at: (None, None),
            wake: Wake::Change,
            observer: Arc::new(crate::observer::NoopObserver),
            spawned: Vec::new(),
            progress: None,
        }
    }

    /// Tell `progress` when a live build lands.
    pub(super) fn with_progress(mut self, progress: Arc<Progress>) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Report each wait for the budget to `observer`.
    pub(super) fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// The loop woke on `wake`: what the next grants follow.
    pub(super) fn woke(&mut self, wake: Wake) {
        self.wake = wake;
    }

    /// The kind of batch the budget alone held back at the last choice, if
    /// any: a release may let it go.
    pub(super) fn refused(&self) -> Option<Work> {
        self.refused
    }

    fn estimate(&self, work: Work, ids: usize) -> usize {
        self.per_document.of(work).saturating_mul(ids)
    }

    /// Whether the budget has room for a batch of `ids` documents of `work`;
    /// when it hasn't, remember it and ask for a wake-up on the next release.
    fn affordable(&mut self, work: Work, ids: usize) -> bool {
        let Some(budget) = &self.budget else {
            return true;
        };
        // The wish comes first: a release landing between it and the check
        // still wakes the engine (kept as a permit, costing at most one
        // spurious wake), where a check first could lose it for good.
        budget.want_release();
        if budget.fits(work, self.estimate(work, ids)) {
            return true;
        }
        self.refused.get_or_insert(work);
        let refused_at = match work {
            Work::Live => &mut self.refused_at.0,
            Work::Backfill => &mut self.refused_at.1,
        };
        refused_at.get_or_insert_with(Instant::now);
        let waiting = match work {
            Work::Live => &mut self.waiting.0,
            Work::Backfill => &mut self.waiting.1,
        };
        if !std::mem::replace(waiting, true) {
            budget.note_refused(work);
        }
        false
    }

    /// Builds running on their own tasks.
    pub(super) fn running(&self) -> usize {
        self.flights.len()
    }

    /// A build may start as far as this run goes: the queue has room and no
    /// build failed. The permit is the gate's to give.
    fn has_room(&self) -> bool {
        !self.halted && self.slots.len() < 2 * self.limit
    }

    /// Whether a running build holds any of `ids`.
    fn holds_any(&self, ids: &[DocumentId]) -> bool {
        if self.holding.is_empty() {
            return false;
        }
        let hashes = hashes(ids);
        self.holding
            .iter()
            .any(|(_, held)| share_any(held, &hashes))
    }

    /// Whether any running build holds a document.
    pub(super) fn holds_some(&self) -> bool {
        !self.holding.is_empty()
    }

    /// Whether a running build holds `index`'s document `key`: a live take
    /// leaves it waiting, so a document is never built twice at once.
    pub(super) fn holds(&self, index: &IndexName, key: &source::RowKey) -> bool {
        if self.holding.is_empty() {
            return false;
        }
        let hash = id_hash(index, key);
        self.holding
            .iter()
            .any(|(_, held)| held.binary_search(&hash).is_ok())
    }

    /// Which waiting batch the next free slot goes to, if any. `live` is the
    /// live candidate (handed back in [`Grant::Live`]), how many documents it
    /// would take and whether its run is full; the caller offers it only when
    /// no running build holds one of them. `backfill` is the snapshot batch's
    /// ids.
    pub(super) fn choose<L>(
        &mut self,
        live: Option<(L, usize, bool)>,
        backfill: Option<&[DocumentId]>,
    ) -> Option<Grant<L>> {
        self.refused = None;
        if !self.has_room() {
            return None;
        }
        let live = live.filter(|(_, documents, full)| {
            (self.live_running.is_empty() || *full) && self.affordable(Work::Live, *documents)
        });
        let backfill = backfill
            .is_some_and(|ids| !self.holds_any(ids) && self.affordable(Work::Backfill, ids.len()));
        let (work, permit) = self.gate.choose(live.is_some(), backfill)?;
        self.granted = Some(permit);
        match (work, live) {
            (Work::Live, Some((live, _, _))) => Some(Grant::Live(live)),
            (Work::Backfill, _) => Some(Grant::Backfill),
            (Work::Live, None) => {
                self.granted = None;
                None
            }
        }
    }

    /// Whether the live batch could be granted ignoring the rule that a
    /// second live build waits for a full batch: for the last batch of a run.
    pub(super) fn can_take_last(&mut self, documents: usize) -> bool {
        self.refused = None;
        if !(self.has_room() && self.affordable(Work::Live, documents)) {
            return false;
        }
        match self.gate.choose(true, false) {
            Some((_, permit)) => {
                self.granted = Some(permit);
                true
            }
            None => false,
        }
    }

    /// An empty id list to fill, reusing one a finished build gave back.
    pub(super) fn spare(&mut self, capacity: usize) -> Vec<DocumentId> {
        self.spare
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(capacity))
    }

    /// Queue `SnapshotComplete` behind every slot granted so far.
    pub(super) fn complete(&mut self, snapshot: Snapshot) {
        self.slots.push_back(Slot {
            job: Job::Complete(Box::new(snapshot)),
            built: None,
            reservation: None,
            charge: None,
        });
    }

    /// Grant a slot to `job` and build `ids` for it: polled once here, and
    /// spawned when that doesn't finish it.
    pub(super) async fn start(
        &mut self,
        documents: &Arc<dyn DocumentBuilder>,
        ids: Vec<DocumentId>,
        job: Job,
        halts: &(dyn Fn(&Built) -> bool + Sync),
    ) {
        let seq = self.front + self.slots.len() as u64;
        if let Some(permit) = self.granted.take() {
            self.permits.push((seq, permit));
        }
        let live = matches!(job, Job::Live { .. });
        let work = if live { Work::Live } else { Work::Backfill };
        match work {
            Work::Live => self.waiting.0 = false,
            Work::Backfill => self.waiting.1 = false,
        }
        let refused_at = match work {
            Work::Live => self.refused_at.0.take(),
            Work::Backfill => self.refused_at.1.take(),
        };
        if let Some(since) = refused_at {
            self.observer
                .on_budget_wait(work, self.wake, since.elapsed());
        }
        let reservation = self
            .budget
            .as_ref()
            .and_then(|budget| budget.try_reserve(work, self.estimate(work, ids.len())));
        let measure = self.budget.is_some();
        self.slots.push_back(Slot {
            job,
            built: None,
            reservation,
            charge: None,
        });
        // With one build at a time nothing is ever compared, so nothing is
        // hashed.
        let held = (self.limit > 1).then(|| hashes(&ids));
        let documents = Arc::clone(documents);
        let mut build: BoxFuture<'static, Output> = Box::pin(async move {
            let started = Instant::now();
            let documents = documents.build_many(&ids).await.map_err(EngineError::from);
            let took = started.elapsed();
            // Measured here, on the build's own task when it was spawned.
            let bytes = match &documents {
                Ok(documents) if measure => documents.iter().map(document_bytes).sum(),
                _ => 0,
            };
            Output {
                ids,
                built: Built { documents, took },
                bytes,
            }
        });
        // A build that panics on this first poll panics the ingest task, as
        // a build run inline always did: the daemon then ends the deployment
        // with the task's error. One that panics later, on its own task,
        // lands as a failed build (`land`), which ends the run.
        if let Poll::Ready(output) = futures::poll!(build.as_mut()) {
            self.land(seq, Some(output), halts);
            return;
        }
        if let Some(held) = held {
            self.holding.push((seq, held));
        }
        let handle = tokio::spawn(build);
        self.spawned.push((seq, handle.abort_handle()));
        let landing = Box::pin(async move { (seq, handle.await.ok()) });
        if live {
            self.live_running.push(seq);
        }
        if let Some(output) = self.flights.start(landing) {
            self.land(output.0, output.1, halts);
        }
    }

    /// The next spawned build to finish; `None` when none runs.
    pub(super) async fn next(&mut self) -> Option<(u64, Option<Output>)> {
        self.flights.next().await
    }

    /// Record a finished build in its slot. A build that panicked is a
    /// failed build; one that `halts` stops every further grant.
    pub(super) fn land(
        &mut self,
        seq: u64,
        output: Option<Output>,
        halts: &(dyn Fn(&Built) -> bool + Sync),
    ) {
        if let Some(at) = self.holding.iter().position(|(held, _)| *held == seq) {
            self.holding.swap_remove(at);
        }
        if let Some(at) = self.live_running.iter().position(|live| *live == seq) {
            self.live_running.swap_remove(at);
        }
        if let Some(at) = self.spawned.iter().position(|(spawned, _)| *spawned == seq) {
            self.spawned.swap_remove(at);
        }
        if let Some(at) = self.permits.iter().position(|(held, _)| *held == seq) {
            self.permits.swap_remove(at);
        }
        let Some(slot) = seq
            .checked_sub(self.front)
            .and_then(|at| self.slots.get_mut(at as usize))
        else {
            return;
        };
        if let (Job::Live { taken, .. }, Some(progress)) = (&slot.job, &self.progress) {
            for (index, token, _) in taken {
                progress.build_landed(index, *token);
            }
        }
        let built = match output {
            Some(Output {
                mut ids,
                built,
                bytes,
            }) => {
                if let Some(reservation) = slot.reservation.take() {
                    let work = reservation.work();
                    if !ids.is_empty() {
                        let average = self.per_document.of_mut(work);
                        *average = (*average * 7 + bytes / ids.len()) / 8;
                    }
                    slot.charge = Some(Arc::new(reservation.settle(bytes)));
                }
                ids.clear();
                self.spare.push(ids);
                built
            }
            None => Built {
                documents: Err(EngineError::Source(source::SourceError::Connection(
                    "a document build panicked".to_owned(),
                ))),
                took: Duration::ZERO,
            },
        };
        if halts(&built) {
            self.halted = true;
        }
        slot.built = Some(built);
    }

    /// The front slot, once it is ready to publish, with the budget's charge
    /// for its documents.
    pub(super) fn pop_ready(&mut self) -> Option<(Job, Option<Built>, Option<Arc<Charge>>)> {
        if !self.slots.front().is_some_and(Slot::is_ready) {
            return None;
        }
        let slot = self.slots.pop_front()?;
        self.front += 1;
        Some((slot.job, slot.built, slot.charge))
    }

    /// Wait for every running build and drop what it built: the run is
    /// ending on an error, and no read of the source should outlive it.
    pub(super) async fn abandon(&mut self) {
        self.halted = true;
        while self.flights.next().await.is_some() {}
        self.permits.clear();
        self.holding.clear();
        self.slots.clear();
    }
}

/// The hash a document is compared by: its index, then its key.
fn id_hash(index: &IndexName, key: &source::RowKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    index.hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

/// The sorted, distinct hashes of `ids`.
fn hashes(ids: &[DocumentId]) -> Vec<u64> {
    let mut hashes: Vec<u64> = ids.iter().map(|id| id_hash(&id.index, &id.key)).collect();
    hashes.sort_unstable();
    hashes.dedup();
    hashes
}

/// Whether two sorted lists share an element, in one merge walk.
fn share_any(left: &[u64], right: &[u64]) -> bool {
    let (mut left, mut right) = (left.iter().peekable(), right.iter().peekable());
    while let (Some(a), Some(b)) = (left.peek(), right.peek()) {
        match a.cmp(b) {
            std::cmp::Ordering::Less => {
                left.next();
            }
            std::cmp::Ordering::Greater => {
                right.next();
            }
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ingest::confirm::CONFIRM_TICK;

    /// A release landing right after the budget refused a
    /// batch, before anything else, still wakes the engine at once; the wake
    /// isn't left to the confirm tick's backstop.
    #[tokio::test(start_paused = true)]
    async fn a_release_right_after_the_check_wakes_the_refused_engine() {
        let budget = Arc::new(Budget::new(64 * 1024));
        let charge = budget
            .try_reserve(Work::Live, 60 * 1024)
            .unwrap()
            .settle(60 * 1024);
        let mut builds = Builds::new(4, Some(Arc::clone(&budget)));
        budget.after_check(move || drop(charge));

        assert!(
            !builds.affordable(Work::Live, 1),
            "60 KiB held: a 16 KiB batch doesn't fit in 64"
        );
        tokio::time::timeout(CONFIRM_TICK - Duration::from_millis(1), budget.released())
            .await
            .expect("the release after the check wakes the engine before the confirm tick");
        assert!(builds.affordable(Work::Live, 1), "and the batch is granted");
    }
}
