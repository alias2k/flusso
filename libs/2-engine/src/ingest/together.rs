//! The reader and the builder as two futures of one task (ADR 0012 stage 8a,
//! §3.8, review round 3), with the stop watched once for the whole run.
//!
//! Each future gets a waker of its own. Woken while the task is being polled,
//! a waker only raises its future's flag and the poll goes on with that
//! future; woken from elsewhere, it also wakes the task. So a hand-over (the
//! reader waking the builder) and a grant (the builder waking the reader) cost
//! a flag, not a trip through the scheduler. A poll ends when neither future
//! was woken during its last pass, or after [`PASSES`] passes, when it yields
//! to the worker (and is woken again at once), so the pair keeps the worker
//! from the tasks beside it no longer than one loop would.
//!
//! The third future, `watch`, is polled only when its own waker fires; once
//! it ends it wakes `first`. The engine run hands it the stop: registered once
//! for the run instead of once per wait of the reader at its lead, where each
//! registration took the stop's lock twice.
//!
//! ```text
//! poll ─▶ [watch if woken] ─▶ [first if woken] ─▶ [second if woken] ─▶ woken during the pass? ─▶ again / Pending
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use futures::task::AtomicWaker;

/// The most passes one poll makes before yielding to the worker.
const PASSES: usize = 32;

/// Poll `first` and `second` in one task until `second` ends, and return its
/// output. `first` is polled before `second` on every pass, and only while it
/// hasn't ended; `watch` wakes `first` once when it ends. See the module docs.
pub(super) async fn together<A, B, W>(first: A, second: B, watch: W) -> B::Output
where
    A: Future<Output = ()>,
    B: Future,
    W: Future<Output = ()>,
{
    let shared = Arc::new(Shared::default());
    let watch_child = Child::new(&shared);
    let first_child = Child::new(&shared);
    let second_child = Child::new(&shared);
    let watch_waker = Waker::from(Arc::clone(&watch_child));
    let first_waker = Waker::from(Arc::clone(&first_child));
    let second_waker = Waker::from(Arc::clone(&second_child));
    let mut watch = std::pin::pin!(watch);
    let mut watch_done = false;
    let mut first = std::pin::pin!(first);
    let mut first_done = false;
    let mut second = std::pin::pin!(second);
    std::future::poll_fn(|cx| {
        shared.task.register(cx.waker());
        for _ in 0..PASSES {
            shared.polling.store(true, Ordering::SeqCst);
            if !watch_done && watch_child.take() {
                watch_done = poll(watch.as_mut(), &watch_waker).is_ready();
                if watch_done {
                    first_child.raise();
                }
            }
            if !first_done && first_child.take() {
                first_done = poll(first.as_mut(), &first_waker).is_ready();
            }
            if second_child.take()
                && let Poll::Ready(output) = poll(second.as_mut(), &second_waker)
            {
                return Poll::Ready(output);
            }
            // A wake from another thread during the pass saw `polling` and
            // woke no task: the flags are read after it drops.
            #[cfg(test)]
            tests::before_the_mark_drops();
            shared.polling.store(false, Ordering::SeqCst);
            if !watch_child.is_raised() && !first_child.is_raised() && !second_child.is_raised() {
                return Poll::Pending;
            }
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await
}

fn poll<F: Future + ?Sized>(future: Pin<&mut F>, waker: &Waker) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(waker))
}

/// What the children's wakers share: the task's waker, and whether the task
/// is being polled (then a wake only raises the child's flag).
#[derive(Debug, Default)]
struct Shared {
    task: AtomicWaker,
    polling: AtomicBool,
}

/// One future's waker: its flag, and the task when it isn't being polled.
#[derive(Debug)]
struct Child {
    raised: AtomicBool,
    shared: Arc<Shared>,
}

impl Child {
    /// Raised, so its future is polled on the first pass.
    fn new(shared: &Arc<Shared>) -> Arc<Self> {
        Arc::new(Self {
            raised: AtomicBool::new(true),
            shared: Arc::clone(shared),
        })
    }

    fn raise(&self) {
        self.raised.store(true, Ordering::SeqCst);
    }

    /// Lower the flag, returning whether it was raised.
    fn take(&self) -> bool {
        self.raised.swap(false, Ordering::SeqCst)
    }

    fn is_raised(&self) -> bool {
        self.raised.load(Ordering::SeqCst)
    }
}

impl Wake for Child {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.raise();
        if !self.shared.polling.load(Ordering::SeqCst) {
            self.shared.task.wake();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
