#![doc = include_str!("../README.md")]

// The pipeline benchmark (in `benches/`) pulls a concrete source and sink as
// dev-dependencies the unit-test build doesn't touch; allow that only under
// `cfg(test)` — the normal build still enforces unused dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod error;
mod observer;
mod pipeline;
mod policy;

#[cfg(test)]
mod tests;

pub use error::*;
pub use observer::*;
pub use policy::{BatchPolicy, FailurePolicies, FailurePolicy};

use std::sync::Arc;

use sinks_core::Sink;
use sources_core::cdc::ChangeCapture;
use sources_core::document::DocumentBuilder;

use crate::pipeline::{Pipeline, run_inner};

/// Pending changes buffered between capture and the worker.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Drives changes from a source through to a sink.
#[derive(Debug)]
pub struct Engine {
    source: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    observer: Arc<dyn Observer>,
    queue_capacity: usize,
    batch: BatchPolicy,
    skip_backfill: bool,
    failure_policies: FailurePolicies,
}

impl Engine {
    pub fn new(
        source: Arc<dyn ChangeCapture>,
        documents: Arc<dyn DocumentBuilder>,
        sink: Arc<dyn Sink>,
    ) -> Self {
        Self {
            source,
            documents,
            sink,
            observer: Arc::new(NoopObserver),
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            batch: BatchPolicy::default(),
            skip_backfill: false,
            failure_policies: FailurePolicies::default(),
        }
    }

    /// Report lifecycle and progress events to `observer` (metrics, a live
    /// status surface, …). Defaults to [`NoopObserver`]. See [`Observer`].
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// Set how many changes may buffer between capture and the worker.
    pub fn with_queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity.max(1);
        self
    }

    /// Set how the worker groups changes into one sink flush (see
    /// [`BatchPolicy`]). `max_changes` is clamped to at least 1.
    pub fn with_batch(mut self, batch: BatchPolicy) -> Self {
        self.batch = BatchPolicy {
            max_changes: batch.max_changes.max(1),
            ..batch
        };
        self
    }

    /// Force-skip the backfill phase entirely, regardless of what the sink
    /// reports. An escape hatch for sinks that can't persist seeded-state (so
    /// they would otherwise re-seed every run) or to resume without re-checking.
    pub fn skip_backfill(mut self, skip: bool) -> Self {
        self.skip_backfill = skip;
        self
    }

    /// Set how the engine resolves the policy for documents a sink rejects at
    /// the item level (see [`FailurePolicies`]). Defaults to
    /// [`FailurePolicy::Stop`] for every index.
    pub fn with_failure_policies(mut self, policies: FailurePolicies) -> Self {
        self.failure_policies = policies;
        self
    }

    /// Run until the live change stream ends or an error stops the pipeline.
    ///
    /// First seeds any unseeded index (unless [`skip_backfill`](Self::skip_backfill)
    /// is set), then follows live changes.
    #[tracing::instrument(
        name = "engine.run",
        skip_all,
        fields(
            skip_backfill = self.skip_backfill,
            queue_capacity = self.queue_capacity,
            max_changes = self.batch.max_changes,
            max_delay_ms = self.batch.max_delay.as_millis() as u64,
        ),
    )]
    pub async fn run(self) -> Result<()> {
        let Engine {
            source,
            documents,
            sink,
            observer,
            queue_capacity,
            batch,
            skip_backfill,
            failure_policies,
        } = self;
        let pipeline = Pipeline {
            documents: documents.as_ref(),
            sink: sink.as_ref(),
            observer: &observer,
            queue_capacity,
            batch,
            failure_policies: &failure_policies,
        };
        let result = run_inner(pipeline, source.as_ref(), skip_backfill).await;
        if let Err(error) = &result {
            observer.on_error(&error.to_string());
        }
        result
    }
}
