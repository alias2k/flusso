//! The build gate: `builds` permits shared by every build of the ingest side
//! (ADR 0012 §5), live first.
//!
//! **The rule.** A free permit goes to a live waiter before a backfill one,
//! except every fourth time both kinds wait (*contested*), when it goes to
//! the oldest backfill waiter: a steady trickle can't starve a backfill, and
//! `builds = 1` still runs one. A grant only one kind could take doesn't
//! move the count. The count is global, over every index.
//!
//! **Fairness between indexes.** Each kind's waiters are served first in,
//! first out. An index task asks for a permit when it has a batch ready and
//! gives it back when the build lands; asking again puts it behind every
//! waiter that asked meanwhile, so an index with a deep backlog can't keep
//! every permit while another index waits.
//!
//! **Two ways to ask.** [`Gate::choose`] decides at once between the kinds a
//! caller has ready (one task building every index), [`Gate::acquire`] waits
//! in line (a task per index). Both draw on the one count of free permits and
//! the one contested counter. A [`Permit`] gives itself back when dropped,
//! to the next waiter by the rule, so a build that fails or is aborted still
//! frees its permit.
//!
//! ```text
//! free 0   live waiters [users, orders]   backfill waiters [products]
//! release  contested 1 → users;  2 → orders (behind it: users again);  3 → users;  4 → products
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::oneshot;

use crate::observer::Work;

/// How many of every kind of contested grant go to backfill.
const BACKFILL_EVERY: u64 = 4;

/// The permits of every build of the ingest side.
#[derive(Debug)]
pub(crate) struct Gate {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    free: usize,
    live: VecDeque<oneshot::Sender<Permit>>,
    backfill: VecDeque<oneshot::Sender<Permit>>,
    contested: u64,
}

/// A build's permit; dropping it gives it back.
#[derive(Debug)]
pub(crate) struct Permit {
    gate: Option<Arc<Gate>>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(gate) = self.gate.take() {
            gate.release();
        }
    }
}

impl Gate {
    /// A gate of `permits` (at least one).
    pub(crate) fn new(permits: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                free: permits.max(1),
                live: VecDeque::new(),
                backfill: VecDeque::new(),
                contested: 0,
            }),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A permit now for one of the kinds the caller has ready, by the rule,
    /// or `None` when none is free or someone waits in line ahead of it.
    pub(crate) fn choose(self: &Arc<Self>, live: bool, backfill: bool) -> Option<(Work, Permit)> {
        let mut state = self.state();
        state.prune();
        let live = live && state.live.is_empty();
        let backfill = backfill && state.backfill.is_empty();
        if state.free == 0 {
            return None;
        }
        let work = match (live, backfill) {
            (true, true) => {
                state.contested += 1;
                if state.contested.is_multiple_of(BACKFILL_EVERY) {
                    Work::Backfill
                } else {
                    Work::Live
                }
            }
            (true, false) => Work::Live,
            (false, true) => Work::Backfill,
            (false, false) => return None,
        };
        state.free -= 1;
        Some((work, self.permit()))
    }

    /// Wait in line for a permit for `work`.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the index tasks wait in line from ADR 0012 stage 8a step 4"
        )
    )]
    pub(crate) async fn acquire(self: &Arc<Self>, work: Work) -> Permit {
        let receiver = {
            let mut state = self.state();
            state.prune();
            let ahead = match work {
                Work::Live => !state.live.is_empty(),
                Work::Backfill => !state.backfill.is_empty(),
            };
            // A backfill asking while live waits takes its turn in line, so
            // the contested count decides; live asking first is served now.
            let contested = matches!(work, Work::Backfill) && !state.live.is_empty();
            if state.free > 0 && !ahead && !contested {
                state.free -= 1;
                drop(state);
                return self.permit();
            }
            let (sender, receiver) = oneshot::channel();
            match work {
                Work::Live => state.live.push_back(sender),
                Work::Backfill => state.backfill.push_back(sender),
            }
            receiver
        };
        match receiver.await {
            Ok(permit) => permit,
            // Only a waiter that stopped waiting is dropped from the line.
            Err(_) => Permit { gate: None },
        }
    }

    fn permit(self: &Arc<Self>) -> Permit {
        Permit {
            gate: Some(Arc::clone(self)),
        }
    }

    /// A permit came back: hand it to the next waiter by the rule, or keep it.
    fn release(self: &Arc<Self>) {
        let mut state = self.state();
        loop {
            state.prune();
            let sender = match (state.live.is_empty(), state.backfill.is_empty()) {
                (false, false) => {
                    state.contested += 1;
                    if state.contested.is_multiple_of(BACKFILL_EVERY) {
                        state.backfill.pop_front()
                    } else {
                        state.live.pop_front()
                    }
                }
                (false, true) => state.live.pop_front(),
                (true, false) => state.backfill.pop_front(),
                (true, true) => None,
            };
            let Some(sender) = sender else {
                state.free += 1;
                return;
            };
            // A permit the waiter stops waiting for after it was sent is
            // dropped on the waiter's side, which gives it back then; one
            // whose send failed is disarmed here, under the lock, and the
            // next waiter tried.
            match sender.send(self.permit()) {
                Ok(()) => return,
                Err(mut permit) => {
                    permit.gate = None;
                }
            }
        }
    }
}

impl State {
    /// Drop waiters that stopped waiting.
    fn prune(&mut self) {
        self.live.retain(|sender| !sender.is_closed());
        self.backfill.retain(|sender| !sender.is_closed());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use futures::FutureExt;

    use super::*;

    #[test]
    fn choose_takes_live_first_but_every_fourth_contested_grant_is_backfill() {
        let gate = Gate::new(1);
        let mut kinds = Vec::new();
        for _ in 0..8 {
            let (work, permit) = gate.choose(true, true).unwrap();
            kinds.push(work);
            drop(permit);
        }
        assert_eq!(
            kinds,
            [
                Work::Live,
                Work::Live,
                Work::Live,
                Work::Backfill,
                Work::Live,
                Work::Live,
                Work::Live,
                Work::Backfill
            ]
        );
        assert_eq!(
            gate.choose(false, true).map(|(work, _)| work),
            Some(Work::Backfill),
            "uncontested: no count moves"
        );
    }

    #[test]
    fn no_permit_beyond_the_count_and_a_dropped_one_comes_back() {
        let gate = Gate::new(2);
        let a = gate.choose(true, false).unwrap();
        let _b = gate.choose(true, false).unwrap();
        assert!(gate.choose(true, false).is_none());
        drop(a);
        assert!(gate.choose(true, false).is_some());
    }

    /// Waiters are served first in, first out: an index that builds again
    /// right after its build lands asks behind the index that waited, so a
    /// deep backlog can't keep every permit.
    #[tokio::test]
    async fn a_busy_index_cannot_take_every_build_permit() {
        let gate = Gate::new(1);
        let busy = gate.acquire(Work::Live).await;
        let quiet = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move { gate.acquire(Work::Live).await }
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let again = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move { gate.acquire(Work::Live).await }
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(busy);
        let quiet = tokio::time::timeout(Duration::from_secs(1), quiet)
            .await
            .unwrap()
            .unwrap();
        assert!(
            again.now_or_never().is_none(),
            "the busy index waits its turn"
        );
        drop(quiet);
    }

    /// With live and backfill both waiting in line, released permits follow
    /// the same rule: every fourth goes to backfill.
    #[tokio::test]
    async fn waiting_kinds_share_permits_by_the_rule() {
        let gate = Gate::new(1);
        let first = gate.acquire(Work::Live).await;
        let mut waiters = Vec::new();
        for work in [Work::Live, Work::Live, Work::Live, Work::Backfill] {
            let gate = Arc::clone(&gate);
            waiters.push((
                work,
                tokio::spawn(async move { (work, gate.acquire(work).await) }),
            ));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let mut order = Vec::new();
        drop(first);
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let finished = waiters.iter().position(|(_, waiter)| waiter.is_finished());
            let (_, waiter) = waiters.remove(finished.unwrap());
            let (work, permit) = waiter.await.unwrap();
            order.push(work);
            drop(permit);
        }
        assert_eq!(
            order,
            [Work::Live, Work::Live, Work::Live, Work::Backfill],
            "contested 1, 2, 3 live, 4 backfill"
        );
    }

    /// A waiter that stops waiting gives no permit away: the next one gets it.
    #[tokio::test]
    async fn a_waiter_that_stops_waiting_loses_no_permit() {
        let gate = Gate::new(1);
        let held = gate.acquire(Work::Live).await;
        let gone = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move { gate.acquire(Work::Live).await }
        });
        tokio::time::sleep(Duration::from_millis(5)).await;
        gone.abort();
        let _ = gone.await;
        drop(held);
        assert!(gate.choose(true, false).is_some());
    }
}
