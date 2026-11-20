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
//! - **G3.** A waiter is *eligible* unless the budget refused it at the
//!   budget's current generation; a refusal made while behind (G6) stands
//!   only while the waiter still is behind. While a waiter is in a line, the
//!   budget moves its generation on every change of what it holds
//!   ([`Budget::watch`]: a release, a grant, a settle larger than its
//!   estimate), so a refusal at the current generation is what the budget
//!   would answer now. Standing is read from every waiter's refusal and the
//!   generation at the moment of asking, under the lock: no mark another
//!   waiter has to refresh, so none is out of date after a release, an add,
//!   a grant or a drop.
//! - **G4.** A free permit goes to the oldest eligible live waiter, except
//!   every fourth time both lines have an eligible waiter (*contested*), when
//!   it goes to the oldest eligible backfill waiter: a steady trickle can't
//!   starve a backfill, and `builds = 1` still runs one. A grant only one
//!   kind could take doesn't move the count.
//! - **G5.** The gate's own budget is asked at grant, under the gate's lock
//!   (gate, then budget): refused, the waiter keeps its place, becomes
//!   ineligible, and the permit goes on. The gate makes every refusal itself,
//!   so each carries its budget's generation.
//! - **G6.** A waiter is *behind* while an older waiter of its kind is
//!   ineligible with a refusal by the limit at the current generation; it
//!   reserves behind it ([`Budget::reserve_behind`]): it takes no room by the
//!   limit, so small batches can't starve a large one. A refusal by an
//!   index's share, or one the budget has changed since, holds no one back.
//!   A waiter refused while behind is eligible again as soon as nothing holds
//!   it back (the one ahead left or was refused by its share, or the budget
//!   changed).
//! - **G7.** Every change wakes every waiter, after the gate's lock is let
//!   go (a grant's add wakes the budget's waiters with it held: that only
//!   schedules them); a waiter registers its wake-ups before reading the
//!   state, so none is lost. A dropped permit wakes only while someone waits.
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

use kernel::IndexName;

use crate::budget::{Budget, Refusal, Reservation};
use crate::observer::Work;

/// How many of every kind of contested grant go to backfill.
const BACKFILL_EVERY: u64 = 4;

/// The permits of every build of the ingest side.
#[derive(Debug)]
pub(crate) struct Gate {
    state: Mutex<State>,
    /// What every waiter reserves from, asked at grant (G5).
    budget: Arc<Budget>,
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
    /// Its last refusal: standing only at that generation (G3).
    refused: Option<Refusal>,
    /// The refusal came while it was behind (G6): standing only while it
    /// still is.
    behind: bool,
}

/// A build's permit; dropping it gives it back.
#[derive(Debug)]
pub(crate) struct Permit {
    gate: Arc<Gate>,
}

impl Drop for Permit {
    /// Gives the permit back, and wakes the waiters if any is in a line:
    /// decided under the lock, so a waiter joining after it sees the permit
    /// free, and one that joined before is woken (G7). With no one waiting
    /// (today's one task) it costs no wake-up.
    fn drop(&mut self) {
        let waiting = {
            let mut state = self.gate.state();
            state.free += 1;
            state.is_waiting()
        };
        if waiting {
            self.gate.changed.notify_waiters();
        }
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

    /// Each waiter of `work`'s line, oldest first, with whether it is eligible
    /// (G3) and whether it is behind (G6), read from every waiter's refusal
    /// as it stands at the budget's generation `now`.
    fn standing(&self, work: Work, now: u64) -> impl Iterator<Item = Standing> + '_ {
        self.line(work)
            .iter()
            .scan(false, move |held_back, waiting| {
                let behind = *held_back;
                let eligible = match waiting.refused {
                    Some(refusal) if refusal.generation == now => {
                        let eligible = waiting.behind && !behind;
                        if !eligible && refusal.by_limit {
                            *held_back = true;
                        }
                        eligible
                    }
                    _ => true,
                };
                Some(Standing {
                    id: waiting.id,
                    eligible,
                    behind,
                })
            })
    }

    /// The oldest eligible waiter of `work`'s line.
    fn front(&self, work: Work, now: u64) -> Option<u64> {
        self.standing(work, now)
            .find(|standing| standing.eligible)
            .map(|standing| standing.id)
    }

    /// Whether both lines have an eligible waiter.
    fn is_contested(&self, now: u64) -> bool {
        self.front(Work::Live, now).is_some() && self.front(Work::Backfill, now).is_some()
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
    fn selected(&self, now: u64) -> Option<(Work, u64)> {
        let live = self.front(Work::Live, now);
        let backfill = self.front(Work::Backfill, now);
        match self.turn(live.is_some(), backfill.is_some())? {
            Work::Live => live.map(|id| (Work::Live, id)),
            Work::Backfill => backfill.map(|id| (Work::Backfill, id)),
        }
    }

    /// Whether waiter `id` of `work` is behind (G6).
    fn is_behind(&self, work: Work, id: u64, now: u64) -> bool {
        self.standing(work, now)
            .find(|standing| standing.id == id)
            .is_some_and(|standing| standing.behind)
    }

    fn waiting_mut(&mut self, work: Work, id: u64) -> Option<&mut Waiting> {
        self.line_mut(work)
            .iter_mut()
            .find(|waiting| waiting.id == id)
    }

    fn is_waiting(&self) -> bool {
        !self.live.is_empty() || !self.backfill.is_empty()
    }
}

/// A waiter's standing at one moment (G3, G6).
struct Standing {
    id: u64,
    eligible: bool,
    behind: bool,
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
            self.gate
                .state()
                .line_mut(self.work)
                .retain(|waiting| waiting.id != self.id);
        }
        self.gate.budget.unwatch();
        if !self.granted {
            self.gate.changed.notify_waiters();
        }
    }
}

impl Gate {
    /// A gate of `permits` (at least one) whose waiters reserve from
    /// `budget`.
    pub(crate) fn new(permits: usize, budget: Arc<Budget>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                free: permits.max(1),
                contested: 0,
                live: VecDeque::new(),
                backfill: VecDeque::new(),
                next_id: 0,
            }),
            budget,
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
        if state.free == 0
            || (state.is_waiting() && state.selected(self.budget.generation()).is_some())
        {
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

    /// Wait in `work`'s line for a permit and a reservation of `parts` from
    /// the gate's budget (G2–G8), asked when the permit is offered, behind an
    /// older waiter the limit refused if there is one (G6). Refused, the
    /// waiter keeps its place until a release or the waiters ahead let it be
    /// asked again (G3, G5). Dropping the future leaves the line (G8).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the index tasks of ADR 0012 stage 8a's 6a/6b wait here"
        )
    )]
    pub(crate) async fn acquire(
        self: &Arc<Self>,
        work: Work,
        parts: &[(IndexName, usize)],
    ) -> (Permit, Vec<Reservation>) {
        let mut place = {
            let mut state = self.state();
            let id = state.next_id;
            state.next_id += 1;
            self.budget.watch();
            state.line_mut(work).push_back(Waiting {
                id,
                refused: None,
                behind: false,
            });
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
            let standing_refusal = {
                let mut state = self.state();
                let now = self.budget.generation();
                if state.free > 0 && state.selected(now) == Some((work, place.id)) {
                    let behind = state.is_behind(work, place.id, now);
                    match self.budget.reserve_behind(work, parts, behind) {
                        Ok(reserved) => {
                            if state.is_contested(now) {
                                state.contested += 1;
                            }
                            state.free -= 1;
                            state
                                .line_mut(work)
                                .retain(|waiting| waiting.id != place.id);
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
                                waiting.behind = behind;
                            }
                            drop(state);
                            self.changed.notify_waiters();
                            Some(refusal.generation)
                        }
                    }
                } else {
                    state
                        .waiting_mut(work, place.id)
                        .and_then(|waiting| waiting.refused)
                        .map(|refusal| refusal.generation)
                        .filter(|&generation| generation == now)
                }
            };
            match standing_refusal {
                Some(generation) => {
                    tokio::select! {
                        () = &mut changed => {}
                        () = self.budget.released_since(generation) => {
                            self.changed.notify_waiters();
                        }
                    }
                }
                None => changed.await,
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
