//! What the ingest engine knows of each index that an operator reads (ADR
//! 0012 stage 8a, slice 7): whether it is exempt, whether something holds
//! its builder, whether its walk runs, and whether it is stuck.
//! One [`IndexStates`] lives as long as the ingest engine, across its runs,
//! and the daemon's `Status` reads it as a handle, as it reads `Progress`.
//!
//! **Written at edges only**, never on the path a change takes:
//!
//! | State | Written by |
//! | --- | --- |
//! | exemption | the dispatcher's 1 s tick, from the rule the reader decides by |
//! | held by the budget | the builder's loop, when its live batch's refusal starts or ends |
//! | held by a lane | a publish that pends on a full lane, until it goes through |
//! | walking | a snapshot's start; its drop (completed, stopped or failed) clears it |
//!
//! **An engine run that ends**, by any path, clears the exemption and the
//! budget hold of every index ([`clear_run`](IndexStates::clear_run), a
//! guard in `IngestEngine::run`): nothing is read between runs, so nothing
//! is exempt, and a source down for an hour shows no exemption from before
//! it. Lane holds and walks clear with their own guards.
//!
//! The writers are public so a status can be shown in a test; only the
//! engine calls them in a deployment.
//!
//! **Stuck** is read, not stored, and is about progress, not holds: items of
//! the index wait on a lane and none was acknowledged for 5 s (6b-2's rule,
//! the record the exemption decides by). A held index whose sink keeps
//! acknowledging is not stuck. With one index the engine keeps no record of
//! its items (one index pays nothing for it), so the view has no answer
//! (`None`) and the daemon reads the lanes' stall ages instead.
//!
//! ```
//! use engine::{ExemptionState, IndexStates};
//! use kernel::IndexName;
//!
//! let users = IndexName::try_new("users").unwrap();
//! let states = IndexStates::new([users.clone()]);
//! let view = states.view(&users).unwrap();
//! assert_eq!(view.exemption, ExemptionState::None);
//! assert_eq!(view.stuck, None, "one index: no record");
//! assert!(view.held.is_none() && view.backfill.is_none());
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use kernel::{IndexName, SinkName};
use tokio::time::Instant;

use crate::ingest::Outstanding;

/// An index's place in 6b-2's exemption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExemptionState {
    /// Not exempt: it paces the reading at its cap.
    None,
    /// Exempt and growing: held at its cap while another index works, so
    /// the reading goes on for the others.
    Exempt,
    /// Exempt and frozen: its hold ended, or no other index works; it may
    /// drain but not grow.
    Frozen,
}

impl ExemptionState {
    /// `none`, `exempt` or `frozen`: the name `/status` gives.
    pub fn as_str(self) -> &'static str {
        match self {
            ExemptionState::None => "none",
            ExemptionState::Exempt => "exempt",
            ExemptionState::Frozen => "frozen",
        }
    }
}

/// What holds an index's builder.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeldBy {
    /// Its live batch is refused by the memory budget.
    Budget,
    /// A publish is blocked on these sinks' full lanes.
    Lanes(BTreeSet<SinkName>),
}

impl HeldBy {
    /// `budget` or `lane`.
    pub fn as_str(&self) -> &'static str {
        match self {
            HeldBy::Budget => "budget",
            HeldBy::Lanes(_) => "lane",
        }
    }
}

/// One index's state, as read now.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexView {
    /// Its place in the exemption, as of the last tick.
    pub exemption: ExemptionState,
    /// When it entered its exemption state (to the tick); `None` while not
    /// exempt.
    pub exemption_since: Option<Instant>,
    /// What holds its builder; `None` while nothing does.
    pub held: Option<HeldBy>,
    /// When the hold began.
    pub held_since: Option<Instant>,
    /// Whether items of it wait on a lane and none was acknowledged for 5 s
    /// (6b-2's rule); `None` where the engine keeps no record (one index).
    pub stuck: Option<bool>,
    /// The sinks its walk in progress serves; `None` while none runs.
    pub backfill: Option<BTreeSet<SinkName>>,
    /// The reader waits at this index's bound now: its cap
    /// (`max_waiting_per_index`), or, frozen, the count it froze at.
    pub pacing_reader: bool,
}

/// Every index's state, for the life of the ingest engine.
#[derive(Debug)]
pub struct IndexStates {
    indexes: BTreeMap<IndexName, One>,
    outstanding: Option<Arc<Outstanding>>,
}

#[derive(Debug, Default)]
struct One {
    /// Held by the budget: read on every pass of its builder's loop, written
    /// only when it changes.
    budget: AtomicBool,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    exemption: (ExemptionState, Option<Instant>),
    lanes: BTreeSet<SinkName>,
    held_since: Option<Instant>,
    walking: Option<BTreeSet<SinkName>>,
    pacing_reader: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            exemption: (ExemptionState::None, None),
            lanes: BTreeSet::new(),
            held_since: None,
            walking: None,
            pacing_reader: false,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl IndexStates {
    /// A state for each of `indexes`, nothing held, nothing walking.
    pub fn new(indexes: impl IntoIterator<Item = IndexName>) -> Self {
        Self {
            indexes: indexes
                .into_iter()
                .map(|index| (index, One::default()))
                .collect(),
            outstanding: None,
        }
    }

    /// The same, reading whether an index's items wait unacknowledged from
    /// the engine's record (two or more indexes).
    pub(crate) fn with_outstanding(mut self, outstanding: Arc<Outstanding>) -> Self {
        self.outstanding = Some(outstanding);
        self
    }

    /// `index`'s state now; `None` for an index the engine doesn't build.
    pub fn view(&self, index: &IndexName) -> Option<IndexView> {
        let one = self.indexes.get(index)?;
        let budget = one.budget.load(Ordering::Relaxed);
        let state = lock(&one.state);
        let held = if budget {
            Some(HeldBy::Budget)
        } else if state.lanes.is_empty() {
            None
        } else {
            Some(HeldBy::Lanes(state.lanes.clone()))
        };
        let stuck = self
            .outstanding
            .as_ref()
            .map(|outstanding| outstanding.stuck(index));
        Some(IndexView {
            exemption: state.exemption.0,
            exemption_since: state.exemption.1,
            stuck,
            held_since: held.as_ref().and(state.held_since),
            held,
            backfill: state.walking.clone(),
            pacing_reader: state.pacing_reader,
        })
    }

    /// The indexes it tracks.
    pub fn indexes(&self) -> impl Iterator<Item = &IndexName> {
        self.indexes.keys()
    }

    /// An engine run ended (any path): no index is exempt or held by the
    /// budget until the next run decides again. Lane holds and walks clear
    /// with their own guards.
    pub fn clear_run(&self) {
        for (index, one) in &self.indexes {
            self.budget(index, false);
            lock(&one.state).exemption = (ExemptionState::None, None);
        }
    }

    /// `index` is in `state` (the tick): its since moves only on a change.
    pub fn set_exemption(&self, index: &IndexName, state: ExemptionState) {
        let Some(one) = self.indexes.get(index) else {
            return;
        };
        let mut current = lock(&one.state);
        if current.exemption.0 != state {
            let since = (state != ExemptionState::None).then(Instant::now);
            current.exemption = (state, since);
        }
    }

    /// Whether the budget refuses `index`'s live batch now: one load when
    /// nothing changed, a lock on a change.
    pub fn budget(&self, index: &IndexName, held: bool) {
        let Some(one) = self.indexes.get(index) else {
            return;
        };
        if one.budget.load(Ordering::Relaxed) == held {
            return;
        }
        let mut state = lock(&one.state);
        one.budget.store(held, Ordering::Relaxed);
        if held {
            state.held_since.get_or_insert_with(Instant::now);
        } else if state.lanes.is_empty() {
            state.held_since = None;
        }
    }

    /// The reader waits at `index`'s bound, until the returned guard is
    /// dropped (the wait ended, or was dropped with the run).
    pub fn pacing_reader(&self, index: &IndexName) -> PacingReader<'_> {
        if let Some(one) = self.indexes.get(index) {
            lock(&one.state).pacing_reader = true;
        }
        PacingReader {
            states: self,
            index: index.clone(),
        }
    }

    /// A publish of `index` pends on `sink`'s full lane, until the returned
    /// guard is dropped (the publish went through, or was dropped).
    pub fn lane_full(&self, index: &IndexName, sink: &SinkName) -> LaneFull<'_> {
        if let Some(one) = self.indexes.get(index) {
            let mut state = lock(&one.state);
            state.lanes.insert(sink.clone());
            state.held_since.get_or_insert_with(Instant::now);
        }
        LaneFull {
            states: self,
            index: index.clone(),
            sink: sink.clone(),
        }
    }

    fn lane_free(&self, index: &IndexName, sink: &SinkName) {
        if let Some(one) = self.indexes.get(index) {
            let mut state = lock(&one.state);
            state.lanes.remove(sink);
            if state.lanes.is_empty() && !one.budget.load(Ordering::Relaxed) {
                state.held_since = None;
            }
        }
    }

    /// The walk of each of `indexes` runs, for `sinks` of each, until the
    /// returned guard is dropped.
    pub fn walking(
        self: &Arc<Self>,
        indexes: impl IntoIterator<Item = (IndexName, BTreeSet<SinkName>)>,
    ) -> Walking {
        let indexes: Vec<IndexName> = indexes
            .into_iter()
            .filter_map(|(index, sinks)| {
                let one = self.indexes.get(&index)?;
                lock(&one.state).walking = Some(sinks);
                Some(index)
            })
            .collect();
        Walking {
            states: Arc::clone(self),
            indexes,
        }
    }
}

/// A publish pending on a full lane; dropping it frees the lane.
#[derive(Debug)]
pub struct LaneFull<'a> {
    states: &'a IndexStates,
    index: IndexName,
    sink: SinkName,
}

impl Drop for LaneFull<'_> {
    fn drop(&mut self) {
        self.states.lane_free(&self.index, &self.sink);
    }
}

/// The reader waiting at an index's bound; dropping it clears it.
#[derive(Debug)]
pub struct PacingReader<'a> {
    states: &'a IndexStates,
    index: IndexName,
}

impl Drop for PacingReader<'_> {
    fn drop(&mut self) {
        if let Some(one) = self.states.indexes.get(&self.index) {
            lock(&one.state).pacing_reader = false;
        }
    }
}

/// A snapshot's walks in progress; dropping it (completed, stopped or
/// failed) clears them.
#[derive(Debug)]
pub struct Walking {
    states: Arc<IndexStates>,
    indexes: Vec<IndexName>,
}

impl Drop for Walking {
    fn drop(&mut self) {
        for index in &self.indexes {
            if let Some(one) = self.states.indexes.get(index) {
                lock(&one.state).walking = None;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn users() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    fn sink(name: &str) -> SinkName {
        SinkName::try_new(name).unwrap()
    }

    /// The exemption's since moves only when the state changes; back to
    /// none, it has none.
    #[tokio::test(start_paused = true)]
    async fn the_exemption_since_moves_only_on_a_change() {
        let states = IndexStates::new([users()]);
        states.set_exemption(&users(), ExemptionState::Exempt);
        let entered = states.view(&users()).unwrap().exemption_since.unwrap();
        tokio::time::advance(Duration::from_secs(5)).await;
        states.set_exemption(&users(), ExemptionState::Exempt);
        assert_eq!(
            states.view(&users()).unwrap().exemption_since,
            Some(entered)
        );
        states.set_exemption(&users(), ExemptionState::Frozen);
        let view = states.view(&users()).unwrap();
        assert_eq!(view.exemption, ExemptionState::Frozen);
        assert_eq!(view.exemption_since, Some(entered + Duration::from_secs(5)));
        states.set_exemption(&users(), ExemptionState::None);
        assert_eq!(states.view(&users()).unwrap().exemption_since, None);
    }

    /// Held by the budget, then by a full lane too: one hold, from its first
    /// start, not working; the lane's guard dropped and the budget's refusal
    /// ended, nothing holds it and it works again.
    #[tokio::test(start_paused = true)]
    async fn a_hold_runs_from_its_first_reason_to_its_last() {
        let states = IndexStates::new([users()]);
        states.budget(&users(), true);
        let began = states.view(&users()).unwrap().held_since.unwrap();
        assert_eq!(states.view(&users()).unwrap().held, Some(HeldBy::Budget));
        tokio::time::advance(Duration::from_secs(1)).await;
        let full = states.lane_full(&users(), &sink("audit"));
        states.budget(&users(), false);
        let view = states.view(&users()).unwrap();
        assert_eq!(
            view.held,
            Some(HeldBy::Lanes(BTreeSet::from([sink("audit")])))
        );
        assert_eq!(view.held_since, Some(began), "one hold");
        drop(full);
        let view = states.view(&users()).unwrap();
        assert_eq!((view.held, view.held_since), (None, None));
    }

    /// Review of #454, item 6: a hold that ended and a new one later: the
    /// second starts when it does, not at the first's start.
    #[tokio::test(start_paused = true)]
    async fn a_second_hold_starts_when_it_does() {
        let states = IndexStates::new([users()]);
        states.budget(&users(), true);
        states.budget(&users(), false);
        tokio::time::advance(Duration::from_secs(10)).await;
        let full = states.lane_full(&users(), &sink("audit"));
        assert_eq!(
            states.view(&users()).unwrap().held_since,
            Some(Instant::now()),
            "the lane's hold starts now"
        );
        drop(full);
        tokio::time::advance(Duration::from_secs(10)).await;
        states.budget(&users(), true);
        assert_eq!(
            states.view(&users()).unwrap().held_since,
            Some(Instant::now()),
            "and the budget's next one too"
        );
    }

    /// Review of #454, item 3: a run's end clears the exemption and the
    /// budget hold.
    #[tokio::test(start_paused = true)]
    async fn a_runs_end_clears_the_exemption_and_the_budget_hold() {
        let states = IndexStates::new([users()]);
        states.set_exemption(&users(), ExemptionState::Exempt);
        states.budget(&users(), true);
        states.clear_run();
        let view = states.view(&users()).unwrap();
        assert_eq!(
            (
                view.exemption,
                view.exemption_since,
                view.held,
                view.held_since
            ),
            (ExemptionState::None, None, None, None)
        );
    }

    /// A walk's sinks while it runs; none once its guard is dropped.
    #[test]
    fn a_walk_is_shown_until_its_guard_is_dropped() {
        let states = Arc::new(IndexStates::new([users()]));
        let walking = states.walking([(users(), BTreeSet::from([sink("primary")]))]);
        assert_eq!(
            states.view(&users()).unwrap().backfill,
            Some(BTreeSet::from([sink("primary")]))
        );
        drop(walking);
        assert_eq!(states.view(&users()).unwrap().backfill, None);
    }

    /// 6b-2's rule: an index whose item waited 5 s unacknowledged is stuck,
    /// held or not; without the record there is no answer.
    #[tokio::test(start_paused = true)]
    async fn an_index_whose_items_wait_unacknowledged_is_stuck() {
        let outstanding = Arc::new(Outstanding::new([users()]));
        let states = IndexStates::new([users()]).with_outstanding(Arc::clone(&outstanding));
        let _item = outstanding.publish(&users()).unwrap();
        states.budget(&users(), true);
        assert_eq!(
            states.view(&users()).unwrap().stuck,
            Some(false),
            "held, not stuck"
        );
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(states.view(&users()).unwrap().stuck, Some(true));
        assert_eq!(
            IndexStates::new([users()]).view(&users()).unwrap().stuck,
            None
        );
    }
}
