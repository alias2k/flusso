//! The memory budget: how many bytes of built documents the pipeline holds
//! before it builds more (ADR 0012 stage 5, shared per index since stage 8a).
//!
//! **What is counted.** The heap a built batch's documents own, measured by a
//! walk over each document ([`document_bytes`]) on the build's own task, plus a
//! floor of [`FLOOR`] per index of a batch so an empty part still counts. A
//! batch is counted once, however many lanes carry it, and per index: each
//! index's documents are charged to that index.
//!
//! **When.** The ingest engine *reserves* an estimate per index when it grants
//! a build ([`Budget::reserve`]), and *settles* each to the walked size when
//! the build lands ([`Reservation::settle`]). The settled [`Charge`] rides
//! inside that index's lane items; when its last copy is dropped (every lane
//! of the index acknowledged it and the channel's ledger let it go), its
//! `Drop` gives the bytes back. A reservation dropped unsettled (a failed
//! build, a stop, a dropped future) gives its estimate back the same way.
//!
//! **The rules.** A build is granted when rules 1–3 allow it, or rule 4
//! or 5 does.
//!
//! | Rule | Bound |
//! | --- | --- |
//! | 1 | everything held, live and backfill, ≤ the limit |
//! | 2 | backfill, over every index, ≤ half the limit, so live always has room |
//! | 3 | with two or more indexes ([`Budget::with_index_share`]), one index's **live** bytes ≤ its share of the limit |
//! | 4 | backfill holding nothing gets one batch, whatever 1 and 2 say |
//! | 5 | an index holding no live bytes gets one live batch, whatever 1 and 3 say (so live holding nothing takes every index waiting) |
//!
//! Rules 1, 2 and 4 (with 5 for live holding nothing) are the budget before it
//! was shared; rule 3 is the share. It counts an index's live bytes only: its
//! backfill is capped with every other index's by rule 2, so an index whose
//! backfill or reindex runs keeps, for its live changes, the room live had
//! before the share, up to its share. Rule 5 is what keeps the others building
//! while one index is stuck (a parked lane holding its share and its backfill
//! for hours): every other index still gets a batch whenever it holds no live
//! bytes, past the limit if need be. That lasts until the stuck index has
//! `2 × max_changes` documents waiting (the ingest engine's deferral cap):
//! then the live batch waits whole and the reading stops for every index, as
//! before the share. ADR 0012 stage 8a's 6b lifts that cap for an index the
//! budget holds. With one index rule 3 applies to
//! nothing and rule 5 is "live holding nothing": the budget answers as before
//! it was shared. A live build takes the indexes that fit and leaves the rest
//! waiting ([`Budget::admit`]); a backfill batch fits whole or not at all.
//! Indexes are offered oldest waiting first, and the first one refused by the
//! limit stops every younger one **that holds live bytes** from taking room by
//! it, so a stream of small batches never starves a large one (a younger
//! index gets at most its one rule-5 batch past it); an index refused by its
//! own share doesn't hold the others back.
//!
//! **One index's batch** is what a live grant takes of it: all its waiting
//! documents. An index the budget leaves waiting keeps fewer than
//! `2 × max_changes` documents, and the grant that takes them also takes
//! what the run being granted marked, so with two or more indexes one
//! index's batch holds the documents of up to about three live runs (with
//! one index, nothing waits: one run's, as before the share).
//!
//! **Bounds**, from the rules: rules 4 and 5 pass the limit, one backfill
//! batch and one live batch per index, and a settled batch may come out
//! larger than its estimate, so the whole stays within the limit plus one
//! live batch per index plus one backfill batch plus what each build in
//! flight settles past its estimate. Backfill stays within half plus one
//! batch and the same. One index's live bytes stay within its share, or one
//! batch when that batch alone is larger, and the same.
//!
//! **Many reservers.** Every count lives under one lock, so a check and its
//! reservation are one step and two reservers never pass the limit together.
//! Every release bumps a generation and wakes every waiter
//! ([`Budget::released_since`]), outside the lock: a waiter that read the
//! generation with its refusal is woken by any release after it. While a
//! waiter is in a build gate's line, an add does the same, so a refusal at
//! the current generation is always what the budget would answer now.
//!
//! ```
//! use std::sync::Arc;
//! use engine::{Budget, Work};
//! use kernel::IndexName;
//!
//! let users = IndexName::try_new("users").unwrap();
//! let orders = IndexName::try_new("orders").unwrap();
//! let budget = Arc::new(Budget::new(64 * 1024).with_index_share(2, 0.5));
//! let held = budget
//!     .reserve(Work::Live, &[(users.clone(), 30 * 1024)])
//!     .unwrap();
//! // `users` holds its share; `orders` still fits.
//! let admitted = budget
//!     .admit(Work::Live, &[(users.clone(), 8 * 1024), (orders.clone(), 8 * 1024)])
//!     .admitted;
//! assert_eq!(admitted, vec![orders]);
//! let charges: Vec<_> = held.into_iter().map(|part| part.settle(20 * 1024)).collect();
//! assert_eq!(budget.held_by(&users), 20 * 1024);
//! drop(charges);
//! assert_eq!(budget.held().total, 0);
//! ```

use std::collections::BTreeMap;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kernel::{Envelope, GenericValue, IndexName};
use source::document::Document;
use tokio::sync::Notify;

use crate::observer::Work;

/// The least a batch counts per index, so empty batches still bound the item
/// count.
pub const FLOOR: usize = 1024;

/// A B-tree leaf: eleven key-value slots and a small header.
const MAP_LEAF: usize = 11 * (size_of::<String>() + size_of::<GenericValue>()) + 16;

/// An internal B-tree node: a leaf plus its twelve edges.
const MAP_INTERNAL: usize = MAP_LEAF + 12 * size_of::<usize>();

/// Bytes of built documents held, in all and by backfill.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    /// Every kind, reservations included.
    pub total: usize,
    /// Backfill's share of `total`.
    pub backfill: usize,
}

/// What [`Budget::admit`] decided for a build's indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admission {
    /// The indexes that fit, in the order offered: every one or none for a
    /// backfill batch. Empty when nothing was offered.
    pub admitted: Vec<IndexName>,
    /// Whether something offered was refused: the build waits for a release
    /// for it (all of it for a backfill batch, what was left out for live).
    pub refused: bool,
    /// Whether a refusal was by the limit (rules 1 and 2), not by an index's
    /// share: what holds younger waiters back from room by the limit.
    pub by_limit: bool,
    /// The release generation the decision was made at: a waiter passes it
    /// to [`Budget::released_since`].
    pub generation: u64,
}

/// Why [`Budget::reserve_behind`] refused, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// The release generation of the refusal: a release past it may let
    /// the batch go ([`Budget::released_since`]).
    pub generation: u64,
    /// Refused by the limit (rules 1 and 2), not by an index's share.
    pub by_limit: bool,
}

impl Admission {
    /// Whether the build may go ahead: something fit, or nothing was offered
    /// and nothing refused.
    pub fn grants(&self) -> bool {
        !self.admitted.is_empty() || !self.refused
    }
}

/// The counts, under one lock.
#[derive(Debug, Default)]
struct State {
    total: usize,
    backfill: usize,
    /// Each index's bytes, live and backfill; an index holding nothing has
    /// no entry.
    by_index: BTreeMap<IndexName, Held>,
    /// Bumped by every release, and by every add while a waiter is
    /// [`watch`](Budget::watch)ing.
    generation: u64,
    /// Waiters in a gate's lines (stage 8a's G3): while there is one, what
    /// is held never changes without the generation moving, so a refusal
    /// made at the current generation is what the budget would answer now.
    watched: usize,
}

impl State {
    fn held_by(&self, index: &IndexName) -> usize {
        self.by_index.get(index).map_or(0, |held| held.total)
    }

    fn live_by(&self, index: &IndexName) -> usize {
        self.by_index
            .get(index)
            .map_or(0, |held| held.total - held.backfill)
    }

    /// Add `bytes`; whether the generation moved (only while watched).
    fn add(&mut self, work: Work, index: Option<&IndexName>, bytes: usize) -> bool {
        self.total = self.total.saturating_add(bytes);
        if work == Work::Backfill {
            self.backfill = self.backfill.saturating_add(bytes);
        }
        if let Some(index) = index {
            let held = self.by_index.entry(index.clone()).or_default();
            held.total = held.total.saturating_add(bytes);
            if work == Work::Backfill {
                held.backfill = held.backfill.saturating_add(bytes);
            }
        }
        let moved = self.watched > 0;
        if moved {
            self.generation = self.generation.wrapping_add(1);
        }
        moved
    }

    fn release(&mut self, work: Work, index: Option<&IndexName>, bytes: usize) {
        self.total = self.total.saturating_sub(bytes);
        if work == Work::Backfill {
            self.backfill = self.backfill.saturating_sub(bytes);
        }
        if let Some(index) = index
            && let Some(held) = self.by_index.get_mut(index)
        {
            held.total = held.total.saturating_sub(bytes);
            if work == Work::Backfill {
                held.backfill = held.backfill.saturating_sub(bytes);
            }
            held.backfill = held.backfill.min(held.total);
            if held.total == 0 {
                self.by_index.remove(index);
            }
        }
        self.generation = self.generation.wrapping_add(1);
    }
}

/// The pipeline's budget for built documents not yet acknowledged by every
/// lane. Shared by whatever reserves (the ingest engine) and whatever drops
/// the last copy of a batch (which releases).
#[derive(Debug)]
pub struct Budget {
    limit: usize,
    /// Rule 3's bound in bytes; `None` with fewer than two indexes.
    index_share: Option<usize>,
    state: Mutex<State>,
    released: Notify,
    refused_live: AtomicU64,
    refused_backfill: AtomicU64,
    #[cfg(test)]
    after_check: AfterCheck,
}

/// A test's hook run once right after [`Budget::admit`] decided, with the
/// lock let go: where a release racing the engine's check lands.
#[cfg(test)]
#[derive(Default)]
struct AfterCheck(std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>);

#[cfg(test)]
impl std::fmt::Debug for AfterCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AfterCheck")
    }
}

impl Budget {
    /// A budget of `limit` bytes, not shared per index.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            index_share: None,
            state: Mutex::new(State::default()),
            released: Notify::new(),
            refused_live: AtomicU64::new(0),
            refused_backfill: AtomicU64::new(0),
            #[cfg(test)]
            after_check: AfterCheck::default(),
        }
    }

    /// Share the limit over `indexes` indexes: with two or more, one index's
    /// live documents hold at most `share` of it (rule 3); with one, the
    /// share applies to nothing.
    /// `share` is taken as it is: the caller checked it is in (0, 1].
    #[must_use]
    pub fn with_index_share(mut self, indexes: usize, share: f64) -> Self {
        self.index_share = (indexes >= 2).then(|| {
            let bytes = (self.limit as f64 * share).floor();
            if bytes >= self.limit as f64 {
                self.limit
            } else {
                bytes as usize
            }
        });
        self
    }

    /// Rule 3's bound in bytes: what one index's live documents may hold;
    /// `None` when the budget isn't shared (one index).
    pub fn index_share(&self) -> Option<usize> {
        self.index_share
    }

    /// Run `hook` once, right after the next [`admit`](Self::admit) decides.
    #[cfg(test)]
    pub(crate) fn after_check(&self, hook: impl FnOnce() + Send + 'static) {
        *self
            .after_check
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Box::new(hook));
    }

    /// Count a batch of `work` the budget held back: once per wait, not per
    /// look.
    pub fn note_refused(&self, work: Work) {
        self.refusals(work).fetch_add(1, Ordering::Relaxed);
    }

    /// How many times a batch of `work` waited for the budget.
    pub fn refused(&self, work: Work) -> u64 {
        self.refusals(work).load(Ordering::Relaxed)
    }

    fn refusals(&self, work: Work) -> &AtomicU64 {
        match work {
            Work::Live => &self.refused_live,
            Work::Backfill => &self.refused_backfill,
        }
    }

    /// A budget that never refuses: what an engine has unless given one.
    pub fn unlimited() -> Self {
        Self::new(usize::MAX)
    }

    /// The limit in bytes.
    pub fn limit(&self) -> usize {
        self.limit
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What is held now, reservations included, read at one instant.
    pub fn held(&self) -> Held {
        let state = self.state();
        Held {
            total: state.total,
            backfill: state.backfill,
        }
    }

    /// What `index` holds now, live and backfill, reservations included.
    pub fn held_by(&self, index: &IndexName) -> usize {
        self.state().held_by(index)
    }

    /// What `index`'s live documents hold now, reservations included: what
    /// its share bounds.
    pub fn live_held_by(&self, index: &IndexName) -> usize {
        self.state().live_by(index)
    }

    /// The release generation now: [`released_since`](Self::released_since)
    /// returns once a release (or, while a waiter is in a build gate's line,
    /// an add) moves past it.
    pub fn generation(&self) -> u64 {
        self.state().generation
    }

    /// Which of `parts` (each index with its estimate, oldest waiting first)
    /// a build of `work` may take now, by the module's rules, and the
    /// generation the decision was made at. Reserves nothing.
    pub fn admit(&self, work: Work, parts: &[(IndexName, usize)]) -> Admission {
        let admission = {
            let state = self.state();
            self.decide(&state, work, parts, false)
        };
        #[cfg(test)]
        if let Some(hook) = self
            .after_check
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            hook();
        }
        admission
    }

    /// `behind`: the caller waits behind an older one the limit refused, so
    /// it takes no room by the limit (rules 1 and 2), only rule 5 or 4.
    fn decide(
        &self,
        state: &State,
        work: Work,
        parts: &[(IndexName, usize)],
        behind: bool,
    ) -> Admission {
        let generation = state.generation;
        let all = || parts.iter().map(|(index, _)| index.clone()).collect();
        if parts.is_empty() {
            return Admission {
                admitted: Vec::new(),
                refused: false,
                by_limit: false,
                generation,
            };
        }
        match work {
            Work::Live => {
                let mut running = state.total;
                let mut blocked = behind;
                let mut by_limit = false;
                let mut admitted = Vec::with_capacity(parts.len());
                for (index, estimate) in parts {
                    let live = state.live_by(index);
                    let fits = if live == 0 {
                        // Rule 5: an index holding no live bytes gets one
                        // live batch, past the limit and its share, never
                        // held back by an older index: one stuck index can't
                        // stop the others. (Live holding nothing is every
                        // index holding none: rule 4 for live.)
                        true
                    } else if self
                        .index_share
                        .is_some_and(|share| live.saturating_add(*estimate) > share)
                    {
                        // Rule 3: its live bytes within its share (its
                        // backfill counts toward rule 2, not here). A refusal
                        // here holds no other index back.
                        false
                    } else if !blocked && running.saturating_add(*estimate) <= self.limit {
                        // Rule 1.
                        true
                    } else {
                        // The oldest refused by the limit keeps younger
                        // indexes that hold live bytes from taking room by it.
                        blocked = true;
                        by_limit = true;
                        false
                    };
                    if fits {
                        running = running.saturating_add(*estimate);
                        admitted.push(index.clone());
                    }
                }
                let refused = admitted.len() < parts.len();
                Admission {
                    admitted,
                    refused,
                    by_limit,
                    generation,
                }
            }
            Work::Backfill => {
                let estimate: usize = parts.iter().map(|(_, bytes)| *bytes).sum();
                // Rule 4, then rules 2 and 1: backfill as on `main`, whichever
                // indexes it walks.
                let fits = state.backfill == 0
                    || (!behind
                        && state.backfill.saturating_add(estimate) <= self.limit / 2
                        && state.total.saturating_add(estimate) <= self.limit);
                Admission {
                    admitted: if fits { all() } else { Vec::new() },
                    refused: !fits,
                    by_limit: !fits,
                    generation,
                }
            }
        }
    }

    /// Reserve each of `parts` for a build of `work`, if all of them fit now
    /// ([`admit`](Self::admit) admitting every one), in one step: two
    /// reservers never pass the limit together. `Err` carries the generation
    /// of the refusal.
    pub fn reserve(
        self: &Arc<Self>,
        work: Work,
        parts: &[(IndexName, usize)],
    ) -> Result<Vec<Reservation>, u64> {
        self.reserve_behind(work, parts, false)
            .map_err(|refusal| refusal.generation)
    }

    /// [`reserve`](Self::reserve) for a waiter behind an older one: with
    /// `behind`, it takes no room by the limit (rules 1 and 2), so only rule
    /// 5 (live) or 4 (backfill) can grant it, and small batches can't starve
    /// the larger one ahead. The refusal says whether the limit refused it.
    pub fn reserve_behind(
        self: &Arc<Self>,
        work: Work,
        parts: &[(IndexName, usize)],
        behind: bool,
    ) -> Result<Vec<Reservation>, Refusal> {
        let state = self.state();
        self.reserve_locked(state, work, parts, behind)
    }

    /// [`reserve_behind`](Self::reserve_behind), only while the generation
    /// is still `generation`: `None` when it moved since, so a caller that
    /// chose at `generation` reserves nothing on a choice that change made
    /// out of date.
    pub(crate) fn reserve_at(
        self: &Arc<Self>,
        work: Work,
        parts: &[(IndexName, usize)],
        behind: bool,
        generation: u64,
    ) -> Option<Result<Vec<Reservation>, Refusal>> {
        let state = self.state();
        (state.generation == generation).then(|| self.reserve_locked(state, work, parts, behind))
    }

    fn reserve_locked(
        self: &Arc<Self>,
        mut state: MutexGuard<'_, State>,
        work: Work,
        parts: &[(IndexName, usize)],
        behind: bool,
    ) -> Result<Vec<Reservation>, Refusal> {
        let admission = self.decide(&state, work, parts, behind);
        if admission.refused {
            return Err(Refusal {
                generation: admission.generation,
                by_limit: admission.by_limit,
            });
        }
        let mut moved = false;
        let reserved = parts
            .iter()
            .map(|(index, bytes)| {
                moved |= state.add(work, Some(index), *bytes);
                Reservation {
                    budget: Arc::clone(self),
                    work,
                    index: Some(index.clone()),
                    bytes: *bytes,
                }
            })
            .collect();
        drop(state);
        if moved {
            self.released.notify_waiters();
        }
        Ok(reserved)
    }

    /// Reserve `estimate` bytes for a build of `work` counted toward no
    /// index, if the kind's rules allow: within the limit, backfill within
    /// half of it, and always one batch for a kind holding nothing.
    pub fn try_reserve(self: &Arc<Self>, work: Work, estimate: usize) -> Option<Reservation> {
        let mut state = self.state();
        let fits = match work {
            Work::Live => {
                state.total == state.backfill || state.total.saturating_add(estimate) <= self.limit
            }
            Work::Backfill => {
                state.backfill == 0
                    || (state.backfill.saturating_add(estimate) <= self.limit / 2
                        && state.total.saturating_add(estimate) <= self.limit)
            }
        };
        if !fits {
            return None;
        }
        let moved = state.add(work, None, estimate);
        drop(state);
        if moved {
            self.released.notify_waiters();
        }
        Some(Reservation {
            budget: Arc::clone(self),
            work,
            index: None,
            bytes: estimate,
        })
    }

    /// Returns once a release (or, while a waiter is in a build gate's line,
    /// an add) has moved the generation past `since` (at once if one already
    /// has). The wait is registered before the generation is
    /// read, so a release between the two still wakes it.
    pub async fn released_since(&self, since: u64) {
        loop {
            let notified = self.released.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.generation() != since {
                return;
            }
            notified.await;
        }
    }

    fn add(&self, work: Work, index: Option<&IndexName>, bytes: usize) {
        let moved = self.state().add(work, index, bytes);
        if moved {
            self.released.notify_waiters();
        }
    }

    /// A waiter joins a gate's line: until it [`unwatch`](Self::unwatch)es,
    /// every add moves the generation and wakes the waiters too, not only a
    /// release, so no refusal it or another waiter reads is out of date.
    pub(crate) fn watch(&self) {
        self.state().watched += 1;
    }

    /// A waiter leaves a gate's line (granted or dropped).
    pub(crate) fn unwatch(&self) {
        let mut state = self.state();
        state.watched = state.watched.saturating_sub(1);
    }

    /// Give `bytes` back and wake every waiter, once, with the lock let go.
    fn release(&self, work: Work, index: Option<&IndexName>, bytes: usize) {
        if bytes == 0 {
            return;
        }
        self.state().release(work, index, bytes);
        self.released.notify_waiters();
    }
}

impl Drop for Budget {
    /// Every reservation and charge holds the budget, so it is dropped only
    /// once all of them are: anything still counted then is an accounting
    /// mistake (a charge given back twice, or never), a standing guard in
    /// every debug build's tests.
    fn drop(&mut self) {
        if !std::thread::panicking() {
            debug_assert_eq!(
                self.held(),
                Held::default(),
                "the memory budget was dropped with bytes still counted"
            );
            debug_assert!(
                self.state().by_index.is_empty(),
                "the memory budget was dropped with an index still counted"
            );
        }
    }
}

/// Bytes reserved for one index's part of a build in flight. Dropped
/// unsettled, it gives them back.
#[derive(Debug)]
pub struct Reservation {
    budget: Arc<Budget>,
    work: Work,
    index: Option<IndexName>,
    bytes: usize,
}

impl Reservation {
    /// The kind of work it was reserved for.
    pub fn work(&self) -> Work {
        self.work
    }

    /// The index it counts toward; `None` for none.
    pub fn index(&self) -> Option<&IndexName> {
        self.index.as_ref()
    }

    /// The part landed with `bytes` of documents: hold that much (at least
    /// [`FLOOR`]) instead of the estimate, until the returned charge drops.
    pub fn settle(mut self, bytes: usize) -> Charge {
        let bytes = bytes.max(FLOOR);
        let reserved = std::mem::take(&mut self.bytes);
        let index = self.index.take();
        if bytes > reserved {
            self.budget.add(self.work, index.as_ref(), bytes - reserved);
        } else {
            self.budget
                .release(self.work, index.as_ref(), reserved - bytes);
        }
        Charge {
            budget: Arc::clone(&self.budget),
            work: self.work,
            index,
            bytes,
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget
            .release(self.work, self.index.as_ref(), self.bytes);
    }
}

/// Bytes a published batch's part holds until its last copy is dropped.
#[derive(Debug)]
pub struct Charge {
    budget: Arc<Budget>,
    work: Work,
    index: Option<IndexName>,
    bytes: usize,
}

impl Charge {
    /// The bytes it holds.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.budget
            .release(self.work, self.index.as_ref(), self.bytes);
    }
}

/// The heap a built document will hold once it is an envelope: its body's
/// tree, its rendered id, its index name (each envelope owns a copy) and the
/// envelope itself.
pub fn document_bytes(document: &Document) -> usize {
    let (id, body) = match document {
        Document::Upsert { id, body } => (id, Some(body)),
        Document::Delete { id } | Document::Failed { id, .. } => (id, None),
    };
    let key: usize = id
        .key
        .0
        .iter()
        .map(|(column, value)| column.as_ref().len() + value_bytes(value) + 8)
        .sum();
    size_of::<Envelope>() + id.index.as_ref().len() + key + body.map_or(0, value_bytes)
}

/// The nodes of a map of `len` entries: one leaf up to eleven, then leaves
/// about two-thirds full under internal nodes.
fn map_nodes(len: usize) -> usize {
    match len {
        0 => 0,
        1..=11 => MAP_LEAF,
        _ => {
            let leaves = len.div_ceil(8);
            leaves * MAP_LEAF + leaves.div_ceil(8) * MAP_INTERNAL
        }
    }
}

/// The heap `value` owns, not counting the value itself.
pub fn value_bytes(value: &GenericValue) -> usize {
    match value {
        GenericValue::String(text) => text.capacity(),
        GenericValue::Bytes(bytes) => bytes.capacity(),
        GenericValue::BigDecimal(decimal) => decimal.as_str().len(),
        GenericValue::Array(items) => {
            items.capacity() * size_of::<GenericValue>()
                + items.iter().map(value_bytes).sum::<usize>()
        }
        GenericValue::Map(entries) => {
            map_nodes(entries.len())
                + entries
                    .iter()
                    .map(|(key, value)| key.capacity() + value_bytes(value))
                    .sum::<usize>()
        }
        _ => 0,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    #[test]
    fn a_document_counts_its_index_name() {
        let delete = |index: &str| Document::Delete {
            id: source::document::DocumentId {
                index: kernel::IndexName::try_new(index).unwrap(),
                key: source::RowKey(Vec::new()),
            },
        };
        assert_eq!(
            document_bytes(&delete("a_much_longer_index_name")) - document_bytes(&delete("a")),
            "a_much_longer_index_name".len() - 1
        );
    }

    use super::*;

    const KIB: usize = 1024;

    fn budget(kib: usize) -> Arc<Budget> {
        Arc::new(Budget::new(kib * KIB))
    }

    #[test]
    fn a_reservation_holds_its_estimate_and_settles_to_the_real_size() {
        let budget = budget(64);
        let reservation = budget.try_reserve(Work::Live, 10 * KIB).unwrap();
        assert_eq!(budget.held().total, 10 * KIB);
        let charge = reservation.settle(12 * KIB);
        assert_eq!(charge.bytes(), 12 * KIB);
        assert_eq!(budget.held().total, 12 * KIB);
        drop(charge);
        assert_eq!(budget.held(), Held::default());
    }

    #[test]
    fn a_reservation_dropped_unsettled_gives_its_estimate_back() {
        let budget = budget(64);
        let reservation = budget.try_reserve(Work::Backfill, 8 * KIB).unwrap();
        assert_eq!(budget.held().backfill, 8 * KIB);
        drop(reservation);
        assert_eq!(budget.held(), Held::default());
    }

    #[test]
    fn a_batch_counts_at_least_the_floor() {
        let budget = budget(64);
        let charge = budget.try_reserve(Work::Live, 0).unwrap().settle(0);
        assert_eq!(charge.bytes(), FLOOR);
        assert_eq!(budget.held().total, FLOOR);
    }

    #[test]
    fn nothing_past_the_limit_is_reserved() {
        let budget = budget(64);
        let _first = budget.try_reserve(Work::Live, 40 * KIB).unwrap();
        assert!(budget.try_reserve(Work::Live, 30 * KIB).is_none());
        assert!(budget.try_reserve(Work::Live, 24 * KIB).is_some());
    }

    #[test]
    fn backfill_holds_at_most_half_and_live_keeps_the_rest() {
        let budget = budget(64);
        let _backfill = budget.try_reserve(Work::Backfill, 24 * KIB).unwrap();
        assert!(
            budget.try_reserve(Work::Backfill, 16 * KIB).is_none(),
            "backfill past half"
        );
        assert!(budget.try_reserve(Work::Backfill, 8 * KIB).is_some());
        assert!(budget.try_reserve(Work::Live, 32 * KIB).is_some());
    }

    #[test]
    fn a_kind_holding_nothing_is_granted_one_batch_whatever_its_size() {
        let budget = budget(64);
        let live = budget.try_reserve(Work::Live, 60 * KIB).unwrap();
        let backfill = budget.try_reserve(Work::Backfill, 500 * KIB);
        assert!(
            backfill.is_some(),
            "backfill holds nothing, so one batch is granted however large"
        );
        assert!(budget.try_reserve(Work::Backfill, KIB).is_none());
        drop(live);
        assert!(
            budget.try_reserve(Work::Live, 900 * KIB).is_some(),
            "live holds nothing again"
        );
    }

    #[test]
    fn a_settle_past_the_estimate_is_held_and_refuses_more() {
        let budget = budget(64);
        let charge = budget
            .try_reserve(Work::Live, 8 * KIB)
            .unwrap()
            .settle(70 * KIB);
        assert_eq!(budget.held().total, 70 * KIB);
        let _other = budget.try_reserve(Work::Backfill, KIB);
        assert!(budget.try_reserve(Work::Live, KIB).is_none());
        drop(charge);
        assert!(budget.try_reserve(Work::Live, KIB).is_some());
    }

    fn index(name: &str) -> IndexName {
        IndexName::try_new(name).unwrap()
    }

    /// `bytes` of live documents of `name`, settled.
    fn held(budget: &Arc<Budget>, name: &str, bytes: usize) -> Vec<Charge> {
        budget
            .reserve(Work::Live, &[(index(name), bytes)])
            .unwrap()
            .into_iter()
            .map(|part| part.settle(bytes))
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn a_release_wakes_an_engine_that_waits_for_one() {
        let budget = budget(64);
        let charge = held(&budget, "users", 60 * KIB);
        let refusal = budget.admit(Work::Live, &[(index("users"), 8 * KIB)]);
        assert!(!refusal.grants());
        let waiter = {
            let budget = Arc::clone(&budget);
            tokio::spawn(async move { budget.released_since(refusal.generation).await })
        };
        tokio::task::yield_now().await;
        drop(charge);
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("the release woke the waiter")
            .unwrap();
    }

    /// The generation is read with the refusal, under the lock: a release
    /// on another thread between the refusal and the wait still wakes the
    /// waiter, where a flag set after the check could lose it.
    #[tokio::test(start_paused = true)]
    async fn a_release_between_the_refusal_and_the_wait_is_not_lost() {
        let budget = budget(64);
        let charge = held(&budget, "users", 60 * KIB);
        budget.after_check(move || drop(charge));
        let refusal = budget.admit(Work::Live, &[(index("users"), 8 * KIB)]);
        assert!(!refusal.grants(), "decided before the release");
        tokio::time::timeout(
            Duration::from_secs(60),
            budget.released_since(refusal.generation),
        )
        .await
        .expect("the waiter is woken by the release that raced its refusal");
        assert!(
            budget
                .admit(Work::Live, &[(index("users"), 8 * KIB)])
                .grants(),
            "room is back"
        );
    }

    /// Every waiter is woken by one release, and each wakes once: three
    /// waiters, one release.
    #[tokio::test(start_paused = true)]
    async fn a_release_wakes_every_waiting_task() {
        let budget = budget(64);
        let charge = budget
            .try_reserve(Work::Live, 64 * KIB)
            .unwrap()
            .settle(64 * KIB);
        let since = budget.generation();
        let waiters: Vec<_> = (0..3)
            .map(|_| {
                let budget = Arc::clone(&budget);
                tokio::spawn(async move { budget.released_since(since).await })
            })
            .collect();
        tokio::task::yield_now().await;
        drop(charge);
        for waiter in waiters {
            tokio::time::timeout(Duration::from_secs(1), waiter)
                .await
                .expect("every waiter is woken")
                .unwrap();
        }
    }

    /// With two or more indexes, one index holds at most its share; the
    /// others still fit beside it.
    #[test]
    fn an_index_at_its_share_is_left_out_and_the_others_fit() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
        assert_eq!(budget.index_share(), Some(32 * KIB));
        let _users = budget
            .reserve(Work::Live, &[(index("users"), 30 * KIB)])
            .unwrap();
        let admission = budget.admit(
            Work::Live,
            &[(index("users"), 4 * KIB), (index("orders"), 4 * KIB)],
        );
        assert_eq!(admission.admitted, vec![index("orders")]);
        assert!(admission.refused, "users is left waiting");
        assert!(
            budget
                .reserve(Work::Live, &[(index("users"), 4 * KIB)])
                .is_err(),
            "past its share"
        );
        assert!(
            budget
                .reserve(Work::Live, &[(index("users"), 2 * KIB)])
                .is_ok()
        );
    }

    /// With one index there is no share: it may hold the whole limit, as
    /// before the budget was shared.
    #[test]
    fn one_index_holds_the_whole_budget() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(1, 0.5));
        assert_eq!(budget.index_share(), None);
        let _first = budget
            .reserve(Work::Live, &[(index("users"), 40 * KIB)])
            .unwrap();
        let _second = budget
            .reserve(Work::Live, &[(index("users"), 20 * KIB)])
            .unwrap();
        assert!(
            budget
                .reserve(Work::Live, &[(index("users"), 8 * KIB)])
                .is_err()
        );
    }

    /// An index holding no live bytes gets one live batch whatever the limit
    /// and its share say (rule 5): no index is shut out by another's bytes.
    #[test]
    fn an_index_holding_no_live_bytes_gets_one_live_batch_past_the_limit() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
        let _users = held(&budget, "users", 8 * KIB);
        let _items = budget
            .reserve(Work::Live, &[(index("items"), 40 * KIB)])
            .unwrap();
        assert!(
            budget
                .reserve(Work::Live, &[(index("items"), KIB)])
                .is_err(),
            "once it holds live bytes past its share, the share applies"
        );
        let _orders = budget
            .reserve(Work::Live, &[(index("orders"), 20 * KIB)])
            .unwrap();
        assert_eq!(budget.held().total, 68 * KIB, "past the limit by one batch");
    }

    /// Review round 3, case A: `a`'s reindex is parked, its backfill at half
    /// and its live documents at its share, and neither comes back; healthy
    /// `b`, holding no live bytes, still gets its batch.
    #[test]
    fn a_parked_index_at_its_share_beside_its_backfill_leaves_a_healthy_index_its_batch() {
        let budget = Arc::new(Budget::new(128 * KIB).with_index_share(2, 0.5));
        let _backfill = budget
            .reserve(Work::Backfill, &[(index("a"), 64 * KIB)])
            .unwrap();
        let _a = held(&budget, "a", 64 * KIB);
        let admission = budget.admit(Work::Live, &[(index("b"), 16 * KIB)]);
        assert_eq!(admission.admitted, vec![index("b")]);
    }

    /// Review round 3, case B: `a` parked at its share; `x`, oldest waiting
    /// and holding no live bytes, has a batch larger than the room left;
    /// younger `b` is small. Both get their batch: an index rule 5 admits is
    /// never held back by an older one the limit refused.
    #[test]
    fn a_large_batch_and_a_small_one_both_build_beside_a_parked_index() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
        let _a = held(&budget, "a", 32 * KIB);
        let admission = budget.admit(Work::Live, &[(index("x"), 48 * KIB), (index("b"), 4 * KIB)]);
        assert_eq!(admission.admitted, vec![index("x"), index("b")]);
    }

    /// Live holding nothing takes every index waiting, at its share or not:
    /// live always has room once it holds no live bytes.
    #[test]
    fn live_holding_nothing_takes_every_index() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(2, 0.5));
        let _backfill = budget
            .reserve(Work::Backfill, &[(index("users"), 32 * KIB)])
            .unwrap();
        let admission = budget.admit(Work::Live, &[(index("users"), 16 * KIB)]);
        assert_eq!(admission.admitted, vec![index("users")]);
    }

    /// The oldest index refused by the limit keeps every younger one from
    /// taking room by the limit, so small batches can't starve a large one;
    /// an index refused by its own share doesn't.
    #[test]
    fn the_oldest_index_refused_by_the_limit_holds_younger_ones_back() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
        let _held = budget
            .reserve(
                Work::Live,
                &[
                    (index("users"), 30 * KIB),
                    (index("orders"), 16 * KIB),
                    (index("items"), 8 * KIB),
                ],
            )
            .unwrap();
        let admission = budget.admit(
            Work::Live,
            &[
                (index("users"), 8 * KIB),
                (index("orders"), 12 * KIB),
                (index("items"), 4 * KIB),
            ],
        );
        assert!(
            admission.admitted.is_empty(),
            "users is past its share and lets the others try; orders is refused by the limit \
             and holds items back: {:?}",
            admission.admitted
        );
        let admission = budget.admit(
            Work::Live,
            &[(index("users"), 8 * KIB), (index("items"), 4 * KIB)],
        );
        assert_eq!(admission.admitted, vec![index("items")]);
    }

    /// An index whose backfill holds half the budget keeps, for its live
    /// changes, all the room `main` gave live beside a backfill (everything
    /// within the limit), up to its share: a reindex of one index on a live
    /// deployment builds its steady stream batch after batch.
    #[test]
    fn an_index_backfilling_keeps_the_live_room_main_gave_it() {
        let budget = Arc::new(Budget::new(128 * KIB).with_index_share(2, 0.5));
        let _backfill = budget
            .reserve(Work::Backfill, &[(index("orders"), 64 * KIB)])
            .unwrap();
        let _users = held(&budget, "users", 16 * KIB);
        let _first = held(&budget, "orders", 16 * KIB);
        let _second = held(&budget, "orders", 16 * KIB);
        let _third = held(&budget, "orders", 16 * KIB);
        assert_eq!(budget.held().total, 128 * KIB);
        assert!(
            budget
                .reserve(Work::Live, &[(index("orders"), KIB)])
                .is_err(),
            "the limit, as on main"
        );
    }

    /// The reviewer's sequence over four indexes: past the limit only by
    /// one live batch per index holding none (rule 5) and one backfill batch
    /// (rule 4).
    #[test]
    fn four_indexes_pass_the_limit_by_at_most_a_live_batch_each_and_a_backfill_batch() {
        let budget = Arc::new(Budget::new(128 * KIB).with_index_share(4, 0.5));
        let mut held = Vec::new();
        for (work, name, kib) in [
            (Work::Backfill, "a", 64),
            (Work::Live, "b", 64),
            (Work::Live, "a", 32),
            (Work::Backfill, "c", 32),
            (Work::Live, "c", 32),
            (Work::Backfill, "d", 32),
            (Work::Live, "d", 32),
        ] {
            if let Ok(parts) = budget.reserve(work, &[(index(name), kib * KIB)]) {
                held.extend(parts);
            }
        }
        assert!(
            budget.held().total <= 128 * KIB + 4 * 64 * KIB + 64 * KIB,
            "{:?}",
            budget.held()
        );
        assert_eq!(
            budget.held(),
            Held {
                total: 224 * KIB,
                backfill: 64 * KIB
            },
            "each index's first live batch, and no backfill past half"
        );
    }

    /// Random reservations and releases over four indexes, both kinds, each
    /// call at most `B`: after every step the whole is within the limit plus
    /// one live batch per index and one backfill batch, backfill within half
    /// plus one batch, and each index's live bytes within its share, or one
    /// batch when it held none.
    #[test]
    fn random_reservations_stay_within_the_stated_bounds() {
        const B: usize = 24 * KIB;
        let limit = 128 * KIB;
        let names: Vec<IndexName> = ["a", "b", "c", "d"].into_iter().map(index).collect();
        for seed in 0..200_u64 {
            let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let mut next = |below: u64| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (state >> 33) % below
            };
            let budget = Arc::new(Budget::new(limit).with_index_share(4, 0.5));
            let share = budget.index_share().unwrap();
            let mut held: Vec<Reservation> = Vec::new();
            for _ in 0..300 {
                if next(3) == 0 && !held.is_empty() {
                    let at = next(held.len() as u64) as usize;
                    drop(held.swap_remove(at));
                    continue;
                }
                let work = if next(2) == 0 {
                    Work::Live
                } else {
                    Work::Backfill
                };
                let count = 1 + next(3) as usize;
                let mut parts: Vec<(IndexName, usize)> = Vec::new();
                let mut left = B;
                for _ in 0..count {
                    let name = names[next(4) as usize].clone();
                    if parts.iter().any(|(index, _)| *index == name) {
                        continue;
                    }
                    let bytes = (1 + next(left as u64 / KIB as u64) as usize) * KIB;
                    left -= bytes.min(left);
                    parts.push((name, bytes));
                    if left < KIB {
                        break;
                    }
                }
                let admission = budget.admit(work, &parts);
                let taken: Vec<(IndexName, usize)> = parts
                    .into_iter()
                    .filter(|(index, _)| admission.admitted.contains(index))
                    .collect();
                if let Ok(parts) = budget.reserve(work, &taken) {
                    held.extend(parts);
                }
                let now = budget.held();
                assert!(now.total <= limit + 4 * B + B, "seed {seed}: {now:?}");
                assert!(now.backfill <= limit / 2 + B, "seed {seed}: {now:?}");
                for name in &names {
                    let live = budget.state().live_by(name);
                    assert!(
                        live <= share.max(B),
                        "seed {seed}: {name} holds {live} live bytes"
                    );
                }
            }
        }
    }

    /// An older index holding live bytes refused by the limit holds back
    /// younger indexes holding live bytes, never one holding none: rule 5
    /// is never blocked.
    #[test]
    fn an_index_refused_by_the_limit_never_holds_back_a_rule_5_batch() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
        let _s1 = held(&budget, "s1", 32 * KIB);
        let _s2 = held(&budget, "s2", 16 * KIB);
        let _backfill = budget
            .reserve(Work::Backfill, &[(index("b"), 16 * KIB)])
            .unwrap();
        let admission = budget.admit(Work::Live, &[(index("s2"), 8 * KIB), (index("y"), 4 * KIB)]);
        assert_eq!(admission.admitted, vec![index("y")]);
        assert!(admission.refused, "s2 waits for the limit");
    }

    /// A backfill batch spanning two indexes fits whole or not at all.
    #[test]
    fn a_backfill_batch_fits_whole_or_not_at_all() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(2, 0.5));
        let _live = budget
            .reserve(Work::Live, &[(index("users"), 4 * KIB)])
            .unwrap();
        let _backfill = budget
            .reserve(Work::Backfill, &[(index("orders"), 30 * KIB)])
            .unwrap();
        let admission = budget.admit(
            Work::Backfill,
            &[(index("users"), KIB), (index("orders"), 4 * KIB)],
        );
        assert!(admission.admitted.is_empty() && admission.refused);
    }

    /// Reservers on many threads never pass the limit together: every
    /// reservation is checked and counted in one step.
    #[test]
    fn concurrent_reservations_never_pass_the_limit() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(4, 1.0));
        let names: Vec<IndexName> = ["a", "b", "c", "d"].into_iter().map(index).collect();
        // Every index holds something, so only the limit decides.
        let _seeds: Vec<Reservation> = names
            .iter()
            .flat_map(|name| budget.reserve(Work::Live, &[(name.clone(), KIB)]).unwrap())
            .collect();
        let most = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for name in &names {
                let (budget, most) = (&budget, &most);
                scope.spawn(move || {
                    for _ in 0..2_000 {
                        if let Ok(parts) = budget.reserve(Work::Live, &[(name.clone(), 3 * KIB)]) {
                            most.fetch_max(budget.held().total, Ordering::SeqCst);
                            drop(parts);
                        }
                    }
                });
            }
        });
        assert!(most.into_inner() <= 64 * KIB);
        drop(_seeds);
        assert_eq!(budget.held(), Held::default());
    }

    /// Every path gives each index's bytes back exactly once: a settle
    /// smaller or larger, a reservation dropped unsettled, charges dropped on
    /// other threads.
    #[test]
    fn every_index_gets_every_byte_back_once() {
        let budget = Arc::new(Budget::new(usize::MAX / 4).with_index_share(2, 0.5));
        let parts = budget
            .reserve(
                Work::Live,
                &[(index("users"), 8 * KIB), (index("orders"), 8 * KIB)],
            )
            .unwrap();
        let mut parts = parts.into_iter();
        let users = parts.next().unwrap().settle(3 * KIB);
        let orders = parts.next().unwrap();
        assert_eq!(budget.held_by(&index("users")), 3 * KIB);
        assert_eq!(budget.held_by(&index("orders")), 8 * KIB);
        drop(orders);
        assert_eq!(budget.held_by(&index("orders")), 0);
        let backfill = budget
            .reserve(Work::Backfill, &[(index("orders"), KIB)])
            .unwrap()
            .into_iter()
            .map(|part| part.settle(20 * KIB))
            .collect::<Vec<_>>();
        assert_eq!(budget.held_by(&index("orders")), 20 * KIB);
        std::thread::scope(|scope| {
            scope.spawn(move || drop(users));
            scope.spawn(move || drop(backfill));
        });
        assert_eq!(budget.held(), Held::default());
        assert_eq!(budget.held_by(&index("users")), 0);
    }

    #[test]
    fn charges_dropped_on_other_threads_all_come_back() {
        let budget = Arc::new(Budget::new(usize::MAX / 4));
        let charges: Vec<Charge> = (0..64)
            .map(|n| {
                let work = if n % 2 == 0 {
                    Work::Live
                } else {
                    Work::Backfill
                };
                budget.try_reserve(work, KIB).unwrap().settle(KIB + n)
            })
            .collect();
        std::thread::scope(|scope| {
            let mut charges = charges;
            while !charges.is_empty() {
                let take: Vec<Charge> = charges.drain(..charges.len().min(8)).collect();
                scope.spawn(move || drop(take));
            }
        });
        assert_eq!(budget.held(), Held::default());
    }

    #[test]
    fn the_walk_counts_every_owned_allocation() {
        let mut map = BTreeMap::new();
        map.insert("name".to_owned(), GenericValue::String("x".repeat(100)));
        map.insert(
            "tags".to_owned(),
            GenericValue::Array(vec![GenericValue::BigInt(1), GenericValue::BigInt(2)]),
        );
        let bytes = value_bytes(&GenericValue::Map(map));
        assert!(
            bytes >= 100 + "name".len() + "tags".len() + 2 * std::mem::size_of::<GenericValue>()
        );
        assert_eq!(value_bytes(&GenericValue::BigInt(5)), 0);
    }
}
