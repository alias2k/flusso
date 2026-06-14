//! Pipeline observability — a backend-neutral [`Observer`] the engine reports
//! lifecycle and progress events to.
//!
//! The engine emits events at the transitions and boundaries it already has
//! (indexes ensured, backfill phases, live start, each change captured, each
//! batch committed, errors). It depends only on this trait, never on a concrete
//! metrics or status backend — exactly the trait-object discipline the rest of
//! the pipeline uses for sources and sinks. A consumer (the `daemon` crate)
//! implements [`Observer`] to fan these events out to metrics, a live status
//! surface, or anything else.
//!
//! Methods are **synchronous and cheap** (`&self`, no `await`): they run inline
//! on the pipeline's hot path, so an implementation must do only fast,
//! non-blocking work (bump a counter, update an atomic). Anything slower belongs
//! on a separate task fed by what the observer records. Every method has a
//! no-op default so an implementation overrides only what it cares about, and
//! [`NoopObserver`] is the default when none is set.

use std::sync::Arc;
use std::time::Duration;

use schema_core::IndexName;

/// What one committed batch did — reported to [`Observer::on_batch_committed`].
#[derive(Debug, Clone)]
pub struct BatchStats {
    /// Changes buffered into this batch (what [`BatchPolicy`](crate::BatchPolicy)
    /// caps).
    pub changes: usize,
    /// Distinct documents the batch built and wrote — `<= changes` after the
    /// per-batch dedup. Equals the sum of [`documents_by_index`](Self::documents_by_index).
    pub documents: usize,
    /// Documents built per target index, for per-index metrics. One entry per
    /// index the batch touched.
    pub documents_by_index: Vec<(IndexName, usize)>,
    /// How long the [`flush`](sinks_core::Sink::flush) that made the batch
    /// durable took.
    pub flush: Duration,
}

/// A sink for the engine's lifecycle and progress events.
///
/// See the [module docs](self) for the hot-path contract. All methods default
/// to no-ops.
pub trait Observer: std::fmt::Debug + Send + Sync {
    /// The target indexes have been ensured at the sink (`count` of them),
    /// before any documents flow.
    fn on_indexes_ensured(&self, count: usize) {
        let _ = count;
    }

    /// Backfill is starting for `indexes` (those the sink reported unseeded).
    fn on_backfill_started(&self, indexes: &[IndexName]) {
        let _ = indexes;
    }

    /// `index`'s backfill is complete and it has been marked seeded.
    fn on_index_seeded(&self, index: &IndexName) {
        let _ = index;
    }

    /// The backfill phase finished (all unseeded indexes seeded), or was skipped.
    fn on_backfill_completed(&self) {}

    /// Live capture has started; the pipeline is now following ongoing changes.
    fn on_live_started(&self) {}

    /// One change was pulled from the source into the queue.
    fn on_change_captured(&self) {}

    /// A batch was built, flushed, and acked. See [`BatchStats`].
    fn on_batch_committed(&self, stats: BatchStats) {
        let _ = stats;
    }

    /// The source's capture lag, in bytes behind the latest position — e.g. a
    /// replication slot's distance from the server's current WAL. Reported by
    /// whoever polls [`ChangeCapture::lag`](sources_core::cdc::ChangeCapture::lag),
    /// not by the engine loop itself.
    fn on_slot_lag(&self, bytes: u64) {
        let _ = bytes;
    }

    /// A document was **quarantined**: the sink rejected it at the item level
    /// and the engine's failure policy is to skip and continue (see
    /// [`FailurePolicy::Skip`](crate::FailurePolicy)). The document is not
    /// applied and the batch proceeds, so it is *not* redelivered — this is the
    /// signal to surface it (a metric, a log, a dead-letter record). `index` and
    /// `id` are the destination's names for it; `reason` is why it was rejected.
    fn on_document_quarantined(&self, index: &str, id: &str, reason: &str) {
        let _ = (index, id, reason);
    }

    /// The pipeline stopped on an error (rendered to a string, since the engine's
    /// error type is not part of this neutral surface).
    fn on_error(&self, error: &str) {
        let _ = error;
    }
}

/// The default [`Observer`]: every event is dropped. Used when an engine is run
/// without [`with_observer`](crate::Engine::with_observer).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl Observer for NoopObserver {}

/// An [`Observer`] that forwards every event to several observers in turn.
///
/// The engine drives a single observer, so this composes many — e.g. one that
/// updates a status surface and one that records metrics — without the engine
/// knowing how many there are. Mirrors [`FanOutSink`](sinks_core::FanOutSink).
#[derive(Debug, Default)]
pub struct FanOut {
    observers: Vec<Arc<dyn Observer>>,
}

impl FanOut {
    /// Compose the given observers; each receives every event, in order.
    pub fn new(observers: Vec<Arc<dyn Observer>>) -> Self {
        Self { observers }
    }
}

impl Observer for FanOut {
    fn on_indexes_ensured(&self, count: usize) {
        for observer in &self.observers {
            observer.on_indexes_ensured(count);
        }
    }

    fn on_backfill_started(&self, indexes: &[IndexName]) {
        for observer in &self.observers {
            observer.on_backfill_started(indexes);
        }
    }

    fn on_index_seeded(&self, index: &IndexName) {
        for observer in &self.observers {
            observer.on_index_seeded(index);
        }
    }

    fn on_backfill_completed(&self) {
        for observer in &self.observers {
            observer.on_backfill_completed();
        }
    }

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

    fn on_batch_committed(&self, stats: BatchStats) {
        for observer in &self.observers {
            observer.on_batch_committed(stats.clone());
        }
    }

    fn on_slot_lag(&self, bytes: u64) {
        for observer in &self.observers {
            observer.on_slot_lag(bytes);
        }
    }

    fn on_document_quarantined(&self, index: &str, id: &str, reason: &str) {
        for observer in &self.observers {
            observer.on_document_quarantined(index, id, reason);
        }
    }

    fn on_error(&self, error: &str) {
        for observer in &self.observers {
            observer.on_error(error);
        }
    }
}
