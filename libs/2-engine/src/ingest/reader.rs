//! The reader: the task that reads the live stream, resolves each change to
//! the documents it touches, and marks them pending, ahead of the builds (ADR
//! 0012 stage 8a, slice 6a; the rules R1–R10 are in the stage note, §3.8).
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
//! way, without handing over the run in progress; its changes still count as
//! read, as each change did when `main` read it.
//!
//! ```text
//! stream ─▶ resolve ─▶ mark ─▶ count ─▶ Run ──channel──▶ builder
//!   ▲                                                       │
//!   └──────────── wait while the lead is reached ◀── take ──┘
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::{FutureExt, StreamExt};
use kernel::Position;
use source::cdc::{ChangeEvent, LiveChange};
use source::document::{DocumentBuilder, DocumentId};
use tokio::sync::{Notify, mpsc};
use tokio::time::{Duration, Instant};

use super::LiveStream;
use crate::error::{EngineError, Result};
use crate::observer::Observer;
use crate::policy::{BatchPolicy, MaxWaiting};
use crate::progress::Progress;
use crate::stop::Stop;

/// One run, handed to the builder once its documents are marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Run {
    /// Its last change.
    pub(super) position: Position,
    /// How many changes it read.
    pub(super) changes: usize,
    /// How many changes had been read through it, over the engine's life:
    /// what a step through it closes at.
    pub(super) read: u64,
    /// When its first change arrived: its deadline is this plus `max_delay`,
    /// and the builder's idle wait ends here.
    pub(super) started: Instant,
    /// How long reading and resolving it took: the builder reports it as
    /// [`Stage::Resolve`](crate::Stage::Resolve) once it closed the idle
    /// wait before it, so the two are reported in the order they happened.
    pub(super) resolved: Duration,
}

/// What the reader and the builder share for the lead.
#[derive(Debug)]
pub(super) struct Lead {
    max: MaxWaiting,
    /// Notified after every live take: the lead may have room.
    taken: Notify,
    /// How many changes had been read through the last granted batch.
    granted: AtomicU64,
}

impl Lead {
    /// A lead of `max`, nothing read ahead past `read`.
    pub(super) fn new(max: MaxWaiting, read: u64) -> Arc<Self> {
        Arc::new(Self {
            max,
            taken: Notify::new(),
            granted: AtomicU64::new(read),
        })
    }

    /// A live batch through the `read`th change was granted and its
    /// documents taken: the reader may have room again.
    pub(super) fn granted(&self, read: u64) {
        self.granted.fetch_max(read, Ordering::Relaxed);
        self.taken.notify_waiters();
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

/// The reader of one engine run (its stream is handed to [`ReaderTask::spawn`]
/// apart: a stream is `Send`, not `Sync`).
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
    /// A test's hook between finding the lead reached and waiting.
    #[cfg(test)]
    pub(super) checked: Option<Box<dyn Fn() + Send + Sync>>,
}

/// The run being read: its documents with their changes' positions, and
/// where it stands.
struct Reading {
    marks: Vec<(DocumentId, Position)>,
    changes: usize,
    position: Option<Position>,
    deadline: Instant,
}

impl Reading {
    fn is_open(&self, max_changes: usize) -> bool {
        self.changes < max_changes && Instant::now() < self.deadline
    }
}

impl Reader {
    /// Read runs until the stop or the stream's end (see the module docs).
    pub(super) async fn run(self, mut live: LiveStream) -> Result<Ended> {
        let mut reading = Reading {
            marks: Vec::with_capacity(self.batch.max_changes),
            changes: 0,
            position: None,
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
            let read = match self.resolve(&mut reading, position, event).await {
                Ok(()) => self.drain_ready(&mut live, &mut reading).await,
                Err(error) => Err(error),
            };
            let ended = match read {
                Ok(ended) => ended,
                Err(error) => {
                    // Read, though never handed over: counted as `main`
                    // counted each change as it read it. The next engine run
                    // closes them into a step no lane is routed.
                    self.progress.read_changes(reading.changes as u64);
                    return Err(error);
                }
            };
            self.hand_over(&mut reading, started);
            if ended {
                return Ok(end(live, true, false));
            }
        }
    }

    /// Wait while the lead is reached. `false` when the stop came first.
    /// The wake-up is registered before the lead is read, so a take between
    /// the two still wakes it.
    async fn room(&self) -> bool {
        loop {
            let taken = self.lead.taken.notified();
            tokio::pin!(taken);
            taken.as_mut().enable();
            if self.stop.is_requested() {
                return false;
            }
            if !self.lead.reached(&self.progress) {
                return true;
            }
            #[cfg(test)]
            if let Some(checked) = &self.checked {
                checked();
            }
            tokio::select! {
                () = self.stop.requested() => return false,
                () = &mut taken => {}
            }
        }
    }

    /// Read every change the stream has ready right now into the run, until
    /// it is full, past its deadline, the stream would block, or the stop is
    /// requested. Returns whether the stream ended.
    async fn drain_ready(&self, live: &mut LiveStream, reading: &mut Reading) -> Result<bool> {
        while reading.is_open(self.batch.max_changes) && !self.stop.is_requested() {
            match live.next().now_or_never() {
                None => return Ok(false),
                Some(None) => return Ok(true),
                Some(Some(Err(error))) => return Err(error.into()),
                Some(Some(Ok((position, event)))) => {
                    self.resolve(reading, position, event).await?;
                }
            }
        }
        Ok(false)
    }

    /// Resolve one change into the run: the documents it touches, each with
    /// its position.
    async fn resolve(
        &self,
        reading: &mut Reading,
        position: Position,
        event: ChangeEvent,
    ) -> Result<()> {
        self.observer.on_change_captured();
        reading.changes += 1;
        let affected = self
            .resolver
            .resolve(event.table(), event.key(), event.before(), event.after())
            .await?;
        tracing::trace!(documents = affected.len(), "change resolved to documents");
        reading
            .marks
            .extend(affected.into_iter().map(|id| (id, position)));
        reading.position = Some(position);
        Ok(())
    }

    /// Mark the run's documents, count its changes, then hand it over (R1).
    fn hand_over(&self, reading: &mut Reading, started: Instant) {
        let Some(position) = reading.position.take() else {
            return;
        };
        self.progress.mark(reading.marks.drain(..));
        let read = self.progress.read_changes(reading.changes as u64);
        let run = Run {
            position,
            changes: std::mem::take(&mut reading.changes),
            read,
            started,
            resolved: started.elapsed(),
        };
        #[cfg(test)]
        if let Some(handing) = &self.handing {
            handing(&run);
        }
        // The builder is gone only when the engine run ended: this task is
        // aborted with it.
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
/// check is an atomic load; the stop's waker is registered only when the
/// stream would wait, so a busy stream pays nothing more.
pub(super) async fn next_live(live: &mut LiveStream, stop: &Stop) -> Next {
    let notified = stop.notified();
    tokio::pin!(notified);
    std::future::poll_fn(|cx| {
        if stop.is_requested() {
            return std::task::Poll::Ready(Next::Stopped);
        }
        if let std::task::Poll::Ready(change) = live.poll_next_unpin(cx) {
            return std::task::Poll::Ready(Next::Change(change));
        }
        match notified.as_mut().poll(cx) {
            std::task::Poll::Ready(()) => std::task::Poll::Ready(Next::Stopped),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    })
    .await
}

/// The reader running on its own task, aborted when dropped, so it never
/// outlives its engine run (R9).
pub(super) struct ReaderTask(Option<tokio::task::JoinHandle<Result<Ended>>>);

impl ReaderTask {
    pub(super) fn spawn(reader: Reader, live: LiveStream) -> Self {
        use tracing::Instrument;
        Self(Some(tokio::spawn(reader.run(live).in_current_span())))
    }

    /// Wait for the reader's end: its result, or its panic resumed here, as
    /// a panic of the engine's one task was before the reader had its own.
    pub(super) async fn join(&mut self) -> Result<Ended> {
        let Some(handle) = self.0.as_mut() else {
            return Err(EngineError::Source(source::SourceError::Connection(
                "the live reader was already joined".to_owned(),
            )));
        };
        let joined = handle.await;
        self.0 = None;
        match joined {
            Ok(ended) => ended,
            Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Err(_) => Err(EngineError::Source(source::SourceError::Connection(
                "the live reader was cancelled".to_owned(),
            ))),
        }
    }
}

impl Drop for ReaderTask {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
