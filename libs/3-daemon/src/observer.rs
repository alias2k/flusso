//! The [`Observer`] that keeps a [`Status`] current: the daemon's own view of
//! every engine event, serialized by the binary at `/status`.

use std::sync::Arc;

use engine::{BuildStats, CommitStats, EngineId, Observer};
use kernel::{IndexName, SinkName};

use crate::status::{Phase, Status};

/// Updates a shared [`Status`] from the engines' events.
#[derive(Debug)]
pub struct StatusObserver {
    status: Arc<Status>,
}

impl StatusObserver {
    pub fn new(status: Arc<Status>) -> Self {
        Self { status }
    }
}

impl Observer for StatusObserver {
    fn on_live_started(&self) {
        self.status.mark_ingest_live();
    }

    fn on_change_captured(&self) {
        self.status.record_capture();
    }

    fn on_batch_built(&self, stats: BuildStats) {
        self.status.record_build(stats.documents as u64);
    }

    fn on_backfill_requested(&self, sink: &SinkName, indexes: &[IndexName]) {
        self.status.mark_backfilling(sink, indexes);
    }

    fn on_index_seeded(&self, sink: &SinkName, index: &IndexName) {
        self.status.mark_seeded(sink, index);
    }

    fn on_sink_started(&self, sink: &SinkName) {
        self.status.mark_sink_started(sink);
    }

    fn on_batch_committed(&self, sink: &SinkName, stats: CommitStats) {
        self.status.record_commit(
            sink,
            stats.changes as u64,
            stats.envelopes as u64,
            stats.flush.as_micros() as u64,
        );
    }

    fn on_document_quarantined(&self, sink: &SinkName, _index: &str, _id: &str, _reason: &str) {
        self.status.record_quarantine(sink);
    }

    fn on_slot_lag(&self, bytes: u64) {
        self.status.record_lag(bytes);
    }

    fn on_engine_error(&self, engine: &EngineId, error: &str) {
        self.status.record_error(error);
        match engine {
            EngineId::Ingest => self.status.mark_ingest_failed(),
            EngineId::Sink(sink) => self.status.mark_sink_failed(sink),
        }
    }

    fn on_engine_stopped(&self, engine: &EngineId) {
        match engine {
            EngineId::Ingest => self.status.set_phase(Phase::Stopped),
            EngineId::Sink(sink) => self.status.mark_sink_stopped(sink),
        }
    }
}
