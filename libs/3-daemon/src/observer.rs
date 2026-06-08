//! The daemon's [`Observer`] — the single consumer of the engine's event stream.
//!
//! It fans every event out two ways at once, which is the whole point of routing
//! both observability layers through one trait:
//!
//! - **metrics** — recorded onto OpenTelemetry instruments built from the global
//!   meter (see [`metrics`](crate::metrics)). They feed whichever readers are
//!   installed — Prometheus (`/metrics`), OTLP push, or none (a no-op).
//! - **status** — the live [`Status`] surface served at `/status`.

use std::sync::Arc;

use engine::{BatchStats, Observer};
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Gauge, Histogram, ObservableGauge};
use schema_core::IndexName;

use crate::status::{Phase, Status};

/// Implements [`Observer`] by updating the shared [`Status`] and recording
/// OpenTelemetry metrics. Cheap and non-blocking, per the [`Observer`] hot-path
/// contract — `add`/`record` on an instrument is a lock-light atomic update.
#[derive(Debug)]
pub struct DaemonObserver {
    status: Arc<Status>,
    changes_captured: Counter<u64>,
    changes_committed: Counter<u64>,
    documents_built: Counter<u64>,
    batches: Counter<u64>,
    indexes_seeded: Counter<u64>,
    errors: Counter<u64>,
    flush_duration: Histogram<f64>,
    slot_lag: Gauge<u64>,
    indexes: Gauge<u64>,
    // In-flight is derived from the captured/committed atomics, so it's an
    // *observable* gauge sampled at collection time rather than pushed on the
    // hot path. This keeps it current even while the sink is stalled (no
    // commits) — exactly when a growing backlog matters most. The handle is
    // retained only to keep its callback registered.
    _in_flight: ObservableGauge<u64>,
}

impl DaemonObserver {
    /// Build the observer, creating its instruments from the global meter. When
    /// no meter provider is installed (metrics off, or under test) the global
    /// meter is a no-op and every instrument records nothing.
    pub fn new(status: Arc<Status>) -> Self {
        let meter = global::meter("flusso");
        let in_flight_status = Arc::clone(&status);
        Self {
            changes_captured: meter
                .u64_counter("flusso.changes.captured")
                .with_description("Changes pulled from the source into the queue")
                .build(),
            changes_committed: meter
                .u64_counter("flusso.changes.committed")
                .with_description("Changes whose documents have been flushed and acked")
                .build(),
            documents_built: meter
                .u64_counter("flusso.documents.built")
                .with_description("Documents assembled and written to the sink")
                .build(),
            batches: meter
                .u64_counter("flusso.batches")
                .with_description("Batches flushed")
                .build(),
            indexes_seeded: meter
                .u64_counter("flusso.indexes.seeded")
                .with_description("Indexes whose backfill completed this run")
                .build(),
            errors: meter
                .u64_counter("flusso.errors")
                .with_description("Errors that stopped the pipeline")
                .build(),
            flush_duration: meter
                .f64_histogram("flusso.flush.duration")
                .with_unit("s")
                .with_description("Time taken by each sink flush")
                .build(),
            slot_lag: meter
                .u64_gauge("flusso.replication.slot_lag")
                .with_unit("By")
                .with_description("Bytes the destination trails the source by")
                .build(),
            indexes: meter
                .u64_gauge("flusso.indexes")
                .with_description("Target indexes ensured at the sink")
                .build(),
            _in_flight: meter
                .u64_observable_gauge("flusso.changes.in_flight")
                .with_description("Captured but not yet committed changes (back-pressure)")
                .with_callback(move |observer| observer.observe(in_flight_status.in_flight(), &[]))
                .build(),
            status,
        }
    }
}

impl Observer for DaemonObserver {
    fn on_indexes_ensured(&self, count: usize) {
        self.indexes.record(count as u64, &[]);
    }

    fn on_backfill_started(&self, indexes: &[IndexName]) {
        self.status.set_phase(Phase::Backfilling);
        self.status.mark_backfilling(indexes);
    }

    fn on_index_seeded(&self, index: &IndexName) {
        self.status.mark_seeded(index);
        self.indexes_seeded
            .add(1, &[KeyValue::new("index", index.as_ref().to_owned())]);
    }

    fn on_live_started(&self) {
        self.status.mark_all_seeded();
        self.status.set_phase(Phase::Live);
    }

    fn on_change_captured(&self) {
        self.status.record_capture();
        self.changes_captured.add(1, &[]);
    }

    fn on_batch_committed(&self, stats: BatchStats) {
        let changes = stats.changes as u64;
        self.status.record_commit(
            changes,
            stats.documents as u64,
            stats.flush.as_micros() as u64,
        );

        self.changes_committed.add(changes, &[]);
        self.batches.add(1, &[]);
        self.flush_duration.record(stats.flush.as_secs_f64(), &[]);
        // Documents are counted per target index; the unlabeled total is the
        // sum across the `index` label.
        for (index, count) in &stats.documents_by_index {
            self.documents_built.add(
                *count as u64,
                &[KeyValue::new("index", index.as_ref().to_owned())],
            );
        }
        // `flusso.changes.in_flight` is an observable gauge — no push here.
    }

    fn on_slot_lag(&self, bytes: u64) {
        self.status.record_lag(bytes);
        self.slot_lag.record(bytes, &[]);
    }

    fn on_error(&self, error: &str) {
        self.status.record_error(error);
        self.status.set_phase(Phase::Stopped);
        self.errors.add(1, &[]);
    }
}
