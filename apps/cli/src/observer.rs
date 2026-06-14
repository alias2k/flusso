//! The binary's metrics observer — records the engine's events onto
//! OpenTelemetry instruments.
//!
//! This is *telemetry*, so it lives in the binary, not the daemon: the daemon
//! emits backend-agnostic [`Observer`](daemon::Observer) events and updates its
//! own status; this attaches alongside (via [`Daemon::with_observer`](daemon::Daemon::with_observer))
//! to record metrics. The metric names, labels, and units are defined here
//! because they're a presentation choice, like the exporter.
//!
//! `flusso.changes.in_flight` is deliberately *not* here — it's derived from the
//! status, so it's registered as an observable gauge in [`metrics`](crate::metrics)
//! once the status handle exists.

use daemon::{BatchStats, IndexName, Observer};
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Gauge, Histogram};

/// Records pipeline events onto OpenTelemetry instruments built from the global
/// meter. With no meter provider installed, the global meter is a no-op and
/// every instrument records nothing.
#[derive(Debug)]
pub(crate) struct OtelObserver {
    changes_captured: Counter<u64>,
    changes_committed: Counter<u64>,
    documents_built: Counter<u64>,
    batches: Counter<u64>,
    indexes_seeded: Counter<u64>,
    documents_quarantined: Counter<u64>,
    errors: Counter<u64>,
    flush_duration: Histogram<f64>,
    slot_lag: Gauge<u64>,
    indexes: Gauge<u64>,
}

impl OtelObserver {
    pub(crate) fn new() -> Self {
        let meter = global::meter("flusso");
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
            documents_quarantined: meter
                .u64_counter("flusso.documents.quarantined")
                .with_description(
                    "Documents the sink rejected and the engine skipped (on_error = skip). \
                     A non-zero value means data is being dropped — alert on it.",
                )
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
        }
    }
}

impl Observer for OtelObserver {
    fn on_indexes_ensured(&self, count: usize) {
        self.indexes.record(count as u64, &[]);
    }

    fn on_index_seeded(&self, index: &IndexName) {
        self.indexes_seeded
            .add(1, &[KeyValue::new("index", index.as_ref().to_owned())]);
    }

    fn on_change_captured(&self) {
        self.changes_captured.add(1, &[]);
    }

    fn on_batch_committed(&self, stats: BatchStats) {
        self.changes_committed.add(stats.changes as u64, &[]);
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
    }

    fn on_document_quarantined(&self, index: &str, _id: &str, _reason: &str) {
        self.documents_quarantined
            .add(1, &[KeyValue::new("index", index.to_owned())]);
    }

    fn on_slot_lag(&self, bytes: u64) {
        self.slot_lag.record(bytes, &[]);
    }

    fn on_error(&self, _error: &str) {
        self.errors.add(1, &[]);
    }
}
