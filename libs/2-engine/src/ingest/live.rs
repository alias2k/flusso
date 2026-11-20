//! The live side of a batch: buffering ready changes, resolving them, and
//! the batch they fill.

use std::collections::HashSet;

use futures::{FutureExt, StreamExt};
use kernel::Position;
use source::cdc::{ChangeEvent, LiveChange};
use source::document::DocumentId;
use tokio::time::Instant;

use crate::error::Result;
use crate::stop::Stop;

use super::{IngestEngine, LiveStream};

impl IngestEngine {
    /// Buffer every change the live stream has ready *right now*, stopping at
    /// a full batch, the batch's deadline, or the moment the stream would
    /// block. Returns whether the stream ended. The caller commits on return
    /// either way: a change on a quiet stream has nothing to batch with, so
    /// waiting out `max_delay` would only add latency, while a burst keeps the
    /// stream ready and fills the batch — for at most `max_delay`.
    pub(super) async fn drain_ready(
        &self,
        live: &mut LiveStream,
        pending: &mut LiveRun,
    ) -> Result<bool> {
        while pending.is_open() && !self.stop.is_requested() {
            match live.next().now_or_never() {
                None => return Ok(false),
                Some(None) => return Ok(true),
                Some(Some(Err(error))) => return Err(error.into()),
                Some(Some(Ok((position, event)))) => {
                    self.buffer_live(pending, position, event).await?;
                }
            }
        }
        Ok(false)
    }

    /// Resolve one live change into the run: the documents it touches, each
    /// with its position, and the run's position.
    pub(super) async fn buffer_live(
        &self,
        pending: &mut LiveRun,
        position: Position,
        event: ChangeEvent,
    ) -> Result<()> {
        self.observer.on_change_captured();
        let affected = self
            .resolver
            .resolve(event.table(), event.key(), event.before(), event.after())
            .await?;
        tracing::trace!(documents = affected.len(), "change resolved to documents");
        pending
            .marks
            .extend(affected.into_iter().map(|id| (id, position)));
        pending.count += 1;
        pending.position = Some(position);
        if pending.deadline.is_none() {
            pending.deadline = Some(Instant::now() + self.batch.max_delay);
        }
        Ok(())
    }
}

/// What the live arm of the loop got.
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

/// The live changes read since the last grant: the documents they touch,
/// each with the change's position (marked pending on their indexes when the
/// next live build is granted, which deduplicates them), how many changes,
/// the run's last position and its deadline. A run cut at `max_changes`
/// changes or its deadline stops the reading until a build takes it, as one
/// batch did.
#[derive(Debug)]
pub(super) struct LiveRun {
    pub(super) marks: Vec<(DocumentId, Position)>,
    pub(super) count: usize,
    pub(super) position: Option<Position>,
    pub(super) deadline: Option<Instant>,
    capacity: usize,
}

impl LiveRun {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            marks: Vec::with_capacity(capacity),
            count: 0,
            position: None,
            deadline: None,
            capacity,
        }
    }

    pub(super) fn is_full(&self) -> bool {
        self.count >= self.capacity
    }

    /// Still accepting changes: neither full nor past its deadline.
    pub(super) fn is_open(&self) -> bool {
        !self.is_full()
            && self
                .deadline
                .is_none_or(|deadline| Instant::now() < deadline)
    }

    pub(super) fn clear(&mut self) {
        self.marks.clear();
        self.count = 0;
        self.position = None;
        self.deadline = None;
    }
}

/// A snapshot batch in the making: the deduplicated document ids of the
/// rows read and how many.
#[derive(Debug)]
pub(super) struct PendingBatch {
    pub(super) ids: Vec<DocumentId>,
    pub(super) seen: HashSet<DocumentId>,
    pub(super) count: usize,
    pub(super) capacity: usize,
}

impl PendingBatch {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
            count: 0,
            capacity,
        }
    }

    /// Buffer one change's affected documents, deduplicated.
    pub(super) fn add(&mut self, affected: impl IntoIterator<Item = DocumentId>) {
        for id in affected {
            if self.seen.insert(id.clone()) {
                self.ids.push(id);
            }
        }
        self.count += 1;
    }

    pub(super) fn is_full(&self) -> bool {
        self.count >= self.capacity
    }

    pub(super) fn clear(&mut self) {
        self.ids.clear();
        self.seen.clear();
        self.count = 0;
    }
}
