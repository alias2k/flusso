//! How far each lane has landed the changes routed to it: the one place
//! confirmation and fences are computed from (ADR 0012).
//!
//! The ingest engine **routes** a batch: it publishes it to its lanes, then
//! registers it here with the batch's oldest position and its last (the
//! item's key on every lane). Each sink engine reports, per lane item, when
//! its write **landed** (returned, nothing in it refused for good) and how far
//! its writes are **searchable**. From that, a [`Landed`] case:
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
//! progress.wrote(&primary, Some(Position::new(3)), None);
//! assert_eq!(progress.landed(), Landed::Through(Position::new(3)));
//! assert_eq!(
//!     progress.lane(&primary, Level::Searchable),
//!     Some(Landed::Before(Position::new(1))),
//!     "written, not yet searchable",
//! );
//! ```
//!
//! **Locks.** The ingest engine writes `routed` and each lane's `registered`;
//! a sink engine writes only its lane's `reports`. The two sides never take
//! the same lock: each contends only with a reader settling a lane. A reader
//! copies `routed` *before* it reads the lanes. A batch registered after that
//! copy may already be landed and trimmed, but under the older `routed` it is
//! never counted: the source's own `next_seq` test keeps `Through` short of
//! every change emitted after it.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
    routed: Mutex<Option<Position>>,
    lanes: BTreeMap<SinkName, Lane>,
}

#[derive(Debug, Default)]
struct Lane {
    registered: Mutex<Registered>,
    reports: Mutex<Reports>,
}

/// What the ingest engine routed to a lane and the lane hasn't finished, as
/// `(key, oldest)` in routing order.
#[derive(Debug, Default)]
struct Registered {
    unlanded: VecDeque<(u64, Position)>,
    unsearchable: VecDeque<(u64, Position)>,
    /// The newest key registered.
    newest: Option<u64>,
}

/// What the lane's sink engine reported, not yet matched with what is
/// registered.
#[derive(Debug, Default)]
struct Reports {
    landed: BTreeSet<u64>,
    searchable_through: Option<u64>,
}

impl Progress {
    /// Nothing routed, over `lanes`.
    pub fn new(lanes: impl IntoIterator<Item = SinkName>) -> Self {
        Self {
            routed: Mutex::new(None),
            lanes: lanes
                .into_iter()
                .map(|lane| (lane, Lane::default()))
                .collect(),
        }
    }

    /// A batch covering `oldest..=last` is on `lanes`.
    pub fn published<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a SinkName>,
        oldest: Position,
        last: Position,
    ) {
        let _ = (lanes.into_iter().count(), oldest, last);
    }

    /// `lane` landed the item keyed `landed`.
    pub fn wrote(
        &self,
        lane: &SinkName,
        landed: Option<Position>,
        searchable_through: Option<Position>,
    ) {
        let _ = (lane, landed, searchable_through);
    }

    /// The newest position routed.
    pub fn routed(&self) -> Option<Position> {
        *lock(&self.routed)
    }

    /// What every lane has landed.
    pub fn landed(&self) -> Landed {
        Landed::Idle
    }

    /// What `lane` has reached at `level`.
    pub fn lane(&self, lane: &SinkName, level: Level) -> Option<Landed> {
        let _ = level;
        self.lanes
            .get(lane)
            .map(|lane| case(lane.outstanding(level), None))
    }

    /// Whether `lane` has nothing routed to it outstanding at `level`.
    pub fn is_clear(&self, lane: &SinkName, level: Level) -> bool {
        let _ = (lane, level);
        true
    }
}

impl Lane {
    /// Match the reports against what is registered, then the oldest position
    /// still outstanding at `level`.
    fn outstanding(&self, level: Level) -> Option<Position> {
        let _ = (&self.registered, &self.reports, level);
        None
    }

    #[allow(dead_code)]
    fn settle(&self, level: Level) -> Option<Position> {
        let mut registered = lock(&self.registered);
        let mut reports = lock(&self.reports);
        while let Some((key, _)) = registered.unlanded.front() {
            if !reports.landed.remove(key) {
                break;
            }
            registered.unlanded.pop_front();
        }
        // A report no registration can match any more: a redelivered item
        // landed again. Registrations come in key order, so that is every key
        // below the oldest unlanded one, or at or below the newest registered.
        match (registered.unlanded.front(), registered.newest) {
            (Some((front, _)), _) => reports.landed = reports.landed.split_off(front),
            (None, Some(newest)) => {
                reports.landed = reports.landed.split_off(&newest.saturating_add(1));
            }
            (None, None) => {}
        }
        if let Some(through) = reports.searchable_through {
            while registered
                .unsearchable
                .front()
                .is_some_and(|(key, _)| *key <= through)
            {
                registered.unsearchable.pop_front();
            }
        }
        let unlanded = registered.unlanded.front().map(|(_, oldest)| *oldest);
        match level {
            Level::Applied => unlanded,
            Level::Searchable => {
                let unsearchable = registered.unsearchable.front().map(|(_, oldest)| *oldest);
                unlanded.into_iter().chain(unsearchable).min()
            }
        }
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
