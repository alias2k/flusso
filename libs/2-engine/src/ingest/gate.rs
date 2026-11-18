//! The build gate: `builds` permits shared by every build of the ingest side
//! (ADR 0012 §5), live first.
//!
//! **The rule.** A free permit goes to a live batch before a backfill one,
//! except every fourth time both kinds are ready (*contested*), when it goes
//! to the backfill: a steady trickle can't starve a backfill, and `builds = 1`
//! still runs one. A grant only one kind could take doesn't move the count.
//!
//! A [`Permit`] gives itself back when dropped, so a build that fails or is
//! aborted still frees its permit.
//!
//! ```text
//! both kinds ready, one permit at a time:  live, live, live, backfill, live, …
//! ```

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

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
    contested: u64,
}

/// A build's permit; dropping it gives it back.
#[derive(Debug)]
pub(crate) struct Permit {
    gate: Arc<Gate>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.gate.state().free += 1;
    }
}

impl Gate {
    /// A gate of `permits` (at least one).
    pub(crate) fn new(permits: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                free: permits.max(1),
                contested: 0,
            }),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A permit for one of the kinds the caller has ready, by the rule, or
    /// `None` when none is free.
    pub(crate) fn choose(self: &Arc<Self>, live: bool, backfill: bool) -> Option<(Work, Permit)> {
        let mut state = self.state();
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
        drop(state);
        Some((
            work,
            Permit {
                gate: Arc::clone(self),
            },
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
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
}
