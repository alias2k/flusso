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

/// No builders.
fn none() -> Vec<std::future::Pending<()>> {
    Vec::new()
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
    let mut run = std::pin::pin!(together(first, second, none(), std::future::pending()));
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
        none(),
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
    let mut run = std::pin::pin!(together(first, second, none(), std::future::pending()));
    assert_eq!(poll_once(run.as_mut(), &waker), Poll::Ready("both ran"));
    assert_eq!(task.wakes(), 0);
}

/// Two futures that keep waking themselves get [`MAX_POLLS`] polls between
/// them, then the poll yields to the worker, waking the task once to come
/// back.
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
        if polls > MAX_POLLS / 2 {
            return Poll::Ready(polls);
        }
        cx.waker().wake_by_ref();
        Poll::Pending
    });
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(first, second, none(), std::future::pending()));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(
        polled.load(Ordering::SeqCst),
        MAX_POLLS / 2,
        "one poll per pass, two futures a pass"
    );
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
    let mut run = std::pin::pin!(together(first, second, none(), watch));
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
        none(),
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
/// and no wake of the task. `first` panics if polled after it ended, as an
/// `async` block would.
#[test]
fn an_ended_first_keeps_no_poll_going() {
    let stale: Arc<std::sync::Mutex<Option<Waker>>> = Arc::default();
    let stash = Arc::clone(&stale);
    let mut ended = false;
    let first = std::future::poll_fn(move |cx| {
        assert!(!ended, "an ended future is never polled again");
        ended = true;
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
    let mut run = std::pin::pin!(together(first, second, none(), watch));
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
    assert_eq!(
        second_polls.load(Ordering::SeqCst),
        2,
        "polled first, then once by the watch's end; never by the stale wake"
    );
}

// [`Flags`] over loom's atomics, with the orderings `std`'s byte has.
word!(loom::sync::atomic::AtomicU64);

/// The handshake of [`Flags`], over loom's atomics: a **bounded** search
/// (`preemption_bound = Some(3)`: every interleaving with at most three
/// preemptions, over four threads) of a poll's passes against wakes from
/// three other threads, with loom's model of acquire/release. It drives the
/// real `Flags` (`begin`, `raise`, `take`, `end_pass`, `end_poll`): the
/// concurrent `fetch_or`s of the wakers, the take-all `fetch_and` that starts
/// a pass, two wakers sharing a group, the end of a pass dropping the mark
/// only while no running flag is raised, and, with `cut`, a first poll that
/// ends as the cap ends it (it takes everything, polls only the own group's
/// future, raises the shared group again and drops the mark; the next poll
/// must find it). Each wake is either taken by a pass (which then sees what
/// the waker wrote before waking: the hand-over's run, in the engine) or
/// wakes the task; never neither. A wake lost this way would leave a future
/// pending forever with its event already happened.
///
/// What it does not cover, and what does instead (the single-threaded unit
/// tests below): the running mask per future and ended futures' stale wakes
/// (`an_ended_first_keeps_no_poll_going`,
/// `an_ended_future_in_a_shared_group_is_never_polled_again`), the watch's
/// bit at the end of a pass (`a_watch_woken_at_the_end_of_a_pass_is_not_lost`)
/// and the local raise on the watch's end
/// (`the_watch_wakes_the_reader_and_the_dispatcher_not_the_builders`), the
/// rotation (the spender tests), the cap's counts
/// (`a_poll_of_n_futures_yields_after_64_polls`,
/// `a_watch_that_keeps_waking_itself_still_yields`), and the task's waker
/// itself (`AtomicWaker`, the futures crate's).
fn handshake(cut: bool) {
    use loom::sync::Arc;
    use loom::sync::atomic::{AtomicBool, AtomicUsize};

    const SHARED: u64 = 1 << 2;
    const OWN: u64 = 1 << 3;
    const BITS: [u64; 3] = [SHARED, SHARED, OWN];

    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(move || {
        let flags = Arc::new(Flags(loom::sync::atomic::AtomicU64::new(0)));
        let woken = Arc::new(AtomicBool::new(false));
        let written = Arc::new([
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        ]);
        flags.begin();
        let wakers: Vec<_> = BITS
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
        let mut seen = [false; 3];
        let look = |raised: u64, seen: &mut [bool; 3]| {
            for (at, bit) in BITS.into_iter().enumerate() {
                if raised & bit != 0 && written[at].load(Ordering::Relaxed) == 1 {
                    seen[at] = true;
                }
            }
        };
        let mut cutting = cut;
        loop {
            let raised = flags.take(SHARED | OWN);
            if std::mem::replace(&mut cutting, false) {
                look(raised & OWN, &mut seen);
                flags.raise(raised & SHARED);
                flags.end_poll();
                // The poll woke its own task: the next one starts.
                flags.begin();
                continue;
            }
            // A raised bit was raised by at least one of its wakers, which
            // the take synchronized with.
            if raised & SHARED != 0 {
                assert!(
                    written[0].load(Ordering::Relaxed) == 1
                        || written[1].load(Ordering::Relaxed) == 1,
                    "a shared group taken with neither waker's write seen"
                );
            }
            if raised & OWN != 0 {
                assert_eq!(
                    written[2].load(Ordering::Relaxed),
                    1,
                    "seen what it woke for"
                );
            }
            look(raised, &mut seen);
            if flags.end_pass(SHARED | OWN) {
                break;
            }
        }
        let woke = woken.load(Ordering::Acquire);
        for waker in wakers {
            waker.join().unwrap();
        }
        let woke = woke || woken.load(Ordering::Acquire);
        let mut next: Option<u64> = None;
        for (at, bit) in BITS.into_iter().enumerate() {
            if !seen[at] {
                // Not seen by this poll: its wake must have woken the task,
                // and the next poll finds its flag.
                assert!(woke, "a wake neither taken nor waking the task");
                let found = *next.get_or_insert_with(|| flags.take(SHARED | OWN));
                assert!(found & bit != 0, "the next poll finds it");
            }
        }
    });
}

#[test]
fn every_wake_is_taken_by_a_pass_or_wakes_the_task_within_three_preemptions() {
    handshake(false);
}

#[test]
fn a_group_raised_again_by_a_cut_poll_is_found_by_the_next_within_three_preemptions() {
    handshake(true);
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
            together(spender, waiter, none(), std::future::pending()).await;
        } else {
            together(waiter, spender, none(), std::future::pending()).await;
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

/// A future that counts its polls, keeps its last waker, and never ends.
fn counted(
    polls: &Arc<AtomicUsize>,
    waker: &Arc<std::sync::Mutex<Option<Waker>>>,
) -> impl Future<Output = ()> + use<> {
    let (polls, stash) = (Arc::clone(polls), Arc::clone(waker));
    std::future::poll_fn(move |cx| {
        polls.fetch_add(1, Ordering::SeqCst);
        *stash.lock().unwrap() = Some(cx.waker().clone());
        Poll::Pending
    })
}

struct Futures {
    polls: Vec<Arc<AtomicUsize>>,
    wakers: Vec<Arc<std::sync::Mutex<Option<Waker>>>>,
}

impl Futures {
    fn new(count: usize) -> Self {
        Self {
            polls: (0..count).map(|_| Arc::default()).collect(),
            wakers: (0..count).map(|_| Arc::default()).collect(),
        }
    }

    fn future(&self, at: usize) -> impl Future<Output = ()> + use<> {
        counted(&self.polls[at], &self.wakers[at])
    }

    fn polls(&self, at: usize) -> usize {
        self.polls[at].load(Ordering::SeqCst)
    }

    fn wake(&self, at: usize) {
        if let Some(waker) = self.wakers[at].lock().unwrap().clone() {
            waker.wake();
        }
    }
}

/// Past 62 futures two share a group: waking one polls both, and a future
/// of another group is not polled.
#[test]
fn a_shared_group_polls_every_future_in_it() {
    let futures = Futures::new(70);
    let builders: Vec<_> = (2..70).map(|at| futures.future(at)).collect();
    let (_task, waker) = task();
    let mut run = std::pin::pin!(together(
        futures.future(0),
        futures.future(1),
        builders,
        std::future::pending()
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert!(
        poll_once(run.as_mut(), &waker).is_pending(),
        "the cap cut the first poll"
    );
    let before: Vec<usize> = (0..70).map(|at| futures.polls(at)).collect();
    assert!(before.iter().all(|polls| *polls >= 1), "{before:?}");
    // Future 64 is in group 64 mod 62 = 2, future 2's.
    futures.wake(64);
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(futures.polls(64), before[64] + 1);
    assert_eq!(
        futures.polls(2),
        before[2] + 1,
        "its group-mate is polled too"
    );
    assert_eq!(futures.polls(3), before[3], "another group is not");
}

/// An ended future is masked on its own: waking its group-mate polls the
/// mate, never the ended one (which panics if polled after it ended, as an
/// `async` block would).
#[test]
fn an_ended_future_in_a_shared_group_is_never_polled_again() {
    let futures = Futures::new(70);
    let mut ended = false;
    let first_builder = std::future::poll_fn(move |_| {
        assert!(!ended, "an ended future is never polled again");
        ended = true;
        Poll::Ready(())
    });
    let mut builders: Vec<std::pin::Pin<Box<dyn Future<Output = ()>>>> =
        vec![Box::pin(first_builder)];
    builders.extend(
        (3..70)
            .map(|at| Box::pin(futures.future(at)) as std::pin::Pin<Box<dyn Future<Output = ()>>>),
    );
    let (_task, waker) = task();
    let mut run = std::pin::pin!(together(
        futures.future(0),
        futures.future(1),
        builders,
        std::future::pending()
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    let before = futures.polls(64);
    futures.wake(64);
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(
        futures.polls(64),
        before + 1,
        "the mate is polled, the ended one not"
    );
}

/// A poll with more futures woken than it may poll (here all 100 at the
/// start) stops at [`MAX_POLLS`], raises the groups it took and didn't poll
/// again, and wakes the task once: the polls that follow poll the rest.
#[test]
fn a_pass_cut_by_the_cap_raises_its_unpolled_groups_again() {
    let futures = Futures::new(100);
    let builders: Vec<_> = (2..100).map(|at| futures.future(at)).collect();
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(
        futures.future(0),
        futures.future(1),
        builders,
        std::future::pending()
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    let polled: usize = (0..100).map(|at| futures.polls(at)).sum();
    assert_eq!(polled + 1, MAX_POLLS, "the watch and 63 futures");
    assert_eq!(task.wakes(), 1, "woken to come back");
    // Come back while the task keeps waking itself: group-mates of the
    // futures raised again are polled with them, so it may take a few.
    let mut polls = 1;
    while polls < 10 && task.wakes() == polls {
        assert!(poll_once(run.as_mut(), &waker).is_pending());
        polls += 1;
    }
    assert!(polls < 10, "the task kept waking itself");
    let never: Vec<usize> = (0..100).filter(|at| futures.polls(*at) == 0).collect();
    assert!(never.is_empty(), "never polled: {never:?}");
}

/// Futures that keep waking themselves get [`MAX_POLLS`] polls between
/// them, then the poll yields to the worker, whatever their number.
#[test]
fn a_poll_of_n_futures_yields_after_64_polls() {
    let polls = Arc::new(AtomicUsize::new(0));
    let spinning = |polls: &Arc<AtomicUsize>| {
        let polls = Arc::clone(polls);
        std::future::poll_fn(move |cx| {
            polls.fetch_add(1, Ordering::SeqCst);
            cx.waker().wake_by_ref();
            Poll::<()>::Pending
        })
    };
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(
        spinning(&polls),
        spinning(&polls),
        vec![spinning(&polls), spinning(&polls), spinning(&polls)],
        std::future::pending()
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(polls.load(Ordering::SeqCst) + 1, MAX_POLLS, "and the watch");
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(polls.load(Ordering::SeqCst) + 1, 2 * MAX_POLLS);
    assert_eq!(task.wakes(), 2);
}

/// A builder nobody wakes is polled once, at the start, however often the
/// reader and the dispatcher hand over between them: an idle future costs
/// nothing per poll.
#[test]
fn an_idle_builder_is_polled_only_when_woken() {
    let futures = Futures::new(3);
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
    let mut run = std::pin::pin!(together(
        first,
        second,
        vec![futures.future(2)],
        std::future::pending()
    ));
    assert_eq!(poll_once(run.as_mut(), &waker), Poll::Ready(3));
    assert_eq!(task.wakes(), 0);
    assert_eq!(futures.polls(2), 1);
}

/// The watch's end wakes the reader and the dispatcher, not the builders
/// (no builder registers on the stop: the dispatcher raises them).
#[test]
fn the_watch_wakes_the_reader_and_the_dispatcher_not_the_builders() {
    let futures = Futures::new(3);
    let (fire, fired) = oneshot::channel::<()>();
    let mut fired = fired;
    let watch = std::future::poll_fn(move |cx| Pin::new(&mut fired).poll(cx).map(|_| ()));
    let (_task, waker) = task();
    let mut run = std::pin::pin!(together(
        futures.future(0),
        futures.future(1),
        vec![futures.future(2)],
        watch
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    fire.send(()).unwrap();
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert_eq!(
        (futures.polls(0), futures.polls(1), futures.polls(2)),
        (2, 2, 1)
    );
}

/// Every future is polled inline: a panic in any of them unwinds the poll,
/// as a panic of the engine's one task did.
#[test]
fn a_panic_in_any_future_unwinds_the_poll() {
    for at in 0..3 {
        let boom = move |me: usize| {
            std::future::poll_fn(move |_| -> Poll<()> {
                assert!(me != at, "boom");
                Poll::Pending
            })
        };
        let (_task, waker) = task();
        let mut run = Box::pin(together(
            boom(0),
            boom(1),
            vec![boom(2)],
            std::future::pending(),
        ));
        let polled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            poll_once(run.as_mut(), &waker)
        }));
        assert!(polled.is_err(), "the panic of future {at} unwound the poll");
    }
}

/// Fairness with three futures: a spender as a builder, or as the reader
/// beside a builder, starves neither the dispatcher waiting on it nor the
/// rest (the start rotates over all three).
async fn a_spender_among_three_starves_nothing(spender_at: usize) {
    const UNITS: usize = 1_000;
    let spent = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (done, finished) = oneshot::channel::<()>();
    let spender = {
        let (spent, stopped) = (Arc::clone(&spent), Arc::clone(&stopped));
        let mut done = Some(done);
        async move {
            while !stopped.load(Ordering::SeqCst) {
                tokio::task::consume_budget().await;
                if spent.fetch_add(1, Ordering::SeqCst) + 1 == UNITS
                    && let Some(done) = done.take()
                {
                    let _ = done.send(());
                }
            }
        }
    };
    let waiter = {
        let stopped = Arc::clone(&stopped);
        async move {
            let _ = finished.await;
            stopped.store(true, Ordering::SeqCst);
        }
    };
    let idle = std::future::pending::<()>();
    let run = async move {
        if spender_at == 0 {
            together(
                spender,
                waiter,
                vec![Box::pin(idle) as Pin<Box<dyn Future<Output = ()> + Send>>],
                std::future::pending(),
            )
            .await;
        } else {
            together(
                idle,
                waiter,
                vec![Box::pin(spender) as Pin<Box<dyn Future<Output = ()> + Send>>],
                std::future::pending(),
            )
            .await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), tokio::spawn(run))
        .await
        .expect("nothing was starved of budget")
        .unwrap();
    assert!(spent.load(Ordering::SeqCst) >= UNITS);
}

#[tokio::test]
async fn a_spender_as_the_reader_beside_a_builder_starves_nothing() {
    a_spender_among_three_starves_nothing(0).await;
}

#[tokio::test]
async fn a_spender_as_a_builder_starves_nothing() {
    a_spender_among_three_starves_nothing(2).await;
}

/// A watch that keeps waking itself without ending (a pass that polls only
/// the watch) still ends the poll at [`MAX_POLLS`]: the cap is checked
/// before every pass, not only before a future.
#[test]
fn a_watch_that_keeps_waking_itself_still_yields() {
    let polls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&polls);
    let watch = std::future::poll_fn(move |cx| {
        let polls = counter.fetch_add(1, Ordering::SeqCst) + 1;
        // Fails the test instead of hanging it in one endless poll.
        assert!(polls <= 10 * MAX_POLLS, "one poll never ended");
        cx.waker().wake_by_ref();
        Poll::<()>::Pending
    });
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(
        std::future::pending::<()>(),
        std::future::pending::<()>(),
        none(),
        watch
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    assert!(polls.load(Ordering::SeqCst) <= MAX_POLLS);
    assert_eq!(task.wakes(), 1, "woken to come back");
}

/// The watch's wake at the very end of a pass, after the pass polled it but
/// while the task still counts as polled (so it wakes no task), is served in
/// the same poll: the end of a pass counts the watch's flag while it runs.
/// Dropped from the mask, the stop would be lost until another wake.
#[test]
fn a_watch_woken_at_the_end_of_a_pass_is_not_lost() {
    let stashed: Arc<std::sync::Mutex<Option<Waker>>> = Arc::default();
    let watch_polls = Arc::new(AtomicUsize::new(0));
    let (stash, counter) = (Arc::clone(&stashed), Arc::clone(&watch_polls));
    let watch = std::future::poll_fn(move |cx| {
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
                std::thread::spawn(move || waker.wake()).join().unwrap();
            }
        }));
    });
    let (task, waker) = task();
    let mut run = std::pin::pin!(together(
        std::future::pending::<()>(),
        std::future::pending::<()>(),
        none(),
        watch
    ));
    assert!(poll_once(run.as_mut(), &waker).is_pending());
    BEFORE_THE_MARK_DROPS.with(|slot| *slot.borrow_mut() = None);
    assert_eq!(task.wakes(), 0, "the wake landed while the task was polled");
    assert_eq!(
        watch_polls.load(Ordering::SeqCst),
        2,
        "the watch was polled again in the same poll"
    );
}
