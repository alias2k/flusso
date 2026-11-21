//! The dispatcher: the future of an engine run that owns its result (ADR
//! 0012 stage 8a, §3.9 §2 and I10). It routes backfill requests to the
//! builder without ever waiting on it, confirms to the source, ticks, signals
//! the builder the end and the stop, and finishes the run: the wind-down's
//! drain and close at a stop, the last confirmation at the stream's end, or
//! the builder's error.
//!
//! **Requests.** Every request ready on the request lane is forwarded to the
//! builder's inbox each time the dispatcher is polled, so a builder that
//! finds its inbox empty has every request that was ready (`main`'s "what is
//! already queued still joins"). The builder coalesces them on a deadline
//! (`builder.rs`); the dispatcher holds none. After the stop nothing more is
//! forwarded: what stays on the lane is served on the next start.
//!
//! **Confirmation** runs once per poll in which something was registered
//! ([`Progress::poll_registered`](crate::Progress::poll_registered): a run routed, a build's lanes, a release),
//! after the future that registered, and on every tick, as 6a's builder
//! confirmed after each of its arms and on the tick.
//!
//! **The end.** The builder reports itself idle once the reader ended with
//! its stream and nothing is left to serve. The dispatcher then forwards
//! what is ready and signals the end once the request lane is empty, and
//! forwards nothing after; the builder settles only while idle, so a request
//! forwarded just before is still served (CLAUDE.md: "a live stream that
//! ends still drains queued requests").
//!
//! ```text
//! request lane ─▶ dispatcher ──inbox──▶ builder ──result──▶ dispatcher ─▶ Ok / Err
//!                    │  ▲                  ▲
//!   registration ────┘  └── tick, stop, end┘
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;

use futures::FutureExt;
use futures::task::AtomicWaker;
use source::cdc::Landed;
use stream::{Consumer, Delivery, Request};
use tokio::sync::{mpsc, oneshot};

use super::builder::BuilderEnd;
use super::confirm::CONFIRM_TICK;
use super::snapshot::ClosedConsumer;
use super::{Ended, IngestEngine};
use crate::error::{EngineError, Result};
use crate::stop::drain_lanes;

/// What the dispatcher and the builder tell each other, inside the task.
#[derive(Debug, Default)]
pub(super) struct Signals {
    /// Dispatcher → builder: settle and return.
    end: AtomicBool,
    /// Dispatcher → builder: the end or the stop came, look (raised once
    /// each, so a builder that can't act on it yet doesn't spin).
    look: AtomicBool,
    /// Dispatcher → builder: a second passed (the budget's backstop grant).
    tick: AtomicBool,
    /// The builder's waker, for the end, the tick and the stop.
    builder: AtomicWaker,
    /// The builder is idle at the stream's end (the reader ended with its
    /// stream and nothing is left to serve) and said so; cleared when it
    /// takes a request, so it says so once per time it goes idle.
    idle: AtomicBool,
    /// Builder → dispatcher: it went idle since the dispatcher looked.
    reported: AtomicBool,
    /// The dispatcher's waker, for the builder's report.
    dispatcher: AtomicWaker,
}

/// What wakes the builder from the dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Signal {
    /// A second passed.
    Tick,
    /// The end or the stop: the builder's loop looks at both.
    Look,
}

impl Signals {
    /// The builder's next signal. The builder registers nothing on the
    /// stop: the dispatcher raises it once the stop is requested.
    pub(super) async fn next(&self) -> Signal {
        std::future::poll_fn(|cx| {
            self.builder.register(cx.waker());
            if self.look.swap(false, Ordering::AcqRel) {
                return Poll::Ready(Signal::Look);
            }
            if self.tick.swap(false, Ordering::AcqRel) {
                return Poll::Ready(Signal::Tick);
            }
            Poll::Pending
        })
        .await
    }

    /// Whether the dispatcher signalled the end.
    pub(super) fn ended(&self) -> bool {
        self.end.load(Ordering::Acquire)
    }

    /// The builder took a request from its inbox: it is busy again.
    pub(super) fn received(&self) {
        self.idle.store(false, Ordering::Release);
    }

    /// The builder is idle at the stream's end: tell the dispatcher once.
    pub(super) fn report_idle(&self) {
        if !self.idle.swap(true, Ordering::AcqRel) {
            self.reported.store(true, Ordering::Release);
            self.dispatcher.wake();
        }
    }

    /// Tell the builder to look: at the stop, or the end.
    fn raise_builder(&self) {
        self.look.store(true, Ordering::Release);
        self.builder.wake();
    }

    /// Ready once the builder reported itself idle since the last time.
    async fn idle_reported(&self) {
        std::future::poll_fn(|cx| {
            self.dispatcher.register(cx.waker());
            if self.reported.swap(false, Ordering::AcqRel) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

impl IngestEngine {
    /// The dispatcher of one engine run (see the module docs): `requests` in,
    /// the builder's `inbox` and `signals` out, its `result` back.
    pub(super) async fn dispatch(
        &self,
        mut requests: Box<dyn Consumer<Request>>,
        inbox: mpsc::UnboundedSender<Delivery<Request>>,
        mut result: oneshot::Receiver<Result<BuilderEnd>>,
        signals: &Signals,
    ) -> Result<Ended> {
        let mut last_confirmed: Option<Landed> = None;
        let mut tick = tokio::time::interval(CONFIRM_TICK);
        let mut stopping = false;
        // Once the end is signalled nothing more is forwarded: as on `main`,
        // a request arriving after the run decided to end waits for the next.
        let mut ending = false;
        loop {
            if !stopping && self.stop.is_requested() {
                stopping = true;
                signals.raise_builder();
            }
            tokio::select! {
                ended = &mut result => {
                    let ended = ended.unwrap_or_else(|_| Err(builder_gone()))?;
                    return self.finish(ended, &mut last_confirmed).await;
                }
                // The watch wakes the dispatcher when the stop is requested:
                // the loop's top raises the builder.
                () = std::future::poll_fn(|_| {
                    if self.stop.is_requested() { Poll::Ready(()) } else { Poll::Pending }
                }), if !stopping => {}
                () = std::future::poll_fn(|cx| self.progress.poll_registered(cx)) => {
                    self.confirm(&mut last_confirmed, false);
                }
                // A builder gone (it returned, its result on the way) takes
                // nothing more: the next poll reads its result.
                request = requests.recv(), if !stopping && !ending && !inbox.is_closed() => {
                    if forward(request?, &mut requests, &inbox) {
                        forward_ready(&mut requests, &inbox)?;
                    }
                }
                // The end, once nothing waits on the request lane. A request
                // forwarded since the builder went idle is still served: the
                // builder acts on the end only while idle itself.
                () = signals.idle_reported(), if !stopping && !ending && !inbox.is_closed() => {
                    forward_ready(&mut requests, &inbox)?;
                    // Drained just above; still not empty only while a sink's
                    // request is half published, which must not be left.
                    if requests.is_empty() {
                        ending = true;
                        signals.end.store(true, Ordering::Release);
                        signals.raise_builder();
                    }
                }
                _ = tick.tick() => {
                    self.confirm(&mut last_confirmed, true);
                    signals.tick.store(true, Ordering::Release);
                    signals.builder.wake();
                }
            }
        }
    }

    /// The run's end once the builder returned: at a stop, wait until every
    /// lane acknowledged what was built (confirming as it moves) and close the
    /// live stream; at the stream's end, confirm.
    async fn finish(
        &self,
        ended: BuilderEnd,
        last_confirmed: &mut Option<Landed>,
    ) -> Result<Ended> {
        if !ended.stopped {
            self.confirm(last_confirmed, false);
            return Ok(Ended::LiveEnded);
        }
        drain_lanes(&*self.stream, &*self.source, &self.progress, last_confirmed).await;
        tracing::info!(position = ?last_confirmed, "every sink acknowledged; closing the live stream");
        self.close(ended.reader.live, ended.reader.stream_ended)
            .await
            .map(|()| Ended::Stopped)
    }
}

/// Hand one request to the builder; a closed request lane is replaced by one
/// that never yields, and warned about once. `false` once the builder is gone:
/// the request goes back to its lane unacknowledged (its handle dropped), and
/// nothing more is forwarded, or it would be received again at once, forever.
fn forward(
    request: Option<Delivery<Request>>,
    requests: &mut Box<dyn Consumer<Request>>,
    inbox: &mpsc::UnboundedSender<Delivery<Request>>,
) -> bool {
    match request {
        Some(delivery) => inbox.send(delivery).is_ok(),
        None => {
            tracing::warn!("request lane closed; no further backfills can be served");
            *requests = Box::new(ClosedConsumer);
            true
        }
    }
}

/// Forward every request ready now, until the builder is gone.
fn forward_ready(
    requests: &mut Box<dyn Consumer<Request>>,
    inbox: &mpsc::UnboundedSender<Delivery<Request>>,
) -> Result<()> {
    while !requests.is_empty() {
        match requests.recv().now_or_never() {
            Some(request) => {
                if !forward(request?, requests, inbox) {
                    break;
                }
            }
            None => break,
        }
    }
    Ok(())
}

fn builder_gone() -> EngineError {
    EngineError::Source(source::SourceError::Connection(
        "the builder ended without an answer".to_owned(),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use kernel::{IndexName, SinkName};
    use stream::{LaneKey, Stream};
    use stream_channel::ChannelStream;

    use super::*;

    fn request() -> Request {
        Request::Backfill {
            index: IndexName::try_new("users").unwrap(),
            sink: SinkName::try_new("primary").unwrap(),
            after: None,
            ticket: stream::Ticket::default(),
        }
    }

    /// Delegates to a real lane and panics past `left` receives: a loop
    /// fails instead of hanging the test.
    #[derive(Debug)]
    struct Bounded {
        inner: Box<dyn Consumer<Request>>,
        left: usize,
    }

    #[async_trait::async_trait]
    impl Consumer<Request> for Bounded {
        async fn recv(&mut self) -> stream::Result<Option<Delivery<Request>>> {
            assert!(self.left > 0, "received again and again");
            self.left -= 1;
            self.inner.recv().await
        }

        fn is_empty(&self) -> bool {
            self.inner.is_empty()
        }
    }

    /// A builder that returned (its error on the way to the dispatcher)
    /// drops its inbox. A request forwarded then goes back to its lane, and
    /// forwarding stops: received again at once, it would loop for ever
    /// (a random schedule found it).
    #[tokio::test]
    async fn a_request_for_a_builder_gone_goes_back_to_its_lane_once() {
        let stream = ChannelStream::new(
            8,
            [LaneKey::new(
                IndexName::try_new("users").unwrap(),
                SinkName::try_new("primary").unwrap(),
            )],
        );
        let lane = stream.requests().unwrap();
        lane.producer.publish(request()).await.unwrap();
        let mut requests: Box<dyn Consumer<Request>> = Box::new(Bounded {
            inner: lane.consumer,
            left: 10,
        });
        let (inbox, gone) = mpsc::unbounded_channel();
        drop(gone);
        forward_ready(&mut requests, &inbox).unwrap();
        assert!(!requests.is_empty(), "the request is back on its lane");
    }
}
