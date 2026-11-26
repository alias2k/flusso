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
//! hurries a flush because a client is waiting.
//!
//! **A fence vouches only for what reads see (#379).** Each lane also tracks
//! whether reads through the sink's alias see what the lane writes: not while
//! an index is seeded for the first time (`backfilling`), nor while a fresh
//! generation is built beside the one serving (`rebuilding`: a reindex, or
//! every seeded index after a `Fresh` source). A lane mid-reindex still
//! vouches for what the serving generation held when the rebuild began. A
//! wait that needs more answers [`FenceOutcome::Unserved`] at its timeout,
//! unless the seed lands first. The sink engines' events set the state
//! ([`FenceObserver`]); they are a few per index per run.
//!
//! **An unread lane vouches for nothing (ADR 0012 stage 10, O29).** A lane
//! answers `starting` from the moment a new stream may answer until its own
//! records were read under that stream; `gap`, `detached`, `rebuilding` and
//! `backfilling` still take precedence as themselves. [`Rereads::open`] (the
//! one place the ingest engine opens its stream) marks every lane unread
//! before the stream can answer. Guard: the property test `gate_property`.
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
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::Duration;

use engine::{Level, Observer, Progress, Reread, Rereads};
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
    /// The timeout passed while some lane the wait needs serves reads from
    /// elsewhere than it writes: it can't be vouched for until it is seeded.
    /// Or, at once, a lane the wait needs is detached and froze short of the
    /// position (ADR 0012 stage 10). `lanes` are those lanes; `position` is
    /// how far the wait got, empty when no lane can say (a lane detached
    /// before this process started).
    Unserved {
        position: String,
        lanes: Vec<(LaneKey, Unserved)>,
    },
    /// At once: the position falls inside changes a reattach accepted as
    /// missing on some lane the wait needs (`flusso reattach --accept-gap`,
    /// ADR 0012 stage 10, S3). Its index never got them; no wait changes
    /// that. `lanes` are those lanes, each with its gap.
    InGap {
        position: String,
        lanes: Vec<(LaneKey, FenceGap)>,
    },
}

/// The changes a reattach accepted as missing on a lane, in the source's own
/// text: none after `from` and through `to` reached its index. An end that
/// is `None` is unknown, and stretches the gap that way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FenceGap {
    /// Where reads of the index stood at the detach.
    pub from: Option<String>,
    /// Where the lane was attached again.
    pub to: Option<String>,
}

/// Why a lane can't vouch for its writes yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unserved {
    /// Its sink engine hasn't staged it yet.
    Starting,
    /// It is being seeded for the first time (or its index was recreated
    /// empty): reads see it filling.
    Backfilling,
    /// A fresh generation is being built while reads see the old one.
    Rebuilding,
    /// Detached (ADR 0012 stage 10): reads are frozen at the detach, and a
    /// wait past it is answered at once.
    Detached,
}

impl Unserved {
    /// The reason on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Unserved::Starting => "starting",
            Unserved::Backfilling => "backfilling",
            Unserved::Rebuilding => "rebuilding",
            Unserved::Detached => "detached",
        }
    }
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
    /// Whether each sink read its detach records since the slot last
    /// opened: a lane of one that didn't vouches for nothing (O29). Unset,
    /// every sink counts as read.
    rereads: OnceLock<Arc<Rereads>>,
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
/// already reached. Beside them, what reads of the lane's index see.
#[derive(Debug, Default)]
struct Furthest {
    applied: Option<SourcePosition>,
    searchable: Option<SourcePosition>,
    serving: Serving,
    /// The changes each reattach accepted as missing, `(from, to)`: a
    /// position after a gap's `from` and at or before its `to` is refused at
    /// once; one between two gaps is vouched as usual. An end that is `None`
    /// is unknown and stretches its gap that way. Gone at the seed.
    gaps: Vec<(Option<SourcePosition>, Option<SourcePosition>)>,
}

impl Furthest {
    /// The first of the lane's gaps `target` falls inside.
    fn gap_holding(
        &self,
        target: SourcePosition,
    ) -> Option<(Option<SourcePosition>, Option<SourcePosition>)> {
        self.gaps.iter().copied().find(|(from, to)| {
            from.is_none_or(|from| target > from) && to.is_none_or(|to| target <= to)
        })
    }
}

/// What reads of a lane's index see, set by the sink engine's events.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Serving {
    /// Not staged yet.
    #[default]
    Pending,
    /// Every write the lane applied.
    Writes,
    /// A partial index, being seeded.
    Backfilling,
    /// The old generation, exactly as it stood at `through` (applied,
    /// searchable), or nothing to vouch for.
    Rebuilding {
        through: (Option<SourcePosition>, Option<SourcePosition>),
    },
    /// The index as it stood at the detach, at `through` (applied,
    /// searchable): nothing since reaches it. `None` after a restart.
    Detached {
        through: (Option<SourcePosition>, Option<SourcePosition>),
    },
}

impl Serving {
    /// How far reads vouch at `level`, given how far the lane `reached`.
    fn vouches(self, level: FenceLevel, reached: Option<SourcePosition>) -> Option<SourcePosition> {
        match self {
            Serving::Writes => reached,
            Serving::Rebuilding { through } | Serving::Detached { through } => match level {
                FenceLevel::Applied => through.0,
                FenceLevel::Searchable => through.1,
            },
            Serving::Pending | Serving::Backfilling => None,
        }
    }

    fn unserved(self) -> Option<Unserved> {
        match self {
            Serving::Writes => None,
            Serving::Pending => Some(Unserved::Starting),
            Serving::Backfilling => Some(Unserved::Backfilling),
            Serving::Rebuilding { .. } => Some(Unserved::Rebuilding),
            Serving::Detached { .. } => Some(Unserved::Detached),
        }
    }
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
            rereads: OnceLock::new(),
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

    /// Vouch for no lane of a sink that hasn't read its detach records since
    /// the slot last opened (ADR 0012 stage 10, O29): the `rereads` its
    /// engines and the ingest engine share. Set once.
    pub fn with_rereads(self, rereads: Arc<Rereads>) -> Self {
        let listener: Weak<Inner> = Arc::downgrade(&self.inner);
        rereads.subscribe(Arc::new(move |_: Reread| {
            if let Some(inner) = listener.upgrade() {
                inner.moved.notify_waiters();
            }
        }));
        let _ = self.inner.rereads.set(rereads);
        self
    }

    /// A sink reported progress: every waiter checks again.
    fn moved(&self) {
        self.inner.moved.notify_waiters();
    }

    /// Set what reads of `lane` see, from what they saw, and wake the waiters.
    fn serve(&self, lane: &LaneKey, next: impl FnOnce(&Furthest) -> Serving) {
        let Some(furthest) = self.inner.lanes.get(lane) else {
            return;
        };
        {
            let mut furthest = lock(furthest);
            furthest.serving = next(&furthest);
        }
        self.moved();
    }

    /// `lane`'s sink is about to build a fresh generation of its index. A
    /// lane that served its writes keeps vouching for what it had reached:
    /// the serving generation gets nothing new from here on.
    fn rebuilding(&self, lane: &LaneKey) {
        // Both levels brought up to date first, so the frozen `through` is
        // what the lane had reached when the rebuild began.
        self.reached(lane, FenceLevel::Searchable);
        self.serve(lane, |furthest| Serving::Rebuilding {
            through: match furthest.serving {
                Serving::Writes => (furthest.applied, furthest.searchable),
                Serving::Rebuilding { through } | Serving::Detached { through } => through,
                Serving::Pending | Serving::Backfilling => (None, None),
            },
        });
    }

    /// `lane` is detached (ADR 0012 stage 10, R5): reads stay as they were,
    /// so the lane vouches for what it had reached, frozen, before it leaves
    /// the slot's minimum; a rebuild under way vouches for what it froze.
    /// Heard before the lane leaves the minimum, so `through` is read off its
    /// own answer.
    ///
    /// `recorded` is where the sink recorded reads stood, in the source's
    /// text (S3): the fence vouches no further, so what it refuses after a
    /// reattach starts exactly where it stopped vouching here.
    fn detached(&self, lane: &LaneKey, recorded: Option<&str>) {
        self.reached(lane, FenceLevel::Searchable);
        let recorded = recorded.and_then(|text| self.inner.source.parse_position(text).ok());
        let cap = |at: Option<SourcePosition>| match recorded {
            Some(recorded) => at.map(|at| at.min(recorded)),
            None => at,
        };
        // While the lane's records are unread since the slot opened (a read
        // adopting a detach is one), what it reached may be the new stream's
        // start, past changes another process dropped: it freezes at
        // nothing (S-c).
        let read = self
            .inner
            .rereads
            .get()
            .is_none_or(|rereads| rereads.current(&lane.sink));
        self.serve(lane, |furthest| Serving::Detached {
            through: match furthest.serving {
                Serving::Writes if !read => (None, None),
                Serving::Writes => (cap(furthest.applied), cap(furthest.searchable)),
                Serving::Rebuilding { through } | Serving::Detached { through } => through,
                Serving::Pending | Serving::Backfilling => (None, None),
            },
        });
    }

    /// `lane` carries the gaps reattaches accepted (S3), in the source's
    /// text, the whole list. An end that doesn't parse is taken as unknown:
    /// its gap only grows (the sink engine settles such an end at a start).
    fn gaps(&self, lane: &LaneKey, gaps: &[sink::Gap]) {
        let parse = |text: &Option<String>| {
            text.as_deref()
                .and_then(|text| self.inner.source.parse_position(text).ok())
        };
        let gaps = gaps
            .iter()
            .map(|gap| (parse(&gap.from), parse(&gap.to)))
            .collect();
        if let Some(furthest) = self.inner.lanes.get(lane) {
            lock(furthest).gaps = gaps;
        }
        self.moved();
    }

    /// `lane` is attached again and open (ADR 0012 stage 10, R8): reads see
    /// its writes from here, so it vouches as it reaches. A lane that was not
    /// detached keeps what it served.
    fn attached(&self, lane: &LaneKey) {
        self.serve(lane, |furthest| match furthest.serving {
            Serving::Detached { .. } => Serving::Writes,
            other => other,
        });
    }

    /// A snapshot was requested for `lane`: a rebuild stays one, and so does
    /// a detached lane (its reads stay frozen); anything else is a seed of an
    /// index that reads see filling.
    fn backfilling(&self, lane: &LaneKey) {
        self.serve(lane, |furthest| match furthest.serving {
            kept @ (Serving::Rebuilding { .. } | Serving::Detached { .. }) => kept,
            _ => Serving::Backfilling,
        });
    }

    /// `lane`'s index is seeded: reads see every write it applied, and
    /// every change: a gap a reattach accepted is gone with the rebuild.
    fn seeded(&self, lane: &LaneKey) {
        if let Some(furthest) = self.inner.lanes.get(lane) {
            lock(furthest).gaps.clear();
        }
        self.serve(lane, |_| Serving::Writes);
    }

    /// Whether `target` falls inside a gap `lane` carries; the gap, as text.
    fn gap_holding(&self, lane: &LaneKey, target: SourcePosition) -> Option<FenceGap> {
        let (from, to) = lock(self.inner.lanes.get(lane)?).gap_holding(target)?;
        let format =
            |at: Option<SourcePosition>| at.map(|at| self.inner.source.format_position(at));
        Some(FenceGap {
            from: format(from),
            to: format(to),
        })
    }

    /// `sink` finished staging: its lanes not seeding serve their writes.
    fn started(&self, sink: &SinkName) {
        for lane in self.inner.lanes.keys().filter(|lane| &lane.sink == sink) {
            self.serve(lane, |furthest| match furthest.serving {
                Serving::Pending => Serving::Writes,
                other => other,
            });
        }
    }

    /// How far reads of `lane` vouch at `level`, with why they don't when
    /// they can't; `reached` is how far the lane got.
    fn vouched(
        &self,
        lane: &LaneKey,
        level: FenceLevel,
    ) -> (
        Option<SourcePosition>,
        Option<SourcePosition>,
        Option<Unserved>,
    ) {
        let reached = self.reached(lane, level);
        let read = self
            .inner
            .rereads
            .get()
            .is_none_or(|rereads| rereads.current(&lane.sink));
        let Some(furthest) = self.inner.lanes.get(lane) else {
            return (reached, None, Serving::Pending.unserved());
        };
        let furthest = lock(furthest);
        // O29: an unread record can make only `Writes` false (another
        // process may have detached the lane since), so an unread lane
        // serving its writes vouches for nothing; the other states vouch for
        // nothing new and answer as themselves.
        if !read && furthest.serving == Serving::Writes {
            return (reached, None, Some(Unserved::Starting));
        }
        let serving = furthest.serving;
        (reached, serving.vouches(level, reached), serving.unserved())
    }

    /// How far `lane` is for `level`, in the source's terms. `None` while the
    /// source cannot say.
    ///
    /// Read off what the lane landed (or made searchable), so it keeps moving
    /// over idle WAL. Searchable never runs ahead of applied.
    fn reached(&self, lane: &LaneKey, level: FenceLevel) -> Option<SourcePosition> {
        let mut furthest = lock(self.inner.lanes.get(lane)?);
        // A detached lane is out of the minimum: what it answers now is not
        // what reads of it see.
        if let Serving::Detached { through } = furthest.serving {
            return match level {
                FenceLevel::Applied => through.0,
                FenceLevel::Searchable => through.1,
            };
        }
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

            let mut slowest: Option<SourcePosition> = None;
            let mut vouched: Option<SourcePosition> = None;
            let mut short = false;
            let mut unserved = Vec::new();
            let mut detached = false;
            let in_gap: Vec<(LaneKey, FenceGap)> = lanes
                .iter()
                .filter_map(|lane| Some(((*lane).clone(), self.gap_holding(lane, target)?)))
                .collect();
            if !in_gap.is_empty() {
                return Ok(FenceOutcome::InGap {
                    position: request.position.clone(),
                    lanes: in_gap,
                });
            }
            for lane in lanes.iter().copied() {
                let (reached, vouches, why) = self.vouched(lane, request.level);
                // A detached lane never moves: past what it froze, the wait
                // is answered at once, and it never reads as "not following".
                if why == Some(Unserved::Detached) {
                    if let Some(reached) = reached {
                        slowest = Some(slowest.map_or(reached, |low| low.min(reached)));
                    }
                    match vouches.filter(|vouches| *vouches >= target) {
                        Some(vouches) => {
                            vouched = Some(vouched.map_or(vouches, |low| low.min(vouches)));
                        }
                        None => {
                            detached = true;
                            short = true;
                            unserved.push((lane.clone(), Unserved::Detached));
                        }
                    }
                    continue;
                }
                let Some(reached) = reached else {
                    return Err(FenceError::Unavailable(
                        "the source is not following live changes yet".to_owned(),
                    ));
                };
                slowest = Some(slowest.map_or(reached, |low| low.min(reached)));
                match vouches.filter(|vouches| *vouches >= target) {
                    Some(vouches) => {
                        vouched = Some(vouched.map_or(vouches, |low| low.min(vouches)));
                    }
                    None => {
                        short = true;
                        if let Some(why) = why {
                            unserved.push((lane.clone(), why));
                        }
                    }
                }
            }
            if detached {
                let position = slowest.map_or_else(String::new, |at| source.format_position(at));
                return Ok(FenceOutcome::Unserved {
                    position,
                    lanes: unserved,
                });
            }
            let Some(slowest) = slowest else {
                return Err(FenceError::Unavailable(
                    "the source is not following live changes yet".to_owned(),
                ));
            };
            if !short && let Some(vouched) = vouched {
                let position = source.format_position(vouched);
                return Ok(FenceOutcome::Reached { position });
            }
            if timeout_at(deadline, moved).await.is_err() {
                let position = source.format_position(slowest);
                return Ok(if unserved.is_empty() {
                    FenceOutcome::TimedOut { position }
                } else {
                    FenceOutcome::Unserved {
                        position,
                        lanes: unserved,
                    }
                });
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

/// Wakes a [`Fence`]'s waiters on the sink engines' position events, and
/// tells it what reads of each lane see from their staging, rebuild and seed
/// events. What a sink reached is read from the deployment's `Progress`, not
/// from the event.
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

    fn on_index_rebuilding(&self, sink: &SinkName, index: &IndexName) {
        self.fence
            .rebuilding(&LaneKey::new(index.clone(), sink.clone()));
    }

    fn on_backfill_requested(&self, sink: &SinkName, indexes: &[IndexName]) {
        for index in indexes {
            self.fence
                .backfilling(&LaneKey::new(index.clone(), sink.clone()));
        }
    }

    fn on_index_seeded(&self, sink: &SinkName, index: &IndexName) {
        self.fence
            .seeded(&LaneKey::new(index.clone(), sink.clone()));
    }

    fn on_sink_started(&self, sink: &SinkName) {
        self.fence.started(sink);
    }

    fn on_lane_health(&self, lane: &LaneKey, health: &engine::LaneHealth) {
        if let engine::LaneHealth::Detached { through, .. } = health {
            self.fence.detached(lane, through.as_deref());
        } else if health.is_open() {
            self.fence.attached(lane);
        }
    }

    fn on_lane_gaps(&self, lane: &LaneKey, gaps: &[sink::Gap]) {
        self.fence.gaps(lane, gaps);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
