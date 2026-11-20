//! The metrics-recording [`Observer`]: engine events → OpenTelemetry
//! instruments on the global meter. Names, labels, and buckets live here
//! because they're a presentation choice; the engines only emit events.
//!
//! Every sink-side instrument carries a `sink` label, so a stalled or failing
//! sink is visible on its own.
//!
//! Where time goes is `flusso.stage.duration`, one histogram for every
//! [`Stage`](daemon::Stage) labelled by `stage`, `work`, `sink` and `index`
//! where the engine knows them: its `_sum` is the seconds a stage was busy, so
//! `rate(_sum)` is its busy seconds per wall second (the mean number in flight:
//! up to 1 for a stage one task runs one at a time, more once a task runs it
//! concurrently), and its buckets are the per-batch distribution. Start-up is
//! `flusso.startup.duration`, a gauge per step, `live` once per process.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Instant;

use daemon::{
    BuildStats, CommitStats, EngineId, IndexName, Observer, SinkName, StageTiming, StartupStep,
};
use kernel::FailurePolicy;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter};
use opentelemetry::{KeyValue, StringValue};
use source::document::BuildFailure;

use super::script_log::field_label;

#[derive(Debug)]
pub(crate) struct OtelObserver {
    changes_captured: Counter<u64>,
    documents_built: Counter<u64>,
    batches: Counter<u64>,
    indexes_seeded: Counter<u64>,
    backfill_checkpoints: Counter<u64>,
    documents_quarantined: Counter<u64>,
    lane_probes: Counter<u64>,
    script_failures: Counter<u64>,
    errors: Counter<u64>,
    refreshes: Counter<u64>,
    flush_duration: Histogram<f64>,
    flush_documents: Histogram<u64>,
    flush_size: Histogram<u64>,
    stage_duration: Histogram<f64>,
    memory_wait: Histogram<f64>,
    startup_duration: Gauge<f64>,
    slot_lag: Gauge<u64>,
    indexes: Gauge<u64>,
    started: Instant,
    live_recorded: AtomicBool,
    sink_labels: RwLock<BTreeMap<SinkName, KeyValue>>,
}

impl OtelObserver {
    /// Record into the global meter; `started` is when `flusso run` began, so
    /// the `live` start-up step is the whole time to the live stream.
    pub(crate) fn new(started: Instant) -> Self {
        Self::with_meter(&global::meter("flusso"), started)
    }

    pub(crate) fn with_meter(meter: &Meter, started: Instant) -> Self {
        Self {
            started,
            live_recorded: AtomicBool::new(false),
            sink_labels: RwLock::new(BTreeMap::new()),
            changes_captured: meter
                .u64_counter("flusso.changes.captured")
                .with_description("Changes pulled from the source by the ingest engine")
                .build(),
            lane_probes: meter
                .u64_counter("flusso.lane.probes")
                .with_description(
                    "A parked lane's front item written again alone, by how it came back",
                )
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
            backfill_checkpoints: meter
                .u64_counter("flusso.backfill.checkpoints")
                .with_description(
                    "Times a sink recorded how far a lane's backfill durably got, so a restart \
                     resumes after it",
                )
                .build(),
            documents_quarantined: meter
                .u64_counter("flusso.documents.quarantined")
                .with_description(
                    "Documents a sink rejected and its engine skipped (on_error = skip). \
                     A non-zero value means data is being dropped — alert on it.",
                )
                .build(),
            script_failures: meter
                .u64_counter("flusso.script.failures")
                .with_description(
                    "Documents that failed to build (a script failed, or an `array: true` \
                     column held an object), by index, field path (array positions folded \
                     to `[]`) and policy: `skip` left the document at its indexed version, \
                     `stop` stopped the ingest engine. A non-zero value means documents are \
                     going stale — alert on it.",
                )
                .build(),
            errors: meter
                .u64_counter("flusso.errors")
                .with_description("Errors that stopped an engine (the daemon restarts it)")
                .build(),
            refreshes: meter
                .u64_counter("flusso.sink.refreshes")
                .with_description(
                    "Requests a sink sent that refresh the destination (OpenSearch: `_refresh` \
                     calls and `_bulk` requests with `?refresh=true`)",
                )
                .build(),
            flush_duration: meter
                .f64_histogram("flusso.flush.duration")
                .with_unit("s")
                .with_description("Time taken by each sink flush")
                .build(),
            flush_documents: meter
                .u64_histogram("flusso.flush.documents")
                .with_description("Documents in each sink flush")
                .build(),
            flush_size: meter
                .u64_histogram("flusso.flush.size")
                .with_unit("By")
                .with_description("Bytes each sink flush sent, where the sink counts them")
                .build(),
            stage_duration: meter
                .f64_histogram("flusso.stage.duration")
                .with_unit("s")
                .with_description(
                    "Time each pipeline stage took, once per batch, lane item, idle stretch or \
                     operation, by stage (idle, page, resolve, build, publish, lane_wait, \
                     snapshot, write, seed, reindex), work (live, backfill), sink and index \
                     where known. rate(_sum) is busy seconds per wall second: the mean number \
                     in flight, above 1 when a task runs the stage concurrently.",
                )
                .build(),
            memory_wait: meter
                .f64_histogram("flusso.memory.wait")
                .with_unit("s")
                .with_description(
                    "How long a batch the memory budget held back waited before it was \
                     granted, by work (live, backfill) and the event that let it go (released: \
                     a sink acknowledged bytes; landed, change, page; tick: the once-a-second \
                     backstop; settle).",
                )
                .build(),
            startup_duration: meter
                .f64_gauge("flusso.startup.duration")
                .with_unit("s")
                .with_description(
                    "Time the last start spent in each step (source, stage per sink, prepare, \
                     open_live), and from the process start to the first live stream (live)",
                )
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

impl OtelObserver {
    /// The `sink` label, built once per sink: its value is shared, so a label
    /// costs a reference count, not a copy of the name.
    fn sink_label(&self, sink: &SinkName) -> KeyValue {
        if let Some(label) = self
            .sink_labels
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(sink)
        {
            return label.clone();
        }
        let label = KeyValue::new("sink", StringValue::from(Arc::<str>::from(sink.as_ref())));
        self.sink_labels
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(sink.clone(), label.clone());
        label
    }
}

impl Observer for OtelObserver {
    fn on_change_captured(&self) {
        self.changes_captured.add(1, &[]);
    }

    fn on_live_started(&self) {
        // Once per process: a restarted ingest engine opens the stream again,
        // and the time since the process started would read as a slow start.
        if self.live_recorded.swap(true, Ordering::Relaxed) {
            return;
        }
        self.startup_duration.record(
            self.started.elapsed().as_secs_f64(),
            &[KeyValue::new("step", "live")],
        );
    }

    fn on_budget_wait(&self, work: daemon::Work, wake: daemon::Wake, waited: std::time::Duration) {
        self.memory_wait.record(
            waited.as_secs_f64(),
            &[
                KeyValue::new("work", work.as_str()),
                KeyValue::new("wake", wake.as_str()),
            ],
        );
    }

    fn on_stage(&self, timing: StageTiming<'_>) {
        let mut labels = Vec::with_capacity(4);
        labels.push(KeyValue::new("stage", timing.stage.as_str()));
        if let Some(work) = timing.work {
            labels.push(KeyValue::new("work", work.as_str()));
        }
        if let Some(sink) = timing.sink {
            labels.push(self.sink_label(sink));
        }
        if let Some(index) = timing.index {
            labels.push(KeyValue::new("index", index.as_ref().to_owned()));
        }
        self.stage_duration
            .record(timing.elapsed.as_secs_f64(), &labels);
    }

    fn on_startup_step(
        &self,
        step: StartupStep,
        sink: Option<&SinkName>,
        elapsed: std::time::Duration,
    ) {
        let mut labels = vec![KeyValue::new("step", step.as_str())];
        if let Some(sink) = sink {
            labels.push(self.sink_label(sink));
        }
        self.startup_duration.record(elapsed.as_secs_f64(), &labels);
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
        self.indexes.record(count as u64, &[self.sink_label(sink)]);
    }

    fn on_index_seeded(&self, sink: &SinkName, index: &IndexName) {
        self.indexes_seeded.add(
            1,
            &[
                self.sink_label(sink),
                KeyValue::new("index", index.as_ref().to_owned()),
            ],
        );
    }

    fn on_walk_recorded(&self, lane: &stream::LaneKey) {
        self.backfill_checkpoints.add(
            1,
            &[
                self.sink_label(&lane.sink),
                KeyValue::new("index", lane.index.as_ref().to_owned()),
            ],
        );
    }

    fn on_batch_committed(&self, sink: &SinkName, stats: CommitStats) {
        let label = [self.sink_label(sink)];
        self.batches.add(1, &label);
        self.flush_duration
            .record(stats.flush.as_secs_f64(), &label);
        // One denominator for both: a flush that wrote nothing (a live
        // batch that resolved to no document) is in neither.
        if stats.envelopes == 0 {
            return;
        }
        let sized = [
            self.sink_label(sink),
            KeyValue::new("work", stats.work.as_str()),
        ];
        self.flush_documents.record(stats.envelopes as u64, &sized);
        if let Some(bytes) = stats.bytes {
            self.flush_size.record(bytes, &sized);
        }
    }

    fn on_refreshes(&self, sink: &SinkName, refreshes: u64) {
        self.refreshes.add(refreshes, &[self.sink_label(sink)]);
    }

    fn on_document_quarantined(&self, sink: &SinkName, index: &str, _id: &str, _reason: &str) {
        self.documents_quarantined.add(
            1,
            &[
                self.sink_label(sink),
                KeyValue::new("index", index.to_owned()),
            ],
        );
    }

    fn on_lane_probe(&self, lane: &stream::LaneKey, outcome: daemon::ProbeOutcome) {
        self.lane_probes.add(
            1,
            &[
                KeyValue::new("index", lane.index.as_ref().to_owned()),
                self.sink_label(&lane.sink),
                KeyValue::new("outcome", outcome.label()),
            ],
        );
    }

    fn on_document_failed(
        &self,
        index: &IndexName,
        _id: &str,
        failure: &BuildFailure,
        policy: FailurePolicy,
    ) {
        self.script_failures.add(
            1,
            &[
                KeyValue::new("index", index.as_ref().to_owned()),
                KeyValue::new("field", field_label(&failure.field)),
                KeyValue::new("policy", policy_label(policy)),
            ],
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

fn policy_label(policy: FailurePolicy) -> &'static str {
    match policy {
        FailurePolicy::Stop => "stop",
        FailurePolicy::Skip => "skip",
    }
}

#[cfg(test)]
mod tests;
