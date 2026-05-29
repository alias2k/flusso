//! Progress tracking that turns per-event confirmations into a single LSN the
//! slot can advance to.
//!
//! The engine confirms each [`Change`](sources_core::cdc::Change) independently and
//! possibly out of order. The replication slot, though, can only safely advance
//! to a point where *everything before it* is durably processed. So we track a
//! contiguous watermark: each emitted change gets a monotonically increasing
//! sequence number paired with its commit LSN, and the confirmed LSN only moves
//! up to the highest sequence whose predecessors are all confirmed.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use sources_core::cdc::AckSink;

/// Shared, lock-guarded progress. Written from the stream task (registering
/// emitted changes, reading the watermark) and from the engine (confirming).
#[derive(Debug)]
pub(crate) struct AckShared {
    inner: Mutex<AckInner>,
}

#[derive(Debug)]
struct AckInner {
    /// Sequence number to assign to the next emitted change.
    next_seq: u64,
    /// Lowest sequence not yet confirmed — the front of the contiguous run.
    lowest_unconfirmed: u64,
    /// Sequences confirmed ahead of `lowest_unconfirmed`, awaiting the gap to fill.
    confirmed_ahead: BTreeSet<u64>,
    /// Commit LSN of each emitted-but-not-yet-cleared sequence.
    lsn_by_seq: BTreeMap<u64, u64>,
    /// Highest LSN safe to report to the server.
    confirmed_lsn: u64,
}

impl AckShared {
    pub(crate) fn new(start_lsn: u64) -> Self {
        Self {
            inner: Mutex::new(AckInner {
                next_seq: 0,
                lowest_unconfirmed: 0,
                confirmed_ahead: BTreeSet::new(),
                lsn_by_seq: BTreeMap::new(),
                confirmed_lsn: start_lsn,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AckInner> {
        // A poisoned lock means another holder panicked mid-update. The data is
        // still structurally valid for our purposes, so recover rather than
        // propagate a panic (the workspace forbids `unwrap`/`expect`).
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record a change about to be emitted and return its sequence number.
    pub(crate) fn register(&self, lsn: u64) -> u64 {
        let mut inner = self.lock();
        let seq = inner.next_seq;
        inner.next_seq += 1;
        inner.lsn_by_seq.insert(seq, lsn);
        seq
    }

    /// The highest LSN whose every preceding change has been confirmed.
    pub(crate) fn confirmed_lsn(&self) -> u64 {
        self.lock().confirmed_lsn
    }

    /// Confirm one sequence, advancing the watermark across any newly contiguous run.
    fn confirm(&self, seq: u64) {
        let mut inner = self.lock();

        if seq < inner.lowest_unconfirmed {
            return; // already accounted for
        }
        if seq > inner.lowest_unconfirmed {
            inner.confirmed_ahead.insert(seq);
            return;
        }

        // seq == lowest_unconfirmed: clear it and any contiguous successors.
        let mut current = seq;
        loop {
            if let Some(lsn) = inner.lsn_by_seq.remove(&current)
                && lsn > inner.confirmed_lsn
            {
                inner.confirmed_lsn = lsn;
            }
            let next = current + 1;
            inner.lowest_unconfirmed = next;
            if inner.confirmed_ahead.remove(&next) {
                current = next;
            } else {
                break;
            }
        }
    }
}

/// The [`AckSink`] handed to every [`Ack`](sources_core::cdc::Ack). Forwards
/// confirmations to the shared watermark; the stream task does the actual
/// reporting to the server when it next reads the watermark.
#[derive(Debug)]
pub(crate) struct WalAckSink {
    shared: Arc<AckShared>,
}

impl WalAckSink {
    pub(crate) fn new(shared: Arc<AckShared>) -> Self {
        Self { shared }
    }
}

impl AckSink for WalAckSink {
    fn confirm(&self, seq: u64) {
        self.shared.confirm(seq);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn in_order_confirmations_advance_watermark() {
        let s = AckShared::new(0);
        let a = s.register(10);
        let b = s.register(20);
        assert_eq!(s.confirmed_lsn(), 0);
        s.confirm(a);
        assert_eq!(s.confirmed_lsn(), 10);
        s.confirm(b);
        assert_eq!(s.confirmed_lsn(), 20);
    }

    #[test]
    fn out_of_order_confirmation_holds_until_gap_fills() {
        let s = AckShared::new(0);
        let a = s.register(10);
        let b = s.register(20);
        let c = s.register(30);

        s.confirm(c); // gap: a and b still open
        assert_eq!(s.confirmed_lsn(), 0);
        s.confirm(b); // still gated on a
        assert_eq!(s.confirmed_lsn(), 0);
        s.confirm(a); // fills the gap → jumps across b and c
        assert_eq!(s.confirmed_lsn(), 30);
    }

    #[test]
    fn never_regresses_below_start_lsn() {
        let s = AckShared::new(100);
        let a = s.register(50); // a commit at a lower LSN than the start point
        s.confirm(a);
        assert_eq!(s.confirmed_lsn(), 100);
    }
}
