//! Confirming the stream's watermark to the source.

use std::sync::PoisonError;
use std::sync::atomic::Ordering;

use kernel::Position;
use source::cdc::SourcePosition;
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
    /// Hand the stream's watermark to the source when it moved, and count an
    /// advance past the furthest this engine ever got.
    pub(super) fn confirm(&self, last_confirmed: &mut Option<Position>) {
        let watermark = self.stream.watermark();
        if let Some(watermark) = watermark
            && *last_confirmed != Some(watermark)
        {
            self.source.confirm(watermark);
            *last_confirmed = Some(watermark);
        }
        let reached = self.source.reached(watermark);
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
