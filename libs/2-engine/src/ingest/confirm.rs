//! Confirming to the source how far every lane has landed.

use std::sync::PoisonError;
use std::sync::atomic::Ordering;

use source::cdc::{Landed, SourcePosition};
use tokio::time::Duration;

use super::IngestEngine;

/// How often the engine confirms while no commit happens. Every tick
/// confirms, whether or not anything landed since: the source's answer moves
/// with idle WAL while what landed stays put.
pub(super) const CONFIRM_TICK: Duration = Duration::from_secs(1);

/// The furthest an ingest engine has moved its source, over its life.
#[derive(Debug, Default)]
pub(super) struct Furthest {
    pub(super) covered: Option<u64>,
    pub(super) reached: Option<SourcePosition>,
    /// The last answer was `Free`: every lane detached (ADR 0012 stage 10).
    free: bool,
}

impl IngestEngine {
    /// Hand how far every lane landed to the source, after a commit only when
    /// it changed, on the tick always; and count an advance past the furthest
    /// this engine ever got.
    pub(super) fn confirm(&self, last: &mut Option<Landed>, tick: bool) {
        let landed = self.progress.landed();
        if !tick && *last == Some(landed) {
            return;
        }
        self.source.confirm(landed);
        *last = Some(landed);
        let reached = self.source.reached(landed);
        let covered = landed.covered_seq();
        let mut furthest = self.furthest.lock().unwrap_or_else(PoisonError::into_inner);
        // Free reaches the source's horizon (and covers every position) with
        // no lane behind it. A lane attached again answers from what it holds,
        // further back: measured from there, or no landing on it would count
        // until it passed the horizon the free source had reached.
        let free = landed == Landed::Free;
        if furthest.free && !free {
            furthest.free = false;
            furthest.covered = covered;
            furthest.reached = reached;
            return;
        }
        furthest.free = free;
        // A reopened stream numbers a replayed change afresh, so the stream's
        // own position moves on every replay: the source's answer decides
        // whenever it gives one.
        let moved = match reached {
            Some(_) => reached > furthest.reached,
            None => covered > furthest.covered,
        };
        furthest.covered = furthest.covered.max(covered);
        furthest.reached = furthest.reached.max(reached);
        if moved {
            self.advances.fetch_add(1, Ordering::Relaxed);
        }
    }
}
