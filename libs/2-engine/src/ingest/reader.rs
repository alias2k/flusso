//! The reader: the future that reads the live stream, resolves each change to
//! the documents it touches, and marks them pending, ahead of the builds (ADR
//! 0012 stage 8a, slice 6a; the rules R1–R10 are in the stage note, §3.8).
//! The engine run polls it beside the builder in its own task ([`reading`],
//! [`together`](mod@super::together)): a hand-over or a grant wakes one task,
//! never another worker, and the two interleave at their awaits as `main`'s
//! one task did. Which of the two a pass polls first decides nothing: a run
//! handed over is queued before any batch is formed (R3).
//!
//! **A run** is what one read takes: every change the stream has ready, cut
//! at `max_changes` changes, at `max_delay` from its first change, when the
//! stream would block, or at the stop (R1). Per run, in this order:
//!
//! 1. resolve each change, in position order;
//! 2. mark every document in [`Progress`] (one lock per touched index; a
//!    document of an index with no lane is dropped there);
//! 3. count the run's changes read ([`Progress::read_changes`]);
//! 4. hand the run to the builder ([`Run`], over an unbounded channel).
//!
//! So whatever the builder sees of a run is already marked and counted.
//!
//! **The lead** (R2). Before reading a run the reader waits while one index
//! has [`MaxWaiting::per_index`] documents waiting (marked, not taken), every
//! index together has [`MaxWaiting::total`], or `total` changes are read and
//! not yet in a granted batch. It waits on the builder's take and on the
//! stop, registering the wake-up before it reads the lead, so a take between
//! the two still wakes it.
//!
//! **The end** (R7, R8). The stop is checked before every read. Once it is
//! requested, or the stream ends, the reader hands over the run it was
//! reading, closes the channel by ending, and returns the live stream, which
//! the builder closes. An error (the stream's, a resolve's) ends it the same
//! way, without handing over the run in progress. On every path its changes
//! count as read, the run dropped included (the run's guard counts them when
//! it is dropped), as each change did when `main` read it.
//!
//! ```text
//! stream ─▶ resolve ─▶ mark ─▶ count ─▶ Run ──channel──▶ builder
//!   ▲                                                       │
//!   └──────────── wait while the lead is reached ◀── take ──┘
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering, fence};
use std::task::Poll;

use futures::task::AtomicWaker;
use futures::{FutureExt, StreamExt};
use kernel::Position;
use source::cdc::{ChangeEvent, LiveChange};
use source::document::{DocumentBuilder, DocumentId};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, Instant};

use super::LiveStream;
use crate::error::{EngineError, Result};
use crate::observer::Observer;
use crate::policy::{BatchPolicy, MaxWaiting};
use crate::progress::Progress;
use crate::stop::Stop;

/// One run, handed to the builder once its documents are marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Run {
    /// Each change, in order, with the moment the reader read it: the
    /// builder cuts batches by these as `main` cut them (R3).
    pub(super) changes: Vec<(Position, Instant)>,
    /// How many changes had been read through its last, over the engine's
    /// life: what a step through it closes at.
    pub(super) read: u64,
    /// How long reading and resolving it took: the builder reports it as
    /// [`Stage::Resolve`](crate::Stage::Resolve) once it closed the idle
    /// wait before it, so the two are reported in the order they happened.
    pub(super) resolved: Duration,
}

impl Run {
    /// When its first change was read: the builder's idle wait ends here.
    pub(super) fn started(&self) -> Option<Instant> {
        self.changes.first().map(|(_, at)| *at)
    }
}

/// What the reader and the builder share for the lead.
#[derive(Debug)]
pub(super) struct Lead {
    max: MaxWaiting,
    /// The reader's wake-up, woken after a live take while it waits: the
    /// lead may have room. One waiter, so no list and no lock.
    taken: AtomicWaker,
    /// How many changes had been read through the last granted batch.
    granted: AtomicU64,
    /// The reader waits, or is about to: a grant wakes it only then, so a
    /// reader that keeps up costs no wake-up.
    waiting: AtomicBool,
}

impl Lead {
    /// A lead of `max`, nothing read ahead past `read`.
    pub(super) fn new(max: MaxWaiting, read: u64) -> Arc<Self> {
        Arc::new(Self {
            max,
            taken: AtomicWaker::new(),
            granted: AtomicU64::new(read),
            waiting: AtomicBool::new(false),
        })
    }

    /// A live batch through the `read`th change was granted and its
    /// documents taken: the reader may have room again.
    pub(super) fn granted(&self, read: u64) {
        self.granted.fetch_max(read, Ordering::Relaxed);
        // Pairs with the fence in `Reader::room`: either the reader sees
        // this grant's take when it reads the lead, or this sees it waiting.
        fence(Ordering::SeqCst);
        if self.waiting.load(Ordering::Relaxed) {
            self.taken.wake();
        }
    }

    /// Whether the reader is as far ahead as it may be.
    fn reached(&self, progress: &Progress) -> bool {
        progress.most_waiting() >= self.max.per_index
            || progress.waiting() >= self.max.total
            || progress
                .changes_read()
                .saturating_sub(self.granted.load(Ordering::Relaxed))
                >= self.max.total as u64
    }
}

/// Where a test's hook in [`Reader::room`] runs, once the lead was found
/// reached.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Check {
    /// After the first check, before the wake-up is registered.
    BeforeRegistering,
    /// After the check made once it is registered, just before waiting.
    BeforeWaiting,
}

/// How the reader ended without an error.
pub(super) struct Ended {
    /// The live stream, for the builder to close.
    pub(super) live: LiveStream,
    /// The stream ended (rather than the stop being requested).
    pub(super) stream_ended: bool,
    /// It ended while waiting (on the stream, or at its lead), not in the
    /// middle of a run: the builder's wait until then was idle.
    pub(super) waiting: bool,
}

/// The reader of one engine run (its stream is handed to [`reading`] apart:
/// a stream is `Send`, not `Sync`).
pub(super) struct Reader {
    pub(super) resolver: Arc<dyn DocumentBuilder>,
    pub(super) progress: Arc<Progress>,
    pub(super) observer: Arc<dyn Observer>,
    pub(super) stop: Stop,
    pub(super) batch: BatchPolicy,
    pub(super) lead: Arc<Lead>,
    pub(super) runs: mpsc::UnboundedSender<Run>,
    /// A test's look at each run just before it is handed over.
    #[cfg(test)]
    pub(super) handing: Option<Box<dyn Fn(&Run) + Send + Sync>>,
    /// A test's hook where the lead was found reached: before the wake-up
    /// is registered, and after, just before waiting.
    #[cfg(test)]
    pub(super) checked: Option<Box<dyn Fn(Check) + Send + Sync>>,
}

/// The run being read: its documents with their changes' positions, each
/// change with when it was read, and how many changes were read and not yet
/// counted. Dropped with changes still uncounted (an error, the engine run
/// dropped), it counts them read, so `changes_captured` and the
/// read count move together on every path, as `main` counted each change as
/// it read it.
struct Reading<'a> {
    progress: &'a Progress,
    marks: Vec<(DocumentId, Position)>,
    changes: Vec<(Position, Instant)>,
    uncounted: usize,
    deadline: Instant,
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        if self.uncounted > 0 {
            self.progress.read_changes(self.uncounted as u64);
        }
    }
}

impl Reader {
    /// Read runs until the stop or the stream's end (see the module docs).
    pub(super) async fn run(self, mut live: LiveStream) -> Result<Ended> {
        let mut reading = Reading {
            progress: &self.progress,
            marks: Vec::with_capacity(self.batch.max_changes),
            changes: Vec::new(),
            uncounted: 0,
            deadline: Instant::now(),
        };
        loop {
            if !self.room().await {
                return Ok(end(live, false, true));
            }
            let (position, event) = match next_live(&mut live, &self.stop).await {
                Next::Stopped => return Ok(end(live, false, true)),
                Next::Change(None) => return Ok(end(live, true, true)),
                Next::Change(Some(Err(error))) => return Err(error.into()),
                Next::Change(Some(Ok(change))) => change,
            };
            let started = Instant::now();
            reading.deadline = started + self.batch.max_delay;
            reading.changes = Vec::with_capacity(self.batch.max_changes.min(64));
            self.resolve(&mut reading, position, event, started).await?;
            let ended = self.drain_ready(&mut live, &mut reading).await?;
            self.hand_over(&mut reading, started);
            if ended {
                return Ok(end(live, true, false));
            }
        }
    }

    /// Wait while the lead is reached. `false` when the stop came first.
    /// The wake-up is registered before the lead is read again, so a take
    /// between the two still wakes it. The stop is read on every poll; what
    /// wakes a reader waiting here for it is the engine run's watch
    /// ([`together`](mod@super::together)).
    async fn room(&self) -> bool {
        // Room now: nothing to register.
        if !self.lead.reached(&self.progress) {
            return !self.stop.is_requested();
        }
        #[cfg(test)]
        if let Some(checked) = &self.checked {
            checked(Check::BeforeRegistering);
        }
        std::future::poll_fn(|cx| {
            self.lead.taken.register(cx.waker());
            self.lead.waiting.store(true, Ordering::Relaxed);
            fence(Ordering::SeqCst);
            if self.stop.is_requested() || !self.lead.reached(&self.progress) {
                self.lead.waiting.store(false, Ordering::Relaxed);
                return Poll::Ready(!self.stop.is_requested());
            }
            #[cfg(test)]
            if let Some(checked) = &self.checked {
                checked(Check::BeforeWaiting);
            }
            Poll::Pending
        })
        .await
    }

    /// Read every change the stream has ready right now into the run, until
    /// it is full, past its deadline, the stream would block, or the stop is
    /// requested. Returns whether the stream ended.
    async fn drain_ready(&self, live: &mut LiveStream, reading: &mut Reading<'_>) -> Result<bool> {
        loop {
            // A full run reads no clock.
            if reading.changes.len() >= self.batch.max_changes || self.stop.is_requested() {
                return Ok(false);
            }
            let now = Instant::now();
            if now >= reading.deadline {
                return Ok(false);
            }
            match live.next().now_or_never() {
                None => return Ok(false),
                Some(None) => return Ok(true),
                Some(Some(Err(error))) => return Err(error.into()),
                Some(Some(Ok((position, event)))) => {
                    self.resolve(reading, position, event, now).await?;
                }
            }
        }
    }

    /// Resolve one change into the run: the documents it touches, each with
    /// its position.
    async fn resolve(
        &self,
        reading: &mut Reading<'_>,
        position: Position,
        event: ChangeEvent,
        at: Instant,
    ) -> Result<()> {
        self.observer.on_change_captured();
        reading.uncounted += 1;
        let affected = self
            .resolver
            .resolve(event.table(), event.key(), event.before(), event.after())
            .await?;
        tracing::trace!(documents = affected.len(), "change resolved to documents");
        reading
            .marks
            .extend(affected.into_iter().map(|id| (id, position)));
        reading.changes.push((position, at));
        Ok(())
    }

    /// Mark the run's documents, count its changes, then hand it over (R1).
    fn hand_over(&self, reading: &mut Reading<'_>, started: Instant) {
        if reading.changes.is_empty() {
            return;
        }
        self.progress.mark(reading.marks.drain(..));
        let read = self
            .progress
            .read_changes(std::mem::take(&mut reading.uncounted) as u64);
        let run = Run {
            changes: std::mem::take(&mut reading.changes),
            read,
            resolved: started.elapsed(),
        };
        #[cfg(test)]
        if let Some(handing) = &self.handing {
            handing(&run);
        }
        // The builder is gone only when the engine run ended, which drops
        // this reader with it.
        let _ = self.runs.send(run);
    }
}

/// The reader's end: the stream for the builder to close, and how it ended.
/// Dropping the reader drops its channel's sender, which is what the
/// builder sees first.
fn end(live: LiveStream, stream_ended: bool, waiting: bool) -> Ended {
    Ended {
        live,
        stream_ended,
        waiting,
    }
}

/// What the live read got.
pub(super) enum Next {
    /// The stop was requested first; nothing was read.
    Stopped,
    Change(Option<source::Result<LiveChange>>),
}

/// The next live change, unless the stop is requested first: checked before
/// the stream on every poll, so a stop is never raced by a ready change. The
/// check is an atomic load and nothing is registered for the stop: the
/// engine run's watch wakes the reader when it is requested
/// ([`together`](mod@super::together)).
pub(super) async fn next_live(live: &mut LiveStream, stop: &Stop) -> Next {
    std::future::poll_fn(|cx| {
        if stop.is_requested() {
            return Poll::Ready(Next::Stopped);
        }
        live.poll_next_unpin(cx).map(Next::Change)
    })
    .await
}

/// The reader as a future for the engine run to poll beside the builder in
/// its own task (§3.8, review round 3), and the builder's handle on its end.
/// The future hands the reader's result to the handle when the reader
/// returns, then stays pending: the run ends with the builder. Nothing is
/// spawned, so dropping the run drops the reader in place (R9), and a panic
/// of the reader unwinds the run's task, as a panic of `main`'s one task did.
pub(super) fn reading(
    reader: Reader,
    live: LiveStream,
) -> (impl Future<Output = ()> + Send, ReaderEnd) {
    let (sender, receiver) = oneshot::channel();
    let reading = async move {
        // The builder may already have ended on its own error: then nobody
        // asks for the reader's end.
        let _ = sender.send(reader.run(live).await);
        std::future::pending::<()>().await;
    };
    (reading, ReaderEnd(Some(receiver)))
}

/// The builder's handle on the reader's end: the live stream to close, or
/// the reader's error.
pub(super) struct ReaderEnd(Option<oneshot::Receiver<Result<Ended>>>);

impl ReaderEnd {
    /// Wait for the reader's end. Its channel closes just before, so the
    /// builder asks only once the reader returned.
    pub(super) async fn join(&mut self) -> Result<Ended> {
        let gone = || {
            EngineError::Source(source::SourceError::Connection(
                "the live reader ended without an answer".to_owned(),
            ))
        };
        let Some(receiver) = self.0.as_mut() else {
            return Err(gone());
        };
        let ended = receiver.await;
        self.0 = None;
        ended.unwrap_or_else(|_| Err(gone()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
