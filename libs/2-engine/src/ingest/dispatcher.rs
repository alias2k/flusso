//! The dispatcher: the future of an engine run that owns its result (ADR
//! 0012 stage 8a, §3.9 §2 and I10). It routes each backfill request to its
//! index's builder without ever waiting on one, confirms to the source,
//! ticks, signals the builders the end and the stop, and finishes the run:
//! the wind-down's drain and close at a stop, the last confirmation at the
//! stream's end, or the first error (a builder's, the reader's, its own).
//!
//! **Requests.** Every request ready on the request lane is forwarded to its
//! index's builder each time the dispatcher is polled, so a builder that
//! finds its inbox empty has every request that was ready (`main`'s "what is
//! already queued still joins"). A request for an index no builder builds
//! (no lane of it) is warned about and acknowledged. Each builder coalesces
//! its own requests on a deadline (`builder.rs`); the dispatcher holds none.
//! After the stop nothing more is forwarded: what stays on the lane is
//! served on the next start.
//!
//! **Confirmation** runs once per poll in which something was registered
//! ([`Progress::poll_registered`](crate::Progress::poll_registered): a run
//! routed, a build's lanes, a release), after the future that registered,
//! and on every tick.
//!
//! **The end.** A builder reports itself idle once the reader ended and
//! nothing is left for it to serve. With every builder idle and the request
//! lane empty the dispatcher signals the end to all of them and forwards
//! nothing after; a builder settles only while idle itself, so a request
//! forwarded just before is still served (CLAUDE.md: "a live stream that
//! ends still drains queued requests").
//!
//! ```text
//! request lane ─▶ dispatcher ──inbox (per index)──▶ builders ──results──▶ dispatcher ─▶ Ok / Err
//!                    │  ▲                               ▲
//!   registration ────┘  └────────── tick, stop, end ────┘
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use futures::FutureExt;
use futures::task::AtomicWaker;
use kernel::IndexName;
use source::cdc::Landed;
use stream::{Consumer, Delivery, Request};
use tokio::sync::{mpsc, oneshot};

use super::builder::BuilderEnd;
use super::confirm::CONFIRM_TICK;
use super::reader::Lead;
use super::reader::{self, ReaderEnd};
use super::snapshot::ClosedConsumer;
use super::{Ended, IngestEngine};
use crate::error::{EngineError, Result};
use crate::stop::drain_lanes;

/// What the dispatcher and one builder tell each other, inside the task.
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
    /// The builder is idle at the stream's end (the reader ended and nothing
    /// is left for it to serve) and said so; cleared when it takes a
    /// request, so it says so once per time it goes idle.
    idle: AtomicBool,
}

/// The builders' reports to the dispatcher: one flag and one waker for all.
#[derive(Debug, Default)]
pub(super) struct Reports {
    /// A builder went idle since the dispatcher looked.
    reported: AtomicBool,
    dispatcher: AtomicWaker,
}

/// What wakes a builder from the dispatcher.
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
    pub(super) fn report_idle(&self, reports: &Reports) {
        if !self.idle.swap(true, Ordering::AcqRel) {
            reports.reported.store(true, Ordering::Release);
            reports.dispatcher.wake();
        }
    }

    /// Tell the builder to look: at the stop, or the end.
    fn raise(&self) {
        self.look.store(true, Ordering::Release);
        self.builder.wake();
    }

    fn is_idle(&self) -> bool {
        self.idle.load(Ordering::Acquire)
    }
}

impl Reports {
    /// Ready once a builder reported itself idle since the last time.
    fn poll_idle(&self, cx: &mut Context<'_>) -> Poll<()> {
        self.dispatcher.register(cx.waker());
        if self.reported.swap(false, Ordering::AcqRel) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/// How the request side of a run ended.
enum Fed {
    /// The end was signalled: nothing more is forwarded.
    End,
    /// A builder is gone; its result is on the way.
    Gone,
}

/// Where one builder's requests go: backfills to its inbox, fresh builds to
/// their own channel, so a walk in progress doesn't hold a parked lane's
/// fresh build back (stage 9).
#[derive(Clone)]
pub(super) struct Route {
    pub(super) requests: mpsc::UnboundedSender<Delivery<Request>>,
    pub(super) rebuilds: mpsc::UnboundedSender<Delivery<Request>>,
}

/// One builder as the dispatcher sees it: its index's inbox, its signals,
/// and its result once it returned.
pub(super) struct Builder<'a> {
    pub(super) index: IndexName,
    pub(super) route: Route,
    pub(super) signals: &'a Signals,
    pub(super) result: oneshot::Receiver<Result<BuilderEnd>>,
}

impl IngestEngine {
    /// The dispatcher of one engine run (see the module docs).
    pub(super) async fn dispatch(
        &self,
        mut requests: Box<dyn Consumer<Request>>,
        builders: Vec<Builder<'_>>,
        reports: &Reports,
        mut reader: ReaderEnd,
        lead: Arc<Lead>,
    ) -> Result<Ended> {
        let mut last_confirmed: Option<Landed> = None;
        let mut tick = tokio::time::interval(CONFIRM_TICK);
        let mut stopping = false;
        let signals: Vec<&Signals> = builders.iter().map(|builder| builder.signals).collect();
        let routes: Vec<(IndexName, Route)> = builders
            .iter()
            .map(|builder| (builder.index.clone(), builder.route.clone()))
            .collect();
        let mut results: Vec<oneshot::Receiver<Result<BuilderEnd>>> =
            builders.into_iter().map(|builder| builder.result).collect();
        let mut ends: Vec<Option<BuilderEnd>> = results.iter().map(|_| None).collect();
        let mut reader_end: Option<reader::Ended> = None;
        // The request side, one future for the run (`feed`): it keeps the
        // lane's `recv` across polls, so a wake for a registration or the
        // tick creates no future and takes no lock there.
        let mut feed = std::pin::pin!(feed(&mut requests, &routes, &signals, reports));
        let mut fed = false;
        loop {
            if !stopping && self.stop.is_requested() {
                stopping = true;
                for builder in &signals {
                    builder.raise();
                }
            }
            if reader_end.is_some() && ends.iter().all(Option::is_some) {
                // Every builder built what it was handed, an exempt backlog
                // included (O3): a document still waiting is a read change
                // unbuilt, never an `Ok` end.
                let left = self.progress.waiting_by_index();
                if !left.is_empty() {
                    return Err(EngineError::DocumentsLeft(
                        left.iter()
                            .map(|(index, count)| format!("{index}: {count}"))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ));
                }
                let stopped = ends.iter().flatten().any(|end| end.stopped);
                if let Some(ended) = reader_end.take() {
                    return self.finish(stopped, ended, &mut last_confirmed).await;
                }
            }
            tokio::select! {
                // Every builder's result, as it comes: the first error ends
                // the run, dropping the others in place.
                returned = std::future::poll_fn(|cx| {
                    let mut ready = false;
                    for (result, end) in results.iter_mut().zip(ends.iter_mut()) {
                        if end.is_none()
                            && let Poll::Ready(outcome) = result.poll_unpin(cx)
                        {
                            *end = Some(match outcome {
                                Ok(Ok(builder)) => builder,
                                Ok(Err(error)) => return Poll::Ready(Err(error)),
                                Err(_) => return Poll::Ready(Err(builder_gone())),
                            });
                            ready = true;
                        }
                    }
                    if ready { Poll::Ready(Ok(())) } else { Poll::Pending }
                }) => {
                    returned?;
                }
                ended = reader.join(), if reader_end.is_none() => {
                    reader_end = Some(ended?);
                }
                // The watch wakes the dispatcher when the stop is requested:
                // the loop's top raises the builders.
                () = std::future::poll_fn(|_| {
                    if self.stop.is_requested() { Poll::Ready(()) } else { Poll::Pending }
                }), if !stopping => {}
                // Confirms inside its poll and stays pending, so a
                // registration costs one pass through this `select!`, not a
                // second one to park again.
                () = std::future::poll_fn(|cx| {
                    if self.progress.poll_registered(cx).is_ready() {
                        self.confirm(&mut last_confirmed, false);
                    }
                    Poll::<()>::Pending
                }) => {}
                outcome = &mut feed, if !stopping && !fed => {
                    fed = true;
                    if let Fed::End = outcome? {
                        tracing::debug!("the stream ended and nothing waits: the run ends");
                    }
                }
                _ = tick.tick() => {
                    self.confirm(&mut last_confirmed, true);
                    // A reader stopped because no index was free re-checks
                    // once a second, and the exemption's lines are checked.
                    lead.tick(&self.progress);
                    for builder in &signals {
                        builder.tick.store(true, Ordering::Release);
                        builder.builder.wake();
                    }
                }
            }
        }
    }

    /// The run's end once every builder returned and the reader ended: at a
    /// stop, wait until every lane acknowledged what was built (confirming as
    /// it moves) and close the live stream; at the stream's end, confirm.
    async fn finish(
        &self,
        stopped: bool,
        reader: reader::Ended,
        last_confirmed: &mut Option<Landed>,
    ) -> Result<Ended> {
        if !stopped {
            self.confirm(last_confirmed, false);
            return Ok(Ended::LiveEnded);
        }
        drain_lanes(&*self.stream, &*self.source, &self.progress, last_confirmed).await;
        tracing::info!(position = ?last_confirmed, "every sink acknowledged; closing the live stream");
        self.close(reader.live, reader.stream_ended)
            .await
            .map(|()| Ended::Stopped)
    }
}

/// Hand one request to its index's builder; a request for an index no
/// builder builds is warned about and acknowledged, and a closed request
/// lane is replaced by one that never yields, warned about once. `false`
/// once the builder is gone: the request goes back to its lane
/// unacknowledged (its handle dropped), and nothing more is forwarded, or it
/// would be received again at once, forever.
async fn forward(
    request: Option<Delivery<Request>>,
    requests: &mut Box<dyn Consumer<Request>>,
    routes: &[(IndexName, Route)],
) -> Result<bool> {
    let Some(delivery) = request else {
        tracing::warn!("request lane closed; no further backfills can be served");
        *requests = Box::new(ClosedConsumer);
        return Ok(true);
    };
    let (index, sink) = (delivery.item().index(), delivery.item().sink());
    match routes.iter().find(|(own, _)| own == index) {
        Some((_, route)) => Ok(match delivery.item() {
            Request::Backfill { .. } => route.requests.send(delivery).is_ok(),
            Request::Rebuild { .. } => route.rebuilds.send(delivery).is_ok(),
        }),
        None => {
            tracing::warn!(%index, %sink, "a request for an index with no lane here; ignoring");
            let (_, handle) = delivery.into_parts();
            handle.ack().await?;
            Ok(true)
        }
    }
}

/// Forward every request ready now, until a builder is gone.
async fn forward_ready(
    requests: &mut Box<dyn Consumer<Request>>,
    routes: &[(IndexName, Route)],
) -> Result<bool> {
    while !requests.is_empty() {
        match requests.recv().now_or_never() {
            Some(request) => {
                if !forward(request?, requests, routes).await? {
                    return Ok(false);
                }
            }
            None => break,
        }
    }
    Ok(true)
}

/// The request side of a run: forward every request to its builder as it
/// arrives, and on an idle report forward what is ready and, with every
/// builder idle and the lane empty, signal the end (see the module docs).
/// Returns once the end is signalled or a builder is gone. The lane's `recv`
/// lives across polls until it yields or a report comes first; then it is
/// dropped (it is cancel-safe) and the lane is used directly.
async fn feed(
    requests: &mut Box<dyn Consumer<Request>>,
    routes: &[(IndexName, Route)],
    signals: &[&Signals],
    reports: &Reports,
) -> Result<Fed> {
    loop {
        if routes.iter().any(|(_, route)| route.requests.is_closed()) {
            return Ok(Fed::Gone);
        }
        let next = {
            let mut recv = requests.recv();
            std::future::poll_fn(|cx| {
                if let Poll::Ready(request) = recv.as_mut().poll(cx) {
                    return Poll::Ready(Some(request));
                }
                reports.poll_idle(cx).map(|()| None)
            })
            .await
        };
        match next {
            Some(request) => {
                if !forward(request?, requests, routes).await?
                    || !forward_ready(requests, routes).await?
                {
                    return Ok(Fed::Gone);
                }
            }
            // The end, once every builder is idle and nothing waits on the
            // request lane. A request forwarded since a builder went idle is
            // still served: a builder acts on the end only while idle itself.
            None => {
                if !forward_ready(requests, routes).await? {
                    return Ok(Fed::Gone);
                }
                // Drained just above; still not empty only while a sink's
                // request is half published, which must not be left.
                if requests.is_empty() && signals.iter().all(|builder| builder.is_idle()) {
                    for builder in signals {
                        builder.end.store(true, Ordering::Release);
                        builder.raise();
                    }
                    return Ok(Fed::End);
                }
            }
        }
    }
}

fn builder_gone() -> EngineError {
    EngineError::Source(source::SourceError::Connection(
        "a builder ended without an answer".to_owned(),
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
        let (rebuilds, _kept) = mpsc::unbounded_channel();
        let routes = vec![(
            IndexName::try_new("users").unwrap(),
            Route {
                requests: inbox,
                rebuilds,
            },
        )];
        forward_ready(&mut requests, &routes).await.unwrap();
        assert!(!requests.is_empty(), "the request is back on its lane");
    }
}
