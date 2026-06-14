//! The daemon's [`Observer`] — it updates the shared [`Status`] from the engine's
//! event stream, and nothing else.
//!
//! Telemetry is deliberately *not* here: the daemon is backend-agnostic, so it
//! depends only on the engine's [`Observer`] trait and its own [`Status`]. The
//! binary attaches whatever metrics/telemetry observer it wants alongside this
//! one (the engine drives a [`FanOut`](engine::FanOut) of both).

use std::sync::Arc;

use engine::{BatchStats, Observer};
use schema_core::IndexName;

use crate::status::{Phase, Status};

/// Updates the shared [`Status`] as the engine reports lifecycle and progress.
/// Cheap and non-blocking, per the [`Observer`] hot-path contract.
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
    fn on_backfill_started(&self, indexes: &[IndexName]) {
        self.status.set_phase(Phase::Backfilling);
        self.status.mark_backfilling(indexes);
    }

    fn on_index_seeded(&self, index: &IndexName) {
        self.status.mark_seeded(index);
    }

    fn on_live_started(&self) {
        self.status.mark_all_seeded();
        self.status.set_phase(Phase::Live);
    }

    fn on_change_captured(&self) {
        self.status.record_capture();
    }

    fn on_batch_committed(&self, stats: BatchStats) {
        self.status.record_commit(
            stats.changes as u64,
            stats.documents as u64,
            stats.flush.as_micros() as u64,
        );
    }

    fn on_document_quarantined(&self, _index: &str, _id: &str, _reason: &str) {
        self.status.record_quarantine();
    }

    fn on_slot_lag(&self, bytes: u64) {
        self.status.record_lag(bytes);
    }

    fn on_error(&self, error: &str) {
        self.status.record_error(error);
        self.status.set_phase(Phase::Stopped);
    }
}
