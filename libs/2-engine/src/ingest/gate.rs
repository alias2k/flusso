//! The build gate: `builds` permits shared by every build of the ingest side
//! (ADR 0012 §5), live first, with a waiting line per kind (stage 8a §3.5).
//!
//! **The rules** (G1–G9 of the stage note):
//!
//! - **G1.** At most `builds` permits are out; a [`Permit`] gives itself back
//!   when dropped, so a build that fails or is aborted still frees its permit.
//! - **G2.** One FIFO line per kind (live, backfill): a waiter
//!   ([`Gate::acquire`]) joins the back of its kind's line and keeps its place
//!   until it is granted or dropped.
//! - **G3.** A waiter is *eligible* unless the budget refused it and no
//!   release has come since.
//! - **G4.** A free permit goes to the oldest eligible live waiter, except
//!   every fourth time both lines have an eligible waiter (*contested*), when
//!   it goes to the oldest eligible backfill waiter: a steady trickle can't
//!   starve a backfill, and `builds = 1` still runs one. A grant only one
//!   kind could take doesn't move the count.
//! - **G5.** The budget is asked at grant, under the gate's lock: refused,
//!   the waiter keeps its place, becomes ineligible, and the permit goes on.
//! - **G6.** A waiter behind an older one of its kind that the budget refused
//!   by the limit reserves *behind* it ([`Budget::reserve_behind`]): it takes
//!   no room by the limit, so small batches can't starve a large one; a
//!   refusal by an index's share holds no one back.
//! - **G7.** Every change wakes every waiter, after the lock is let go; a
//!   waiter registers its wake-up before reading the state, so no wake-up is
//!   lost.
//! - **G8.** A waiter dropped before its grant leaves its line and wakes the
//!   others; a permit dropped gives itself back, once.
//! - **G9.** The one task of today doesn't wait in a line: at each grant point
//!   it asks [`Gate::choose`] with the kinds it has ready, and gets a permit by
//!   G4 only while no eligible waiter is in a line. With one task that is
//!   always, so the order of grants is the same as before the lines.
//!
//! ```text
//! both kinds ready, one permit at a time:  live, live, live, backfill, live, …
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::Notify;

use crate::budget::{Budget, Refusal};
use crate::observer::Work;

/// How many of every kind of contested grant go to backfill.
const BACKFILL_EVERY: u64 = 4;

/// The permits of every build of the ingest side.
#[derive(Debug)]
pub(crate) struct Gate {
    state: Mutex<State>,
    /// Every change of the state (G7).
    changed: Notify,
}

#[derive(Debug)]
struct State {
    free: usize,
    contested: u64,
    /// The waiters of each kind, oldest first.
    live: VecDeque<Waiting>,
    backfill: VecDeque<Waiting>,
    next_id: u64,
}

#[derive(Debug)]
struct Waiting {
    id: u64,
    /// Refused by the budget, and not yet made eligible again by a release.
    refused: Option<Refusal>,
}

/// A build's permit; dropping it gives it back.
#[derive(Debug)]
pub(crate) struct Permit {
    gate: Arc<Gate>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.gate.state().free += 1;
        self.gate.changed.notify_waiters();
    }
}

impl State {
    fn line(&self, work: Work) -> &VecDeque<Waiting> {
        match work {
            Work::Live => &self.live,
            Work::Backfill => &self.backfill,
        }
    }

    fn line_mut(&mut self, work: Work) -> &mut VecDeque<Waiting> {
        match work {
            Work::Live => &mut self.live,
            Work::Backfill => &mut self.backfill,
        }
    }

    /// The oldest eligible waiter of `work`'s line.
    fn front(&self, work: Work) -> Option<u64> {
        self.line(work)
            .iter()
            .find(|waiting| waiting.refused.is_none())
            .map(|waiting| waiting.id)
    }

    /// Whether both lines have an eligible waiter.
    fn is_contested(&self) -> bool {
        self.front(Work::Live).is_some() && self.front(Work::Backfill).is_some()
    }

    /// The kind G4 gives the next permit to, among `live` and `backfill`
    /// ready, without moving the count.
    fn turn(&self, live: bool, backfill: bool) -> Option<Work> {
        match (live, backfill) {
            (true, true) if (self.contested + 1).is_multiple_of(BACKFILL_EVERY) => {
                Some(Work::Backfill)
            }
            (true, _) => Some(Work::Live),
            (false, true) => Some(Work::Backfill),
            (false, false) => None,
        }
    }

    /// The waiter G4 offers the next permit to.
    fn selected(&self) -> Option<(Work, u64)> {
        let live = self.front(Work::Live);
        let backfill = self.front(Work::Backfill);
        match self.turn(live.is_some(), backfill.is_some())? {
            Work::Live => live.map(|id| (Work::Live, id)),
            Work::Backfill => backfill.map(|id| (Work::Backfill, id)),
        }
    }

    /// Whether an older waiter of `work` than `id` was refused by the limit
    /// (G6).
    fn behind_a_limit_refusal(&self, work: Work, id: u64) -> bool {
        self.line(work)
            .iter()
            .take_while(|waiting| waiting.id != id)
            .any(|waiting| waiting.refused.is_some_and(|refusal| refusal.by_limit))
    }

    fn waiting_mut(&mut self, work: Work, id: u64) -> Option<&mut Waiting> {
        self.line_mut(work)
            .iter_mut()
            .find(|waiting| waiting.id == id)
    }

    fn leave(&mut self, work: Work, id: u64) {
        self.line_mut(work).retain(|waiting| waiting.id != id);
    }
}

/// A waiter's place in its line: dropped before its grant, it leaves the
/// line and wakes the others (G8).
struct Place<'a> {
    gate: &'a Gate,
    work: Work,
    id: u64,
    granted: bool,
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        if !self.granted {
            self.gate.state().leave(self.work, self.id);
            self.gate.changed.notify_waiters();
        }
    }
}

impl Gate {
    /// A gate of `permits` (at least one).
    pub(crate) fn new(permits: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                free: permits.max(1),
                contested: 0,
                live: VecDeque::new(),
                backfill: VecDeque::new(),
                next_id: 0,
            }),
            changed: Notify::new(),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A permit for one of the kinds the caller has ready, by G4, or `None`
    /// when none is free or an eligible waiter is in a line (G9: it was there
    /// first).
    pub(crate) fn choose(self: &Arc<Self>, live: bool, backfill: bool) -> Option<(Work, Permit)> {
        let mut state = self.state();
        if state.free == 0 || state.selected().is_some() {
            return None;
        }
        let work = state.turn(live, backfill)?;
        if live && backfill {
            state.contested += 1;
        }
        state.free -= 1;
        drop(state);
        Some((
            work,
            Permit {
                gate: Arc::clone(self),
            },
        ))
    }

    /// Wait in `work`'s line for a permit (G2–G8). When the permit is offered,
    /// `reserve` is called under the gate's lock with whether the waiter is
    /// behind an older one the limit refused (G6), and reserves its batch;
    /// refused, the waiter keeps its place until `budget` releases past the
    /// refusal (G3, G5). Dropping the future leaves the line (G8).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the index tasks of ADR 0012 stage 8a's 6a/6b wait here"
        )
    )]
    pub(crate) async fn acquire<R>(
        self: &Arc<Self>,
        work: Work,
        budget: Option<&Budget>,
        mut reserve: impl FnMut(bool) -> Result<R, Refusal>,
    ) -> (Permit, R) {
        let mut place = {
            let mut state = self.state();
            let id = state.next_id;
            state.next_id += 1;
            state
                .line_mut(work)
                .push_back(Waiting { id, refused: None });
            Place {
                gate: self,
                work,
                id,
                granted: false,
            }
        };
        self.changed.notify_waiters();
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let refused = {
                let mut state = self.state();
                let refused = state
                    .waiting_mut(work, place.id)
                    .and_then(|waiting| waiting.refused);
                if refused.is_none() && state.free > 0 && state.selected() == Some((work, place.id))
                {
                    let behind = state.behind_a_limit_refusal(work, place.id);
                    match reserve(behind) {
                        Ok(reserved) => {
                            if state.is_contested() {
                                state.contested += 1;
                            }
                            state.free -= 1;
                            state.leave(work, place.id);
                            place.granted = true;
                            drop(state);
                            self.changed.notify_waiters();
                            return (
                                Permit {
                                    gate: Arc::clone(self),
                                },
                                reserved,
                            );
                        }
                        Err(refusal) => {
                            if let Some(waiting) = state.waiting_mut(work, place.id) {
                                waiting.refused = Some(refusal);
                            }
                            drop(state);
                            self.changed.notify_waiters();
                            Some(refusal)
                        }
                    }
                } else {
                    refused
                }
            };
            match (refused, budget) {
                (Some(refusal), Some(budget)) => {
                    tokio::select! {
                        () = &mut changed => {}
                        () = budget.released_since(refusal.generation) => {
                            if let Some(waiting) = self.state().waiting_mut(work, place.id) {
                                waiting.refused = None;
                            }
                            self.changed.notify_waiters();
                        }
                    }
                }
                _ => changed.await,
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
