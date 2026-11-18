//! How far each lane has landed the changes routed to it: the one place
//! confirmation and fences are computed from (ADR 0012).
//!
//! The ingest engine **routes** a batch: it publishes it to its lanes, then
//! registers it here with the batch's oldest position and its last (the
//! item's key on every lane). Each sink engine reports how far, in lane
//! order, its writes have **landed** (returned, nothing refused for good) and
//! are **searchable**: the last item written with every item before it. From
//! that, a [`Landed`] case:
//!
//! - `Before(o)`: `o` is the oldest position some lane hasn't landed;
//! - `Through(p)`: nothing outstanding, `p` the newest routed;
//! - `Idle`: nothing routed yet.
//!
//! The source turns the case into its own terms (`ChangeCapture::confirm`,
//! `ChangeCapture::reached`), so it keeps no map of the changes it emitted.
//!
//! ```
//! use engine::{Level, Progress};
//! use kernel::{Position, SinkName};
//! use source::cdc::Landed;
//!
//! let primary = SinkName::try_new("primary").unwrap();
//! let progress = Progress::new([primary.clone()]);
//! assert_eq!(progress.landed(), Landed::Idle);
//!
//! progress.published([&primary], Position::new(1), Position::new(3));
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
//! **Who writes what.** The ingest engine is the only writer of the
//! registrations and `routed`, under one lock it takes once per batch; the
//! sink engines never take it. A lane's sink engine is the only writer of
//! its two marks, one atomic each, which it raises without a lock. Lane
//! order is registration order, so "through key k" covers every registered
//! item up to k. A reader (the ingest engine's confirmation, a fence) takes
//! the registrations' lock, reads the marks, and drops what they cover. A
//! mark may run ahead of the registrations (a lane lands an item before it
//! is registered): it is kept, and covers the item once it is.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use kernel::{Position, SinkName};
use source::cdc::Landed;

/// Which of a lane's two levels to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The item's write returned: confirmation and an *applied* fence.
    Applied,
    /// A search sees the item too: a *searchable* fence.
    Searchable,
}

/// The landing state of every lane, and the newest routed position. One per
/// deployment; it outlives every engine run.
#[derive(Debug)]
pub struct Progress {
    registry: Mutex<Registry>,
    marks: BTreeMap<SinkName, Marks>,
}

/// What the ingest engine routed and no lane's marks cover yet.
#[derive(Debug, Default)]
struct Registry {
    routed: Option<Position>,
    lanes: BTreeMap<SinkName, Registered>,
}

/// One lane's routed items not yet covered, as `(key, oldest)` in routing
/// order.
#[derive(Debug, Default)]
struct Registered {
    unlanded: VecDeque<(u64, Position)>,
    unsearchable: VecDeque<(u64, Position)>,
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
    pub fn new(lanes: impl IntoIterator<Item = SinkName>) -> Self {
        let marks: BTreeMap<SinkName, Marks> = lanes
            .into_iter()
            .map(|lane| (lane, Marks::default()))
            .collect();
        Self {
            registry: Mutex::new(Registry {
                routed: None,
                lanes: marks
                    .keys()
                    .map(|lane| (lane.clone(), Registered::default()))
                    .collect(),
            }),
            marks,
        }
    }

    /// A batch covering `oldest..=last` is on `lanes`. Call it after the
    /// items are published, in position order; a lane not named has nothing
    /// outstanding for the batch. One lock, whatever the number of lanes.
    pub fn published<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a SinkName>,
        oldest: Position,
        last: Position,
    ) {
        let mut registry = lock(&self.registry);
        for lane in lanes {
            if let Some(registered) = registry.lanes.get_mut(lane) {
                registered.unlanded.push_back((last.seq, oldest));
                registered.unsearchable.push_back((last.seq, oldest));
            }
        }
        registry.routed = Some(registry.routed.map_or(last, |routed| routed.max(last)));
    }

    /// `lane` has landed, in lane order, every item through the one keyed
    /// `through` (its last position). Never moves back.
    pub fn applied(&self, lane: &SinkName, through: Position) {
        if let Some(marks) = self.marks.get(lane) {
            raise(&marks.applied, through);
        }
    }

    /// Every item `lane` routed through the one keyed `through` is
    /// searchable. Never moves back.
    pub fn searchable(&self, lane: &SinkName, through: Position) {
        if let Some(marks) = self.marks.get(lane) {
            raise(&marks.searchable, through);
        }
    }

    /// The newest position routed.
    pub fn routed(&self) -> Option<Position> {
        lock(&self.registry).routed
    }

    /// What every lane has landed: what the source may confirm.
    pub fn landed(&self) -> Landed {
        let mut registry = lock(&self.registry);
        let routed = registry.routed;
        let oldest = registry
            .lanes
            .iter_mut()
            .filter_map(|(lane, registered)| {
                registered.outstanding(self.marks.get(lane)?, Level::Applied)
            })
            .min();
        case(oldest, routed)
    }

    /// What `lane` has reached at `level`; `None` for a lane this progress
    /// doesn't know.
    pub fn lane(&self, lane: &SinkName, level: Level) -> Option<Landed> {
        let marks = self.marks.get(lane)?;
        let mut registry = lock(&self.registry);
        let routed = registry.routed;
        let registered = registry.lanes.get_mut(lane)?;
        Some(case(registered.outstanding(marks, level), routed))
    }

    /// Whether `lane` has nothing routed to it outstanding at `level`.
    pub fn is_clear(&self, lane: &SinkName, level: Level) -> bool {
        self.lane(lane, level)
            .is_none_or(|landed| !matches!(landed, Landed::Before(_)))
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

fn case(oldest: Option<Position>, routed: Option<Position>) -> Landed {
    match (oldest, routed) {
        (Some(oldest), _) => Landed::Before(oldest),
        (None, Some(routed)) => Landed::Through(routed),
        (None, None) => Landed::Idle,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
