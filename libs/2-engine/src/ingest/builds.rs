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

use super::gate::{Asked, Gate, Permit, Waiter};
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
        /// Each index's step at the grant, with its own frontier: its lanes
        /// are registered with it.
        steps: BTreeMap<IndexName, Step>,
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
    ids: Arc<Vec<DocumentId>>,
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
    /// The builders held by the budget now, by their index (`None`: a lone
    /// `Builds` of every index). Overlapping holds of several builders are
    /// one wait (§3.9 §9): it ends when none is held.
    held: std::collections::BTreeSet<Option<IndexName>>,
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
}

/// Which kind of batch a free slot goes to; a live grant carries back what
/// the live candidate was offered with and the indexes it may take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Grant<L> {
    /// What the live candidate was offered with, and the indexes taken.
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
    /// The ids of each running backfill build, by slot number, shared with
    /// the build: a live grant check looks each up in the pending set
    /// instead of walking what waits (a live build's ids are in the pending
    /// set already).
    backfilling: Vec<(u64, Arc<Vec<DocumentId>>)>,
    /// The slot numbers of the live builds running on their own tasks.
    live_running: Vec<u64>,
    /// The permits every build holds while it runs (ADR 0012 §5).
    gate: Arc<Gate>,
    /// The permit the last choice took, for the build it granted: the
    /// build holds it while it runs and drops it as it ends, on its own
    /// task, so a builder blocked publishing holds none for a finished
    /// build (§3.9 §6).
    granted: Option<Permit>,
    /// A build failed: nothing more is granted.
    halted: bool,
    spare: Vec<Vec<DocumentId>>,
    budget: Option<Arc<Budget>>,
    /// Bytes per document, per index, live and backfill: a running average
    /// of what landed, which a grant reserves per id.
    per_document: PerDocument,
    /// What the last choice reserved for the build it granted.
    reserved: Vec<Reservation>,
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
    /// The index this builder builds (ADR 0012 stage 8a, 6b-1c); `None`
    /// builds every index (tests of a lone `Builds`).
    index: Option<IndexName>,
    /// The builder folds (§3.9 I7): held by the budget, it queues no slice
    /// and takes its backlog oldest first instead of R3's batches.
    folding: bool,
    /// Other builders ask the gate too (G9′): a batch a pass didn't grant
    /// keeps a waiter in its kind's line, so a freed permit has someone to
    /// wake and G4, G6 hold across builders. Alone, a builder asks with G9's
    /// pass: nobody else can stand in a line.
    several: bool,
    /// Its waiters, one per kind at most, while it has a batch of that kind
    /// ready and a slot to run it.
    waiters: Vec<(Work, Waiter)>,
    /// The gate's generation when it last asked: a change past it is its
    /// turn to ask again.
    asked_at: u64,
}

impl Drop for Builds {
    /// The engine run ends (an error, an abort, a clean end with a hold
    /// still open): a wait still open is reported up to now, so an outage
    /// before the next run doesn't stretch it; the hold stays counted.
    fn drop(&mut self) {
        for work in [Work::Live, Work::Backfill] {
            let since = {
                let mut waits = lock(&self.waits);
                let wait = waits.of(work);
                wait.held.clear();
                wait.since.take()
            };
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
            backfilling: Vec::new(),
            live_running: Vec::new(),
            gate: Gate::new(
                limit,
                budget
                    .clone()
                    .unwrap_or_else(|| Arc::new(Budget::unlimited())),
            ),
            granted: None,
            halted: false,
            spare: Vec::new(),
            budget,
            per_document: PerDocument::default(),
            reserved: Vec::new(),
            refused: None,
            refused_generation: 0,
            waits: Arc::default(),
            wake: Wake::Change,
            observer: Arc::new(crate::observer::NoopObserver),
            spawned: Vec::new(),
            progress: None,
            index: None,
            folding: false,
            several: false,
            waiters: Vec::new(),
            asked_at: 0,
        }
    }

    /// Ask the gate beside other builders (G9′): see `several`.
    pub(super) fn beside_others(mut self, several: bool) -> Self {
        self.several = several;
        self
    }

    /// Whether other builders ask the gate beside it.
    pub(super) fn several(&self) -> bool {
        self.several
    }

    /// Whether it waits in a line of the gate.
    pub(super) fn in_line(&self) -> bool {
        !self.waiters.is_empty()
    }

    /// Ready once the gate changed since it last asked: its waiters may be
    /// offered a permit now (G7, G9′). Owned, so the builder's loop polls it
    /// beside the arms that borrow the builds.
    pub(super) fn turn(&self) -> impl Future<Output = ()> + Send + 'static {
        let gate = Arc::clone(&self.gate);
        let asked_at = self.asked_at;
        async move { gate.changed_since(asked_at).await }
    }

    /// Leave every line: no batch ready, no slot, or an await outside the
    /// builder's loop is about to pend (G9′: a waiter that can't ask holds no
    /// place).
    pub(super) fn leave_lines(&mut self) {
        self.waiters.clear();
    }

    /// Ask `gate` for permits, the one every builder of the engine run
    /// shares, instead of a gate of its own.
    pub(super) fn with_gate(mut self, gate: Arc<Gate>) -> Self {
        self.gate = gate;
        self
    }

    /// Build `index` alone.
    pub(super) fn of_index(mut self, index: IndexName) -> Self {
        self.index = Some(index);
        self
    }

    /// The index this builder builds; `None` for every index.
    pub(super) fn index(&self) -> Option<&IndexName> {
        self.index.as_ref()
    }

    /// Whether the builder folds (§3.9 I7).
    pub(super) fn folding(&self) -> bool {
        self.folding
    }

    /// Enter or leave the fold.
    pub(super) fn set_folding(&mut self, folding: bool) {
        self.folding = folding;
    }

    /// Keep the budget waits in `waits`, which outlives this engine run: a
    /// wait still open when the run ends is reported up to then, and a
    /// refusal of the same hold in the next run starts a new wait without
    /// counting the hold again.
    pub(super) fn with_waits(mut self, waits: Arc<std::sync::Mutex<Waits>>) -> Self {
        self.waits = waits;
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

    /// What a waiter for `documents` of `work` reserves: each index's
    /// estimate, or nothing when no budget counts them.
    fn reserving(&self, work: Work, documents: &[(IndexName, usize)]) -> Vec<(IndexName, usize)> {
        if self.budget.is_none() {
            return Vec::new();
        }
        self.parts(work, documents)
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
            wait.held.insert(self.index.clone());
            wait.since.get_or_insert_with(Instant::now);
            !std::mem::replace(&mut wait.counted, true)
        };
        if first && let Some(budget) = &self.budget {
            budget.note_refused(work);
        }
    }

    /// Ask the gate for a permit for the batches ready (R5, G9), keeping
    /// the permit and the reservation it grants. Every refusal is recorded
    /// as a hold.
    fn ask(
        &mut self,
        live: Option<Vec<(IndexName, usize)>>,
        backfill: Option<Vec<(IndexName, usize)>>,
    ) -> Option<Work> {
        if self.several {
            return self.ask_in_lines(live, backfill);
        }
        let passed = self.gate.pass(live, backfill);
        for (work, refusal) in passed.refused {
            self.refuse(work, refusal.generation);
        }
        let (work, permit, reserved) = passed.granted?;
        self.granted = Some(permit);
        self.reserved = reserved;
        Some(work)
    }

    /// [`ask`](Self::ask) beside other builders (G9′): each kind ready keeps
    /// its waiter, joined at the back of its line the first time and asked
    /// with its current parts after; a kind not ready leaves its line. A
    /// granted waiter has left its line already.
    fn ask_in_lines(
        &mut self,
        live: Option<Vec<(IndexName, usize)>>,
        backfill: Option<Vec<(IndexName, usize)>>,
    ) -> Option<Work> {
        self.asked_at = self.gate.generation();
        for (work, parts) in [(Work::Live, live), (Work::Backfill, backfill)] {
            let at = self.waiters.iter().position(|(kind, _)| *kind == work);
            match (parts, at) {
                (Some(parts), Some(at)) => {
                    if let Some((_, waiter)) = self.waiters.get_mut(at) {
                        waiter.set_parts(parts);
                    }
                }
                (Some(parts), None) => {
                    let waiter = self.gate.join(work, parts);
                    self.waiters.push((work, waiter));
                }
                (None, Some(at)) => {
                    self.waiters.remove(at);
                }
                (None, None) => {}
            }
        }
        for work in [Work::Live, Work::Backfill, Work::Live] {
            let Some(at) = self.waiters.iter().position(|(kind, _)| *kind == work) else {
                continue;
            };
            let Some((_, waiter)) = self.waiters.get_mut(at) else {
                continue;
            };
            match waiter.try_grant() {
                Asked::Granted(permit, reserved) => {
                    self.waiters.remove(at);
                    self.granted = Some(permit);
                    self.reserved = reserved;
                    return Some(work);
                }
                Asked::Refused(refusal) => self.refuse(work, refusal.generation),
                // Still refused at the budget's generation: a release, not a
                // change of the gate, lets it go, so the builder waits on the
                // budget for it.
                Asked::NotNow(Some(generation)) => self.refuse(work, generation),
                Asked::NotNow(None) => {}
            }
        }
        None
    }

    /// Whether a live build runs: only a full live batch may start beside
    /// it.
    pub(super) fn live_running(&self) -> bool {
        !self.live_running.is_empty()
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

    /// Whether a running backfill build holds a document: then a live
    /// grant check also looks up what it holds
    /// ([`held_by_backfill`](Self::held_by_backfill)).
    pub(super) fn backfill_holds_some(&self) -> bool {
        !self.backfilling.is_empty()
    }

    /// Offer `index`'s documents the running backfill builds hold to
    /// `found` until it answers `true`; whether it did.
    pub(super) fn held_by_backfill(
        &self,
        index: &IndexName,
        found: &mut dyn FnMut(&source::RowKey) -> bool,
    ) -> bool {
        self.backfilling.iter().any(|(_, ids)| {
            ids.iter()
                .filter(|id| id.index == *index)
                .any(|id| found(&id.key))
        })
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
            self.leave_lines();
            return None;
        }
        let live = live
            .filter(|candidate| self.live_running.is_empty() || candidate.full)
            .and_then(|candidate| {
                let taken = self.live_part(&candidate.documents)?;
                Some((candidate.live, taken))
            });
        let backfill = backfill.and_then(|ids| {
            let documents = per_index(ids);
            (!self.holds_any(ids)
                && (self.several || self.affordable(Work::Backfill, &documents).is_some()))
            .then_some(documents)
        });
        let live_parts = live
            .as_ref()
            .map(|(_, taken)| self.reserving(Work::Live, taken));
        let backfill_parts = backfill
            .as_ref()
            .map(|documents| self.reserving(Work::Backfill, documents));
        match (self.ask(live_parts, backfill_parts)?, live) {
            (Work::Live, Some((live, taken))) => Some(Grant::Live(
                live,
                taken.into_iter().map(|(index, _)| index).collect(),
            )),
            (Work::Backfill, _) => Some(Grant::Backfill),
            (Work::Live, None) => None,
        }
    }

    /// Whether the live batch's `documents` (per index, oldest waiting
    /// first) could be granted ignoring the rule that a second live build
    /// waits for a full batch: for the last batch of a run. The indexes it
    /// takes, `None` when it waits.
    pub(super) fn can_take_last(
        &mut self,
        documents: &[(IndexName, usize)],
    ) -> Option<Vec<IndexName>> {
        self.refused = None;
        if !self.has_room() {
            self.leave_lines();
            return None;
        }
        let taken = self.live_part(documents)?;
        let parts = self.reserving(Work::Live, &taken);
        self.ask(Some(parts), None)?;
        Some(taken.into_iter().map(|(index, _)| index).collect())
    }

    /// A live batch's `documents` (per index, oldest waiting first) when the
    /// budget has room for all of them; `None` when the batch waits whole
    /// (each builder builds one index: no index is left out while others
    /// build).
    ///
    /// Beside other builders the gate decides (G5): a batch the budget
    /// refuses keeps its waiter in the line, so a younger builder reserves
    /// behind it (G6) and small batches of other indexes can't starve it.
    fn live_part(&mut self, documents: &[(IndexName, usize)]) -> Option<Vec<(IndexName, usize)>> {
        if self.several {
            return Some(documents.to_vec());
        }
        let admitted = self.affordable(Work::Live, documents)?;
        documents
            .iter()
            .all(|(index, _)| admitted.contains(index))
            .then(|| documents.to_vec())
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
        let permit = self.granted.take();
        let live = matches!(job, Job::Live { .. });
        let work = if live { Work::Live } else { Work::Backfill };
        {
            let since = {
                let mut waits = lock(&self.waits);
                let wait = waits.of(work);
                wait.held.remove(&self.index);
                if wait.held.is_empty() {
                    wait.counted = false;
                    wait.since.take()
                } else {
                    None
                }
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
        let ids = Arc::new(ids);
        let backfilling = (self.limit > 1 && !live).then(|| Arc::clone(&ids));
        let documents = Arc::clone(documents);
        let mut build: BoxFuture<'static, Output> = Box::pin(async move {
            let started = Instant::now();
            let documents = documents.build_many(&ids).await.map_err(EngineError::from);
            let took = started.elapsed();
            drop(permit);
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
        if let Some(ids) = backfilling {
            self.backfilling.push((seq, ids));
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
        if let Some(at) = self.backfilling.iter().position(|(held, _)| *held == seq) {
            self.backfilling.swap_remove(at);
        }
        if let Some(at) = self.live_running.iter().position(|live| *live == seq) {
            self.live_running.swap_remove(at);
        }
        if let Some(at) = self.spawned.iter().position(|(spawned, _)| *spawned == seq) {
            self.spawned.swap_remove(at);
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
            Some(Output { ids, built, bytes }) => {
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
                if let Ok(mut ids) = Arc::try_unwrap(ids) {
                    ids.clear();
                    self.spare.push(ids);
                }
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
        self.holding.clear();
        self.backfilling.clear();
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
            steps: BTreeMap::new(),
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

    fn index(name: &str) -> IndexName {
        IndexName::try_new(name).unwrap()
    }

    /// `kib` of live documents of `name`, settled.
    fn held(budget: &Arc<Budget>, name: &str, kib: usize) -> Vec<Charge> {
        budget
            .reserve(Work::Live, &[(index(name), kib * 1024)])
            .unwrap()
            .into_iter()
            .map(|part| part.settle(kib * 1024))
            .collect()
    }

    fn one_user() -> LiveCandidate<()> {
        LiveCandidate {
            live: (),
            documents: vec![(index("users"), 1)],
            full: true,
        }
    }

    /// ADR 0012 stage 8a's acceptance test for R5, across two builders
    /// (G9′): `orders`' live batch is refused by the limit and waits in the
    /// live line; `users`' batch would fit by the limit, but `orders` is
    /// ahead of it, so it reserves behind (G6) and takes no room by the
    /// limit: refused. Once `orders` leaves the line, `users` is granted.
    #[tokio::test(start_paused = true)]
    async fn no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused() {
        let budget = Arc::new(Budget::new(64 * 1024));
        let _x = held(&budget, "x", 36);
        let _users = held(&budget, "users", 8);
        let _orders = held(&budget, "orders", 4);
        let gate = Gate::new(2, Arc::clone(&budget));
        let builder = |name: &str| {
            Builds::new(2, Some(Arc::clone(&budget)))
                .with_gate(Arc::clone(&gate))
                .of_index(index(name))
                .beside_others(true)
        };
        let mut orders = builder("orders");
        let mut users = builder("users");
        let two_orders = LiveCandidate {
            live: (),
            documents: vec![(index("orders"), 2)],
            full: true,
        };
        assert!(
            orders.choose(Some(two_orders), None).is_none(),
            "orders is refused by the limit (80 of 64 KiB)"
        );
        assert!(orders.in_line(), "orders waits in the live line");
        let held_before = budget.held();
        assert!(
            users.choose(Some(one_user()), None).is_none(),
            "users would fit by the limit (64 of 64 KiB), but not behind orders"
        );
        assert_eq!(budget.held(), held_before, "nothing reserved");
        orders.leave_lines();
        assert!(
            users.choose(Some(one_user()), None).is_some(),
            "with orders gone, users fits"
        );
    }

    /// G4 across builders (G9′): one permit, `orders` with a live batch
    /// ready and `users` with a backfill batch ready, both waiting in their
    /// lines each time the permit is freed. Every fourth contested grant
    /// goes to `users`' backfill, the others to `orders`' live batch.
    #[tokio::test(start_paused = true)]
    async fn every_fourth_contested_grant_goes_to_another_builders_backfill() {
        let gate = Gate::new(1, Arc::new(Budget::unlimited()));
        let builder = |name: &str| {
            Builds::new(1, None)
                .with_gate(Arc::clone(&gate))
                .of_index(index(name))
                .beside_others(true)
        };
        let mut orders = builder("orders");
        let mut users = builder("users");
        let live = || LiveCandidate {
            live: (),
            documents: vec![(index("orders"), 1)],
            full: true,
        };
        let ids = [DocumentId {
            index: index("users"),
            key: source::RowKey(Vec::new()),
        }];
        assert!(orders.choose(Some(live()), None).is_some(), "alone: live");
        let mut permit = orders.granted.take();
        let mut grants = Vec::new();
        for _ in 0..8 {
            assert!(orders.choose(Some(live()), None).is_none());
            assert!(users.choose::<()>(None, Some(&ids)).is_none());
            assert!(orders.in_line() && users.in_line(), "both wait");
            drop(permit.take());
            let to_users = users.choose::<()>(None, Some(&ids)).is_some();
            let to_orders = orders.choose(Some(live()), None).is_some();
            assert!(to_users != to_orders, "one grant per freed permit");
            grants.push(if to_users { "backfill" } else { "live" });
            permit = if to_users {
                users.granted.take()
            } else {
                orders.granted.take()
            };
        }
        assert_eq!(
            grants,
            [
                "live", "live", "live", "backfill", "live", "live", "live", "backfill"
            ]
        );
    }

    /// R5: a grant pass joins the lines only for the batches it has ready
    /// and leaves them before it returns, granted or not, so the builder
    /// never holds a place it can't use.
    #[tokio::test(start_paused = true)]
    async fn the_builder_leaves_the_lines_after_each_grant_pass() {
        let mut builds = Builds::new(1, None);
        let ids = [DocumentId {
            index: index("users"),
            key: source::RowKey(Vec::new()),
        }];
        assert!(builds.choose(Some(one_user()), Some(&ids)).is_some());
        assert_eq!(builds.gate.waiters(), 0, "granted: both waiters left");
        assert!(builds.choose(Some(one_user()), Some(&ids)).is_none());
        assert_eq!(builds.gate.waiters(), 0, "no permit free: both left");
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
