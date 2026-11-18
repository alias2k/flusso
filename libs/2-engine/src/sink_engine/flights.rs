//! The writes a sink engine's run has in flight, each polled only when its
//! own waker fired.
//!
//! `FuturesUnordered` would do the same job, but it wakes its task after any
//! pass that polled every future it holds and left them pending ("to avoid
//! starving other tasks"). With one write in flight that is every write that
//! doesn't finish at its first poll: one wake-up of the run for nothing, about
//! 4,800 per `live_drain_held/sinks2/1` run, which was the whole difference to
//! one write at a time. A run never needs that yield: it waits in one
//! `select!` and holds at most `writes` (64) futures.
//!
//! A write is started with [`Flights::start`], which polls it once with its
//! own waker, so it is never polled again until that waker fires.
//! [`Flights::next`] registers the run's waker before it looks for woken
//! writes, so a wake between two polls is never lost.
//!
//! ```text
//! start(a)   a polled: pending, kept
//! start(b)   b polled: pending, kept
//! b's waker  b marked; the run is woken
//! next()     polls b only: ready, returned
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use futures::future::BoxFuture;
use futures::task::{ArcWake, AtomicWaker, waker_ref};

/// Futures in flight, polled when woken, finishing in any order.
pub(crate) struct Flights<'a, T> {
    flights: Vec<Flight<'a, T>>,
    run: Arc<AtomicWaker>,
}

struct Flight<'a, T> {
    future: BoxFuture<'a, T>,
    woken: Arc<Woken>,
}

/// One future's waker: marks the future and wakes the run.
struct Woken {
    marked: AtomicBool,
    run: Arc<AtomicWaker>,
}

impl ArcWake for Woken {
    fn wake_by_ref(woken: &Arc<Self>) {
        woken.marked.store(true, Ordering::Release);
        woken.run.wake();
    }
}

impl<T> std::fmt::Debug for Flights<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flights")
            .field("len", &self.flights.len())
            .finish_non_exhaustive()
    }
}

impl<'a, T> Flights<'a, T> {
    pub(crate) fn new() -> Self {
        Self {
            flights: Vec::new(),
            run: Arc::new(AtomicWaker::new()),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.flights.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.flights.is_empty()
    }

    /// Poll `future` once with its own waker: its output if that finished
    /// it, else it is kept until [`next`](Self::next) hands it out.
    pub(crate) fn start(&mut self, mut future: BoxFuture<'a, T>) -> Option<T> {
        let woken = Arc::new(Woken {
            marked: AtomicBool::new(false),
            run: Arc::clone(&self.run),
        });
        let waker = waker_ref(&woken);
        match future.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(output) => Some(output),
            Poll::Pending => {
                self.flights.push(Flight { future, woken });
                None
            }
        }
    }

    /// The next future to finish; `None` when none is in flight.
    pub(crate) async fn next(&mut self) -> Option<T> {
        std::future::poll_fn(|cx| self.poll_next(cx)).await
    }

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        if self.flights.is_empty() {
            return Poll::Ready(None);
        }
        self.run.register(cx.waker());
        let mut index = 0;
        while let Some(flight) = self.flights.get_mut(index) {
            if flight.woken.marked.swap(false, Ordering::AcqRel) {
                let waker = waker_ref(&flight.woken);
                if let Poll::Ready(output) = flight
                    .future
                    .as_mut()
                    .poll(&mut Context::from_waker(&waker))
                {
                    self.flights.swap_remove(index);
                    return Poll::Ready(Some(output));
                }
            }
            index += 1;
        }
        Poll::Pending
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use futures::FutureExt;
    use futures::task::waker;
    use tokio::sync::oneshot;

    use super::*;

    struct CountWakes(AtomicUsize);

    impl ArcWake for CountWakes {
        fn wake_by_ref(count: &Arc<Self>) {
            count.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn pending_until(rx: oneshot::Receiver<u32>) -> BoxFuture<'static, u32> {
        async move { rx.await.unwrap() }.boxed()
    }

    #[test]
    fn a_pass_that_leaves_every_future_pending_does_not_wake_the_run() {
        let mut flights = Flights::new();
        let (_tx, rx) = oneshot::channel();
        assert!(flights.start(pending_until(rx)).is_none());

        let wakes = Arc::new(CountWakes(AtomicUsize::new(0)));
        let run = waker(Arc::clone(&wakes));
        let mut cx = Context::from_waker(&run);
        assert!(flights.poll_next(&mut cx).is_pending());
        assert!(flights.poll_next(&mut cx).is_pending());
        assert_eq!(wakes.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_woken_future_wakes_the_run_and_is_handed_out() {
        let mut flights = Flights::new();
        let (tx_a, rx_a) = oneshot::channel();
        let (tx_b, rx_b) = oneshot::channel();
        assert!(flights.start(pending_until(rx_a)).is_none());
        assert!(flights.start(pending_until(rx_b)).is_none());

        let wakes = Arc::new(CountWakes(AtomicUsize::new(0)));
        let run = waker(Arc::clone(&wakes));
        let mut cx = Context::from_waker(&run);
        assert!(flights.poll_next(&mut cx).is_pending());

        tx_b.send(2).unwrap();
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert_eq!(flights.poll_next(&mut cx), Poll::Ready(Some(2)));
        assert_eq!(flights.len(), 1);

        tx_a.send(1).unwrap();
        assert_eq!(flights.poll_next(&mut cx), Poll::Ready(Some(1)));
        assert!(flights.is_empty());
        assert_eq!(flights.poll_next(&mut cx), Poll::Ready(None));
    }

    #[test]
    fn a_future_finishing_at_its_first_poll_is_returned_at_once() {
        let mut flights: Flights<'_, u32> = Flights::new();
        assert_eq!(flights.start(async { 7 }.boxed()), Some(7));
        assert!(flights.is_empty());
    }

    #[test]
    fn a_wake_before_the_run_first_looks_is_not_lost() {
        let mut flights = Flights::new();
        let (tx, rx) = oneshot::channel();
        assert!(flights.start(pending_until(rx)).is_none());
        tx.send(3).unwrap();

        let wakes = Arc::new(CountWakes(AtomicUsize::new(0)));
        let run = waker(Arc::clone(&wakes));
        assert_eq!(
            flights.poll_next(&mut Context::from_waker(&run)),
            Poll::Ready(Some(3))
        );
    }

    #[test]
    fn only_the_runs_latest_waker_is_woken() {
        let mut flights = Flights::new();
        let (tx, rx) = oneshot::channel();
        assert!(flights.start(pending_until(rx)).is_none());
        let a = Arc::new(CountWakes(AtomicUsize::new(0)));
        let b = Arc::new(CountWakes(AtomicUsize::new(0)));
        let wa = waker(Arc::clone(&a));
        let wb = waker(Arc::clone(&b));
        assert!(
            flights
                .poll_next(&mut Context::from_waker(&wa))
                .is_pending()
        );
        assert!(
            flights
                .poll_next(&mut Context::from_waker(&wb))
                .is_pending()
        );
        tx.send(1).unwrap();
        assert_eq!(a.0.load(Ordering::SeqCst), 0);
        assert_eq!(b.0.load(Ordering::SeqCst), 1);
        assert_eq!(
            flights.poll_next(&mut Context::from_waker(&wb)),
            Poll::Ready(Some(1))
        );
    }

    #[test]
    fn a_wake_after_its_future_finished_leaves_the_others_in_flight() {
        let mut flights = Flights::new();
        let stash: Arc<std::sync::Mutex<Option<std::task::Waker>>> = Arc::default();
        let s2 = Arc::clone(&stash);
        let mut first = true;
        let fut = std::future::poll_fn(move |cx| {
            if first {
                first = false;
                *s2.lock().unwrap() = Some(cx.waker().clone());
                cx.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(5u32)
            }
        })
        .boxed();
        assert!(flights.start(fut).is_none());
        let (_tx, rx) = oneshot::channel();
        assert!(flights.start(pending_until(rx)).is_none());
        let w = Arc::new(CountWakes(AtomicUsize::new(0)));
        let run = waker(Arc::clone(&w));
        assert_eq!(
            flights.poll_next(&mut Context::from_waker(&run)),
            Poll::Ready(Some(5))
        );
        stash.lock().unwrap().take().unwrap().wake();
        assert!(
            flights
                .poll_next(&mut Context::from_waker(&run))
                .is_pending()
        );
        assert_eq!(flights.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn wakes_from_other_threads_are_never_lost() {
        fn rnd(x: &mut u64) -> u64 {
            *x ^= *x << 13;
            *x ^= *x >> 7;
            *x ^= *x << 17;
            *x
        }
        for round in 0..200u32 {
            let mut flights = Flights::new();
            let mut senders = Vec::new();
            let n = 64u32;
            let mut got = 0u32;
            for i in 0..n {
                let (tx, rx) = oneshot::channel();
                senders.push((i, tx));
                if let Some(_v) = flights.start(pending_until(rx)) {
                    got += 1;
                }
            }
            let handle = std::thread::spawn(move || {
                let mut x = 0x9E37_79B9_7F4A_7C15u64 ^ u64::from(round);
                for (i, tx) in senders {
                    if rnd(&mut x) % 10 < 3 {
                        std::thread::sleep(std::time::Duration::from_micros(rnd(&mut x) % 50));
                    } else {
                        std::thread::yield_now();
                    }
                    let _ = tx.send(i);
                }
            });
            let collected = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let mut seen = 0u32;
                let mut y = 12345u64 + u64::from(round);
                while let Some(_v) = flights.next().await {
                    seen += 1;
                    if rnd(&mut y) % 5 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
                seen
            })
            .await
            .unwrap_or_else(|_| panic!("round {round} hung"));
            handle.join().unwrap();
            assert_eq!(collected + got, n);
        }
    }
}
