//! Position bookkeeping for the live stream: which LSN each emitted position
//! stands for, how far the slot may advance once the engine confirms one, and
//! how far a sink that has processed through a position has *reached* in WAL
//! terms (what a fence compares a client's LSN with).
//!
//! Two facts are tracked:
//!
//! - **entries** — every emitted change's position and its commit LSN, kept
//!   until every sink has confirmed it.
//! - **the horizon** — the LSN through which everything the stream has read is
//!   registered. It moves when the last change of a commit is emitted, and on a
//!   keepalive or a filtered-out commit while nothing is buffered, so it keeps
//!   following the WAL while the watched tables are idle.
//!
//! One question drives both confirmation and fences: with every change through
//! position `p` processed, which LSN is fully covered? Nothing outstanding
//! past `p` → the horizon. Otherwise everything *strictly below* the next
//! outstanding change's commit LSN. Strictly, because the changes of one
//! transaction share its commit LSN: a transaction split across two batches is
//! not covered until its last change is, or a restart would resume past the
//! commit and lose the rest.
//!
//! The two callers stop at different points below that commit:
//!
//! - a **fence** ([`Positions::reached`]) may claim everything short of it,
//!   one byte before its end, so unwatched WAL in between does not hold a
//!   waiter back;
//! - **confirmation** ([`Positions::confirm`]) stops at the end of the last
//!   *covered* commit. The server skips a transaction whose commit record
//!   starts before the confirmed LSN, so the slot must never be advanced into
//!   the middle of an unprocessed commit's record.
//!
//! Confirmation is **cumulative**: confirming `p` advances the resume point
//! and drops the entries at or before `p`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use source::cdc::AdvanceListener;

/// The listeners a capture was handed through `on_advance`, shared with every
/// stream it opens.
#[derive(Default)]
pub(crate) struct Listeners(Mutex<Vec<AdvanceListener>>);

impl std::fmt::Debug for Listeners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Listeners").finish_non_exhaustive()
    }
}

impl Listeners {
    pub(crate) fn add(&self, listener: AdvanceListener) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(listener);
    }

    fn notify(&self) {
        let listeners = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for listener in listeners {
            listener();
        }
    }
}

/// The seq → LSN map for one live stream, shared between the stream (which
/// registers) and the capture (which confirms and answers `reached`).
#[derive(Debug)]
pub(crate) struct Positions {
    inner: Mutex<Inner>,
    listeners: Arc<Listeners>,
}

#[derive(Debug)]
struct Inner {
    next_seq: u64,
    entries: BTreeMap<u64, u64>,
    horizon: u64,
    confirmed_lsn: u64,
}

impl Inner {
    /// The first change still outstanding once everything at or before `seq`
    /// is processed.
    fn next_outstanding(&self, seq: Option<u64>) -> Option<u64> {
        match seq {
            Some(seq) => seq
                .checked_add(1)
                .and_then(|next| self.entries.range(next..).next()),
            None => self.entries.iter().next(),
        }
        .map(|(_, lsn)| *lsn)
    }

    /// What a fence may claim: every commit ending at or before the returned
    /// LSN is processed. With a change outstanding that is everything short
    /// of its commit, whatever unwatched WAL lies in between.
    fn reached(&self, seq: Option<u64>) -> u64 {
        match self.next_outstanding(seq) {
            None => self.horizon.max(self.confirmed_lsn),
            Some(next_lsn) => next_lsn.saturating_sub(1),
        }
    }

    /// What the slot may be advanced to: like [`reached`](Self::reached), but
    /// with a change outstanding it stops at the **end of the last covered
    /// commit**. The server skips a transaction whose commit record *starts*
    /// before the confirmed LSN, so confirming into the outstanding commit's
    /// record would lose it on restart.
    fn confirmable(&self, seq: u64) -> u64 {
        let Some(next_lsn) = self.next_outstanding(Some(seq)) else {
            return self.horizon.max(self.confirmed_lsn);
        };
        self.entries
            .range(..=seq)
            .rev()
            .map(|(_, lsn)| *lsn)
            .find(|lsn| *lsn < next_lsn)
            .unwrap_or(0)
            .max(self.confirmed_lsn)
    }
}

impl Positions {
    /// A map whose resume point and horizon start at `start_lsn`, assigning
    /// positions from `first_seq`. A reopened stream continues from the
    /// previous map's [`next_seq`](Self::next_seq): positions stay monotonic
    /// for the life of the capture, so a watermark the lanes still hold from
    /// before the reopen can never name a change of the new stream.
    pub(crate) fn new(start_lsn: u64, first_seq: u64, listeners: Arc<Listeners>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_seq: first_seq,
                entries: BTreeMap::new(),
                horizon: start_lsn,
                confirmed_lsn: start_lsn,
            }),
            listeners,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Assign the next position to a change committed at `lsn`.
    pub(crate) fn register(&self, lsn: u64) -> u64 {
        let mut inner = self.lock();
        let seq = inner.next_seq;
        inner.next_seq += 1;
        inner.entries.insert(seq, lsn);
        seq
    }

    /// The position the next registered change will get.
    pub(crate) fn next_seq(&self) -> u64 {
        self.lock().next_seq
    }

    /// Everything the stream has read through `lsn` is registered: the last
    /// change of a commit ending there was just emitted, or a keepalive or an
    /// empty commit reported it with nothing buffered.
    pub(crate) fn read_through(&self, lsn: u64) {
        let advanced = {
            let mut inner = self.lock();
            let advanced = lsn > inner.horizon;
            inner.horizon = inner.horizon.max(lsn);
            if inner.entries.is_empty() {
                inner.confirmed_lsn = inner.confirmed_lsn.max(inner.horizon);
            }
            advanced
        };
        if advanced {
            self.listeners.notify();
        }
    }

    /// The LSN fully covered once every change at or before `seq` is processed
    /// (`None`: nothing processed on this stream yet).
    pub(crate) fn reached(&self, seq: Option<u64>) -> u64 {
        self.lock().reached(seq)
    }

    /// The LSN the slot may be advanced to. With no change outstanding it
    /// follows the horizon, so unrelated WAL does not pin retention.
    pub(crate) fn confirmed_lsn(&self) -> u64 {
        self.lock().confirmed_lsn
    }

    /// Every position at or before `seq` is durable downstream.
    pub(crate) fn confirm(&self, seq: u64) {
        let mut inner = self.lock();
        inner.confirmed_lsn = inner.confirmable(seq);
        let Some(after) = seq.checked_add(1) else {
            inner.entries.clear();
            return;
        };
        inner.entries = inner.entries.split_off(&after);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
