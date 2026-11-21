//! `together`'s promises, each polled by hand with a waker that counts the
//! task's wakes, so a broken one fails instead of hanging.

use std::sync::atomic::AtomicUsize;

use tokio::sync::{mpsc, oneshot};

use super::*;

thread_local! {
    /// What runs on this thread just before a pass's "being polled" mark
    /// drops: a wake there lands while the task still counts as polled.
    static BEFORE_THE_MARK_DROPS: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
        std::cell::RefCell::new(None);
}

/// The hook `together` calls at the end of each pass, before the mark drops.
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
