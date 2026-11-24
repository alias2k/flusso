//! Position bookkeeping for the live stream: the stamps each emitted position
//! carries, and how far the slot may advance or a fence may claim once the
//! engine says how far its lanes have landed ([`Landed`]).
//!
//! Nothing is kept per change. A position describes itself, stamped as it is
//! emitted (the visibility hold keeps whole transactions, so the stream knows
//! then whether a change is its commit's last):
//!
//! - `fence_before`: its commit LSN − 1, everything short of its commit;
//! - `confirm_before`: the **horizon** before it, at or before the start of
//!   its commit record;
//! - `confirm_through`: its commit LSN if it is the commit's last change,
//!   else `confirm_before`.
//!
//! The horizon is the LSN through which everything the stream has read is
//! emitted. It moves when the last change of a commit is emitted, and on a
//! keepalive or a filtered-out commit while nothing is buffered, so it keeps
//! following the WAL while the watched tables are idle.
//!
//! | [`Landed`] | confirm to | a fence claims |
//! | --- | --- | --- |
//! | `Before(o)` | `o.confirm_before` | `o.fence_before` |
//! | `Through(p)`, nothing emitted after `p` | the horizon | the horizon |
//! | `Through(p)`, a later change emitted | `p.confirm_through` | `p.confirm_through` |
//! | `Idle`, nothing ever emitted | the horizon | the horizon |
//! | `Idle`, something emitted | no move | the confirmed LSN |
//! | `Free` (no lane needs anything) | the horizon | the horizon |
//!
//! The changes of one transaction share its commit LSN, so a transaction split
//! across batches is neither confirmed nor fenced until its last change has
//! landed. Confirmation stops at the end of the last covered commit: the
//! server skips a transaction whose commit record *starts* before the
//! confirmed LSN. "Nothing emitted after `p`" is the `next_seq` test: a change
//! emitted but not yet routed is in no lane, so only the stream knows it is
//! outstanding.
//!
//! Confirmation is monotonic. A confirmation that moves the resume point wakes
//! the stream ([`Positions::confirmed_moved`]), which hands it to the
//! replication client at once instead of at its next event.

use std::sync::{Arc, Mutex, PoisonError};

use kernel::Position;
use source::cdc::{AdvanceListener, Landed};
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

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

/// The first position a capture assigns. `Idle` follows the horizon only
/// while nothing has been emitted since.
pub(crate) const FIRST_SEQ: u64 = 0;

/// The position bookkeeping of one live stream, shared between the stream
/// (which stamps and moves the horizon) and the capture (which confirms and
/// answers `reached`).
#[derive(Debug)]
pub(crate) struct Positions {
    inner: Mutex<Inner>,
    listeners: Arc<Listeners>,
    confirmed_moved: Notify,
    close: AtomicBool,
    /// Where the stream started: what the slot already holds as confirmed.
    start_lsn: u64,
}

#[derive(Debug)]
struct Inner {
    next_seq: u64,
    horizon: u64,
    confirmed_lsn: u64,
}

impl Inner {
    /// Whether `position` is the last change emitted.
    fn is_last_emitted(&self, position: Position) -> bool {
        position.seq.checked_add(1) == Some(self.next_seq)
    }

    /// Whether the capture has emitted nothing in its life.
    fn emitted_nothing(&self) -> bool {
        self.next_seq == FIRST_SEQ
    }

    /// What a fence may claim: every commit ending at or before the returned
    /// LSN has landed.
    fn reached(&self, landed: Landed) -> u64 {
        let live = self.horizon.max(self.confirmed_lsn);
        match landed {
            Landed::Before(oldest) => oldest.fence_before,
            Landed::Through(last) if self.is_last_emitted(last) => live,
            Landed::Through(last) => last.confirm_through,
            Landed::Idle if self.emitted_nothing() => live,
            Landed::Idle => self.confirmed_lsn,
            Landed::Free => live,
        }
    }

    /// What the slot may be advanced to: never into the record of a commit
    /// that hasn't landed.
    fn confirmable(&self, landed: Landed) -> u64 {
        match landed {
            Landed::Before(oldest) => oldest.confirm_before,
            Landed::Through(last) if self.is_last_emitted(last) => self.horizon,
            Landed::Through(last) => last.confirm_through,
            Landed::Idle if self.emitted_nothing() => self.horizon,
            Landed::Idle => self.confirmed_lsn,
            Landed::Free => self.horizon,
        }
    }
}

impl Positions {
    /// Bookkeeping whose resume point and horizon start at `start_lsn`,
    /// assigning positions from `first_seq`. A reopened stream continues from
    /// the previous one's [`next_seq`](Self::next_seq): positions stay
    /// monotonic for the life of the capture, so a [`Landed`] the engine built
    /// from the previous stream's positions never names a change of the new
    /// one.
    pub(crate) fn new(start_lsn: u64, first_seq: u64, listeners: Arc<Listeners>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_seq: first_seq,
                horizon: start_lsn,
                confirmed_lsn: start_lsn,
            }),
            listeners,
            confirmed_moved: Notify::new(),
            close: AtomicBool::new(false),
            start_lsn,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stamp the next position for a change committed at `lsn`. The last
    /// change of its commit also moves the horizon there: from here on,
    /// everything read through it is emitted.
    pub(crate) fn emit(&self, lsn: u64, last_of_commit: bool) -> Position {
        let (position, advanced) = {
            let mut inner = self.lock();
            let seq = inner.next_seq;
            inner.next_seq += 1;
            let before = inner.horizon;
            let through = if last_of_commit { lsn } else { before };
            let advanced = last_of_commit && lsn > inner.horizon;
            if advanced {
                inner.horizon = lsn;
            }
            (
                Position::stamped(seq, lsn.saturating_sub(1), before, through),
                advanced,
            )
        };
        if advanced {
            self.listeners.notify();
        }
        position
    }

    /// The position the next emitted change will get.
    pub(crate) fn next_seq(&self) -> u64 {
        self.lock().next_seq
    }

    /// Everything the stream has read through `lsn` is emitted: a keepalive
    /// or an empty commit reported it with nothing buffered. It confirms
    /// nothing by itself: the engine's next confirmation does.
    pub(crate) fn read_through(&self, lsn: u64) {
        let advanced = {
            let mut inner = self.lock();
            let advanced = lsn > inner.horizon;
            inner.horizon = inner.horizon.max(lsn);
            advanced
        };
        if advanced {
            self.listeners.notify();
        }
    }

    /// The LSN fully covered once the lanes have landed `landed`.
    pub(crate) fn reached(&self, landed: Landed) -> u64 {
        self.lock().reached(landed)
    }

    /// The LSN the slot may be advanced to.
    pub(crate) fn confirmed_lsn(&self) -> u64 {
        self.lock().confirmed_lsn
    }

    /// The lanes have landed `landed`. Advances the resume point as far as
    /// that covers, never back; wakes
    /// [`confirmed_moved`](Self::confirmed_moved) when it moved.
    pub(crate) fn confirm(&self, landed: Landed) {
        let moved = {
            let mut inner = self.lock();
            let confirmable = inner.confirmable(landed);
            let moved = confirmable > inner.confirmed_lsn;
            if moved {
                inner.confirmed_lsn = confirmable;
            }
            moved
        };
        if moved {
            self.confirmed_moved.notify_one();
        }
    }

    /// Ask the stream to end: report the confirmed position and close, emitting
    /// nothing more. Wakes it like a confirmation.
    pub(crate) fn request_close(&self) {
        self.close.store(true, Ordering::SeqCst);
        self.confirmed_moved.notify_one();
    }

    /// The position the stream started at, which the slot already holds.
    pub(crate) fn start_lsn(&self) -> u64 {
        self.start_lsn
    }

    pub(crate) fn close_requested(&self) -> bool {
        self.close.load(Ordering::SeqCst)
    }

    /// Resolves once a [`confirm`](Self::confirm) moved the resume point since
    /// the last call returned. A move with nobody waiting is remembered, so
    /// the stream never misses one between two polls.
    pub(crate) async fn confirmed_moved(&self) {
        self.confirmed_moved.notified().await;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
