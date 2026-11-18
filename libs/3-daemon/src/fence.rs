//! Fences: wait until a sink has caught up with a position a client read from
//! the source, so the client can read its own write.
//!
//! A client commits, reads the source's own position (Postgres:
//! `pg_current_wal_lsn()`), and asks a [`Fence`] to wait for it. The fence
//! passes once every source commit at or before that position is, at the
//! chosen sink (or at all of them), either **applied** (its flush returned: a
//! fetch by id sees it) or **searchable** (a refresh covered
//! it: a search sees it). See [`FenceLevel`].
//!
//! It only ever *observes*. The engines record in the deployment's
//! [`Progress`] how far each sink's lane has landed and made searchable what
//! was routed to it (a [`FenceObserver`] wakes waiters when a sink reports);
//! the source turns that into its own terms ([`ChangeCapture::reached`], off
//! the positions' own stamps) and wakes the fence when the answer moves with
//! no new change (idle WAL). Nothing here refreshes a destination or
//! hurries a flush because a client is waiting, and a backfill is not part of
//! it: seeding is reported by the status, not fenced.
//!
//! The daemon owns the wait; putting it on a wire is the transport's job
//! (the CLI serves it as `GET /wait`).
//!
//! ```
//! use daemon::{FenceLevel, FenceRequest};
//! use std::time::Duration;
//!
//! let request = FenceRequest {
//!     position: "0/16B3748".to_owned(),
//!     level: "applied".parse().unwrap(),
//!     sink: None,
//!     indexes: Vec::new(),
//!     timeout: Duration::from_secs(5),
//! };
//! assert_eq!(request.level, FenceLevel::Applied);
//! assert_eq!(FenceLevel::default(), FenceLevel::Searchable);
//! assert!("visible".parse::<FenceLevel>().is_err());
//! ```

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use engine::{Level, Observer, Progress};
use kernel::{IndexName, Position, SinkName};
use serde::Serialize;
use source::cdc::{ChangeCapture, SourcePosition};
use stream::LaneKey;
use tokio::sync::Notify;
use tokio::time::{Instant, timeout_at};

/// How far a sink must be for a fence to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceLevel {
    /// The sink's flush for the change returned: a fetch by id sees it.
    Applied,
    /// The destination can also *search* it. The default: it is the level
    /// that is right for any read.
    #[default]
    Searchable,
}

impl FenceLevel {
    /// The level's name on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            FenceLevel::Applied => "applied",
            FenceLevel::Searchable => "searchable",
        }
    }
}

impl std::fmt::Display for FenceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for FenceLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "applied" => Ok(FenceLevel::Applied),
            "searchable" => Ok(FenceLevel::Searchable),
            other => Err(format!(
                "unknown level `{other}` (expected `applied` or `searchable`)"
            )),
        }
    }
}

/// One wait.
#[derive(Debug, Clone)]
pub struct FenceRequest {
    /// The position to wait for, in the source's own text form.
    pub position: String,
    /// How far the sink must be.
    pub level: FenceLevel,
    /// The sink to wait for; `None` waits for every sink.
    pub sink: Option<SinkName>,
    /// The indexes to wait for; empty waits for every index.
    pub indexes: Vec<IndexName>,
    /// How long to wait before giving up.
    pub timeout: Duration,
}

/// How a wait ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FenceOutcome {
    /// The fence passed. `position` is how far the sink (the slowest one, for
    /// an all-sinks wait) has reached, at or past the one asked for.
    Reached { position: String },
    /// The timeout passed first. `position` is how far it got.
    TimedOut { position: String },
}

/// Why a wait could not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FenceError {
    /// The source could not read the position.
    #[error("invalid position: {0}")]
    InvalidPosition(String),
    /// No sink of that name is configured.
    #[error("unknown sink `{0}`")]
    UnknownSink(String),
    /// No enabled index of that name is configured.
    #[error("unknown index `{0}`")]
    UnknownIndex(String),
    /// The source cannot say how far it is: its live stream is not open yet,
    /// or it has no position a client could wait on.
    #[error("fences are not available: {0}")]
    Unavailable(String),
}

/// The fence positions of one deployment, in the source's own text form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FencePositions {
    /// How far every sink has applied (the slowest sink).
    pub applied: Option<String>,
    /// How far every sink is searchable (the slowest sink).
    pub searchable: Option<String>,
    /// The same pair for each sink.
    pub sinks: BTreeMap<SinkName, SinkPositions>,
}

/// One sink's fence positions, in the source's own text form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SinkPositions {
    /// How far this sink has applied (its slowest index).
    pub applied: Option<String>,
    /// How far this sink is searchable (its slowest index).
    pub searchable: Option<String>,
    /// The same pair for each index of this sink: each `(index, sink)` lane.
    pub indexes: BTreeMap<IndexName, LanePositions>,
}

/// One `(index, sink)` lane's fence positions, in the source's own text form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanePositions {
    /// How far this lane has applied.
    pub applied: Option<String>,
    /// How far this lane is searchable.
    pub searchable: Option<String>,
    /// Items routed to this lane and not landed.
    pub outstanding: usize,
}

/// The handle a transport waits on. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Fence {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    source: Arc<dyn ChangeCapture>,
    progress: Arc<Progress>,
    lanes: BTreeMap<LaneKey, Mutex<Furthest>>,
    /// Woken whenever any sink's position or the source's answer may have
    /// moved; every waiter re-checks.
    moved: Notify,
    /// How many waits on `searchable` are open.
    searchable_waits: AtomicUsize,
    /// How many waits are open: a registration wakes them only then. Its own
    /// `Arc`, so the registration listener reads it without upgrading.
    waits: Arc<AtomicUsize>,
}

/// Counts one open wait, so registrations know to wake it.
struct OpenWait<'a>(&'a Inner);

impl<'a> OpenWait<'a> {
    fn enter(inner: &'a Inner) -> Self {
        inner.waits.fetch_add(1, Ordering::AcqRel);
        Self(inner)
    }
}

impl Drop for OpenWait<'_> {
    fn drop(&mut self) {
        self.0.waits.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Counts one open wait on `searchable`, and wakes [`Fence::settled`] when it
/// ends.
struct SearchableWait<'a>(&'a Inner);

impl Drop for SearchableWait<'_> {
    fn drop(&mut self) {
        self.0.searchable_waits.fetch_sub(1, Ordering::SeqCst);
        self.0.moved.notify_waiters();
    }
}

/// The furthest source position each level of one lane has been seen to
/// stand for. They only grow, so a reopened source stream (which restarts
/// from what *every* sink confirmed) never walks a sink back from what it
/// already reached.
#[derive(Debug, Default)]
struct Furthest {
    applied: Option<SourcePosition>,
    searchable: Option<SourcePosition>,
}

impl Fence {
    /// A fence over `source` for `lanes` (one per `(index, sink)`), reading
    /// how far each landed from `progress`.
    pub fn new(
        source: Arc<dyn ChangeCapture>,
        progress: Arc<Progress>,
        lanes: impl IntoIterator<Item = LaneKey>,
    ) -> Self {
        let inner = Arc::new(Inner {
            source: Arc::clone(&source),
            progress,
            lanes: lanes
                .into_iter()
                .map(|lane| (lane, Mutex::new(Furthest::default())))
                .collect(),
            moved: Notify::new(),
            searchable_waits: AtomicUsize::new(0),
            waits: Arc::new(AtomicUsize::new(0)),
        });
        // Weak: the source keeps its listeners for life, and must not keep the
        // fence (and through it, itself) alive.
        let listener: Weak<Inner> = Arc::downgrade(&inner);
        source.on_advance(Arc::new(move || {
            if let Some(inner) = listener.upgrade() {
                inner.moved.notify_waiters();
            }
        }));
        // A lane registering an item it already landed can let a waiter
        // pass: wake them, and only when some wait (registration is on the
        // ingest engine's path for every batch).
        let registrations: Weak<Inner> = Arc::downgrade(&inner);
        let waits = Arc::clone(&inner.waits);
        inner.progress.on_registered(Arc::new(move || {
            if waits.load(Ordering::Acquire) > 0
                && let Some(inner) = registrations.upgrade()
            {
                inner.moved.notify_waiters();
            }
        }));
        Self { inner }
    }

    /// A sink reported progress: every waiter checks again.
    fn moved(&self) {
        self.inner.moved.notify_waiters();
    }

    /// How far `lane` is for `level`, in the source's terms. `None` while the
    /// source cannot say.
    ///
    /// Read off what the lane landed (or made searchable), so it keeps moving
    /// over idle WAL. Searchable never runs ahead of applied.
    fn reached(&self, lane: &LaneKey, level: FenceLevel) -> Option<SourcePosition> {
        let mut furthest = lock(self.inner.lanes.get(lane)?);
        let applied = self
            .inner
            .progress
            .lane(lane, Level::Applied)
            .and_then(|landed| self.inner.source.reached(landed));
        furthest.applied = furthest.applied.max(applied);
        match level {
            FenceLevel::Applied => furthest.applied,
            FenceLevel::Searchable => {
                let searchable = self
                    .inner
                    .progress
                    .lane(lane, Level::Searchable)
                    .and_then(|landed| self.inner.source.reached(landed));
                furthest.searchable = furthest.searchable.max(searchable.min(furthest.applied));
                furthest.searchable
            }
        }
    }

    /// The slowest of `lanes` for `level`. `None` if any cannot be placed.
    fn slowest<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a LaneKey>,
        level: FenceLevel,
    ) -> Option<SourcePosition> {
        let mut slowest: Option<SourcePosition> = None;
        let mut any = false;
        for lane in lanes {
            let reached = self.reached(lane, level)?;
            slowest = Some(slowest.map_or(reached, |low| low.min(reached)));
            any = true;
        }
        if any { slowest } else { None }
    }

    /// Wait until the requested lanes (the named sink, or every sink, by the
    /// named indexes, or every index) have reached the requested position at
    /// the requested level, or the timeout passes.
    ///
    /// Returns at once when the position is already reached, and `Err` at once
    /// when the wait cannot start: an unreadable position, an unknown sink or
    /// index, or a source whose live stream is not open yet.
    pub async fn wait(&self, request: &FenceRequest) -> Result<FenceOutcome, FenceError> {
        let source = &self.inner.source;
        let target = source
            .parse_position(&request.position)
            .map_err(|error| FenceError::InvalidPosition(error.to_string()))?;
        let lanes = self.select(request.sink.as_ref(), &request.indexes)?;
        let deadline = Instant::now() + request.timeout;
        let _open = OpenWait::enter(&self.inner);
        let _counted = (request.level == FenceLevel::Searchable).then(|| {
            self.inner.searchable_waits.fetch_add(1, Ordering::SeqCst);
            SearchableWait(&self.inner)
        });
        loop {
            // Registered before the check, so a move between the check and the
            // sleep still wakes this waiter.
            let moved = self.inner.moved.notified();
            tokio::pin!(moved);
            moved.as_mut().enable();

            let Some(reached) = self.slowest(lanes.iter().copied(), request.level) else {
                return Err(FenceError::Unavailable(
                    "the source is not following live changes yet".to_owned(),
                ));
            };
            let position = source.format_position(reached);
            if reached >= target {
                return Ok(FenceOutcome::Reached { position });
            }
            if timeout_at(deadline, moved).await.is_err() {
                return Ok(FenceOutcome::TimedOut { position });
            }
        }
    }

    /// Resolves once no open wait on `searchable` could still be answered by
    /// time alone: none is open, or every sink has reported searchable
    /// everything it applied. A stop waits on it before ending the sink
    /// engines, so a searchable waiter is answered on facts, not cut off,
    /// while a stop nobody waits on doesn't sit out a refresh interval.
    pub(crate) async fn settled(&self) {
        loop {
            let moved = self.inner.moved.notified();
            tokio::pin!(moved);
            moved.as_mut().enable();
            if self.inner.searchable_waits.load(Ordering::SeqCst) == 0
                || self.inner.lanes.keys().all(|lane| {
                    self.inner.progress.lane(lane, Level::Searchable)
                        == self.inner.progress.lane(lane, Level::Applied)
                })
            {
                return;
            }
            moved.await;
        }
    }

    /// The lanes a request names: `sink` (or every sink) by `indexes` (or
    /// every index).
    fn select(
        &self,
        sink: Option<&SinkName>,
        indexes: &[IndexName],
    ) -> Result<Vec<&LaneKey>, FenceError> {
        if let Some(sink) = sink
            && !self.inner.lanes.keys().any(|lane| &lane.sink == sink)
        {
            return Err(FenceError::UnknownSink(sink.to_string()));
        }
        for index in indexes {
            if !self.inner.lanes.keys().any(|lane| &lane.index == index) {
                return Err(FenceError::UnknownIndex(index.to_string()));
            }
        }
        Ok(self
            .inner
            .lanes
            .keys()
            .filter(|lane| sink.is_none_or(|sink| &lane.sink == sink))
            .filter(|lane| indexes.is_empty() || indexes.contains(&lane.index))
            .collect())
    }

    /// Each lane and the items routed to it it hasn't landed, for a metric.
    pub fn outstanding(&self) -> Vec<(LaneKey, usize)> {
        self.inner
            .lanes
            .keys()
            .map(|lane| (lane.clone(), self.inner.progress.outstanding(lane)))
            .collect()
    }

    /// Where every sink and lane is right now, for a status surface.
    pub fn positions(&self) -> FencePositions {
        let format = |position: Option<SourcePosition>| {
            position.map(|p| self.inner.source.format_position(p))
        };
        let lanes = || self.inner.lanes.keys();
        let mut sinks: BTreeMap<SinkName, SinkPositions> = BTreeMap::new();
        for lane in lanes() {
            sinks.entry(lane.sink.clone()).or_default().indexes.insert(
                lane.index.clone(),
                LanePositions {
                    applied: format(self.reached(lane, FenceLevel::Applied)),
                    searchable: format(self.reached(lane, FenceLevel::Searchable)),
                    outstanding: self.inner.progress.outstanding(lane),
                },
            );
        }
        for (sink, positions) in &mut sinks {
            let of_sink = || lanes().filter(|lane| &lane.sink == sink);
            positions.applied = format(self.slowest(of_sink(), FenceLevel::Applied));
            positions.searchable = format(self.slowest(of_sink(), FenceLevel::Searchable));
        }
        FencePositions {
            applied: format(self.slowest(lanes(), FenceLevel::Applied)),
            searchable: format(self.slowest(lanes(), FenceLevel::Searchable)),
            sinks,
        }
    }
}

fn lock(furthest: &Mutex<Furthest>) -> std::sync::MutexGuard<'_, Furthest> {
    furthest.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Wakes a [`Fence`]'s waiters on the sink engines' position events. What a
/// sink reached is read from the deployment's `Progress`, not from the event.
#[derive(Debug)]
pub struct FenceObserver {
    fence: Fence,
}

impl FenceObserver {
    /// An observer feeding `fence`.
    pub fn new(fence: Fence) -> Self {
        Self { fence }
    }
}

impl Observer for FenceObserver {
    fn on_position_applied(&self, _: &LaneKey, _: Position) {
        self.fence.moved();
    }

    fn on_position_searchable(&self, _: &LaneKey, _: Position) {
        self.fence.moved();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
