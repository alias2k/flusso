//! The memory budget: how many bytes of built documents the pipeline holds
//! before it builds more (ADR 0012 stage 5).
//!
//! **What is counted.** The heap a built batch's documents own, measured by a
//! walk over each document ([`document_bytes`]) on the build's own task, plus a
//! floor of [`FLOOR`] per batch so an empty batch still counts. A batch is
//! counted once, however many lanes carry it.
//!
//! **When.** The ingest engine *reserves* an estimate when it grants a build
//! ([`Budget::try_reserve`]), and *settles* it to the walked size when the
//! build lands ([`Reservation::settle`]). The settled [`Charge`] rides inside
//! the published batch; when its last copy is dropped (every lane has
//! acknowledged it and the channel's ledger let it go), its `Drop` gives the
//! bytes back. A reservation dropped unsettled (a failed build, a stop) gives
//! its estimate back the same way.
//!
//! **The rules.**
//!
//! - Everything held stays within the limit.
//! - Backfill holds at most half of it, so live builds always have room.
//! - A kind (live or backfill) holding nothing is always granted one batch,
//!   so a batch larger than the budget still builds, alone.
//!
//! A settled batch may come out larger than its estimate, so held bytes stay
//! within the limit plus one batch per build in flight.
//!
//! ```
//! use std::sync::Arc;
//! use engine::{Budget, Work};
//!
//! let budget = Arc::new(Budget::new(64 * 1024));
//! let reservation = budget.try_reserve(Work::Live, 48 * 1024).unwrap();
//! assert!(budget.try_reserve(Work::Live, 48 * 1024).is_none());
//! let charge = reservation.settle(40 * 1024);
//! assert_eq!(budget.held().total, 40 * 1024);
//! drop(charge);
//! assert_eq!(budget.held().total, 0);
//! ```

#![allow(dead_code)] // stub until the accountant lands

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use kernel::{Envelope, GenericValue};
use source::document::Document;
use tokio::sync::Notify;

use crate::observer::Work;

/// The least a batch counts, so empty batches still bound the item count.
pub const FLOOR: usize = 1024;

/// Bytes of built documents held, in all and by backfill.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub total: usize,
    pub backfill: usize,
}

/// The pipeline's budget for built documents not yet acknowledged by every
/// lane. Shared by the ingest engine (which reserves) and whatever drops the
/// last copy of a batch (which releases).
#[derive(Debug)]
pub struct Budget {
    limit: usize,
    total: AtomicUsize,
    backfill: AtomicUsize,
    waiting: AtomicBool,
    released: Notify,
}

impl Budget {
    /// A budget of `limit` bytes.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            total: AtomicUsize::new(0),
            backfill: AtomicUsize::new(0),
            waiting: AtomicBool::new(false),
            released: Notify::new(),
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

    /// What is held now, reservations included.
    pub fn held(&self) -> Held {
        Held::default()
    }

    /// Reserve `estimate` bytes for a build of `work`, if the rules allow.
    pub fn try_reserve(self: &Arc<Self>, work: Work, estimate: usize) -> Option<Reservation> {
        let _ = (work, estimate);
        None
    }

    /// Say the engine waits for a release: the next release wakes
    /// [`released`](Self::released).
    pub fn want_release(&self) {
        self.waiting.store(true, Ordering::Release);
    }

    /// Resolves after a release that followed [`want_release`](Self::want_release).
    pub async fn released(&self) {
        self.released.notified().await;
    }
}

/// Bytes reserved for a build in flight. Dropped unsettled, it gives them
/// back.
#[derive(Debug)]
pub struct Reservation {
    budget: Arc<Budget>,
    work: Work,
    bytes: usize,
}

impl Reservation {
    /// The build landed with `bytes` of documents: hold that much (at least
    /// [`FLOOR`]) instead of the estimate, until the returned charge drops.
    pub fn settle(self, bytes: usize) -> Charge {
        let _ = bytes;
        Charge {
            budget: Arc::clone(&self.budget),
            work: self.work,
            bytes: 0,
        }
    }
}

/// Bytes a published batch holds until its last copy is dropped.
#[derive(Debug)]
pub struct Charge {
    budget: Arc<Budget>,
    work: Work,
    bytes: usize,
}

impl Charge {
    /// The bytes it holds.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

/// The heap a built document will hold once it is an envelope: its body's
/// tree, its rendered id and the envelope itself.
pub fn document_bytes(document: &Document) -> usize {
    let _ = document;
    0
}

/// The heap `value` owns, not counting the value itself.
pub fn value_bytes(value: &GenericValue) -> usize {
    let _ = value;
    0
}

#[allow(dead_code)]
fn sizes() -> (usize, usize) {
    (
        std::mem::size_of::<Envelope>(),
        std::mem::size_of::<BTreeMap<String, GenericValue>>(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

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

    #[tokio::test(start_paused = true)]
    async fn a_release_wakes_an_engine_that_waits_for_one() {
        let budget = budget(64);
        let charge = budget
            .try_reserve(Work::Live, 60 * KIB)
            .unwrap()
            .settle(60 * KIB);
        let _other = budget.try_reserve(Work::Backfill, 2 * KIB).unwrap();
        assert!(budget.try_reserve(Work::Live, 8 * KIB).is_none());
        budget.want_release();
        let waiter = {
            let budget = Arc::clone(&budget);
            tokio::spawn(async move { budget.released().await })
        };
        tokio::task::yield_now().await;
        drop(charge);
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("the release woke the waiter")
            .unwrap();
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
