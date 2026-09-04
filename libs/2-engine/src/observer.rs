//! The progress trait both engines report through: sync, cheap, no-op by
//! default. The daemon fans it out to a live status surface and to whatever
//! metrics observer the binary attaches; the engines depend on the trait only.
//!
//! Every sink-side event names the sink, so status and metrics carry a sink
//! dimension. Ingest-side events have no sink: they describe the one build
//! path every lane is fed from.

use std::sync::Arc;
use std::time::Duration;

use kernel::{IndexName, SinkName};

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
