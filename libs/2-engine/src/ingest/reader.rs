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
use kernel::{IndexName, Position};
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

/// One run's slice for one builder, handed over once its documents are
/// marked: with one builder every change of the run, with several the
/// changes that touched the builder's index (§3.9 I2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Run {
    /// Each change, in order, with the moment the reader read it and its
    /// read number: the builder cuts batches by these as `main` cut them (R3).
    pub(super) changes: Vec<(Position, Instant, u64)>,
    /// The read number of its last change.
    pub(super) read: u64,
    /// How long reading and resolving it took: the builder reports it as
    /// [`Stage::Resolve`](crate::Stage::Resolve) once it closed the idle
    /// wait before it, so the two are reported in the order they happened.
    pub(super) resolved: Duration,
}

impl Run {
    /// When its first change was read: the builder's idle wait ends here.
    pub(super) fn started(&self) -> Option<Instant> {
        self.changes.first().map(|(_, at, _)| *at)
    }
}

/// What the reader and the builders share for the lead.
#[derive(Debug)]
pub(super) struct Lead {
    max: MaxWaiting,
    /// With two or more builders, each queue's index and the bound on the
    /// exempt indexes' documents (6b-2, §3.9 §5); `None` with one builder,
    /// which keeps 6b-1b's check.
    exemption: Option<Exemption>,
    /// The reader's wake-up, woken after a live take while it waits: the
    /// lead may have room. One waiter, so no list and no lock.
    taken: AtomicWaker,
    /// The reader waits, or is about to: a grant wakes it only then, so a
    /// reader that keeps up costs no wake-up.
    waiting: AtomicBool,
    /// One per builder, in the builders' order.
    queues: Vec<Queue>,
    /// The reader ended while waiting (on the stream, or at its lead), not
    /// in the middle of a run: a builder's wait until then was idle.
    ended_waiting: AtomicBool,
}

/// One builder's queue as the lead counts it (§3.9 I6): the read number of
/// its oldest change queued and not granted (`u64::MAX` for none) and of
/// the last change routed to it; changes read ahead are counted from the
/// oldest queued change of any builder that doesn't fold.
#[derive(Debug)]
pub(super) struct Queue {
    oldest: AtomicU64,
    routed: AtomicU64,
    folding: AtomicBool,
    /// Its builder is held (§3.9 §5): its live batch refused by the budget,
    /// or a publish blocked on a full lane, since its last live grant.
    /// Written by the builder.
    held: AtomicBool,
    /// The reader latched its index exempt: held with `max_waiting_per_index`
    /// documents or more waiting, until fewer wait. Written by the reader.
    exempt: AtomicBool,
}

/// What the reader needs to exempt a held index (6b-2).
#[derive(Debug)]
struct Exemption {
    /// Each queue's index, in queue order.
    indexes: Vec<IndexName>,
    /// The exempt indexes together stop the reading at this many documents.
    max_pending: usize,
    /// The reader is stopped at `max_pending`: logged once per stretch.
    at_cap: AtomicBool,
}

impl Lead {
    /// A lead of `max` over one builder, nothing read ahead past `read`.
    #[cfg(test)]
    pub(super) fn new(max: MaxWaiting, read: u64) -> Arc<Self> {
        Self::over(max, read, 1)
    }

    /// A lead of `max` over `builders` builders, nothing read ahead past
    /// `read`.
    pub(super) fn over(max: MaxWaiting, read: u64, builders: usize) -> Arc<Self> {
        Arc::new(Self::of(max, read, builders, None))
    }

    /// A lead of `max` over a builder per index of `indexes` (in queue
    /// order), nothing read ahead past `read`, exempting a held index at its
    /// cap: the exempt indexes together stop the reading at
    /// `max_pending_documents` (6b-2).
    pub(super) fn exempting(
        max: MaxWaiting,
        read: u64,
        indexes: Vec<IndexName>,
        max_pending_documents: usize,
    ) -> Arc<Self> {
        let builders = indexes.len();
        Arc::new(Self::of(
            max,
            read,
            builders,
            Some(Exemption {
                indexes,
                max_pending: max_pending_documents,
                at_cap: AtomicBool::new(false),
            }),
        ))
    }

    fn of(max: MaxWaiting, read: u64, builders: usize, exemption: Option<Exemption>) -> Self {
        Self {
            max,
            exemption,
            taken: AtomicWaker::new(),
            waiting: AtomicBool::new(false),
            ended_waiting: AtomicBool::new(false),
            queues: (0..builders.max(1))
                .map(|_| Queue {
                    oldest: AtomicU64::new(u64::MAX),
                    routed: AtomicU64::new(read),
                    folding: AtomicBool::new(false),
                    held: AtomicBool::new(false),
                    exempt: AtomicBool::new(false),
                })
                .collect(),
        }
    }

    /// Builder `queue` is held, or no longer (§3.9 §5). Becoming held wakes
    /// the reader: it may wait at that index's cap, which only it can turn
    /// into an exemption (D4).
    pub(super) fn hold(&self, queue: usize, held: bool) {
        let Some(queue) = self.queues.get(queue) else {
            return;
        };
        if queue.held.swap(held, Ordering::Relaxed) != held && held {
            self.wake();
        }
    }

    /// A live batch of the first builder through its change read `read`th
    /// was granted, the builder's changes read one after another (one
    /// builder, handed every change).
    #[cfg(test)]
    pub(super) fn granted(&self, read: u64) {
        let next = self
            .queues
            .first()
            .is_some_and(|queue| queue.routed.load(Ordering::Relaxed) > read)
            .then_some(read + 1);
        self.queued(0, next);
    }

    /// Builder `queue` granted a live batch and took its documents: its
    /// oldest change still queued is the `next`th read (`None`: none). The
    /// reader may have room again.
    pub(super) fn queued(&self, queue: usize, next: Option<u64>) {
        if let Some(queue) = self.queues.get(queue) {
            queue
                .oldest
                .store(next.unwrap_or(u64::MAX), Ordering::Relaxed);
        }
        self.wake();
    }

    /// Whether the reader ended while waiting, not in the middle of a run.
    pub(super) fn ended_waiting(&self) -> bool {
        self.ended_waiting.load(Ordering::Relaxed)
    }

    /// Whether builder `queue` folds: the reader hands it nothing.
    pub(super) fn folds(&self, queue: usize) -> bool {
        self.queues
            .get(queue)
            .is_some_and(|queue| queue.folding.load(Ordering::Relaxed))
    }

    /// Builder `queue` enters or leaves the fold (§3.9 I7): its queue counts
    /// toward no bound while it folds, and leaving it, nothing it had is
    /// outstanding. Wakes the reader: the bound may have moved.
    pub(super) fn fold(&self, queue: usize, folding: bool) {
        if let Some(queue) = self.queues.get(queue) {
            queue.oldest.store(u64::MAX, Ordering::Relaxed);
            queue.folding.store(folding, Ordering::Relaxed);
        }
        self.wake();
    }

    /// The reader routed builder `queue` a slice from its `first`th read to
    /// its `last`th.
    fn routed(&self, queue: usize, first: u64, last: u64) {
        if let Some(queue) = self.queues.get(queue) {
            queue.oldest.fetch_min(first, Ordering::Relaxed);
            queue.routed.fetch_max(last, Ordering::Relaxed);
        }
    }

    /// The reader may have room again: a folding builder took some of its
    /// backlog, so its index waits on fewer documents.
    pub(super) fn wake(&self) {
        // Pairs with the fence in `Reader::room`: either the reader sees
        // this grant's take when it reads the lead, or this sees it waiting.
        fence(Ordering::SeqCst);
        if self.waiting.load(Ordering::Relaxed) {
            self.taken.wake();
        }
    }

    /// Whether the reader is as far ahead as it may be.
    pub(super) fn reached(&self, progress: &Progress) -> bool {
        match &self.exemption {
            None => {
                if progress.most_waiting() >= self.max.per_index
                    || progress.waiting() >= self.max.total
                {
                    return true;
                }
            }
            Some(exemption) => {
                if self.documents_reached(progress, exemption) {
                    return true;
                }
            }
        }
        let read = progress.changes_read();
        self.queues.iter().any(|queue| {
            // A folding queue holds nothing: entering the fold cleared it,
            // and the reader routes it nothing while it folds.
            let oldest = queue.oldest.load(Ordering::Relaxed);
            oldest != u64::MAX && read.saturating_sub(oldest - 1) >= self.max.total as u64
        })
    }
}

impl Lead {
    /// The documents bounds with exemption (§3.9 §5): each non-exempt index
    /// at `max_waiting_per_index`, the non-exempt indexes together at
    /// `max_waiting`, the exempt ones together at `max_pending_documents`.
    fn documents_reached(&self, progress: &Progress, exemption: &Exemption) -> bool {
        let mut others = 0;
        let mut exempt = 0;
        for (queue, index) in self.queues.iter().zip(&exemption.indexes) {
            let waiting = progress.waiting_on(index);
            if self.latched(queue, index, waiting) {
                exempt += waiting;
                continue;
            }
            if waiting >= self.max.per_index {
                return true;
            }
            others += waiting;
        }
        if others >= self.max.total {
            return true;
        }
        let full = exempt >= exemption.max_pending;
        if exemption.at_cap.swap(full, Ordering::Relaxed) != full {
            if full {
                tracing::warn!(
                    documents = exempt,
                    max_pending_documents = exemption.max_pending,
                    "reading stopped for every index: the exempt indexes have \
                     max_pending_documents documents waiting to be built; it reads on as they are built"
                );
            } else {
                tracing::info!(
                    documents = exempt,
                    "the exempt indexes are below max_pending_documents: reading on"
                );
            }
        }
        full
    }

    /// Whether `queue`'s index, with `waiting` documents, is exempt now:
    /// latched while its builder is held at the cap, cleared below it.
    fn latched(&self, queue: &Queue, index: &IndexName, waiting: usize) -> bool {
        if waiting < self.max.per_index {
            if queue.exempt.swap(false, Ordering::Relaxed) {
                tracing::info!(
                    %index,
                    waiting,
                    "index below max_waiting_per_index: no longer exempt; it paces the reading again"
                );
            }
            return false;
        }
        if queue.held.load(Ordering::Relaxed) {
            if !queue.exempt.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    %index,
                    waiting,
                    "index held at max_waiting_per_index: exempt, so reading goes on for the other \
                     indexes; its changes are kept as documents waiting, up to max_pending_documents \
                     for every exempt index, and the replication slot stays pinned by it, so WAL grows"
                );
            }
            return true;
        }
        queue.exempt.load(Ordering::Relaxed)
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
    /// Each builder's hand-over, in the lead's queue order, with the index it
    /// builds. With one builder it is handed every change.
    pub(super) runs: Vec<(IndexName, mpsc::UnboundedSender<Run>)>,
    /// A test's look at each run just before it is handed over.
    #[cfg(test)]
    pub(super) handing: Option<Box<dyn Fn(&Run) + Send + Sync>>,
    /// A test's hook where the lead was found reached: before the wake-up
    /// is registered, and after, just before waiting.
    #[cfg(test)]
    pub(super) checked: Option<Box<dyn Fn(Check) + Send + Sync>>,
}

/// The run being read: its documents with their changes' positions and
/// read numbers, each change with when it was read, how many changes were
/// counted before the run and how many were read and not yet counted.
/// Dropped with changes still uncounted (an error, the engine run dropped),
/// it counts them read, so `changes_captured` and the read count move
/// together on every path, as `main` counted each change as it read it; as
/// read and never marked, so their replay is waited for (§3.9 D2).
struct Reading<'a> {
    progress: &'a Progress,
    marks: Vec<(DocumentId, Position, u64)>,
    changes: Vec<(Position, Instant, u64)>,
    /// With several builders, each one's slice of the run: the changes that
    /// touched its index, with their read numbers.
    slices: Vec<Vec<(Position, Instant, u64)>>,
    counted: u64,
    uncounted: usize,
    deadline: Instant,
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        if self.uncounted > 0 {
            self.progress.read_unmarked(self.uncounted as u64);
        }
    }
}

impl Reader {
    /// Read runs until the stop or the stream's end (see the module docs).
    pub(super) async fn run(self, mut live: LiveStream) -> Result<Ended> {
        let mut reading = Reading {
            progress: &self.progress,
            marks: Vec::with_capacity(self.batch.max_changes),
            counted: self.progress.changes_read(),
            changes: Vec::new(),
            slices: vec![
                Vec::new();
                if self.runs.len() > 1 {
                    self.runs.len()
                } else {
                    0
                }
            ],
            uncounted: 0,
            deadline: Instant::now(),
        };
        loop {
            if !self.room().await {
                return Ok(self.end(live, false, true));
            }
            let (position, event) = match next_live(&mut live, &self.stop).await {
                Next::Stopped => return Ok(self.end(live, false, true)),
                Next::Change(None) => return Ok(self.end(live, true, true)),
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
                return Ok(self.end(live, true, false));
            }
        }
    }

    /// The reader's end, recording on the lead whether it ended `waiting`.
    fn end(&self, live: LiveStream, stream_ended: bool, waiting: bool) -> Ended {
        self.lead.ended_waiting.store(waiting, Ordering::Relaxed);
        end(live, stream_ended)
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
        let read = reading.counted + reading.uncounted as u64;
        let affected = self
            .resolver
            .resolve(event.table(), event.key(), event.before(), event.after())
            .await?;
        tracing::trace!(documents = affected.len(), "change resolved to documents");
        if !reading.slices.is_empty() {
            // A change's documents come grouped by index: the builder is
            // looked up once per group, not per document.
            let mut previous: Option<&IndexName> = None;
            for id in &affected {
                if previous == Some(&id.index) {
                    continue;
                }
                previous = Some(&id.index);
                if let Some(slice) = self
                    .runs
                    .iter()
                    .position(|(index, _)| *index == id.index)
                    .and_then(|at| reading.slices.get_mut(at))
                    && slice.last().is_none_or(|(_, _, last)| *last != read)
                {
                    slice.push((position, at, read));
                }
            }
        }
        reading
            .marks
            .extend(affected.into_iter().map(|id| (id, position, read)));
        reading.changes.push((position, at, read));
        Ok(())
    }

    /// Mark the run's documents, count its changes, route its last, then
    /// hand it over (R1; routed after the marks, §3.9 D1).
    fn hand_over(&self, reading: &mut Reading<'_>, started: Instant) {
        let Some(&(last, _, _)) = reading.changes.last() else {
            return;
        };
        self.progress.mark_reads(reading.marks.drain(..));
        let read = self
            .progress
            .read_changes(std::mem::take(&mut reading.uncounted) as u64);
        reading.counted = read;
        self.progress.routed(last);
        let resolved = started.elapsed();
        let changes = std::mem::take(&mut reading.changes);
        // One builder is handed the whole run, changes that touched nothing
        // included: its batches are 6b-1b's (§3.9, 6b-1c against the code).
        if reading.slices.is_empty() {
            let run = Run {
                changes,
                read,
                resolved,
            };
            self.hand(0, run);
            return;
        }
        // The run's `Resolve` goes with its first slice; a run that touched
        // no index reports its own.
        let mut resolved = Some(resolved);
        for at in 0..reading.slices.len() {
            let Some(slice) = reading.slices.get_mut(at).filter(|slice| !slice.is_empty()) else {
                continue;
            };
            let changes = std::mem::take(slice);
            if self.lead.folds(at) {
                continue;
            }
            let run = Run {
                read: changes.last().map_or(read, |(_, _, read)| *read),
                changes,
                resolved: resolved.take().unwrap_or_default(),
            };
            self.hand(at, run);
        }
        if let Some(resolved) = resolved {
            self.observer.on_stage(
                crate::observer::StageTiming::new(crate::observer::Stage::Resolve, resolved)
                    .work(crate::observer::Work::Live),
            );
        }
    }

    /// Hand builder `at` its slice.
    fn hand(&self, at: usize, run: Run) {
        #[cfg(test)]
        if let Some(handing) = &self.handing {
            handing(&run);
        }
        if let Some(&(_, _, first)) = run.changes.first() {
            self.lead.routed(at, first, run.read);
        }
        // A builder is gone only when the engine run ended, which drops this
        // reader with it.
        if let Some((_, runs)) = self.runs.get(at) {
            let _ = runs.send(run);
        }
    }
}

/// The reader's end: the stream for the builder to close, and how it ended.
/// Dropping the reader drops its channel's sender, which is what the
/// builder sees first.
fn end(live: LiveStream, stream_ended: bool) -> Ended {
    Ended { live, stream_ended }
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
