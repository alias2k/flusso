//! `together`'s promises, each polled by hand with a waker that counts the
//! task's wakes, so a broken one fails instead of hanging.

use std::sync::atomic::AtomicUsize;

use tokio::sync::{mpsc, oneshot};

use super::*;

thread_local! {
    /// What runs on this thread after a pass read the flags and before it
    /// drops the "being polled" mark: a wake there lands while the task
    /// still counts as polled, after the pass looked.
    static BEFORE_THE_MARK_DROPS: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
        std::cell::RefCell::new(None);
}

/// The hook [`Flags::end_pass`] calls between reading the flags and
/// dropping the mark.
pub(super) fn before_the_mark_drops() {
    BEFORE_THE_MARK_DROPS.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook();
        }
    });
}

/// The task's waker: counts how often the task was woken.
#[derive(Debug, Default)]
struct Task(AtomicUsize);

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl Task {
    fn wakes(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

fn task() -> (Arc<Task>, Waker) {
    let task = Arc::new(Task::default());
    let waker = Waker::from(Arc::clone(&task));
    (task, waker)
}

fn poll_once<F: Future>(future: Pin<&mut F>, waker: &Waker) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(waker))
}

/// A hand-over and its answer, three times over, take one poll and no wake
/// of the task: each side's wake of the other is served in the same poll.
#[test]
fn wakes_between_the_two_are_served_in_the_same_poll() {
    let (to_second, mut from_first) = mpsc::unbounded_channel::<u32>();
    let (to_first, mut from_second) = mpsc::unbounded_channel::<u32>();
    let first = async move {
        for round in 0..3 {
            let _ = to_second.send(round);
            let _ = from_second.recv().await;
        }
        std::future::pending::<()>().await;
    };
    let second = async move {
        let mut seen = 0;
        while seen < 3 {
            let _ = from_first.recv().await;
            seen += 1;
            let _ = to_first.send(seen);
        }
        seen
    };
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(first, second, std::future::pending()));
    assert_eq!(poll_once(run.as_mut(), &waker), Poll::Ready(3));
    assert_eq!(task.wakes(), 0, "no trip through the scheduler");
}

/// A wake from outside while nothing is being polled wakes the task.
#[test]
fn a_wake_from_outside_while_idle_wakes_the_task() {
    let (send, receive) = oneshot::channel::<u32>();
    let second = async move { receive.await.unwrap_or(0) };
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(
        std::future::pending(),
        second,
        std::future::pending()
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(task.wakes(), 0);
    send.send(7).unwrap();
    assert_eq!(task.wakes(), 1, "woken from outside a poll");
    assert_eq!(poll_once(run.as_mut(), &waker), Poll::Ready(7));
}

/// A wake from another thread that lands during a pass (the task is being
/// polled, so it only raises the flag) is served before the poll ends: the
/// flags are read again after the pass.
#[test]
fn a_wake_from_another_thread_during_a_pass_is_not_lost() {
    let (to_first, from_thread) = oneshot::channel::<()>();
    let (to_second, from_first) = oneshot::channel::<()>();
    let first = async move {
        let _ = from_thread.await;
        let _ = to_second.send(());
        std::future::pending::<()>().await;
    };
    let mut to_first = Some(to_first);
    let mut from_first = from_first;
    let second = std::future::poll_fn(move |cx| {
        if let Some(sender) = to_first.take() {
            // Wakes the first's waker from another thread, mid-pass.
            std::thread::spawn(move || {
                let _ = sender.send(());
            })
            .join()
            .unwrap();
        }
        Pin::new(&mut from_first).poll(cx).map(|_| "both ran")
    });
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(first, second, std::future::pending()));
    assert_eq!(poll_once(run.as_mut(), &waker), Poll::Ready("both ran"));
    assert_eq!(task.wakes(), 0);
}

/// Two futures that keep waking themselves get [`PASSES`] passes, then the
/// poll yields to the worker, waking the task once to come back.
#[test]
fn a_poll_yields_to_the_worker_after_its_passes() {
    let polled = Arc::new(AtomicUsize::new(0));
    let first = std::future::poll_fn(|cx| {
        cx.waker().wake_by_ref();
        Poll::<()>::Pending
    });
    let counter = Arc::clone(&polled);
    let second = std::future::poll_fn(move |cx| {
        let polls = counter.fetch_add(1, Ordering::SeqCst) + 1;
        if polls > PASSES {
            return Poll::Ready(polls);
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    });
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(first, second, std::future::pending()));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(polled.load(Ordering::SeqCst), PASSES, "one poll per pass");
    assert_eq!(task.wakes(), 1, "woken to come back");
}

/// The watch is polled only when its own waker fires, not on every pass,
/// and once it ends the first future is polled again.
#[test]
fn the_watch_is_polled_when_it_fires_and_its_end_wakes_the_first() {
    let first_polls = Arc::new(AtomicUsize::new(0));
    let watch_polls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&first_polls);
    let first = std::future::poll_fn(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Poll::<()>::Pending
    });
    let (fire, fired) = oneshot::channel::<()>();
    let counter = Arc::clone(&watch_polls);
    let mut fired = fired;
    let watch = std::future::poll_fn(move |cx| {
        counter.fetch_add(1, Ordering::SeqCst);
        Pin::new(&mut fired).poll(cx).map(|_| ())
    });
    let (poke, mut poked) = mpsc::unbounded_channel::<()>();
    let second = async move { while poked.recv().await.is_some() {} };
    let (_task, waker) = task();
    let mut run = std::pin::pin!(together(first, second, watch));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    for _ in 0..3 {
        poke.send(()).unwrap();
        assert!(poll_once(run.as_mut(), &waker).is_pending());
    }
    assert_eq!(watch_polls.load(Ordering::SeqCst), 1, "only its first poll");
    assert_eq!(first_polls.load(Ordering::SeqCst), 1);
    fire.send(()).unwrap();
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(watch_polls.load(Ordering::SeqCst), 2);
    assert_eq!(
        first_polls.load(Ordering::SeqCst),
        2,
        "the watch's end woke the first"
    );
}

/// A wake that lands at the very end of a pass, after both futures were
/// polled but while the task still counts as polled (so it wakes no task),
/// is served in the same poll: the flags are read only after the mark drops.
/// Read before it, the wake would be lost: no flag seen, no task woken.
#[test]
fn a_wake_at_the_end_of_a_pass_is_not_lost() {
    let stashed: Arc<std::sync::Mutex<Option<Waker>>> = Arc::default();
    let first_polls = Arc::new(AtomicUsize::new(0));
    let (stash, counter) = (Arc::clone(&stashed), Arc::clone(&first_polls));
    let first = std::future::poll_fn(move |cx| {
        *stash.lock().unwrap() = Some(cx.waker().clone());
        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    });
    let mut fired = false;
    let hook = Arc::clone(&stashed);
    BEFORE_THE_MARK_DROPS.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || {
            if !std::mem::replace(&mut fired, true)
                && let Some(waker) = hook.lock().unwrap().take()
            {
                // From another thread, as a source's I/O would.
                std::thread::spawn(move || waker.wake()).join().unwrap();
            }
        }));
    });
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(
        first,
        std::future::pending::<()>(),
        std::future::pending()
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    BEFORE_THE_MARK_DROPS.with(|slot| *slot.borrow_mut() = None);
    assert_eq!(task.wakes(), 0, "the wake landed while the task was polled");
    assert!(
        first_polls.load(Ordering::SeqCst) >= 2,
        "the first was polled again in the same poll"
    );
}

/// Point 2 of review round 4: once `first` has ended, nothing of it keeps a
/// poll going. The watch ending wakes no ended `first`, and a stale waker of
/// it raises a flag no pass reads: the poll ends at once, with no pass spent
/// and no wake of the task.
#[test]
fn an_ended_first_keeps_no_poll_going() {
    let stale: Arc<std::sync::Mutex<Option<Waker>>> = Arc::default();
    let stash = Arc::clone(&stale);
    let first = std::future::poll_fn(move |cx| {
        *stash.lock().unwrap() = Some(cx.waker().clone());
        Poll::Ready(())
    });
    let second_polls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&second_polls);
    let second = std::future::poll_fn(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Poll::<()>::Pending
    });
    let (fire, fired) = oneshot::channel::<()>();
    let mut fired = fired;
    let watch = std::future::poll_fn(move |cx| Pin::new(&mut fired).poll(cx).map(|_| ()));
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(first, second, watch));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(task.wakes(), 0);
    fire.send(()).unwrap();
    assert_eq!(task.wakes(), 1, "the watch woke the task");
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(task.wakes(), 1, "no pass cap reached, no self-wake");
    if let Some(stale) = stale.lock().unwrap().take() {
        stale.wake();
    }
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(task.wakes(), 2, "only the stale wake itself");
    assert_eq!(second_polls.load(Ordering::SeqCst), 1, "never woken");
}

// [`Flags`] over loom's atomics, with the orderings `std`'s byte has.
word!(loom::sync::atomic::AtomicU8);

/// The handshake of [`Flags`], over loom's atomics: every interleaving, and
/// loom's model of acquire/release, of a poll's passes against wakes from
/// two other threads. Each wake is either taken by a pass (which then sees
/// what the waker wrote before waking: the hand-over's run, in the engine)
/// or wakes the task; never neither. A wake lost this way would leave a
/// future pending forever with its event already happened.
#[test]
fn every_wake_is_taken_by_a_pass_or_wakes_the_task_in_every_interleaving() {
    use loom::sync::Arc;
    use loom::sync::atomic::{AtomicBool, AtomicUsize};

    loom::model(|| {
        let flags = Arc::new(Flags(loom::sync::atomic::AtomicU8::new(0)));
        let woken = Arc::new(AtomicBool::new(false));
        let written = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
        flags.begin();
        let wakers: Vec<_> = [FIRST, SECOND]
            .into_iter()
            .enumerate()
            .map(|(at, bit)| {
                let (flags, woken, written) =
                    (Arc::clone(&flags), Arc::clone(&woken), Arc::clone(&written));
                loom::thread::spawn(move || {
                    written[at].store(1, Ordering::Relaxed);
                    if flags.raise(bit) {
                        woken.store(true, Ordering::Release);
                    }
                })
            })
            .collect();
        let mut taken = [false; 2];
        loop {
            for (at, bit) in [FIRST, SECOND].into_iter().enumerate() {
                if flags.take(bit) {
                    taken[at] = true;
                    assert_eq!(
                        written[at].load(Ordering::Relaxed),
                        1,
                        "seen what it woke for"
                    );
                }
            }
            if flags.end_pass(FIRST | SECOND) {
                break;
            }
        }
        let woke = woken.load(Ordering::Acquire);
        for waker in wakers {
            waker.join().unwrap();
        }
        for (at, bit) in [FIRST, SECOND].into_iter().enumerate() {
            if !taken[at] {
                // Not taken by this poll: its wake must have woken the task,
                // and the next poll finds its flag.
                assert!(
                    woke || woken.load(Ordering::Acquire),
                    "a wake neither taken nor waking the task"
                );
                assert!(flags.take(bit), "the next poll finds it");
            }
        }
    });
}

/// Tokio's cooperative budget (review round 4), with the spender polled
/// first or second. A future that spends every unit it is given does not
/// starve the other: the one polled first swaps every poll, so the other
/// gets a fresh budget every second poll. And a spent budget makes no pass
/// for nothing: tokio wakes the spender it refused only once the poll has
/// returned, so 1,000 units at 128 a poll waste about one poll of the
/// spender per poll that spent the budget.
async fn a_spender_starves_nothing(spender_first: bool) {
    const UNITS: usize = 1_000;
    let spent = Arc::new(AtomicUsize::new(0));
    let wasted = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (done, finished) = oneshot::channel::<()>();
    let spender = {
        let (spent, wasted, stopped) = (
            Arc::clone(&spent),
            Arc::clone(&wasted),
            Arc::clone(&stopped),
        );
        let mut done = Some(done);
        let mut spending = Box::pin(async move {
            while !stopped.load(Ordering::SeqCst) {
                tokio::task::consume_budget().await;
                if spent.fetch_add(1, Ordering::SeqCst) + 1 == UNITS
                    && let Some(done) = done.take()
                {
                    let _ = done.send(());
                }
            }
        });
        std::future::poll_fn(move |cx| {
            let polled = spending.as_mut().poll(cx);
            if !tokio::task::coop::has_budget_remaining() {
                wasted.fetch_add(1, Ordering::SeqCst);
            }
            polled
        })
    };
    // Waits for the spender's signal, which needs budget of its own, then
    // stops the spender.
    let waiter = {
        let stopped = Arc::clone(&stopped);
        async move {
            let _ = finished.await;
            stopped.store(true, Ordering::SeqCst);
        }
    };
    let run = async move {
        if spender_first {
            together(spender, waiter, std::future::pending()).await;
        } else {
            together(waiter, spender, std::future::pending()).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), tokio::spawn(run))
        .await
        .expect("the other was not starved of budget")
        .unwrap();
    assert!(spent.load(Ordering::SeqCst) >= UNITS);
    let wasted = wasted.load(Ordering::SeqCst);
    assert!(
        wasted <= 2 * (UNITS / 128 + 2),
        "{wasted} polls found the budget spent"
    );
}

#[tokio::test]
async fn a_spender_polled_first_starves_nothing() {
    a_spender_starves_nothing(true).await;
}

#[tokio::test]
async fn a_spender_polled_second_starves_nothing() {
    a_spender_starves_nothing(false).await;
}
