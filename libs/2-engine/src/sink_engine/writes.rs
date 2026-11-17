//! One run of a sink engine: up to `writes` lane items written at once.
//!
//! The run keeps a **window** of delivered, unfinished items in lane order:
//! the ones waiting, and the ones being written. It reads ahead from the lane
//! only while the window holds fewer than `writes + LOOKAHEAD` items, because
//! a delivered item leaves the lane's bound, so an unbounded read-ahead would
//! unbound memory.
//!
//! **Admission.** An item starts only when no earlier unfinished item shares
//! a document with it, so a document is never written twice at once and a
//! later build always lands after an earlier one. Documents are compared by a
//! 64-bit hash of `(index, id)`, kept sorted per item so two items are
//! compared in one merge walk: a collision only holds an item back. The
//! hashes are computed only when a comparison is needed: with nothing in
//! flight and one kind waiting, the front item is granted as it is, so a
//! sink writing one item at a time hashes nothing.
//!
//! **Selection.** Live items (they carry a position) go before backfill items,
//! except every fourth grant both could take, which goes to the oldest
//! admissible backfill item. A `SnapshotComplete` is a barrier: it runs once
//! everything before it is written, and nothing after it starts until it is
//! done. A reindex waits for the writes in flight, then is staged.
//!
//! **Caught up.** A write is granted as caught up when nothing else is queued
//! on the lane or waiting in the window. Its refresh can't cover a write in
//! flight beside it that returns later, so before the acknowledgement that
//! leaves the engine idle, a position not yet searchable gets an empty
//! caught-up write.
//!
//! **Failure.** A write that fails, a `stop` rejection, or any other error
//! of the run (the lane failing to deliver or acknowledge, staging a reindex)
//! ends the run: no new item starts, the writes in flight are awaited (the
//! sink bounds each by its own timeouts and retries) and acknowledged when
//! they succeeded, then the first error is returned. Every unfinished item is released with its handle, and
//! the lane redelivers it, in lane order, to the restarted engine.
//!
//! **After a failed run** the next run writes the oldest redelivered item
//! alone, as a barrier, and opens the window only once a write landed. So a
//! write that fails every time (a `stop` rejection, a destination refusing
//! it) halts the sink as one write at a time did: nothing behind it lands on
//! each restart, no progress resets the backoff, and the lane fills.
//!
//! ```text
//! window   [L1 a] [B1 a,b] [L2 c] [SnapshotComplete] [L3 a]
//! writes   L1 a, L2 c        B1 waits for L1 (a); L3 waits behind the barrier
//! ```

use std::cmp::Ordering as Order;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use kernel::{Envelope, IndexName, Position};
use sink::{Visibility, WriteReport};
use stream::{AckHandle, Batch, Consumer, Delivery, LaneItem};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::Instrument;

use super::{IDLE_TICK, SinkControl, SinkEngine, shown_id, until};
use crate::error::{EngineError, Result};
use crate::observer::{CommitStats, Stage, StageTiming, Work};
use crate::policy::FailurePolicy;

/// Items the window holds beyond the writes in flight, to find admissible
/// and live items behind blocked or backfill ones.
const LOOKAHEAD: usize = 32;

/// A delivered item waiting in the window.
struct Queued {
    item: Queue,
    handle: Box<dyn AckHandle>,
}

enum Queue {
    Batch { batch: Batch, documents: Documents },
    Complete { indexes: Vec<IndexName> },
}

/// An item's documents as admission keys, hashed on first use.
struct Documents {
    envelopes: Arc<[Envelope]>,
    hashes: OnceLock<Vec<u64>>,
}

impl Documents {
    fn new(envelopes: &Arc<[Envelope]>) -> Self {
        Self {
            envelopes: Arc::clone(envelopes),
            hashes: OnceLock::new(),
        }
    }

    /// The sorted, distinct hashes of the documents.
    fn hashes(&self) -> &[u64] {
        self.hashes.get_or_init(|| {
            let mut hashes: Vec<u64> = self.envelopes.iter().map(document_hash).collect();
            hashes.sort_unstable();
            hashes.dedup();
            hashes
        })
    }

    fn share_none_with(&self, other: &Documents) -> bool {
        disjoint(self.hashes(), other.hashes())
    }
}

/// A write in flight.
struct Flight {
    handle: Box<dyn AckHandle>,
    position: Option<Position>,
    envelopes: usize,
    changes: usize,
    work: Work,
    documents: Documents,
    /// For a caught-up write, how many writes had returned at its grant.
    caught_up: Option<u64>,
}

/// A write that returned.
struct Landed {
    flight: u64,
    result: sink::Result<WriteReport>,
    took: Duration,
}

/// The state of one run.
pub(super) struct Run<'a> {
    engine: &'a SinkEngine,
    lane: Box<dyn Consumer<LaneItem>>,
    window: VecDeque<Queued>,
    flights: FuturesUnordered<BoxFuture<'a, Landed>>,
    flying: HashMap<u64, Flight>,
    next_flight: u64,
    /// Grants both kinds could take; every fourth goes to backfill.
    contested: u64,
    failure: Option<EngineError>,
    lane_closed: bool,
    /// The previous run failed and nothing has landed since: grant only the
    /// window's front item, alone.
    barrier: bool,
    /// When the sink last had nothing in flight and nothing taken, while it
    /// still has nothing: the open idle window.
    idle_since: Option<Instant>,
}

impl<'a> Run<'a> {
    pub(super) fn new(engine: &'a SinkEngine, lane: Box<dyn Consumer<LaneItem>>) -> Self {
        Self {
            engine,
            lane,
            window: VecDeque::new(),
            flights: FuturesUnordered::new(),
            flying: HashMap::new(),
            next_flight: 0,
            contested: 0,
            failure: None,
            lane_closed: false,
            barrier: engine.failed_run.load(Ordering::Relaxed),
            idle_since: Some(Instant::now()),
        }
    }

    /// Follow the lane until it closes and everything taken from it is done
    /// (`Ok`), or until a failure and the writes in flight beside it are done
    /// (`Err`).
    pub(super) async fn run(mut self, control: &mut mpsc::Receiver<SinkControl>) -> Result<()> {
        let mut control_open = true;
        let mut idle_tick = tokio::time::interval_at(Instant::now() + IDLE_TICK, IDLE_TICK);
        idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            if self.failure.is_none()
                && let Err(error) = self.advance().await
            {
                self.fail(error);
            }
            if self.flights.is_empty() {
                if let Some(failure) = self.failure.take() {
                    return Err(failure);
                }
                if self.lane_closed && self.window.is_empty() {
                    return Ok(());
                }
            }

            let room = self.failure.is_none()
                && !self.lane_closed
                && self.window.len() + self.flights.len() < self.capacity();
            let next_due = self.engine.lock_fences().next_due();
            let stop = self.engine.stop.clone();
            let stopped = stop.is_requested();
            if self.flights.is_empty() && self.window.is_empty() {
                self.idle_since.get_or_insert_with(Instant::now);
            }
            tokio::select! {
                landed = self.flights.next(), if !self.flights.is_empty() => {
                    self.close_idle();
                    if let Some(landed) = landed
                        && let Err(error) = self.landed(landed).await
                    {
                        self.fail(error);
                    }
                }
                delivery = self.lane.recv(), if room => {
                    self.close_idle();
                    match delivery {
                        Err(error) => self.fail(error.into()),
                        Ok(None) => self.lane_closed = true,
                        Ok(Some(delivery)) => self.queue(delivery),
                    }
                }
                message = control.recv(), if control_open && !self.engine.reindex_pending() && self.failure.is_none() => {
                    self.close_idle();
                    match message {
                        None => control_open = false,
                        Some(SinkControl::Reindex { indexes }) => {
                            *self.engine.lock_pending_reindex() = Some(indexes);
                        }
                    }
                }
                () = until(next_due) => {
                    self.close_idle();
                    self.engine.report_searchable();
                }
                () = stop.requested(), if !stopped => self.close_idle(),
                _ = idle_tick.tick() => self.close_idle(),
            }
        }
    }

    /// Take what the lane has ready, skip waiting backfill on a stop, stage a
    /// reindex once nothing is in flight, and start what may start. An error
    /// here is the run's failure: the caller lets the writes in flight finish.
    async fn advance(&mut self) -> Result<()> {
        self.fill()?;
        if self.engine.stop.is_requested() {
            self.skip_backfill().await?;
        }
        let pending = if self.flights.is_empty() {
            self.engine.lock_pending_reindex().take()
        } else {
            None
        };
        if let Some(indexes) = pending {
            let started = Instant::now();
            if let Err(error) = self.engine.reindex(indexes.clone()).await {
                *self.engine.lock_pending_reindex() = Some(indexes);
                return Err(error);
            }
            self.engine.observer.on_stage(
                StageTiming::new(Stage::Reindex, started.elapsed()).sink(&self.engine.name),
            );
        }
        if !self.engine.reindex_pending() {
            self.grant().await?;
        }
        Ok(())
    }

    /// Report the open idle window, if any. Every arm closes it before its
    /// work runs, and the tick cuts it every [`IDLE_TICK`], so a quiet sink
    /// reads as idle in every scrape and no work is counted as idle.
    fn close_idle(&mut self) {
        if let Some(since) = self.idle_since.take() {
            self.engine.idle_until(since);
        }
    }

    fn capacity(&self) -> usize {
        self.engine.writes + LOOKAHEAD
    }

    /// Take whatever the lane has ready, while the window has room.
    fn fill(&mut self) -> Result<()> {
        while !self.lane_closed && self.window.len() + self.flights.len() < self.capacity() {
            match self.lane.recv().now_or_never() {
                None => break,
                Some(delivery) => match delivery? {
                    None => self.lane_closed = true,
                    Some(delivery) => self.queue(delivery),
                },
            }
        }
        Ok(())
    }

    fn queue(&mut self, delivery: Delivery<LaneItem>) {
        self.close_idle();
        let (item, handle) = delivery.into_parts();
        let item = match item {
            LaneItem::Batch(batch) => {
                if let Some(position) = batch.position {
                    self.engine.lock_fences().received(position);
                }
                let documents = Documents::new(&batch.envelopes);
                Queue::Batch { batch, documents }
            }
            LaneItem::SnapshotComplete { indexes } => Queue::Complete { indexes },
        };
        self.window.push_back(Queued { item, handle });
    }

    /// Once a stop is requested, backfill items still waiting are
    /// acknowledged unwritten and their index left unseeded: the backfill was
    /// abandoned and runs again on the next start.
    async fn skip_backfill(&mut self) -> Result<()> {
        let mut index = 0;
        while index < self.window.len() {
            let skip = matches!(
                self.window.get(index),
                Some(Queued { item: Queue::Batch { batch, .. }, .. }) if batch.position.is_none()
            );
            if !skip {
                index += 1;
                continue;
            }
            let Some(Queued {
                item: Queue::Batch { batch, .. },
                handle,
            }) = self.window.remove(index)
            else {
                continue;
            };
            self.engine
                .lock_skipped()
                .extend(batch.envelopes.iter().map(|e| e.index.clone()));
            self.acknowledge(handle).await?;
        }
        Ok(())
    }

    /// Start writes while there is a free slot and an admissible item, and
    /// run a `SnapshotComplete` once it is the oldest thing unfinished.
    async fn grant(&mut self) -> Result<()> {
        let slots = if self.barrier { 1 } else { self.engine.writes };
        while self.flights.len() < slots {
            if matches!(
                self.window.front(),
                Some(Queued {
                    item: Queue::Complete { .. },
                    ..
                })
            ) {
                if !self.flights.is_empty() {
                    return Ok(());
                }
                if let Some(Queued {
                    item: Queue::Complete { indexes },
                    handle,
                }) = self.window.pop_front()
                {
                    self.engine.complete_snapshot(indexes).await?;
                    self.acknowledge(handle).await?;
                }
                continue;
            }
            let picked = if self.barrier {
                self.window.front().map(|_| 0)
            } else {
                self.pick()
            };
            let Some(index) = picked else {
                return Ok(());
            };
            let Some(Queued {
                item: Queue::Batch { batch, documents },
                handle,
            }) = self.window.remove(index)
            else {
                return Ok(());
            };
            let caught_up = self.window.is_empty() && self.lane.is_empty();
            self.launch(batch, documents, handle, caught_up);
        }
        Ok(())
    }

    /// The window index of the next item to write, if one is admissible.
    fn pick(&mut self) -> Option<usize> {
        let live_waiting = self.window.iter().any(
            |queued| matches!(&queued.item, Queue::Batch { batch, .. } if batch.position.is_some()),
        );
        let backfill_waiting = self.window.iter().any(
            |queued| matches!(&queued.item, Queue::Batch { batch, .. } if batch.position.is_none()),
        );
        // Nothing unfinished comes before the front item, and with one kind
        // waiting there is no contest: it is the pick, with no hashing.
        if self.flying.is_empty()
            && !(live_waiting && backfill_waiting)
            && matches!(
                self.window.front(),
                Some(Queued {
                    item: Queue::Batch { .. },
                    ..
                })
            )
        {
            return Some(0);
        }
        let mut live = None;
        let mut backfill = None;
        for (index, queued) in self.window.iter().enumerate() {
            let Queue::Batch { batch, documents } = &queued.item else {
                break;
            };
            let admissible = self
                .flying
                .values()
                .all(|flight| documents.share_none_with(&flight.documents))
                && self
                    .window
                    .range(..index)
                    .all(|earlier| match &earlier.item {
                        Queue::Batch {
                            documents: earlier, ..
                        } => documents.share_none_with(earlier),
                        Queue::Complete { .. } => true,
                    });
            if admissible {
                if batch.position.is_some() {
                    live.get_or_insert(index);
                } else {
                    backfill.get_or_insert(index);
                }
            }
            let found_live = live.is_some() || !live_waiting;
            let found_backfill = backfill.is_some() || !backfill_waiting;
            if found_live && found_backfill {
                break;
            }
        }
        match (live, backfill) {
            (Some(live), Some(backfill)) => {
                self.contested += 1;
                Some(if self.contested.is_multiple_of(4) {
                    backfill
                } else {
                    live
                })
            }
            (live, backfill) => live.or(backfill),
        }
    }

    fn launch(
        &mut self,
        batch: Batch,
        documents: Documents,
        handle: Box<dyn AckHandle>,
        caught_up: bool,
    ) {
        let flight = self.next_flight;
        self.next_flight += 1;
        let covers_below = self.engine.lock_fences().returned();
        self.flying.insert(
            flight,
            Flight {
                handle,
                position: batch.position,
                envelopes: batch.envelopes.len(),
                changes: batch.changes,
                work: work_of(batch.position),
                documents,
                caught_up: caught_up.then_some(covers_below),
            },
        );
        let sink = &self.engine.sink;
        let envelopes = batch.envelopes;
        let span = tracing::debug_span!("sink.write", envelopes = envelopes.len(), caught_up);
        self.flights.push(
            async move {
                let started = Instant::now();
                let result = sink.write(&envelopes, caught_up).await;
                Landed {
                    flight,
                    result,
                    took: started.elapsed(),
                }
            }
            .instrument(span)
            .boxed(),
        );
    }

    /// A write returned: decide its rejections, move the fences, and
    /// acknowledge it, or record the failure that ends the run.
    async fn landed(&mut self, landed: Landed) -> Result<()> {
        let Some(flight) = self.flying.remove(&landed.flight) else {
            return Ok(());
        };
        self.engine.observer.on_stage(
            StageTiming::new(Stage::Write, landed.took)
                .work(flight.work)
                .sink(&self.engine.name),
        );
        let report = match landed.result {
            Ok(report) => report,
            Err(error) => {
                self.fail(error.into());
                return Ok(());
            }
        };
        if let Err(error) = self.engine.decide_rejections(&report) {
            self.fail(error);
            return Ok(());
        }
        self.engine.observer.on_batch_committed(
            &self.engine.name,
            CommitStats::new(flight.envelopes, flight.changes, landed.took, flight.work)
                .sent(report.bytes),
        );
        self.engine
            .write_returned(flight.position, report.visibility, flight.caught_up);
        // Let go of the batch before acknowledging it, so the lane frees it
        // as the acknowledgement lands, as it did with one write at a time.
        // Freeing it here after the acknowledgement made `live_drain` at 64
        // changes per batch half again slower (glibc, pinned bench).
        drop(flight.documents);
        self.acknowledge(flight.handle).await?;
        self.barrier = false;
        Ok(())
    }

    fn fail(&mut self, error: EngineError) {
        if self.failure.is_none() {
            tracing::error!(%error, writes_in_flight = self.flights.len(), "the sink engine's run failed; waiting for the writes in flight");
            self.failure = Some(error);
        }
    }

    /// Acknowledge an item. Before the acknowledgement that leaves the engine
    /// idle, a written position not yet searchable gets an empty caught-up
    /// write, so the last writes of a burst don't wait out the refresh
    /// interval (or, with automatic refresh off, for traffic that may not
    /// come). It runs before the acknowledgement so a drain waits for it.
    async fn acknowledge(&mut self, handle: Box<dyn AckHandle>) -> Result<()> {
        let idle = self.window.is_empty() && self.flights.is_empty() && self.lane.is_empty();
        if idle
            && self.failure.is_none()
            && self
                .engine
                .lock_fences()
                .wants_a_caught_up_write(Instant::now())
        {
            let covers_below = self.engine.lock_fences().returned();
            let started = Instant::now();
            let report = self.engine.sink.write(&[], true).await?;
            self.engine.observer.on_stage(
                StageTiming::new(Stage::Write, started.elapsed())
                    .work(Work::Live)
                    .sink(&self.engine.name),
            );
            self.engine
                .write_returned(None, report.visibility, Some(covers_below));
        }
        handle.ack().await?;
        self.engine.acknowledged.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

impl SinkEngine {
    /// Decide item-level rejections by policy. A single `stop` rejection
    /// fails the write, decided *before* any quarantine event is emitted, so a
    /// `skip` document in the same item isn't counted twice when the
    /// unacknowledged item is redelivered.
    fn decide_rejections(&self, report: &WriteReport) -> Result<()> {
        if report.is_clean() {
            return Ok(());
        }
        let mut stop_count = 0usize;
        let mut stop_example = String::new();
        for doc in &report.rejected {
            if self.failure_policies.resolve(&doc.index) == FailurePolicy::Stop {
                if stop_count == 0 {
                    stop_example = format!("{}/{}: {}", doc.index, shown_id(&doc.id), doc.reason);
                }
                stop_count += 1;
            }
        }
        if stop_count > 0 {
            return Err(EngineError::DocumentsRejected(stop_count, stop_example));
        }
        for doc in &report.rejected {
            tracing::warn!(
                index = %doc.index,
                id = %shown_id(&doc.id),
                reason = %doc.reason,
                "document rejected by sink; quarantining and continuing",
            );
            self.observer
                .on_document_quarantined(&self.name, &doc.index, &doc.id, &doc.reason);
        }
        Ok(())
    }

    /// Move the fences for a write that returned and report what moved.
    /// Applied is reported here, before the caller acknowledges: the source
    /// forgets a position once every sink confirmed it, so a listener must
    /// hear of it first.
    fn write_returned(
        &self,
        position: Option<Position>,
        visibility: Visibility,
        caught_up: Option<u64>,
    ) {
        let applied = {
            let mut fences = self.lock_fences();
            fences.write_returned(position, visibility, caught_up, Instant::now());
            fences.advance_applied()
        };
        if let Some(position) = applied {
            self.observer.on_position_applied(&self.name, position);
        }
        self.report_searchable();
    }

    /// Report the last position whose time to be searchable has come.
    pub(super) fn report_searchable(&self) {
        let reached = self.lock_fences().advance_searchable(Instant::now());
        if let Some(position) = reached {
            self.observer.on_position_searchable(&self.name, position);
        }
    }

    /// Record a finished snapshot's indexes as seeded, except those whose
    /// backfill a stop cut short.
    async fn complete_snapshot(&self, indexes: Vec<IndexName>) -> Result<()> {
        let skipped = self.lock_skipped().clone();
        for index in indexes.iter().filter(|index| skipped.contains(*index)) {
            tracing::info!(%index, "stopping: its backfill was cut short, so it stays unseeded and is backfilled on the next start");
        }
        let indexes: Vec<IndexName> = indexes
            .into_iter()
            .filter(|index| !skipped.contains(index))
            .collect();
        for index in &indexes {
            let started = Instant::now();
            self.sink.mark_seeded(index).await?;
            self.observer.on_stage(
                StageTiming::new(Stage::Seed, started.elapsed())
                    .work(Work::Backfill)
                    .sink(&self.name)
                    .index(index),
            );
            self.observer.on_index_seeded(&self.name, index);
        }
        let mut outstanding = self.lock_outstanding();
        for index in &indexes {
            outstanding.remove(index);
        }
        if !indexes.is_empty() {
            tracing::info!(indexes = indexes.len(), "backfill complete");
        }
        Ok(())
    }
}

/// Live changes carry a position; a backfill batch has none.
fn work_of(position: Option<Position>) -> Work {
    if position.is_some() {
        Work::Live
    } else {
        Work::Backfill
    }
}

/// Whether two sorted lists of document hashes share none.
fn disjoint(a: &[u64], b: &[u64]) -> bool {
    let (mut a, mut b) = (a.iter().peekable(), b.iter().peekable());
    while let (Some(x), Some(y)) = (a.peek(), b.peek()) {
        match x.cmp(y) {
            Order::Less => {
                a.next();
            }
            Order::Greater => {
                b.next();
            }
            Order::Equal => return false,
        }
    }
    true
}

/// A document's admission key: a hash of its index and id.
fn document_hash(envelope: &Envelope) -> u64 {
    let mut hasher = DefaultHasher::new();
    envelope.index.as_ref().hash(&mut hasher);
    envelope.id.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_document_is_keyed_by_its_index_and_id() {
        let at = |index: &str, id: &str| {
            document_hash(&Envelope::delete(
                IndexName::try_new(index).unwrap(),
                id,
                None,
                chrono::Utc::now(),
            ))
        };
        assert_eq!(at("users", "1"), at("users", "1"));
        assert_ne!(at("users", "1"), at("orders", "1"));
        assert_ne!(at("users", "1"), at("users", "2"));
    }
}
