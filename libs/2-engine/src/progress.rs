//! How far each lane has landed the changes routed to it: the one place
//! confirmation and fences are computed from (ADR 0012).
//!
//! The ingest engine **routes** a batch: as each lane's send succeeds, it
//! registers the item there with the batch's oldest position and its last
//! (the item's key on every lane), and a lane the batch has nothing for is
//! told it was skipped. Each sink engine reports how far, in lane order, its
//! writes have **landed** (returned, nothing refused for good) and are
//! **searchable**: the last item written with every item before it. Each
//! lane answers a [`Landed`] case:
//!
//! - `Before(o)`: `o` is the oldest position the lane hasn't landed;
//! - `Through(p)`: nothing outstanding, `p` the newest routed to the lane;
//! - `Idle`: nothing routed to the lane yet.
//!
//! Several lanes answer as the one that covers least: what the source may
//! confirm is every lane's answer at once. The source turns the case into its
//! own terms (`ChangeCapture::confirm`, `ChangeCapture::reached`), so it keeps
//! no map of the changes it emitted.
//!
//! ```
//! use engine::{Level, Progress};
//! use kernel::{IndexName, Position, SinkName};
//! use source::cdc::Landed;
//! use stream::LaneKey;
//!
//! let primary = LaneKey::new(
//!     IndexName::try_new("users").unwrap(),
//!     SinkName::try_new("primary").unwrap(),
//! );
//! let progress = Progress::new([primary.clone()]);
//! assert_eq!(progress.landed(), Landed::Idle);
//!
//! progress.published(&primary, Position::new(1), Position::new(3));
//! assert_eq!(progress.landed(), Landed::Before(Position::new(1)));
//!
//! progress.applied(&primary, Position::new(3));
//! assert_eq!(progress.landed(), Landed::Through(Position::new(3)));
//! assert_eq!(
//!     progress.lane(&primary, Level::Searchable),
//!     Some(Landed::Before(Position::new(1))),
//!     "written, not yet searchable",
//! );
//! ```
//!
//! **Why no answer claims too much.** A lane's answer covers only what is
//! registered on it, and an item is registered only after its send succeeded,
//! so a lane never answers past an item it doesn't hold. Until the item is
//! registered the lane answers `Through` its previous newest (or `Idle`):
//! the batch's changes were already emitted, so the source's own `next_seq`
//! test keeps that `Through` short of them, and an `Idle` after an emission
//! moves nothing. Several lanes take the least answer, so a lane still behind
//! a registration holds the others back.
//!
//! **Who writes what.** The ingest engine is the only writer of the
//! registrations, under one lock it takes once per lane per batch; the sink
//! engines never take it. A lane's sink engine is the only writer of its two
//! marks, one atomic each, which it raises without a lock. Lane order is
//! registration order, so "through key k" covers every registered item up to
//! k. A reader (the ingest engine's confirmation, a fence) takes the
//! registrations' lock, reads every lane's marks under it, and drops what they
//! cover: no registration lands between two lanes' answers, and a mark read
//! early only makes its lane's answer older. A mark may run ahead of the
//! registrations (a lane lands an item before it is registered): it is kept,
//! and covers the item once it is.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use kernel::{IndexName, Position, SinkName};
use source::cdc::Landed;
use source::document::DocumentId;
use stream::LaneKey;

mod pending;

use pending::Pending;
pub use pending::Taken;

/// Which of a lane's two levels to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The item's write returned: confirmation and an *applied* fence.
    Applied,
    /// A search sees the item too: a *searchable* fence.
    Searchable,
}

/// The landing state of every lane. One per deployment; it outlives every
/// engine run.
pub struct Progress {
    /// The newest position routed (marked): what an index with nothing
    /// pending and a lane with nothing unlanded have every change through.
    routed: Mutex<Option<Position>>,
    /// Per index, the ids marked and not yet published (ADR 0012 stage 8a).
    pending: BTreeMap<IndexName, Mutex<Pending>>,
    /// The next publish sequence: a live item's key (and position) on its
    /// lanes, in publish order across every index.
    next_key: AtomicU64,
    registry: Mutex<Registry>,
    marks: BTreeMap<LaneKey, Marks>,
    /// Called after every registration: a fence waiter may pass now.
    listener: OnceLock<Listener>,
}

/// What [`Progress::on_registered`] calls.
pub type Listener = Arc<dyn Fn() + Send + Sync>;

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("registry", &self.registry)
            .field("marks", &self.marks)
            .finish_non_exhaustive()
    }
}

/// What the ingest engine routed to each lane and no mark covers yet.
#[derive(Debug, Default)]
struct Registry {
    lanes: BTreeMap<LaneKey, Registered>,
}

/// One lane's routed items not yet covered, as `(key, oldest)` in routing
/// order, and the newest position routed to it.
#[derive(Debug, Default)]
struct Registered {
    unlanded: VecDeque<(u64, Position)>,
    unsearchable: VecDeque<(u64, Position)>,
    /// Every change through it that the lane needs is registered on it.
    newest: Option<Position>,
}

/// How far one lane's sink engine has written, in lane order: the key's
/// `seq + 1`, `0` for nothing.
#[derive(Debug, Default)]
struct Marks {
    applied: AtomicU64,
    searchable: AtomicU64,
}

impl Progress {
    /// Nothing routed, over `lanes`.
    pub fn new(lanes: impl IntoIterator<Item = LaneKey>) -> Self {
        let marks: BTreeMap<LaneKey, Marks> = lanes
            .into_iter()
            .map(|lane| (lane, Marks::default()))
            .collect();
        let pending = marks
            .keys()
            .map(|lane| (lane.index.clone(), Mutex::new(Pending::default())))
            .collect();
        Self {
            routed: Mutex::new(None),
            pending,
            next_key: AtomicU64::new(0),
            registry: Mutex::new(Registry {
                lanes: marks
                    .keys()
                    .map(|lane| (lane.clone(), Registered::default()))
                    .collect(),
            }),
            marks,
            listener: OnceLock::new(),
        }
    }

    /// Nothing routed, over one lane per pair of `indexes` and `sinks`.
    pub fn of(
        indexes: impl IntoIterator<Item = IndexName>,
        sinks: impl IntoIterator<Item = SinkName> + Clone,
    ) -> Self {
        Self::new(indexes.into_iter().flat_map(|index| {
            sinks
                .clone()
                .into_iter()
                .map(move |sink| LaneKey::new(index.clone(), sink))
        }))
    }

    /// Whether `lane` is one this progress keeps.
    pub fn knows(&self, lane: &LaneKey) -> bool {
        lock(&self.registry).lanes.contains_key(lane)
    }

    /// Fail unless every lane in `lanes` is one this progress keeps: an engine
    /// publishing to a lane it doesn't would confirm past that lane's items.
    pub(crate) fn check_lanes<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a LaneKey>,
    ) -> crate::Result<()> {
        let registry = lock(&self.registry);
        match lanes
            .into_iter()
            .find(|lane| !registry.lanes.contains_key(*lane))
        {
            Some(lane) => Err(crate::EngineError::LaneNotInProgress(lane.to_string())),
            None => Ok(()),
        }
    }

    /// `ids` of `index` changed at `at` (the reader resolved a change to
    /// them): they wait until a build takes them, and hold the index's lanes
    /// before `at` until their build lands. Call in position order, then
    /// [`route`](Self::route) the change.
    pub fn mark(&self, index: &IndexName, ids: impl IntoIterator<Item = DocumentId>, at: Position) {
        if let Some(pending) = self.pending.get(index) {
            lock(pending).mark(ids, at);
        }
    }

    /// The change at `at` is routed: its ids are marked (or it touched none).
    pub fn route(&self, at: Position) {
        {
            let mut routed = lock(&self.routed);
            *routed = Some(routed.map_or(at, |routed| routed.max(at)));
        }
        self.registered();
    }

    /// Take up to `max` of `index`'s waiting ids for a build, oldest first,
    /// skipping ids whose build is in flight.
    pub fn take(&self, index: &IndexName, max: usize) -> Option<Taken> {
        lock(self.pending.get(index)?).take(max)
    }

    /// The build `token` of `index` is about to be sent to `lanes`: register
    /// it on each (under a fresh publish sequence, returned as the item's
    /// position), then release it from the index's floor. Send after.
    pub fn publish_taken(&self, index: &IndexName, token: u64, lanes: &[LaneKey]) -> Position {
        let key = Position::new(self.next_key.fetch_add(1, Ordering::Relaxed));
        let Some(pending) = self.pending.get(index) else {
            return key;
        };
        let oldest = lock(pending).oldest_of(token);
        if let Some(oldest) = oldest {
            let mut registry = lock(&self.registry);
            for lane in lanes {
                if let Some(registered) = registry.lanes.get_mut(lane) {
                    registered.unlanded.push_back((key.seq, oldest));
                    registered.unsearchable.push_back((key.seq, oldest));
                }
            }
        }
        // Registered before released: a reader reads the floor first, then
        // the lanes, so the item is in one or the other at every instant.
        lock(pending).published(token);
        self.registered();
        key
    }

    /// The build `token` of `index` won't be published: its ids wait again.
    pub fn give_back(&self, index: &IndexName, token: u64) {
        if let Some(pending) = self.pending.get(index) {
            lock(pending).give_back(token);
        }
    }

    /// Every build in flight, of every index, given back: at the end of an
    /// engine's run, and at the start of the next (a run that was aborted).
    pub fn give_back_all(&self) {
        for pending in self.pending.values() {
            lock(pending).give_back_all();
        }
    }

    /// How many ids wait, over every index.
    pub fn waiting(&self) -> usize {
        self.pending
            .values()
            .map(|pending| lock(pending).waiting())
            .sum()
    }

    /// Call `listener` after every registration (one listener: the fence's).
    pub fn on_registered(&self, listener: Listener) {
        let _ = self.listener.set(listener);
    }

    /// `lane` took the item of a batch covering `oldest..=last`. Call it once
    /// the lane's own send succeeded, in position order per lane.
    pub fn published(&self, lane: &LaneKey, oldest: Position, last: Position) {
        {
            let mut registry = lock(&self.registry);
            if let Some(registered) = registry.lanes.get_mut(lane) {
                registered.unlanded.push_back((last.seq, oldest));
                registered.unsearchable.push_back((last.seq, oldest));
                registered.newest = Some(registered.newest.map_or(last, |n| n.max(last)));
            }
        }
        self.registered();
    }

    /// A batch through `last` had nothing for `lane`: the lane has every
    /// change through `last` it needs, once its earlier items are registered.
    pub fn skipped(&self, lane: &LaneKey, last: Position) {
        {
            let mut registry = lock(&self.registry);
            if let Some(registered) = registry.lanes.get_mut(lane) {
                registered.newest = Some(registered.newest.map_or(last, |n| n.max(last)));
            }
        }
        self.registered();
    }

    fn registered(&self) {
        if let Some(listener) = self.listener.get() {
            listener();
        }
    }

    /// `lane` has landed, in lane order, every item through the one keyed
    /// `through` (its last position). Never moves back.
    pub fn applied(&self, lane: &LaneKey, through: Position) {
        if let Some(marks) = self.marks.get(lane) {
            raise(&marks.applied, through);
        }
    }

    /// Every item `lane` routed through the one keyed `through` is
    /// searchable. Never moves back.
    pub fn searchable(&self, lane: &LaneKey, through: Position) {
        if let Some(marks) = self.marks.get(lane) {
            raise(&marks.searchable, through);
        }
    }

    /// What every lane has landed: what the source may confirm. The lane
    /// that covers least decides.
    pub fn landed(&self) -> Landed {
        self.over(self.marks.keys(), Level::Applied)
            .unwrap_or(Landed::Idle)
    }

    /// What `lane` has reached at `level`; `None` for a lane this progress
    /// doesn't know.
    pub fn lane(&self, lane: &LaneKey, level: Level) -> Option<Landed> {
        self.over([lane], level)
    }

    /// What every one of `lanes` has reached at `level`: the answer of the
    /// lane that covers least. `None` when one is a lane this progress
    /// doesn't know, or `lanes` is empty.
    ///
    /// Every lane is read under the one registry lock, so no registration
    /// lands between two lanes' answers; a mark read early only makes that
    /// lane's answer older, never wider.
    pub fn over<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a LaneKey>,
        level: Level,
    ) -> Option<Landed> {
        // Routed, then the floors, then the lanes: a build is registered on
        // its lanes before it leaves its index's floor, so a position routed
        // by the copy below is in one of the two whenever it is read.
        let routed = *lock(&self.routed);
        let lanes: Vec<&LaneKey> = lanes.into_iter().collect();
        let mut floors: BTreeMap<&IndexName, Option<Position>> = BTreeMap::new();
        for lane in &lanes {
            if !floors.contains_key(&lane.index) {
                let floor = self.pending.get(&lane.index).and_then(|p| lock(p).floor());
                floors.insert(&lane.index, floor);
            }
        }
        let mut registry = lock(&self.registry);
        let mut least: Option<Landed> = None;
        for lane in lanes {
            let marks = self.marks.get(lane)?;
            let registered = registry.lanes.get_mut(lane)?;
            let floor = floors.get(&lane.index).copied().flatten();
            let outstanding = registered.outstanding(marks, level);
            let oldest = match (outstanding, floor) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            let newest = match (registered.newest, routed) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            let answer = match oldest {
                Some(oldest) => Landed::Before(oldest),
                None => newest.map_or(Landed::Idle, Landed::Through),
            };
            least = Some(match least {
                Some(least) if bound(least) <= bound(answer) => least,
                _ => answer,
            });
        }
        least
    }

    /// How many items routed to `lane` it hasn't landed.
    pub fn outstanding(&self, lane: &LaneKey) -> usize {
        let Some(marks) = self.marks.get(lane) else {
            return 0;
        };
        let mut registry = lock(&self.registry);
        registry.lanes.get_mut(lane).map_or(0, |registered| {
            registered.outstanding(marks, Level::Applied);
            registered.unlanded.len()
        })
    }

    /// Whether `lane` has nothing routed to it outstanding at `level`.
    pub fn is_clear(&self, lane: &LaneKey, level: Level) -> bool {
        self.lane(lane, level)
            .is_none_or(|landed| !matches!(landed, Landed::Before(_)))
    }
}

/// The seq just past what `landed` covers; on a tie `Before` is the lower,
/// since it is what a lane with an item outstanding answers.
fn bound(landed: Landed) -> (u64, u8) {
    match landed {
        Landed::Before(oldest) => (oldest.seq, 0),
        Landed::Through(last) => (last.seq.saturating_add(1), 1),
        Landed::Idle => (0, 0),
    }
}

impl Registered {
    /// Drop what the marks cover, then the oldest position still outstanding
    /// at `level`.
    fn outstanding(&mut self, marks: &Marks, level: Level) -> Option<Position> {
        let applied = marks.applied.load(Ordering::Acquire);
        while self.unlanded.front().is_some_and(|(key, _)| *key < applied) {
            self.unlanded.pop_front();
        }
        let searchable = marks.searchable.load(Ordering::Acquire);
        while self
            .unsearchable
            .front()
            .is_some_and(|(key, _)| *key < searchable)
        {
            self.unsearchable.pop_front();
        }
        let unlanded = self.unlanded.front().map(|(_, oldest)| *oldest);
        match level {
            Level::Applied => unlanded,
            Level::Searchable => {
                let unsearchable = self.unsearchable.front().map(|(_, oldest)| *oldest);
                unlanded.into_iter().chain(unsearchable).min()
            }
        }
    }
}

/// Raise a mark to cover `through`. A lane's sink engine is its marks' only
/// writer (one run at a time), so a plain load and store are enough: no
/// read-modify-write on the path every lane item takes.
fn raise(mark: &AtomicU64, through: Position) {
    let covered = through.seq.saturating_add(1);
    if covered > mark.load(Ordering::Relaxed) {
        mark.store(covered, Ordering::Release);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
