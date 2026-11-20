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
        /// The indexes with documents waiting that the budget left out:
        /// their lanes get nothing from this build, not even its step.
        deferred: Vec<IndexName>,
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
/// and the heap its documents hold per index when the budget counts them.
pub(super) struct Output {
    ids: Vec<DocumentId>,
    built: Built,
    bytes: BTreeMap<IndexName, usize>,
}

struct Slot {
    job: Job,
    built: Option<Built>,
    /// The budget's bytes for this build, one part per index: reserved at
    /// grant, settled when it lands.
    reservations: Vec<Reservation>,
    /// Each index's settled part, carried by that index's lane items.
    charges: BTreeMap<IndexName, Arc<Charge>>,
}

impl Slot {
    fn is_ready(&self) -> bool {
        self.built.is_some() || matches!(self.job, Job::Complete(_))
    }
}

/// Bytes per document of each index and kind, a running average of what
/// landed; [`FIRST_ESTIMATE`] before any did.
#[derive(Default)]
struct PerDocument(BTreeMap<(IndexName, Work), usize>);

impl PerDocument {
    fn of(&self, index: &IndexName, work: Work) -> usize {
        self.0
            .get(&(index.clone(), work))
            .copied()
            .unwrap_or(FIRST_ESTIMATE)
    }

    fn landed(&mut self, index: &IndexName, work: Work, bytes: usize, documents: usize) {
        if documents == 0 {
            return;
        }
        let average = self
            .0
            .entry((index.clone(), work))
            .or_insert(FIRST_ESTIMATE);
        *average = (*average * 7 + bytes / documents) / 8;
    }
}

/// Each kind's wait for the budget, kept by the engine across its runs.
#[derive(Debug, Default)]
pub(super) struct Waits {
    live: Wait,
    backfill: Wait,
}

/// One kind's hold: since when this engine run has waited (`None` while it
/// isn't), and whether the hold's refusal was counted already.
#[derive(Debug, Default)]
struct Wait {
    since: Option<Instant>,
    /// Kept across engine runs until a grant ends the hold, so a refusal of
    /// the same hold in the next run isn't counted again.
    counted: bool,
}

impl Waits {
    fn of(&mut self, work: Work) -> &mut Wait {
        match work {
            Work::Live => &mut self.live,
            Work::Backfill => &mut self.backfill,
        }
    }
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Each index's settled charge for a batch's documents.
pub(super) type Charges = BTreeMap<IndexName, Arc<Charge>>;

/// The live batch offered to [`Builds::choose`].
pub(super) struct LiveCandidate<L> {
    /// Handed back in [`Grant::Live`].
    pub(super) live: L,
    /// The documents waiting, per index, oldest waiting first.
    pub(super) documents: Vec<(IndexName, usize)>,
    /// The live run is full: a build may start beside a running live one.
    pub(super) full: bool,
    /// Changes were read since the last grant: a build that takes nothing
    /// still settles them.
    pub(super) changes: bool,
}

/// Which kind of batch a free slot goes to; a live grant carries back what
/// the live candidate was offered with and the indexes it may take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Grant<L> {
    Live(L, Vec<IndexName>),
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
    /// Bytes per document, per index, live and backfill: a running average
    /// of what landed, which a grant reserves per id.
    per_document: PerDocument,
    /// What the last choice reserved for the build it granted.
    reserved: Vec<Reservation>,
    /// The most documents an index may leave waiting while a live build
    /// takes the others (ADR 0012 stage 8a slice 4); `None` lets none wait:
    /// a live batch then goes whole or not at all, as with one index.
    defer_below: Option<usize>,
    /// The last choice found a batch the budget alone refused (for live,
    /// some index it left waiting).
    refused: Option<Work>,
    /// The release generation of that refusal: a release past it may let
    /// the batch go.
    refused_generation: u64,
    /// Each kind's wait for the budget: the engine's, so whether a hold was
    /// counted outlives the run it started in.
    waits: Arc<std::sync::Mutex<Waits>>,
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
    /// The engine run ends (an error, an abort, a clean end with a hold
    /// still open): a wait still open is reported up to now, so an outage
    /// before the next run doesn't stretch it; the hold stays counted.
    fn drop(&mut self) {
        for work in [Work::Live, Work::Backfill] {
            let since = lock(&self.waits).of(work).since.take();
            if let Some(since) = since {
                self.observer
                    .on_budget_wait(work, Wake::Settle, since.elapsed());
            }
        }
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
            per_document: PerDocument::default(),
            reserved: Vec::new(),
            defer_below: None,
            refused: None,
            refused_generation: 0,
            waits: Arc::default(),
            wake: Wake::Change,
            observer: Arc::new(crate::observer::NoopObserver),
            spawned: Vec::new(),
            progress: None,
        }
    }

    /// Keep the budget waits in `waits`, which outlives this engine run: a
    /// wait still open when the run ends is reported up to then, and a
    /// refusal of the same hold in the next run starts a new wait without
    /// counting the hold again.
    pub(super) fn with_waits(mut self, waits: Arc<std::sync::Mutex<Waits>>) -> Self {
        self.waits = waits;
        self
    }

    /// Let a live build take the indexes the budget has room for while each
    /// index left out has fewer than `below` documents waiting; past that,
    /// or without this, the batch waits whole.
    pub(super) fn with_deferral(mut self, below: Option<usize>) -> Self {
        self.defer_below = below;
        self
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

    /// The release generation the last refusal was decided at: what
    /// [`Budget::released_since`] waits past.
    pub(super) fn refused_since(&self) -> u64 {
        self.refused_generation
    }

    /// Each index's estimate for `documents` documents of `work`.
    fn parts(&self, work: Work, documents: &[(IndexName, usize)]) -> Vec<(IndexName, usize)> {
        documents
            .iter()
            .map(|(index, count)| {
                (
                    index.clone(),
                    self.per_document.of(index, work).saturating_mul(*count),
                )
            })
            .collect()
    }

    /// Which of `documents` (per index, oldest waiting first) the budget has
    /// room for in a batch of `work` (all or none for backfill); `None`
    /// when it grants nothing. A refusal, even of part of a live batch, is
    /// remembered with its generation, so the next release wakes the engine.
    fn affordable(
        &mut self,
        work: Work,
        documents: &[(IndexName, usize)],
    ) -> Option<Vec<IndexName>> {
        let Some(budget) = &self.budget else {
            return Some(documents.iter().map(|(index, _)| index.clone()).collect());
        };
        let admission = budget.admit(work, &self.parts(work, documents));
        if admission.refused {
            self.refuse(work, admission.generation);
        }
        admission.grants().then_some(admission.admitted)
    }

    fn refuse(&mut self, work: Work, generation: u64) {
        // The earliest refusal of a choice decides the wait: any release
        // after it may let a batch go.
        if self.refused.is_none() {
            self.refused_generation = generation;
        }
        self.refused.get_or_insert(work);
        let first = {
            let mut waits = lock(&self.waits);
            let wait = waits.of(work);
            wait.since.get_or_insert_with(Instant::now);
            !std::mem::replace(&mut wait.counted, true)
        };
        if first && let Some(budget) = &self.budget {
            budget.note_refused(work);
        }
    }

    /// Reserve what the gate just granted: `documents` of `work`, per index.
    /// Decided again under the budget's lock, so a reserver elsewhere taking
    /// the room meanwhile only makes this a refusal. `false` when refused.
    fn reserve(&mut self, work: Work, documents: &[(IndexName, usize)]) -> bool {
        let Some(budget) = self.budget.clone() else {
            return true;
        };
        match budget.reserve(work, &self.parts(work, documents)) {
            Ok(reservations) => {
                self.reserved = reservations;
                true
            }
            Err(generation) => {
                self.refuse(work, generation);
                false
            }
        }
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
    /// live candidate (handed back in [`Grant::Live`]), its waiting documents
    /// per index (oldest waiting first) and whether its run is full; the
    /// caller offers it only when no running build holds one of them. A live
    /// grant names the indexes the budget has room for; the rest keep
    /// waiting. `backfill` is the snapshot batch's ids.
    pub(super) fn choose<L>(
        &mut self,
        live: Option<LiveCandidate<L>>,
        backfill: Option<&[DocumentId]>,
    ) -> Option<Grant<L>> {
        self.refused = None;
        if !self.has_room() {
            return None;
        }
        let live = live
            .filter(|candidate| self.live_running.is_empty() || candidate.full)
            .and_then(|candidate| {
                let taken = self.live_part(&candidate.documents, candidate.changes)?;
                Some((candidate.live, taken))
            });
        let backfill = backfill.and_then(|ids| {
            let documents = per_index(ids);
            (!self.holds_any(ids) && self.affordable(Work::Backfill, &documents).is_some())
                .then_some(documents)
        });
        let (work, permit) = self.gate.choose(live.is_some(), backfill.is_some())?;
        let granted = match (work, live, backfill) {
            (Work::Live, Some((live, taken)), _) => self
                .reserve(Work::Live, &taken)
                .then(|| Grant::Live(live, taken.into_iter().map(|(index, _)| index).collect())),
            (Work::Backfill, _, Some(documents)) => self
                .reserve(Work::Backfill, &documents)
                .then_some(Grant::Backfill),
            _ => None,
        };
        if granted.is_some() {
            self.granted = Some(permit);
        }
        granted
    }

    /// Which of the live batch's `documents` (per index, oldest waiting
    /// first) could be granted ignoring the rule that a second live build
    /// waits for a full batch: for the last batch of a run. `None` when none.
    pub(super) fn can_take_last(
        &mut self,
        documents: &[(IndexName, usize)],
    ) -> Option<Vec<IndexName>> {
        self.refused = None;
        if !self.has_room() {
            return None;
        }
        let taken = self.live_part(documents, false)?;
        let (_, permit) = self.gate.choose(true, false)?;
        if !self.reserve(Work::Live, &taken) {
            return None;
        }
        self.granted = Some(permit);
        Some(taken.into_iter().map(|(index, _)| index).collect())
    }

    /// The part of a live batch's `documents` (per index, oldest waiting
    /// first) a build may take now: the indexes the budget has room for,
    /// when every index left out may wait (see
    /// [`with_deferral`](Self::with_deferral)) and the build takes
    /// something, or settles `changes` read since the last grant. `None`
    /// when the batch waits.
    fn live_part(
        &mut self,
        documents: &[(IndexName, usize)],
        changes: bool,
    ) -> Option<Vec<(IndexName, usize)>> {
        let admitted = self.affordable(Work::Live, documents);
        let admitted = match (admitted, self.defer_below) {
            (Some(admitted), _) => admitted,
            (None, Some(_)) if changes => Vec::new(),
            (None, _) => return None,
        };
        let (taken, left): (Vec<_>, Vec<_>) = documents
            .iter()
            .cloned()
            .partition(|(index, _)| admitted.contains(index));
        let may_wait = left.is_empty()
            || self
                .defer_below
                .is_some_and(|below| left.iter().all(|(_, waiting)| *waiting < below));
        if !may_wait || (taken.is_empty() && !left.is_empty() && !changes) {
            return None;
        }
        Some(taken)
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
            reservations: Vec::new(),
            charges: BTreeMap::new(),
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
        // A live grant that leaves an index waiting for the budget doesn't
        // end the live wait: one index held back while others build is one
        // wait, from its first refusal to the grant that takes it.
        let still_waiting = matches!(&job, Job::Live { deferred, .. } if !deferred.is_empty());
        if !still_waiting {
            let since = {
                let mut waits = lock(&self.waits);
                let wait = waits.of(work);
                wait.counted = false;
                wait.since.take()
            };
            if let Some(since) = since {
                self.observer
                    .on_budget_wait(work, self.wake, since.elapsed());
            }
        }
        let reservations = std::mem::take(&mut self.reserved);
        let measure = self.budget.is_some();
        self.slots.push_back(Slot {
            job,
            built: None,
            reservations,
            charges: BTreeMap::new(),
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
            let mut bytes: BTreeMap<IndexName, usize> = BTreeMap::new();
            if let (Ok(documents), true) = (&documents, measure) {
                for document in documents {
                    *bytes.entry(document.id().index.clone()).or_insert(0) +=
                        document_bytes(document);
                }
            }
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
                let documents = per_index(&ids);
                for reservation in std::mem::take(&mut slot.reservations) {
                    let Some(index) = reservation.index().cloned() else {
                        continue;
                    };
                    let landed = bytes.get(&index).copied().unwrap_or(0);
                    let count = documents
                        .iter()
                        .find(|(of, _)| *of == index)
                        .map_or(0, |(_, count)| *count);
                    self.per_document
                        .landed(&index, reservation.work(), landed, count);
                    slot.charges
                        .insert(index, Arc::new(reservation.settle(landed)));
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
    /// for each index's documents.
    pub(super) fn pop_ready(&mut self) -> Option<(Job, Option<Built>, Charges)> {
        if !self.slots.front().is_some_and(Slot::is_ready) {
            return None;
        }
        let slot = self.slots.pop_front()?;
        self.front += 1;
        Some((slot.job, slot.built, slot.charges))
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

/// How many of `ids` each index has, in order of first appearance.
fn per_index(ids: &[DocumentId]) -> Vec<(IndexName, usize)> {
    let mut counts: Vec<(IndexName, usize)> = Vec::new();
    for id in ids {
        match counts.iter_mut().find(|(index, _)| *index == id.index) {
            Some((_, count)) => *count += 1,
            None => counts.push((id.index.clone(), 1)),
        }
    }
    counts
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
        let users = IndexName::try_new("users").unwrap();
        let budget = Arc::new(Budget::new(64 * 1024));
        let charge: Vec<Charge> = budget
            .reserve(Work::Live, &[(users.clone(), 60 * 1024)])
            .unwrap()
            .into_iter()
            .map(|part| part.settle(60 * 1024))
            .collect();
        let mut builds = Builds::new(4, Some(Arc::clone(&budget)));
        budget.after_check(move || drop(charge));

        let one = [(users.clone(), 1)];
        assert!(
            builds.affordable(Work::Live, &one).is_none(),
            "60 KiB held: a 16 KiB batch doesn't fit in 64"
        );
        tokio::time::timeout(
            CONFIRM_TICK - Duration::from_millis(1),
            budget.released_since(builds.refused_since()),
        )
        .await
        .expect("the release after the check wakes the engine before the confirm tick");
        assert_eq!(
            builds.affordable(Work::Live, &one),
            Some(vec![users]),
            "and the batch is granted"
        );
    }

    #[derive(Debug, Default)]
    struct Waited(std::sync::Mutex<Vec<(Work, Wake, Duration)>>);

    impl Observer for Waited {
        fn on_budget_wait(&self, work: Work, wake: Wake, waited: Duration) {
            self.0.lock().unwrap().push((work, wake, waited));
        }
    }

    /// One hold over two engine runs (two `Builds` sharing the engine's
    /// waits), refused in both: counted once; the first run's wait is
    /// reported when it ends, up to then, and the second run's refusal starts
    /// a new wait, reported at the grant.
    #[tokio::test(start_paused = true)]
    async fn a_hold_refused_again_in_the_next_run_is_counted_once_and_timed_per_run() {
        let users = IndexName::try_new("users").unwrap();
        let budget = Arc::new(Budget::new(64 * 1024));
        let charge: Vec<Charge> = budget
            .reserve(Work::Live, &[(users.clone(), 60 * 1024)])
            .unwrap()
            .into_iter()
            .map(|part| part.settle(60 * 1024))
            .collect();
        let waits = Arc::default();
        let observer = Arc::new(Waited::default());
        let one = [(users.clone(), 1)];
        {
            let mut run = Builds::new(4, Some(Arc::clone(&budget)))
                .with_observer(Arc::clone(&observer) as Arc<dyn Observer>)
                .with_waits(Arc::clone(&waits));
            assert!(run.affordable(Work::Live, &one).is_none());
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
        let mut run = Builds::new(4, Some(Arc::clone(&budget)))
            .with_observer(Arc::clone(&observer) as Arc<dyn Observer>)
            .with_waits(Arc::clone(&waits));
        assert!(run.affordable(Work::Live, &one).is_none());
        tokio::time::sleep(Duration::from_secs(5)).await;
        drop(charge);
        let documents: Arc<dyn DocumentBuilder> = Arc::new(Nothing);
        let job = Job::Live {
            taken: Vec::new(),
            position: Position::new(0),
            changes: 0,
            step: crate::Progress::new(std::iter::empty()).close_step(),
            deferred: Vec::new(),
        };
        run.start(&documents, Vec::new(), job, &|_| false).await;
        assert_eq!(budget.refused(Work::Live), 1, "one hold, counted once");
        let waited: Vec<Duration> = observer
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|(_, _, w)| *w)
            .collect();
        assert_eq!(
            waited,
            [Duration::from_secs(2), Duration::from_secs(5)],
            "each run's wait, not the hour between them"
        );
    }

    #[derive(Debug)]
    struct Nothing;

    #[async_trait::async_trait]
    impl DocumentBuilder for Nothing {
        async fn resolve(
            &self,
            _: &kernel::TableName,
            _: &source::RowKey,
            _: Option<&source::RowImage>,
            _: Option<&source::RowImage>,
        ) -> source::Result<Vec<DocumentId>> {
            Ok(Vec::new())
        }

        async fn build(&self, id: &DocumentId) -> source::Result<Document> {
            Ok(Document::Delete { id: id.clone() })
        }

        fn backfill_indexes(&self) -> Vec<IndexName> {
            Vec::new()
        }

        async fn last_key(&self, _: &IndexName) -> source::Result<Option<source::RowKey>> {
            Ok(None)
        }

        async fn keys(
            &self,
            _: &IndexName,
            _: Option<&source::RowKey>,
            _: &source::RowKey,
            _: usize,
        ) -> source::Result<Vec<source::RowKey>> {
            Ok(Vec::new())
        }
    }
}
