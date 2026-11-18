//! Confirming the stream's watermark to the source.

use std::sync::PoisonError;
use std::sync::atomic::Ordering;

use kernel::Position;
use source::cdc::{Landed, SourcePosition};
use tokio::time::Duration;

use super::IngestEngine;

/// How often the watermark is confirmed to the source while no commit happens,
/// so acknowledgements that arrive during a quiet period still advance it.
pub(super) const CONFIRM_TICK: Duration = Duration::from_secs(1);

/// The furthest an ingest engine has moved its source, over its life.
#[derive(Debug, Default)]
pub(super) struct Furthest {
    pub(super) confirmed: Option<Position>,
    pub(super) reached: Option<SourcePosition>,
}

impl IngestEngine {
    /// Hand how far the lanes landed to the source, and count an
    /// advance past the furthest this engine ever got.
    pub(super) fn confirm(&self, last_confirmed: &mut Option<Position>) {
        let watermark = self.stream.watermark();
        let landed = watermark.map_or(Landed::Idle, Landed::Through);
        // Every time, not only when the watermark moved: with nothing new
        // landed, the source's answer still follows idle WAL.
        self.source.confirm(landed);
        *last_confirmed = watermark.or(*last_confirmed);
        let reached = self.source.reached(landed);
        let mut furthest = self.furthest.lock().unwrap_or_else(PoisonError::into_inner);
        // A reopened stream numbers a replayed change afresh, so the stream's
        // own position moves on every replay: the source's answer decides
        // whenever it gives one.
        let moved = match reached {
            Some(_) => reached > furthest.reached,
            None => watermark > furthest.confirmed,
        };
        furthest.confirmed = furthest.confirmed.max(watermark);
        furthest.reached = furthest.reached.max(reached);
        if moved {
            self.advances.fetch_add(1, Ordering::Relaxed);
        }
    }
}
