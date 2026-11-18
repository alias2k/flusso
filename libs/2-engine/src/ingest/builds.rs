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
//! `2 × builds` slots, running or built. A slow build at the front holds back
//! at most `2 × builds − 1` built batches behind it.
//!
//! **Admission.** A batch is granted only when no running build holds any of
//! its documents. A later build of a document therefore starts after the
//! earlier one returned, reads newer state, and lands later (a larger slot).
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
//! engine publishes. A spawned build is awaited through [`Flights`].
//!
//! ```text
//! slots    [L1 ready] [B1 running] [L2 ready] [Complete]
//! publish  L1; L2 and Complete wait for B1
//! ```

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::task::Poll;

use futures::future::BoxFuture;
use kernel::{IndexName, Position, SinkName};
use source::document::{Document, DocumentBuilder, DocumentId};
use tokio::time::{Duration, Instant};

use super::snapshot::Snapshot;
use crate::budget::{Budget, Charge, Reservation, document_bytes};
use crate::error::{EngineError, Result};
use crate::observer::Work;
use crate::sink_engine::flights::Flights;

/// How many of every kind of grant go to backfill when both kinds wait.
const BACKFILL_EVERY: u64 = 4;

/// The bytes per document a build is first estimated at, before any build
/// of its kind landed: ADR 0012's measured `users` document.
const FIRST_ESTIMATE: usize = 16 * 1024;

/// What a granted slot publishes.
pub(super) enum Job {
    /// A live batch, to every lane, with the position of its last change.
    Live {
        oldest: Option<Position>,
        position: Option<Position>,
        changes: usize,
    },
    /// A snapshot batch, to each lane the slice it asked for.
    Backfill {
        requested: BTreeMap<SinkName, BTreeSet<IndexName>>,
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

/// Which kind of batch a free slot goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Grant {
    Live,
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
    contested: u64,
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
            contested: 0,
            halted: false,
            spare: Vec::new(),
            budget,
            per_document: PerDocument {
                live: FIRST_ESTIMATE,
                backfill: FIRST_ESTIMATE,
            },
            refused: None,
            waiting: (false, false),
        }
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
        if budget.fits(work, self.estimate(work, ids)) {
            return true;
        }
        budget.want_release();
        self.refused.get_or_insert(work);
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

    /// A build may start: fewer than `builds` run, the queue has room, and
    /// no build failed.
    fn has_room(&self) -> bool {
        !self.halted && self.flights.len() < self.limit && self.slots.len() < 2 * self.limit
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

    /// Which waiting batch the next free slot goes to, if any. `live` is the
    /// live batch's ids and whether it is full; `backfill` the snapshot
    /// batch's ids.
    pub(super) fn choose(
        &mut self,
        live: Option<(&[DocumentId], bool)>,
        backfill: Option<&[DocumentId]>,
    ) -> Option<Grant> {
        self.refused = None;
        if !self.has_room() {
            return None;
        }
        let live = live.is_some_and(|(ids, full)| {
            (self.live_running.is_empty() || full)
                && !self.holds_any(ids)
                && self.affordable(Work::Live, ids.len())
        });
        let backfill = backfill
            .is_some_and(|ids| !self.holds_any(ids) && self.affordable(Work::Backfill, ids.len()));
        match (live, backfill) {
            (true, true) => {
                self.contested += 1;
                Some(if self.contested.is_multiple_of(BACKFILL_EVERY) {
                    Grant::Backfill
                } else {
                    Grant::Live
                })
            }
            (true, false) => Some(Grant::Live),
            (false, true) => Some(Grant::Backfill),
            (false, false) => None,
        }
    }

    /// Whether the live batch could be granted ignoring the rule that a
    /// second live build waits for a full batch: for the last batch of a run.
    pub(super) fn can_take_last(&mut self, ids: &[DocumentId]) -> bool {
        self.refused = None;
        self.has_room() && !self.holds_any(ids) && self.affordable(Work::Live, ids.len())
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
        let live = matches!(job, Job::Live { .. });
        let work = if live { Work::Live } else { Work::Backfill };
        match work {
            Work::Live => self.waiting.0 = false,
            Work::Backfill => self.waiting.1 = false,
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
        if let Poll::Ready(output) = futures::poll!(build.as_mut()) {
            self.land(seq, Some(output), halts);
            return;
        }
        if let Some(held) = held {
            self.holding.push((seq, held));
        }
        let handle = tokio::spawn(build);
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
        let Some(slot) = seq
            .checked_sub(self.front)
            .and_then(|at| self.slots.get_mut(at as usize))
        else {
            return;
        };
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
        self.holding.clear();
        self.slots.clear();
    }
}

/// The sorted, distinct hashes of `ids`.
fn hashes(ids: &[DocumentId]) -> Vec<u64> {
    let mut hashes: Vec<u64> = ids
        .iter()
        .map(|id| {
            let mut hasher = DefaultHasher::new();
            id.hash(&mut hasher);
            hasher.finish()
        })
        .collect();
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
