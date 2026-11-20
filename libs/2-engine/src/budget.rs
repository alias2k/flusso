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
//! **The rules.** A build is granted when every rule allows it, or rule 4 or 5
//! does.
//!
//! | Rule | Bound |
//! | --- | --- |
//! | 1 | everything held ≤ the limit |
//! | 2 | one index ≤ its share of the limit, with two or more indexes ([`Budget::with_index_share`]) |
//! | 3 | backfill over every index ≤ half the limit, so live always has room |
//! | 4 | an index holding nothing gets one batch, whatever 1–3 say; for live, an index holding no live bytes does, so live keeps room beside the index's own backfill |
//! | 5 | a kind (live or backfill) holding nothing gets one batch: live then takes every index waiting |
//!
//! With one index, rules 4 and 5 are the same and rule 2 applies to nothing:
//! the budget answers exactly as before it was shared. A live build takes the
//! indexes that fit and leaves the rest waiting ([`Budget::admit`]); a
//! backfill batch fits whole or not at all. Indexes are offered oldest waiting
//! first, and the first one refused by the limit (rule 1) stops every younger
//! one from taking room by the limit, so a stream of small batches never
//! starves a large one; an index refused by its own share doesn't hold the
//! others back. A settled batch may come out larger than its estimate, so held
//! bytes stay within the limit plus one batch per build in flight and one per
//! index (rule 4).
//!
//! **Many reservers.** Every count lives under one lock, so a check and its
//! reservation are one step and two reservers never pass the limit together.
//! Every release bumps a generation and wakes every waiter
//! ([`Budget::released_since`]), outside the lock: a waiter that read the
//! generation with its refusal is woken by any release after it.
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
    /// The release generation the decision was made at: a waiter passes it
    /// to [`Budget::released_since`].
    pub generation: u64,
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
    /// Bumped by every release.
    generation: u64,
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

    fn add(&mut self, work: Work, index: Option<&IndexName>, bytes: usize) {
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
    /// Rule 2's bound in bytes; `None` with fewer than two indexes.
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

    /// Share the limit over `indexes` indexes: with two or more, one index
    /// holds at most `share` of it (rule 2); with one, it holds all of it.
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

    /// Rule 2's bound in bytes: what one index may hold; `None` when the
    /// budget isn't shared (one index).
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

    /// The release generation now: [`released_since`](Self::released_since)
    /// returns once a release moves past it.
    pub fn generation(&self) -> u64 {
        self.state().generation
    }

    /// Which of `parts` (each index with its estimate, oldest waiting first)
    /// a build of `work` may take now, by the module's rules, and the
    /// generation the decision was made at. Reserves nothing.
    pub fn admit(&self, work: Work, parts: &[(IndexName, usize)]) -> Admission {
        let admission = {
            let state = self.state();
            self.decide(&state, work, parts)
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

    fn decide(&self, state: &State, work: Work, parts: &[(IndexName, usize)]) -> Admission {
        let generation = state.generation;
        let all = || parts.iter().map(|(index, _)| index.clone()).collect();
        if parts.is_empty() {
            return Admission {
                admitted: Vec::new(),
                refused: false,
                generation,
            };
        }
        let within_share = |index: &IndexName, estimate: usize| -> bool {
            self.index_share
                .is_none_or(|share| state.held_by(index).saturating_add(estimate) <= share)
        };
        match work {
            Work::Live => {
                // Rule 5: live holding nothing takes every index waiting.
                if state.total - state.backfill.min(state.total) == 0 {
                    return Admission {
                        admitted: all(),
                        refused: false,
                        generation,
                    };
                }
                let mut running = state.total;
                let mut blocked = false;
                let mut admitted = Vec::with_capacity(parts.len());
                for (index, estimate) in parts {
                    // Rule 4 for live: an index holding no live bytes gets
                    // one live batch, beside its own backfill too.
                    let fits = if state.live_by(index) == 0 {
                        true
                    } else if !within_share(index, *estimate) {
                        false
                    } else if !blocked && running.saturating_add(*estimate) <= self.limit {
                        true
                    } else {
                        blocked = true;
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
                    generation,
                }
            }
            Work::Backfill => {
                let estimate: usize = parts.iter().map(|(_, bytes)| *bytes).sum();
                let fits = state.backfill == 0
                    || parts.iter().all(|(index, _)| state.held_by(index) == 0)
                    || (state.backfill.saturating_add(estimate) <= self.limit / 2
                        && state.total.saturating_add(estimate) <= self.limit
                        && parts
                            .iter()
                            .all(|(index, bytes)| within_share(index, *bytes)));
                Admission {
                    admitted: if fits { all() } else { Vec::new() },
                    refused: !fits,
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
        let mut state = self.state();
        let admission = self.decide(&state, work, parts);
        if admission.refused {
            return Err(admission.generation);
        }
        Ok(parts
            .iter()
            .map(|(index, bytes)| {
                state.add(work, Some(index), *bytes);
                Reservation {
                    budget: Arc::clone(self),
                    work,
                    index: Some(index.clone()),
                    bytes: *bytes,
                }
            })
            .collect())
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
        state.add(work, None, estimate);
        Some(Reservation {
            budget: Arc::clone(self),
            work,
            index: None,
            bytes: estimate,
        })
    }

    /// Returns once a release has moved the generation past `since` (at once
    /// if one already has). The wait is registered before the generation is
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
        self.state().add(work, index, bytes);
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

    /// An index holding nothing gets one batch, past the limit and past
    /// backfill's half (rule 4): the others holding the whole budget never
    /// shut it out.
    #[test]
    fn an_index_holding_nothing_gets_one_batch() {
        let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
        let _users = budget
            .reserve(Work::Live, &[(index("users"), 32 * KIB)])
            .unwrap();
        let _orders = budget
            .reserve(Work::Backfill, &[(index("orders"), 32 * KIB)])
            .unwrap();
        assert_eq!(
            budget
                .admit(Work::Live, &[(index("items"), 100 * KIB)])
                .admitted,
            vec![index("items")]
        );
        let _items = budget
            .reserve(Work::Backfill, &[(index("items"), 100 * KIB)])
            .unwrap();
        assert!(
            budget
                .reserve(Work::Backfill, &[(index("items"), KIB)])
                .is_err(),
            "once it holds something, the rules apply"
        );
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

    /// An index whose backfill holds its share still gets one live batch
    /// while live holds another index's bytes: per index, live keeps room
    /// beside that index's backfill (a reindex of one index on a live
    /// deployment).
    #[test]
    fn an_index_holding_only_backfill_bytes_gets_one_live_batch() {
        let budget = Arc::new(Budget::new(128 * KIB).with_index_share(2, 0.5));
        let _backfill = budget
            .reserve(Work::Backfill, &[(index("orders"), 64 * KIB)])
            .unwrap();
        let _users = held(&budget, "users", 16 * KIB);
        let admission = budget.admit(Work::Live, &[(index("orders"), 16 * KIB)]);
        assert_eq!(admission.admitted, vec![index("orders")]);
        let _live = budget
            .reserve(Work::Live, &[(index("orders"), 16 * KIB)])
            .unwrap();
        assert!(
            budget
                .reserve(Work::Live, &[(index("orders"), 16 * KIB)])
                .is_err(),
            "once it holds live bytes, its share applies again"
        );
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
