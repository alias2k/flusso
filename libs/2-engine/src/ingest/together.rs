//! The reader, the dispatcher and the builders as futures of one task (ADR
//! 0012 stage 8a, §3.8 and §3.9 I1), with the stop watched once for the whole
//! run.
//!
//! Each future gets a waker of its own group. Woken while the task is being
//! polled, a waker only raises its group's flag and the poll goes on with
//! the futures of that group; woken from elsewhere, it also wakes the task.
//! So a hand-over (the reader waking the builder), a grant (the builder
//! waking the reader) and a confirmation (a registration waking the
//! dispatcher) cost a flag, not a trip through the scheduler. A pass polls
//! every raised future once, from a rotating start; a poll ends when no
//! running future was woken during its last pass, or yields to the worker
//! (waking the task again at once) after [`MAX_POLLS`] polls, so the futures
//! keep the worker from the tasks beside them no longer than one loop would,
//! however many there are. A poll cut short mid-pass raises again the groups
//! it took and didn't poll, so the next poll serves them. On a runtime worker
//! (where the daemon spawns the engine) and under a current-thread
//! `block_on`, a spent cooperative budget ends a poll by itself: tokio wakes a
//! future it refused for want of budget only once the task's poll has
//! returned, so no flag is raised for it meanwhile. Under a multi-thread
//! runtime's `block_on`, which sets no scheduler context, tokio wakes it at
//! once, the flag is raised and the refused future is polled again until the
//! cap ends the poll: bounded, and never a lost wake.
//!
//! **One word.** The flags and the "being polled" mark are bits of one
//! atomic word ([`Flags`]): bit 0 the mark, bit 1 the watch, bits 2–63 the
//! groups, future `i` (reader 0, dispatcher 1, builder `k` at `2 + k`) in
//! group `i mod 62`. A wake is one `fetch_or` that also tells whether the task
//! is being polled; a pass starts with one `fetch_and` that takes every raised
//! flag of a running future, and ends with one `compare_exchange` that drops
//! the mark only while no running future's flag is raised. Both sides change
//! the one location, whose changes every thread sees in one order, so a wake
//! is either seen by a pass or wakes the task, on any architecture, with no
//! ordering argument across two locations. The loom model in the tests
//! checks it over every interleaving.
//!
//! **Groups.** Past 62 futures, futures share a group: a raised group polls
//! every running future in it, so one may be polled unwoken, which every
//! future tolerates (a future re-checks its state on every poll). An ended
//! future is masked on its own, apart from the group bits: its group stays
//! live while another future in it runs, and polling the group skips it.
//!
//! The watch is polled only when its own waker fires; once it ends it wakes
//! the reader and the dispatcher. The engine run hands it the stop:
//! registered once for the run instead of once per wait of the reader at its
//! lead, where each registration took the stop's lock twice.
//!
//! **Fairness.** The future a poll's passes start at advances every poll:
//! tokio gives a task one cooperative budget per poll, and a future polled
//! first whose resources stay ready (a reader under a burst) would otherwise
//! spend all of it every time and leave the others only pending answers. No
//! rule depends on the order: a run the reader hands over is queued before
//! any batch is formed, whichever is polled first.
//!
//! ```text
//! poll ─▶ take raised ─▶ [watch] ─▶ [each raised future, from the start] ─▶ a running one woken? ─▶ again / Pending
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use futures::task::AtomicWaker;

/// The most futures one poll polls, the watch included, before yielding to
/// the worker (6a's 32 passes of two).
pub(super) const MAX_POLLS: usize = 64;

/// The task is being polled: a wake only raises its flag.
const POLLING: u64 = 1;
/// The stop's watch.
const WATCH: u64 = 1 << 1;
/// The groups' bits start here.
const FIRST_GROUP: usize = 2;
/// How many groups there are.
const GROUPS: usize = 62;
/// The reader's place among the futures.
const READER: usize = 0;
/// The dispatcher's place: its output is the run's.
const DISPATCHER: usize = 1;

/// The bit of future `at`'s group.
fn group(at: usize) -> u64 {
    1 << (FIRST_GROUP + at % GROUPS)
}

/// Poll `reader`, `dispatcher` and `builders` in one task until `dispatcher`
/// ends, and return its output. A future is polled only while it hasn't
/// ended; `watch` wakes the reader and the dispatcher once when it ends. See
/// the module docs.
pub(super) async fn together<R, D, B, W>(
    reader: R,
    dispatcher: D,
    builders: Vec<B>,
    watch: W,
) -> D::Output
where
    R: Future<Output = ()>,
    D: Future,
    B: Future<Output = ()>,
    W: Future<Output = ()>,
{
    let count = 2 + builders.len();
    let used = (0..count.min(GROUPS)).fold(0, |used, at| used | group(at));
    let shared = Arc::new(Shared {
        flags: Flags(AtomicU64::new(WATCH | used)),
        task: AtomicWaker::new(),
    });
    let waker = |bit| {
        Waker::from(Arc::new(Child {
            bit,
            shared: Arc::clone(&shared),
        }))
    };
    let watch_waker = waker(WATCH);
    let wakers: Vec<Waker> = (0..count.min(GROUPS)).map(|at| waker(group(at))).collect();
    let flags = &shared.flags;
    let mut watch = std::pin::pin!(watch);
    let mut reader = std::pin::pin!(reader);
    let mut dispatcher = std::pin::pin!(dispatcher);
    let mut builders: Vec<Pin<Box<B>>> = builders.into_iter().map(Box::pin).collect();
    let mut running = vec![true; count];
    // How many running futures each group holds: a group's bit counts while
    // one does.
    let mut live = vec![0usize; count.min(GROUPS)];
    for at in 0..count {
        if let Some(live) = live.get_mut(at % GROUPS) {
            *live += 1;
        }
    }
    let mut running_groups = used;
    let mut watching = true;
    let mut start = 0;
    std::future::poll_fn(|cx| {
        shared.task.register(cx.waker());
        flags.begin();
        start = (start + 1) % count;
        let mut polls = 0;
        loop {
            if polls >= MAX_POLLS {
                flags.end_poll();
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            let mut raised = flags.take(running_groups | if watching { WATCH } else { 0 });
            if raised & WATCH != 0 {
                polls += 1;
                if poll(watch.as_mut(), &watch_waker).is_ready() {
                    watching = false;
                    raised |= (group(READER) | group(DISPATCHER)) & running_groups;
                }
            }
            for turn in 0..count {
                let at = (start + turn) % count;
                if raised & group(at) == 0 || !running.get(at).copied().unwrap_or(false) {
                    continue;
                }
                if polls >= MAX_POLLS {
                    // Cut mid-pass: the groups taken and not polled are
                    // raised again, for the next poll.
                    let unpolled = (turn..count)
                        .map(|rest| (start + rest) % count)
                        .filter(|rest| running.get(*rest).copied().unwrap_or(false))
                        .fold(0, |bits, rest| bits | group(rest));
                    flags.raise(raised & unpolled);
                    flags.end_poll();
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                polls += 1;
                let Some(waker) = wakers.get(at % GROUPS) else {
                    continue;
                };
                let ended = match at {
                    READER => poll(reader.as_mut(), waker).is_ready(),
                    DISPATCHER => {
                        if let Poll::Ready(output) = poll(dispatcher.as_mut(), waker) {
                            flags.end_poll();
                            return Poll::Ready(output);
                        }
                        false
                    }
                    builder => match builders.get_mut(builder - 2) {
                        Some(builder) => poll(builder.as_mut(), waker).is_ready(),
                        None => false,
                    },
                };
                if ended {
                    if let Some(running) = running.get_mut(at) {
                        *running = false;
                    }
                    if let Some(live) = live.get_mut(at % GROUPS) {
                        *live -= 1;
                        if *live == 0 {
                            running_groups &= !group(at);
                        }
                    }
                }
            }
            if flags.end_pass(running_groups | if watching { WATCH } else { 0 }) {
                return Poll::Pending;
            }
        }
    })
    .await
}

fn poll<F: Future + ?Sized>(future: Pin<&mut F>, waker: &Waker) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(waker))
}

/// One atomic word for the handshake (the trait lets the loom model drive
/// the same code over loom's atomics).
pub(super) trait Word {
    fn fetch_or(&self, bits: u64) -> u64;
    fn fetch_and(&self, bits: u64) -> u64;
    fn load(&self) -> u64;
    fn compare_exchange(&self, current: u64, new: u64) -> Result<u64, u64>;
}

/// `Word` for an atomic `u64` type with `std`'s API: the one definition of
/// the orderings, for `std`'s and (in the tests) loom's.
macro_rules! word {
    ($atomic:ty) => {
        impl Word for $atomic {
            fn fetch_or(&self, bits: u64) -> u64 {
                <$atomic>::fetch_or(self, bits, Ordering::AcqRel)
            }

            fn fetch_and(&self, bits: u64) -> u64 {
                <$atomic>::fetch_and(self, bits, Ordering::AcqRel)
            }

            fn load(&self) -> u64 {
                <$atomic>::load(self, Ordering::Acquire)
            }

            fn compare_exchange(&self, current: u64, new: u64) -> Result<u64, u64> {
                <$atomic>::compare_exchange(self, current, new, Ordering::AcqRel, Ordering::Acquire)
            }
        }
    };
}

word!(AtomicU64);

/// The futures' flags and the "being polled" mark, in one word (see the
/// module docs).
#[derive(Debug)]
pub(super) struct Flags<W>(pub(super) W);

impl<W: Word> Flags<W> {
    /// A poll starts: wakes from now on only raise flags.
    pub(super) fn begin(&self) {
        self.0.fetch_or(POLLING);
    }

    /// Raise `bits`. `true` when the task isn't being polled, so the waker
    /// must wake it.
    pub(super) fn raise(&self, bits: u64) -> bool {
        self.0.fetch_or(bits) & POLLING == 0
    }

    /// Lower every flag of `bits`, returning those that were raised.
    pub(super) fn take(&self, bits: u64) -> u64 {
        self.0.fetch_and(!bits) & bits
    }

    /// The end of a pass: drop the mark unless a flag of `running` is
    /// raised. `true` when it dropped, so the poll may end: every later wake
    /// sees the mark gone and wakes the task.
    pub(super) fn end_pass(&self, running: u64) -> bool {
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
    flags: Flags<AtomicU64>,
    task: AtomicWaker,
}

/// One group's waker: its flag, and the task when it isn't being polled.
#[derive(Debug)]
struct Child {
    bit: u64,
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
