//! The metrics-recording [`Observer`]: engine events → OpenTelemetry
//! instruments on the global meter. Names, labels, and buckets live here
//! because they're a presentation choice; the engines only emit events.
//!
//! Every sink-side instrument carries a `sink` label, so a stalled or failing
//! sink is visible on its own.

use daemon::{BuildStats, CommitStats, EngineId, IndexName, Observer, SinkName};
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Gauge, Histogram};

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
                .with_description("Changes pulled from the source by the ingest engine")
                .build(),
            changes_committed: meter
                .u64_counter("flusso.changes.committed")
                .with_description("Changes whose batch a sink has flushed and acknowledged")
                .build(),
            documents_built: meter
                .u64_counter("flusso.documents.built")
                .with_description(
                    "Documents built by the ingest engine and published to every lane",
                )
                .build(),
            batches: meter
                .u64_counter("flusso.batches")
                .with_description("Batches flushed by a sink")
                .build(),
            indexes_seeded: meter
                .u64_counter("flusso.indexes.seeded")
                .with_description("Indexes whose backfill completed on a sink this run")
                .build(),
            documents_quarantined: meter
                .u64_counter("flusso.documents.quarantined")
                .with_description(
                    "Documents a sink rejected and its engine skipped (on_error = skip). \
                     A non-zero value means data is being dropped — alert on it.",
                )
                .build(),
            errors: meter
                .u64_counter("flusso.errors")
                .with_description("Errors that stopped an engine (the daemon restarts it)")
                .build(),
            flush_duration: meter
                .f64_histogram("flusso.flush.duration")
                .with_unit("s")
                .with_description("Time taken by each sink flush")
                .build(),
            slot_lag: meter
                .u64_gauge("flusso.replication.slot_lag")
                .with_unit("By")
                .with_description("Bytes the source's resume point trails its latest position by")
                .build(),
            indexes: meter
                .u64_gauge("flusso.indexes")
                .with_description("Target indexes ensured at a sink")
                .build(),
        }
    }
}

fn sink_label(sink: &SinkName) -> KeyValue {
    KeyValue::new("sink", sink.as_ref().to_owned())
}

impl Observer for OtelObserver {
    fn on_change_captured(&self) {
        self.changes_captured.add(1, &[]);
    }

    fn on_batch_built(&self, stats: BuildStats) {
        for (index, count) in &stats.documents_by_index {
            self.documents_built.add(
                *count as u64,
                &[KeyValue::new("index", index.as_ref().to_owned())],
            );
        }
    }

    fn on_indexes_ensured(&self, sink: &SinkName, count: usize) {
        self.indexes.record(count as u64, &[sink_label(sink)]);
    }

    fn on_index_seeded(&self, sink: &SinkName, index: &IndexName) {
        self.indexes_seeded.add(
            1,
            &[
                sink_label(sink),
                KeyValue::new("index", index.as_ref().to_owned()),
            ],
        );
    }

    fn on_batch_committed(&self, sink: &SinkName, stats: CommitStats) {
        let label = [sink_label(sink)];
        self.changes_committed.add(stats.changes as u64, &label);
        self.batches.add(1, &label);
        self.flush_duration
            .record(stats.flush.as_secs_f64(), &label);
    }

    fn on_document_quarantined(&self, sink: &SinkName, index: &str, _id: &str, _reason: &str) {
        self.documents_quarantined.add(
            1,
            &[sink_label(sink), KeyValue::new("index", index.to_owned())],
        );
    }

    fn on_slot_lag(&self, bytes: u64) {
        self.slot_lag.record(bytes, &[]);
    }

    fn on_engine_error(&self, engine: &EngineId, _error: &str) {
        let label = match engine {
            EngineId::Ingest => KeyValue::new("engine", "ingest"),
            EngineId::Sink(sink) => KeyValue::new("engine", format!("sink:{sink}")),
        };
        self.errors.add(1, &[label]);
    }
}
