//! Position bookkeeping for the live stream: which LSN each emitted position
//! stands for, and how far the slot may advance once the engine confirms one.
//!
//! Confirmation is **cumulative**: confirming position `P` covers every position
//! at or before `P`, because the stream's watermark is the lowest position every
//! sink has acknowledged and lanes are acknowledged in order. So the resume
//! point becomes the highest LSN among the confirmed positions, and the
//! entries at or before `P` are dropped.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

/// The seq → LSN map for one live stream, shared between the stream (which
/// registers) and the capture (which confirms).
#[derive(Debug)]
pub(crate) struct Positions {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    next_seq: u64,
    lsn_by_seq: BTreeMap<u64, u64>,
    confirmed_lsn: u64,
}

impl Positions {
    pub(crate) fn new(start_lsn: u64) -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_seq: 0,
                lsn_by_seq: BTreeMap::new(),
                confirmed_lsn: start_lsn,
            }),
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
        inner.lsn_by_seq.insert(seq, lsn);
        seq
    }

    /// The LSN the slot may be advanced to.
    pub(crate) fn confirmed_lsn(&self) -> u64 {
        self.lock().confirmed_lsn
    }

    /// A position with nothing to deliver (a keepalive or an empty commit at
    /// `lsn`): confirmed in place when no emitted change is outstanding, else
    /// queued behind them so the slot never passes an unconfirmed change.
    pub(crate) fn register_confirmed(&self, lsn: u64) {
        let mut inner = self.lock();
        if inner.lsn_by_seq.is_empty() {
            inner.confirmed_lsn = inner.confirmed_lsn.max(lsn);
        } else {
            let seq = inner.next_seq;
            inner.next_seq += 1;
            inner.lsn_by_seq.insert(seq, lsn);
        }
    }

    /// Every position at or before `seq` is durable downstream.
    pub(crate) fn confirm(&self, seq: u64) {
        let mut inner = self.lock();
        let after = inner.lsn_by_seq.split_off(&(seq + 1));
        let covered = std::mem::replace(&mut inner.lsn_by_seq, after);
        if let Some(highest) = covered.values().max() {
            inner.confirmed_lsn = inner.confirmed_lsn.max(*highest);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
