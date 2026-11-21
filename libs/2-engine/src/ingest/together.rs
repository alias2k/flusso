//! The reader and the builder as two futures of one task (ADR 0012 stage 8a,
//! §3.8, review round 3), with the stop watched once for the whole run.
//!
//! Each future gets a waker of its own. Woken while the task is being polled,
//! a waker only raises its future's flag and the poll goes on with that
//! future; woken from elsewhere, it also wakes the task. So a hand-over (the
//! reader waking the builder) and a grant (the builder waking the reader) cost
//! a flag, not a trip through the scheduler. A poll ends when no running
//! future was woken during its last pass, or yields to the worker (waking the
//! task again at once) after [`PASSES`] passes, so the pair keeps the worker
//! from the tasks beside it no longer than one loop would. On a runtime
//! worker (where the daemon spawns the engine) and under a current-thread
//! `block_on`, a spent cooperative budget ends a poll by itself: tokio wakes a
//! future it refused for want of budget only once the task's poll has
//! returned, so no flag is raised for it meanwhile. Under a multi-thread
//! runtime's `block_on` (the in-process `engine` bench) tokio wakes it at
//! once, the flag is raised and the refused future is polled again until the
//! [`PASSES`] cap ends the poll: bounded, and never a lost wake.
//!
//! **One word.** The flags and the "being polled" mark are bits of one
//! atomic byte ([`Flags`]): a wake is one `fetch_or` that also tells whether
//! the task is being polled, and a pass ends with one `compare_exchange`
//! that drops the mark only while no running future's flag is raised. Both
//! sides change the one location, whose changes every thread sees in one
//! order, so a wake is either seen by the pass or wakes the task, on any
//! architecture, with no ordering argument across two locations. The loom
//! model in the tests checks it over every interleaving.
//!
//! The third future, `watch`, is polled only when its own waker fires; once
//! it ends it wakes `first`, while `first` runs. The engine run hands it the
//! stop: registered once for the run instead of once per wait of the reader
//! at its lead, where each registration took the stop's lock twice.
//!
//! **Fairness.** Which of the two a pass polls first swaps on every poll:
//! tokio gives a task one cooperative budget per poll, and a future polled
//! first whose resources stay ready (a reader under a burst) would otherwise
//! spend all of it every time and leave the other only pending answers. No
//! rule depends on the order: a run the reader hands over is queued before
//! any batch is formed, whichever is polled first.
//!
//! ```text
//! poll ─▶ [watch if woken] ─▶ [first if woken] ─▶ [second if woken] ─▶ a running one woken? ─▶ again / Pending
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use futures::task::AtomicWaker;

/// The most passes one poll makes before yielding to the worker.
const PASSES: usize = 32;

/// The task is being polled: a wake only raises its flag.
const POLLING: u8 = 1;
const WATCH: u8 = 1 << 1;
const FIRST: u8 = 1 << 2;
const SECOND: u8 = 1 << 3;

/// Poll `first` and `second` in one task until `second` ends, and return its
/// output. `first` is polled only while it hasn't ended; `watch` wakes it
/// once when it ends. See the module docs.
pub(super) async fn together<A, B, W>(first: A, second: B, watch: W) -> B::Output
where
    A: Future<Output = ()>,
    B: Future,
    W: Future<Output = ()>,
{
    let shared = Arc::new(Shared {
        flags: Flags(AtomicU8::new(WATCH | FIRST | SECOND)),
        task: AtomicWaker::new(),
    });
    let waker = |bit| {
        Waker::from(Arc::new(Child {
            bit,
            shared: Arc::clone(&shared),
        }))
    };
    let (watch_waker, first_waker, second_waker) = (waker(WATCH), waker(FIRST), waker(SECOND));
    let flags = &shared.flags;
    let mut watch = std::pin::pin!(watch);
    let mut first = std::pin::pin!(first);
    let mut second = std::pin::pin!(second);
    // The futures still running: only their flags keep a poll going.
    let mut running = WATCH | FIRST | SECOND;
    // Which of the two a poll's passes take first, swapped every poll: tokio
    // gives one budget per poll, and the one polled first could otherwise
    // spend it all every time.
    let mut second_leads = false;
    std::future::poll_fn(|cx| {
        shared.task.register(cx.waker());
        flags.begin();
        second_leads = !second_leads;
        for _ in 0..PASSES {
            if running & WATCH != 0
                && flags.take(WATCH)
                && poll(watch.as_mut(), &watch_waker).is_ready()
            {
                running &= !WATCH;
                if running & FIRST != 0 {
                    flags.raise(FIRST);
                }
            }
            for turn in if second_leads {
                [SECOND, FIRST]
            } else {
                [FIRST, SECOND]
            } {
                if turn == FIRST {
                    if running & FIRST != 0
                        && flags.take(FIRST)
                        && poll(first.as_mut(), &first_waker).is_ready()
                    {
                        running &= !FIRST;
                    }
                } else if flags.take(SECOND)
                    && let Poll::Ready(output) = poll(second.as_mut(), &second_waker)
                {
                    flags.end_poll();
                    return Poll::Ready(output);
                }
            }
            if flags.end_pass(running) {
                return Poll::Pending;
            }
        }
        flags.end_poll();
        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await
}

fn poll<F: Future + ?Sized>(future: Pin<&mut F>, waker: &Waker) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(waker))
}

/// One atomic byte for the handshake (the trait lets the loom model drive
/// the same code over loom's atomics).
pub(super) trait Word {
    fn fetch_or(&self, bits: u8) -> u8;
    fn fetch_and(&self, bits: u8) -> u8;
    fn load(&self) -> u8;
    fn compare_exchange(&self, current: u8, new: u8) -> Result<u8, u8>;
}

/// `Word` for an atomic byte type with `std`'s API: the one definition of
/// the orderings, for `std`'s byte and (in the tests) loom's.
macro_rules! word {
    ($atomic:ty) => {
        impl Word for $atomic {
            fn fetch_or(&self, bits: u8) -> u8 {
                <$atomic>::fetch_or(self, bits, Ordering::AcqRel)
            }

            fn fetch_and(&self, bits: u8) -> u8 {
                <$atomic>::fetch_and(self, bits, Ordering::AcqRel)
            }

            fn load(&self) -> u8 {
                <$atomic>::load(self, Ordering::Acquire)
            }

            fn compare_exchange(&self, current: u8, new: u8) -> Result<u8, u8> {
                <$atomic>::compare_exchange(self, current, new, Ordering::AcqRel, Ordering::Acquire)
            }
        }
    };
}

word!(AtomicU8);

/// The futures' flags and the "being polled" mark, in one word (see the
/// module docs).
#[derive(Debug)]
pub(super) struct Flags<W>(pub(super) W);

impl<W: Word> Flags<W> {
    /// A poll starts: wakes from now on only raise flags.
    pub(super) fn begin(&self) {
        self.0.fetch_or(POLLING);
    }

    /// Raise `bit`. `true` when the task isn't being polled, so the waker
    /// must wake it.
    pub(super) fn raise(&self, bit: u8) -> bool {
        self.0.fetch_or(bit) & POLLING == 0
    }

    /// Lower `bit`, returning whether it was raised.
    pub(super) fn take(&self, bit: u8) -> bool {
        self.0.fetch_and(!bit) & bit != 0
    }

    /// The end of a pass: drop the mark unless a flag of `running` is
    /// raised. `true` when it dropped, so the poll may end: every later wake
    /// sees the mark gone and wakes the task.
    pub(super) fn end_pass(&self, running: u8) -> bool {
        let mut current = self.0.load();
        loop {
            if current & running != 0 {
                return false;
            }
            #[cfg(test)]
            tests::before_the_mark_drops();
            match self.0.compare_exchange(current, current & !POLLING) {
                Ok(_) => return true,
                Err(now) => current = now,
            }
        }
    }

    /// Drop the mark whatever is raised: the poll ends with the task woken
    /// (or done), so a raised flag is served by the next poll.
    pub(super) fn end_poll(&self) {
        self.0.fetch_and(!POLLING);
    }
}

/// What the futures' wakers share: the flags and the task's waker.
#[derive(Debug)]
struct Shared {
    flags: Flags<AtomicU8>,
    task: AtomicWaker,
}

/// One future's waker: its flag, and the task when it isn't being polled.
#[derive(Debug)]
struct Child {
    bit: u8,
    shared: Arc<Shared>,
}

impl Wake for Child {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if self.shared.flags.raise(self.bit) {
            self.shared.task.wake();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
