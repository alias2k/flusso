//! The progress trait both engines report through: sync, cheap, no-op by
//! default. The daemon fans it out to a live status surface and to whatever
//! metrics observer the binary attaches; the engines depend on the trait only.
//!
//! Every sink-side event names the sink, so status and metrics carry a sink
//! dimension. Ingest-side events have no sink: they describe the one build
//! path every lane is fed from.
//!
//! **Where time goes** is one event, [`Observer::on_stage`]: a [`Stage`] by
//! what it is (resolve, build, publish, wait for a lane, apply, flush, wait
//! for an item), with the sink and index it belongs to where the engine knows
//! them. It is reported once per batch or lane item, never per change or per
//! document, so it costs an `Instant::now()` pair per batch. Start-up is
//! [`Observer::on_startup_step`], once per step.
//!
//! ```
//! use std::time::Duration;
//! use engine::{Observer, Stage, StageTiming, Work};
//!
//! #[derive(Debug)]
//! struct Busy(std::sync::Mutex<Duration>);
//!
//! impl Observer for Busy {
//!     fn on_stage(&self, timing: StageTiming<'_>) {
//!         if timing.stage == Stage::Build {
//!             *self.0.lock().unwrap() += timing.elapsed;
//!         }
//!     }
//! }
//!
//! let busy = Busy(Default::default());
//! busy.on_stage(StageTiming::new(Stage::Build, Duration::from_millis(3)).work(Work::Live));
//! assert_eq!(*busy.0.lock().unwrap(), Duration::from_millis(3));
//! ```

use std::sync::Arc;
use std::time::Duration;

use kernel::{IndexName, Position, SinkName};
use source::document::BuildFailure;

use crate::policy::FailurePolicy;

/// What one ingest commit produced: how many changes it covered, how many
/// documents it built (deduplicated), per index, and how long the build took.
#[derive(Debug, Clone)]
pub struct BuildStats {
    /// Source changes the batch covered; `0` for a snapshot batch.
    pub changes: usize,
    /// Documents built, after deduplication.
    pub documents: usize,
    /// Documents built per index.
    pub documents_by_index: Vec<(IndexName, usize)>,
    /// How long resolving and building took.
    pub build: Duration,
}

/// What one sink engine committed: the batch's envelopes applied and flushed,
/// how many source changes that batch covered, and the flush duration.
#[derive(Debug, Clone)]
pub struct CommitStats {
    /// Envelopes applied to the sink in this batch.
    pub envelopes: usize,
    /// Source changes the batch covered; `0` for a snapshot batch.
    pub changes: usize,
    /// How long the flush took.
    pub flush: Duration,
    /// Live changes or a backfill.
    pub work: Work,
    /// Bytes the flush sent, when the sink reports them.
    pub bytes: Option<u64>,
}

/// A step of the pipeline that takes time, named by what it is. The engines
/// report each through [`Observer::on_stage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Ingest, backfill: waiting for the source to hand over snapshot rows
    /// (a key page). Only the waits are counted: a row already fetched costs
    /// nothing here.
    Read,
    /// Ingest, live: taking the changes the stream has ready and resolving
    /// each to the documents it touches (the reverse lookups).
    Resolve,
    /// Ingest: building a batch's documents (`build_many`) into envelopes.
    Build,
    /// Ingest: publishing a built batch to its lanes, until the last lane took
    /// it. Long when a lane is full.
    Publish,
    /// Ingest, per sink: until that sink's lane took the batch. The longest
    /// of these is [`Publish`](Self::Publish).
    LaneWait,
    /// Sink: idle, waiting for the next lane item.
    Wait,
    /// Sink: applying a batch's envelopes (buffering, rendering).
    Apply,
    /// Sink: flushing a batch to the destination.
    Flush,
}

impl Stage {
    /// Every stage, in pipeline order.
    pub const ALL: [Stage; 8] = [
        Stage::Read,
        Stage::Resolve,
        Stage::Build,
        Stage::Publish,
        Stage::LaneWait,
        Stage::Wait,
        Stage::Apply,
        Stage::Flush,
    ];

    /// The stage's name in metrics and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Read => "read",
            Stage::Resolve => "resolve",
            Stage::Build => "build",
            Stage::Publish => "publish",
            Stage::LaneWait => "lane_wait",
            Stage::Wait => "wait",
            Stage::Apply => "apply",
            Stage::Flush => "flush",
        }
    }
}

/// Which work a timing belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Work {
    /// Live changes from the stream.
    Live,
    /// A backfill (snapshot) batch.
    Backfill,
}

impl Work {
    /// The work's name in metrics and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Work::Live => "live",
            Work::Backfill => "backfill",
        }
    }
}

/// How long one [`Stage`] took, once: for a batch, or a lane item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageTiming<'a> {
    /// What took the time.
    pub stage: Stage,
    /// Live or backfill work; `None` where it is neither (a sink waiting).
    pub work: Option<Work>,
    /// The sink it happened at, for a sink stage or a lane.
    pub sink: Option<&'a SinkName>,
    /// The index it was for, where the stage runs per index.
    pub index: Option<&'a IndexName>,
    /// How long it took.
    pub elapsed: Duration,
}

impl<'a> StageTiming<'a> {
    /// `stage` took `elapsed`, for no particular work, sink or index.
    pub fn new(stage: Stage, elapsed: Duration) -> Self {
        Self {
            stage,
            work: None,
            sink: None,
            index: None,
            elapsed,
        }
    }

    /// For `work`.
    #[must_use]
    pub fn work(mut self, work: Work) -> Self {
        self.work = Some(work);
        self
    }

    /// At `sink`.
    #[must_use]
    pub fn sink(mut self, sink: &'a SinkName) -> Self {
        self.sink = Some(sink);
        self
    }

    /// For `index`.
    #[must_use]
    pub fn index(mut self, index: &'a IndexName) -> Self {
        self.index = Some(index);
        self
    }
}

/// A step between a start and the live stream, timed once per start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StartupStep {
    /// Connecting to the source and validating the indexes against it.
    Source,
    /// Staging one sink: ensuring its indexes, retiring stale seeds, asking for
    /// its backfill. Reported per sink, and again on each re-stage.
    Stage,
    /// Preparing the source's resume point (Postgres: publication, replica
    /// identity, slot).
    Prepare,
    /// Opening the live stream.
    OpenLive,
}

impl StartupStep {
    /// The step's name in metrics and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            StartupStep::Source => "source",
            StartupStep::Stage => "stage",
            StartupStep::Prepare => "prepare",
            StartupStep::OpenLive => "open_live",
        }
    }
}

/// The engine an event or error belongs to: the ingest engine, or one sink's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineId {
    /// The one ingest engine.
    Ingest,
    /// The sink engine of the named sink.
    Sink(SinkName),
}

impl std::fmt::Display for EngineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineId::Ingest => f.write_str("ingest"),
            EngineId::Sink(name) => write!(f, "sink {name}"),
        }
    }
}

/// Lifecycle and progress events. Every method has a no-op default.
pub trait Observer: std::fmt::Debug + Send + Sync {
    /// The ingest engine opened the live stream and is following it.
    fn on_live_started(&self) {}

    /// One stage of the pipeline took `timing.elapsed` (see [`Stage`]).
    /// Reported per batch or lane item, so keep it cheap.
    fn on_stage(&self, timing: StageTiming<'_>) {
        let _ = timing;
    }

    /// A start-up step took `elapsed`; `sink` names the sink a
    /// [`StartupStep::Stage`] staged.
    fn on_startup_step(&self, step: StartupStep, sink: Option<&SinkName>, elapsed: Duration) {
        let _ = (step, sink, elapsed);
    }

    /// One live change was captured into the current batch.
    fn on_change_captured(&self) {}

    /// The ingest engine built one batch and published it to every lane.
    fn on_batch_built(&self, stats: BuildStats) {
        let _ = stats;
    }

    /// A snapshot of `indexes` started, for the lanes that requested it.
    fn on_snapshot_started(&self, indexes: &[IndexName]) {
        let _ = indexes;
    }

    /// The snapshot of `indexes` is fully published.
    fn on_snapshot_completed(&self, indexes: &[IndexName]) {
        let _ = indexes;
    }

    /// The source's resume point trails its latest position by `bytes`.
    fn on_slot_lag(&self, bytes: u64) {
        let _ = bytes;
    }

    /// A sink engine ensured `count` indexes at its destination.
    fn on_indexes_ensured(&self, sink: &SinkName, count: usize) {
        let _ = (sink, count);
    }

    /// A sink engine asked for a snapshot of `indexes` into its lane.
    fn on_backfill_requested(&self, sink: &SinkName, indexes: &[IndexName]) {
        let _ = (sink, indexes);
    }

    /// A sink recorded `index` as seeded.
    fn on_index_seeded(&self, sink: &SinkName, index: &IndexName) {
        let _ = (sink, index);
    }

    /// A sink engine finished staging and is following its lane.
    fn on_sink_started(&self, sink: &SinkName) {
        let _ = sink;
    }

    /// A sink engine applied, flushed, and acknowledged one batch.
    fn on_batch_committed(&self, sink: &SinkName, stats: CommitStats) {
        let _ = (sink, stats);
    }

    /// A sink rejected one document and the `skip` policy left it out.
    fn on_document_quarantined(&self, sink: &SinkName, index: &str, id: &str, reason: &str) {
        let _ = (sink, index, id, reason);
    }

    /// A document could not be built (a script failed on it, or an `array:
    /// true` column held an object). Reported under
    /// either `policy`: `Skip` left it out, so every sink keeps its indexed
    /// version; `Stop` is about to stop the ingest engine.
    fn on_document_failed(
        &self,
        index: &IndexName,
        id: &str,
        failure: &BuildFailure,
        policy: FailurePolicy,
    ) {
        let _ = (index, id, failure, policy);
    }

    /// `sink` flushed every live change up to and including `position`: a
    /// fetch by id at the destination now sees them. Reported as the flush
    /// returns and *before* the batch is acknowledged, so whoever listens
    /// knows of a position before the source can be told it is confirmed. A
    /// redelivered batch can repeat an earlier position; keep the highest.
    fn on_position_applied(&self, sink: &SinkName, position: Position) {
        let _ = (sink, position);
    }

    /// Every live change up to and including `position` can now be *searched*
    /// at `sink`'s destination, as its flushes reported
    /// ([`Visibility`](sink::Visibility)). Trails
    /// [`on_position_applied`](Self::on_position_applied) by however long the
    /// destination takes to refresh. A redelivered batch can repeat an earlier
    /// position; keep the highest.
    fn on_position_searchable(&self, sink: &SinkName, position: Position) {
        let _ = (sink, position);
    }

    /// An engine stopped on an error. The daemon decides whether it restarts.
    fn on_engine_error(&self, engine: &EngineId, error: &str) {
        let _ = (engine, error);
    }

    /// An engine ended its run without an error (its stream or lane closed).
    fn on_engine_stopped(&self, engine: &EngineId) {
        let _ = engine;
    }
}

/// The default: observe nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl Observer for NoopObserver {}

/// Forwards every event to several observers, in order.
#[derive(Debug, Default)]
pub struct FanOut {
    observers: Vec<Arc<dyn Observer>>,
}

impl FanOut {
    /// Forward to `observers`, in this order.
    pub fn new(observers: Vec<Arc<dyn Observer>>) -> Self {
        Self { observers }
    }
}

impl Observer for FanOut {
    fn on_live_started(&self) {
        for observer in &self.observers {
            observer.on_live_started();
        }
    }

    fn on_change_captured(&self) {
        for observer in &self.observers {
            observer.on_change_captured();
        }
    }

    fn on_stage(&self, timing: StageTiming<'_>) {
        for observer in &self.observers {
            observer.on_stage(timing);
        }
    }

    fn on_startup_step(&self, step: StartupStep, sink: Option<&SinkName>, elapsed: Duration) {
        for observer in &self.observers {
            observer.on_startup_step(step, sink, elapsed);
        }
    }

    fn on_batch_built(&self, stats: BuildStats) {
        for observer in &self.observers {
            observer.on_batch_built(stats.clone());
        }
    }

    fn on_snapshot_started(&self, indexes: &[IndexName]) {
        for observer in &self.observers {
            observer.on_snapshot_started(indexes);
        }
    }

    fn on_snapshot_completed(&self, indexes: &[IndexName]) {
        for observer in &self.observers {
            observer.on_snapshot_completed(indexes);
        }
    }

    fn on_slot_lag(&self, bytes: u64) {
        for observer in &self.observers {
            observer.on_slot_lag(bytes);
        }
    }

    fn on_indexes_ensured(&self, sink: &SinkName, count: usize) {
        for observer in &self.observers {
            observer.on_indexes_ensured(sink, count);
        }
    }

    fn on_backfill_requested(&self, sink: &SinkName, indexes: &[IndexName]) {
        for observer in &self.observers {
            observer.on_backfill_requested(sink, indexes);
        }
    }

    fn on_index_seeded(&self, sink: &SinkName, index: &IndexName) {
        for observer in &self.observers {
            observer.on_index_seeded(sink, index);
        }
    }

    fn on_sink_started(&self, sink: &SinkName) {
        for observer in &self.observers {
            observer.on_sink_started(sink);
        }
    }

    fn on_batch_committed(&self, sink: &SinkName, stats: CommitStats) {
        for observer in &self.observers {
            observer.on_batch_committed(sink, stats.clone());
        }
    }

    fn on_document_quarantined(&self, sink: &SinkName, index: &str, id: &str, reason: &str) {
        for observer in &self.observers {
            observer.on_document_quarantined(sink, index, id, reason);
        }
    }

    fn on_document_failed(
        &self,
        index: &IndexName,
        id: &str,
        failure: &BuildFailure,
        policy: FailurePolicy,
    ) {
        for observer in &self.observers {
            observer.on_document_failed(index, id, failure, policy);
        }
    }

    fn on_position_applied(&self, sink: &SinkName, position: Position) {
        for observer in &self.observers {
            observer.on_position_applied(sink, position);
        }
    }

    fn on_position_searchable(&self, sink: &SinkName, position: Position) {
        for observer in &self.observers {
            observer.on_position_searchable(sink, position);
        }
    }

    fn on_engine_error(&self, engine: &EngineId, error: &str) {
        for observer in &self.observers {
            observer.on_engine_error(engine, error);
        }
    }

    fn on_engine_stopped(&self, engine: &EngineId) {
        for observer in &self.observers {
            observer.on_engine_stopped(engine);
        }
    }
}
