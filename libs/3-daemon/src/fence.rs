//! Fences: wait until a sink has caught up with a position a client read from
//! the source, so the client can read its own write.
//!
//! A client commits, reads the source's own position (Postgres:
//! `pg_current_wal_lsn()`), and asks a [`Fence`] to wait for it. The fence
//! passes once every source commit at or before that position is, at the
//! chosen sink (or at all of them), either **applied** (flushed and
//! acknowledged: a fetch by id sees it) or **searchable** (a refresh covered
//! it: a search sees it). See [`FenceLevel`].
//!
//! It only ever *observes*. The sink engines report the stream position each
//! sink has applied and made searchable ([`FenceObserver`]); the source
//! translates a stream position into its own terms
//! ([`ChangeCapture::reached`]) and wakes the fence when that answer moves
//! with no new change (idle WAL). Nothing here refreshes a destination or
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
//!     timeout: Duration::from_secs(5),
//! };
//! assert_eq!(request.level, FenceLevel::Applied);
//! assert_eq!(FenceLevel::default(), FenceLevel::Searchable);
//! assert!("visible".parse::<FenceLevel>().is_err());
//! ```

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use engine::Observer;
use kernel::{Position, SinkName};
use serde::Serialize;
use source::cdc::{ChangeCapture, SourcePosition};
use tokio::sync::Notify;
use tokio::time::{Instant, timeout_at};

/// How far a sink must be for a fence to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceLevel {
    /// The sink flushed and acknowledged the change: a fetch by id sees it.
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
    /// How far this sink has applied.
    pub applied: Option<String>,
    /// How far this sink is searchable.
    pub searchable: Option<String>,
}

/// The handle a transport waits on. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Fence {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    source: Arc<dyn ChangeCapture>,
    sinks: BTreeMap<SinkName, Mutex<Progress>>,
    /// Woken whenever any sink's position or the source's answer may have
    /// moved; every waiter re-checks.
    moved: Notify,
}

/// What one sink has reported, and the furthest source position each level
/// has been seen to stand for. Those only grow, so a reopened source stream
/// (which restarts from what *every* sink confirmed) never walks a sink back
/// from what it already applied.
#[derive(Debug, Default)]
struct Progress {
    /// The highest stream position reported applied.
    applied: Option<Position>,
    applied_reached: Option<SourcePosition>,
    searchable_reached: Option<SourcePosition>,
    /// Applied positions not yet reported searchable, oldest first, each with
    /// the source position it stood for when it was applied. The source
    /// forgets a position once every sink confirmed it, which can be before it
    /// is searchable, so the translation is taken while it is still exact.
    unsearchable: VecDeque<(Position, SourcePosition)>,
}

impl Fence {
    /// A fence over `source` for `sinks`, nothing reached yet.
    pub fn new(source: Arc<dyn ChangeCapture>, sinks: impl IntoIterator<Item = SinkName>) -> Self {
        let inner = Arc::new(Inner {
            source: Arc::clone(&source),
            sinks: sinks
                .into_iter()
                .map(|sink| (sink, Mutex::new(Progress::default())))
                .collect(),
            moved: Notify::new(),
        });
        // Weak: the source keeps its listeners for life, and must not keep the
        // fence (and through it, itself) alive.
        let listener: Weak<Inner> = Arc::downgrade(&inner);
        source.on_advance(Arc::new(move || {
            if let Some(inner) = listener.upgrade() {
                inner.moved.notify_waiters();
            }
        }));
        Self { inner }
    }

    /// `sink` applied everything through `position`.
    fn record_applied(&self, sink: &SinkName, position: Position) {
        if let Some(progress) = self.inner.sinks.get(sink) {
            let mut progress = lock(progress);
            // With nothing waiting for a refresh, everything before this
            // position is searchable. Taken now, while the source still holds
            // this position: it bounds the answer just below it.
            if progress.unsearchable.is_empty()
                && let Some(before) = self.inner.source.reached(progress.applied)
            {
                progress.searchable_reached = progress.searchable_reached.max(Some(before));
            }
            progress.applied = Some(progress.applied.map_or(position, |p| p.max(position)));
            if let Some(reached) = self.inner.source.reached(Some(position)) {
                progress.applied_reached = progress.applied_reached.max(Some(reached));
                progress.unsearchable.push_back((position, reached));
            }
        }
        self.inner.moved.notify_waiters();
    }

    /// Everything `sink` applied through `position` is now searchable.
    fn record_searchable(&self, sink: &SinkName, position: Position) {
        if let Some(progress) = self.inner.sinks.get(sink) {
            let mut progress = lock(progress);
            while let Some((applied, reached)) = progress.unsearchable.front().copied() {
                if applied > position {
                    break;
                }
                progress.searchable_reached = progress.searchable_reached.max(Some(reached));
                progress.unsearchable.pop_front();
            }
        }
        self.inner.moved.notify_waiters();
    }

    /// How far `sink` is for `level`, in the source's terms. `None` while the
    /// source cannot say.
    ///
    /// Applied follows the source live, so it keeps moving over idle WAL.
    /// Searchable does the same while nothing applied is still waiting for a
    /// refresh; otherwise it stands at the last position reported searchable.
    fn reached(&self, sink: &SinkName, level: FenceLevel) -> Option<SourcePosition> {
        let mut progress = lock(self.inner.sinks.get(sink)?);
        if let Some(now) = self.inner.source.reached(progress.applied) {
            progress.applied_reached = progress.applied_reached.max(Some(now));
        }
        match level {
            FenceLevel::Applied => progress.applied_reached,
            FenceLevel::Searchable => {
                if progress.unsearchable.is_empty() {
                    progress.searchable_reached =
                        progress.searchable_reached.max(progress.applied_reached);
                }
                progress.searchable_reached
            }
        }
    }

    /// The slowest of `sinks` for `level`. `None` if any cannot be placed.
    fn slowest<'a>(
        &self,
        sinks: impl IntoIterator<Item = &'a SinkName>,
        level: FenceLevel,
    ) -> Option<SourcePosition> {
        let mut slowest: Option<SourcePosition> = None;
        let mut any = false;
        for sink in sinks {
            let reached = self.reached(sink, level)?;
            slowest = Some(slowest.map_or(reached, |low| low.min(reached)));
            any = true;
        }
        if any { slowest } else { None }
    }

    /// Wait until the requested sink (or every sink) has reached the requested
    /// position at the requested level, or the timeout passes.
    ///
    /// Returns at once when the position is already reached, and `Err` at once
    /// when the wait cannot start: an unreadable position, an unknown sink, or
    /// a source whose live stream is not open yet.
    pub async fn wait(&self, request: &FenceRequest) -> Result<FenceOutcome, FenceError> {
        let source = &self.inner.source;
        let target = source
            .parse_position(&request.position)
            .map_err(|error| FenceError::InvalidPosition(error.to_string()))?;
        let sinks: Vec<&SinkName> = match &request.sink {
            Some(sink) => vec![
                self.inner
                    .sinks
                    .get_key_value(sink)
                    .map(|(name, _)| name)
                    .ok_or_else(|| FenceError::UnknownSink(sink.to_string()))?,
            ],
            None => self.inner.sinks.keys().collect(),
        };
        let deadline = Instant::now() + request.timeout;
        loop {
            // Registered before the check, so a move between the check and the
            // sleep still wakes this waiter.
            let moved = self.inner.moved.notified();
            tokio::pin!(moved);
            moved.as_mut().enable();

            let Some(reached) = self.slowest(sinks.iter().copied(), request.level) else {
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

    /// Where every sink is right now, for a status surface.
    pub fn positions(&self) -> FencePositions {
        let format = |position: Option<SourcePosition>| {
            position.map(|p| self.inner.source.format_position(p))
        };
        let names = || self.inner.sinks.keys();
        FencePositions {
            applied: format(self.slowest(names(), FenceLevel::Applied)),
            searchable: format(self.slowest(names(), FenceLevel::Searchable)),
            sinks: names()
                .map(|sink| {
                    (
                        sink.clone(),
                        SinkPositions {
                            applied: format(self.reached(sink, FenceLevel::Applied)),
                            searchable: format(self.reached(sink, FenceLevel::Searchable)),
                        },
                    )
                })
                .collect(),
        }
    }
}

fn lock(progress: &Mutex<Progress>) -> std::sync::MutexGuard<'_, Progress> {
    progress.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Feeds a [`Fence`] from the sink engines' position events.
#[derive(Debug)]
pub struct FenceObserver {
    fence: Fence,
}

impl FenceObserver {
    pub fn new(fence: Fence) -> Self {
        Self { fence }
    }
}

impl Observer for FenceObserver {
    fn on_position_applied(&self, sink: &SinkName, position: Position) {
        self.fence.record_applied(sink, position);
    }

    fn on_position_searchable(&self, sink: &SinkName, position: Position) {
        self.fence.record_searchable(sink, position);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
