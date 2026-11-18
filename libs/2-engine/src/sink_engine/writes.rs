//! One run of a sink engine: up to `writes` lane items written at once, over
//! every lane of the sink (one per index, ADR 0012 stage 7).
//!
//! The run keeps a **window** of delivered, unfinished items, each tagged
//! with its lane, in the order they were received: the ones waiting, and the
//! ones being written. It takes an item from a lane only to use it: while a
//! write slot is free, or, with only backfill waiting on that lane, to find a
//! live item behind it, never more than `LOOKAHEAD` waiting per lane. A
//! delivered item leaves the lane's bound and is acknowledged only once
//! written, so each sink holds at most `writes + LOOKAHEAD × lanes` delivered
//! items beside its lanes' own capacity, and with live traffic alone about
//! `writes`. While there is room, the run waits on the `recv` of every lane
//! that wants more; between waits it takes what is already queued only when
//! a lane's `is_empty` says something is, and the `recv` futures are made only
//! when that wait is polled.
//!
//! **Admission.** An item starts only when no earlier unfinished item of its
//! lane shares a document with it (two lanes never share one: a document is
//! one index's), so a document is never written twice at once and a
//! later build always lands after an earlier one. Documents are compared by a
//! 64-bit hash of `(index, id)`, kept sorted per item so two items are
//! compared in one merge walk: a collision only holds an item back. The
//! hashes are computed only when a comparison is needed: with nothing in
//! flight and one kind waiting, the front item is granted as it is, so a
//! sink writing one item at a time hashes nothing.
//!
//! **Selection.** Live items (they carry a position) go before backfill items,
//! on every lane, except every fourth grant both could take, which goes to
//! backfill. Among the lanes with a candidate of the chosen kind, lanes take
//! turns, so a busy index never starves another; within a lane, its earliest
//! admissible item. A `SnapshotComplete` is a barrier on its lane: it runs
//! once everything before it on the lane is written, and nothing after it on
//! the lane starts until it is done; other lanes go on. A reindex waits for
//! the writes in flight, then is staged.
//!
//! **Merging.** A write holds every admissible item of the chosen kind that
//! is ready, live with live and backfill with backfill, the first item as
//! above, then the others a lane at a time from its lane on, up to the merge
//! limit in documents (`[batch] max_changes`, and the sink's
//! [`write_limit`](sink::Sink::write_limit)), never splitting an item. So a
//! burst over many indexes goes out in writes of about a batch, as with one
//! lane per sink, and several small items of one lane go out together
//! (#397). It takes what is ready and never waits for more: a lone item is
//! written at once. An item behind a `SnapshotComplete` of its lane is not
//! admissible; a lane behind a failed run's barrier, and an ordered sink,
//! write one item alone. Two items of one lane in a write share no document
//! (each is admissible against every earlier item of its lane). The write
//! takes one slot; when it returns each part is decided on its own: a part
//! whose index decides a rejection `stop` stays unacknowledged and the run
//! fails blamed on its lane, the other parts land, move their lanes' fences
//! and are acknowledged in lane order. A write that fails fails every part,
//! blamed on all their lanes.
//!
//! **Caught up.** A write is granted as caught up when some lane of it has
//! nothing else queued or waiting in the window. Its refresh can't cover a write in
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
//! **An ordered sink** ([`Sink::ordered`](sink::Sink::ordered), an emitting
//! one such as stdout) gets its items one write at a time, whatever `writes`
//! says, each lane in exact lane order: the next item is the live lane front
//! with the lowest position, backfill fronts taking every fourth turn when
//! live ones wait. So a reader sees each document's builds in order, and live
//! changes across indexes in position order.
//!
//! **After a failed run, and on a fresh engine,** each lane the failure is
//! blamed on (every lane, for a fresh engine or a failure no lane caused)
//! writes its oldest item alone, as a barrier, and opens only once a write on
//! it landed; other lanes write freely.
//! A fresh engine counts because a previous process may have stopped on that
//! item. So a write that fails every time (a `stop` rejection, a destination
//! refusing it) halts the sink after its first run: on every restart of the
//! engine nothing behind it lands, no progress resets the backoff, and the
//! lane fills. In the run that first meets it, writes already granted beside
//! it, and disjoint ones granted while it is in flight, can land;
//! confirmation stays before it. A new process writes its oldest *unconfirmed*
//! item alone: when that is an older item already written (a sibling sink
//! lags, or the confirmation never reached the source), it lands, the window
//! opens, and disjoint items past the failing one land again before it fails.
//! Nothing is lost.
//!
//! ```text
//! window   [L1 a] [B1 a,b] [L2 c] [SnapshotComplete] [L3 a]
//! writes   L1 a, L2 c        B1 waits for L1 (a); L3 waits behind the barrier
//! ```

use std::cmp::Ordering as Order;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use kernel::{Envelope, IndexName, Position};
use sink::{Visibility, WriteReport};
use stream::{AckHandle, Batch, Consumer, Delivery, LaneItem, LaneKey};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::Instrument;

use super::flights::Flights;
use super::{Blame, IDLE_TICK, SinkControl, SinkEngine, shown_id, until};
use crate::error::{EngineError, Result};
use crate::observer::{CommitStats, Stage, StageTiming, Work};
use crate::policy::FailurePolicy;

/// The most items the window holds waiting per lane, to find admissible
/// items behind blocked ones and live items behind backfill.
const LOOKAHEAD: usize = 32;

/// A delivered item waiting in the window.
struct Queued {
    item: Queue,
    handle: Box<dyn AckHandle>,
    /// The lane it came from, an index into the run's lanes.
    lane: usize,
    /// Its place in receive order within this run (lane order within a lane).
    seq: u64,
}

/// One lane the run reads.
struct LaneRun {
    key: LaneKey,
    consumer: Box<dyn Consumer<LaneItem>>,
    closed: bool,
    /// The previous run failed on this lane and nothing has landed on it
    /// since: grant only its oldest item, alone.
    barrier: bool,
    /// A landing on this lane counts as progress after the last run's
    /// failure.
    counts: bool,
    live_waiting: usize,
    backfill_waiting: usize,
}

impl LaneRun {
    fn waiting(&self) -> usize {
        self.live_waiting + self.backfill_waiting
    }
}

enum Queue {
    Batch { batch: Batch, documents: Documents },
    Complete { index: IndexName },
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

/// A write in flight: one or more lane items, sent as one write.
struct Flight {
    parts: Vec<Part>,
    work: Work,
    /// For a caught-up write, how many writes had returned at its grant.
    caught_up: Option<u64>,
}

/// One lane item of a write.
struct Part {
    handle: Box<dyn AckHandle>,
    lane: usize,
    position: Option<Position>,
    changes: usize,
    documents: Documents,
    seq: u64,
}

impl Flight {
    fn lanes(&self) -> impl Iterator<Item = usize> + '_ {
        self.parts.iter().map(|part| part.lane)
    }
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
    lanes: Vec<LaneRun>,
    window: VecDeque<Queued>,
    flights: Flights<'a, Landed>,
    flying: Vec<(u64, Flight)>,
    next_flight: u64,
    /// The lane-order number the next delivered item gets.
    next_seq: u64,
    /// Grants both kinds could take; every fourth goes to backfill.
    contested: u64,
    /// The lane whose turn is next among lanes with a candidate.
    turn: usize,
    failure: Option<EngineError>,
    /// The lanes the failure is blamed on, none for one no lane caused.
    failed_lanes: Vec<usize>,
    /// Live (positioned) and backfill batches waiting in the window, over
    /// every lane.
    live_waiting: usize,
    backfill_waiting: usize,
    /// When the oldest written position becomes searchable, as the fences
    /// said when they last changed: refreshed when a position is received, a
    /// write returns or the timer fires, not on every turn.
    next_due: Option<Instant>,
    /// When the sink last had nothing in flight and nothing taken, while it
    /// still has nothing: the open idle window.
    idle_since: Option<Instant>,
}

impl<'a> Run<'a> {
    pub(super) fn new(
        engine: &'a SinkEngine,
        lanes: Vec<(LaneKey, Box<dyn Consumer<LaneItem>>)>,
    ) -> Self {
        // Read once: from here on this run is the one a failure is blamed on.
        let blame = engine.blame();
        engine.set_blame(Blame::Nobody);
        let lanes = lanes
            .into_iter()
            .map(|(key, consumer)| {
                let blamed = blame.covers(&key);
                LaneRun {
                    barrier: blamed && !matches!(blame, Blame::Nobody),
                    counts: blamed,
                    key,
                    consumer,
                    closed: false,
                    live_waiting: 0,
                    backfill_waiting: 0,
                }
            })
            .collect();
        Self {
            engine,
            lanes,
            window: VecDeque::new(),
            flights: Flights::new(),
            flying: Vec::new(),
            next_flight: 0,
            next_seq: 0,
            contested: 0,
            turn: 0,
            failure: None,
            failed_lanes: Vec::new(),
            live_waiting: 0,
            backfill_waiting: 0,
            next_due: None,
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
                self.fail(error, &[]);
            }
            if self.flights.is_empty() {
                if let Some(failure) = self.failure.take() {
                    let blamed: std::collections::BTreeSet<LaneKey> = self
                        .failed_lanes
                        .iter()
                        .filter_map(|lane| self.lanes.get(*lane))
                        .map(|run| run.key.clone())
                        .collect();
                    let blame = if blamed.is_empty() {
                        Blame::Everyone
                    } else {
                        Blame::Lanes(blamed)
                    };
                    self.engine.set_blame(blame);
                    return Err(failure);
                }
                if self.lanes.iter().all(|lane| lane.closed) && self.window.is_empty() {
                    return Ok(());
                }
            }

            let wanting: Vec<bool> = (0..self.lanes.len())
                .map(|lane| self.lane_wants_more(lane))
                .collect();
            let room = wanting.contains(&true);
            let next_due = self.next_due;
            // A stop changes nothing for the loop but the backfill waiting
            // (skipped unwritten), so it is watched only while some waits.
            let engine = self.engine;
            let stop = &engine.stop;
            let watch_stop = self.backfill_waiting > 0 && !stop.is_requested();
            if self.flights.is_empty() && self.window.is_empty() {
                self.idle_since.get_or_insert_with(Instant::now);
            }
            // Writing with nothing waiting and no room to take more, only a
            // write landing, a reindex arriving or a position coming due can
            // change anything: the lane, a stop and the idle tick are left
            // out of the wait, as one write at a time left them while it
            // flushed. A `select!` builds and polls every branch on every
            // turn: about a tenth of the loop's cost per lane item at one
            // change per batch.
            if !room && !watch_stop && self.window.is_empty() && !self.flights.is_empty() {
                tokio::select! {
                    // Control first: under a backlog a landed write is ready
                    // on every turn, and a reindex polled after it would wait
                    // until traffic thins. Taking one closes this arm.
                    biased;
                    message = control.recv(), if control_open && !self.engine.reindex_pending() && self.failure.is_none() => {
                        match message {
                            None => control_open = false,
                            Some(SinkControl::Reindex { indexes }) => {
                                self.engine.set_pending_reindex(indexes);
                            }
                        }
                    }
                    landed = self.flights.next() => {
                        if let Some(landed) = landed
                            && let Err(error) = self.landed(landed).await
                        {
                            self.fail(error, &[]);
                        }
                    }
                    () = until(next_due) => {
                        self.engine.report_searchable();
                        self.refresh_due();
                    }
                }
                continue;
            }
            tokio::select! {
                landed = self.flights.next(), if !self.flights.is_empty() => {
                    self.close_idle();
                    if let Some(landed) = landed
                        && let Err(error) = self.landed(landed).await
                    {
                        self.fail(error, &[]);
                    }
                }
                (lane, delivery) = next_delivery(&mut self.lanes, &wanting), if room => {
                    self.close_idle();
                    match delivery {
                        Err(error) => self.fail(error.into(), &[lane]),
                        Ok(None) => {
                            if let Some(run) = self.lanes.get_mut(lane) {
                                run.closed = true;
                            }
                        }
                        Ok(Some(delivery)) => self.queue(lane, delivery),
                    }
                }
                message = control.recv(), if control_open && !self.engine.reindex_pending() && self.failure.is_none() => {
                    self.close_idle();
                    match message {
                        None => control_open = false,
                        Some(SinkControl::Reindex { indexes }) => {
                            self.engine.set_pending_reindex(indexes);
                        }
                    }
                }
                () = until(next_due) => {
                    self.close_idle();
                    self.engine.report_searchable();
                    self.refresh_due();
                }
                () = stop.requested(), if watch_stop => self.close_idle(),
                _ = idle_tick.tick() => self.close_idle(),
            }
        }
    }

    /// Take what the lane has ready, skip waiting backfill on a stop, stage a
    /// reindex once nothing is in flight, and start what may start. An error
    /// here is the run's failure: the caller lets the writes in flight finish.
    async fn advance(&mut self) -> Result<()> {
        if self.engine.stop.is_requested() {
            self.skip_backfill().await?;
        }
        let pending = if self.flights.is_empty() {
            self.engine.take_pending_reindex()
        } else {
            None
        };
        if let Some(indexes) = pending {
            let started = Instant::now();
            if let Err(error) = self.engine.reindex(indexes.clone()).await {
                self.engine.set_pending_reindex(indexes);
                return Err(error);
            }
            self.engine.observer.on_stage(
                StageTiming::new(Stage::Reindex, started.elapsed()).sink(&self.engine.name),
            );
        }
        if !self.engine.reindex_pending() {
            // Grant what is already waiting first, so an item just received
            // starts without another trip to the lane; read more only for a
            // slot still free, then grant what it brought.
            self.grant().await?;
            if self.wants_more() && self.fill()? {
                if self.engine.stop.is_requested() {
                    self.skip_backfill().await?;
                }
                self.grant().await?;
            }
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

    /// Whether a lane's items are granted only from its front, one at a time:
    /// a lane behind the barrier of a failed run.
    fn in_lane_order(&self, lane: usize) -> bool {
        self.lanes.get(lane).is_some_and(|lane| lane.barrier)
    }

    /// An ordered sink: one write at a time over every lane.
    fn ordered(&self) -> bool {
        self.engine.sink.ordered()
    }

    fn slots(&self) -> usize {
        if self.ordered() {
            1
        } else {
            self.engine.writes
        }
    }

    /// Whether any lane wants another item.
    fn wants_more(&self) -> bool {
        (0..self.lanes.len()).any(|lane| self.lane_wants_more(lane))
    }

    /// Whether to take another item from `lane`. Only to use it: while a write
    /// slot is free (everything waiting is blocked, or nothing waits), or,
    /// with only backfill waiting on the lane, to find a live item behind it,
    /// up to [`LOOKAHEAD`] items waiting on the lane. A delivered item leaves
    /// the lane's bound and is acknowledged only once written, so reading
    /// ahead of the writes holds memory and delays its release for nothing.
    fn lane_wants_more(&self, lane: usize) -> bool {
        let Some(run) = self.lanes.get(lane) else {
            return false;
        };
        if run.closed || self.failure.is_some() {
            return false;
        }
        if run.barrier || self.ordered() {
            return run.waiting() == 0 && !self.lane_in_flight(lane);
        }
        run.waiting() < LOOKAHEAD
            && (self.flights.len() < self.slots()
                || (run.live_waiting == 0 && run.backfill_waiting > 0))
    }

    /// Whether a write of `lane` is in flight.
    fn lane_in_flight(&self, lane: usize) -> bool {
        self.flying
            .iter()
            .any(|(_, flight)| flight.lanes().any(|at| at == lane))
    }

    /// The parts of `lane` being written.
    fn parts_in_flight(&self, lane: usize) -> impl Iterator<Item = &Part> {
        self.flying
            .iter()
            .flat_map(|(_, flight)| &flight.parts)
            .filter(move |part| part.lane == lane)
    }

    /// The most documents one write may hold: the engine's own limit, and the
    /// sink's, whichever is lower.
    fn merge_limit(&self) -> usize {
        self.engine
            .sink
            .write_limit()
            .map_or(self.engine.write_documents, |limit| {
                limit.min(self.engine.write_documents)
            })
            .max(1)
    }

    /// Take whatever the lanes have ready, while it is wanted, the lanes in
    /// turn; whether it took anything.
    fn fill(&mut self) -> Result<bool> {
        let mut took = false;
        for lane in 0..self.lanes.len() {
            // `is_empty` is a cheap look at the lane; a `recv` future is made
            // only when something waits to be taken.
            while self.lane_wants_more(lane)
                && self
                    .lanes
                    .get(lane)
                    .is_some_and(|run| !run.consumer.is_empty())
            {
                let Some(run) = self.lanes.get_mut(lane) else {
                    break;
                };
                match run.consumer.recv().now_or_never() {
                    None => break,
                    Some(delivery) => match delivery {
                        Err(error) => return Err(error.into()),
                        Ok(None) => run.closed = true,
                        Ok(Some(delivery)) => {
                            self.queue(lane, delivery);
                            took = true;
                        }
                    },
                }
            }
        }
        Ok(took)
    }

    fn queue(&mut self, lane: usize, delivery: Delivery<LaneItem>) {
        self.close_idle();
        let (item, handle) = delivery.into_parts();
        let item = match item {
            LaneItem::Batch(batch) => {
                if let Some(position) = batch.position
                    && let Some(run) = self.lanes.get(lane)
                {
                    let mut fences = self.engine.lock_fences();
                    fences.received(&run.key, position);
                    self.next_due = fences.next_due();
                }
                let documents = Documents::new(&batch.envelopes);
                Queue::Batch { batch, documents }
            }
            LaneItem::SnapshotComplete { index } => Queue::Complete { index },
        };
        let seq = self.next_seq;
        self.next_seq += 1;
        self.count(lane, &item, 1);
        self.window.push_back(Queued {
            item,
            handle,
            lane,
            seq,
        });
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
                lane,
                seq,
            }) = self.take_from_window(index)
            else {
                continue;
            };
            self.engine
                .lock_skipped()
                .extend(batch.envelopes.iter().map(|e| e.index.clone()));
            let oldest = self.is_oldest(lane, seq);
            self.acknowledge(handle, lane, oldest, None).await?;
        }
        Ok(())
    }

    /// Start writes while there is a free slot and an admissible item, and
    /// run a lane's `SnapshotComplete` once it is the oldest thing unfinished
    /// on its lane.
    async fn grant(&mut self) -> Result<()> {
        let slots = self.slots();
        // A write handled at its first poll may have failed the run: from
        // then on nothing new starts.
        while self.failure.is_none() {
            if let Some(at) = self.runnable_complete() {
                if let Some(Queued {
                    item: Queue::Complete { index },
                    handle,
                    lane,
                    ..
                }) = self.take_from_window(at)
                {
                    self.engine.complete_snapshot(index).await?;
                    self.acknowledge(handle, lane, true, None).await?;
                }
                continue;
            }
            if self.flights.len() >= slots {
                return Ok(());
            }
            let mut picked = self.pick();
            if picked.is_empty() {
                return Ok(());
            }
            // Taken from the back, so the earlier indexes stay where they are;
            // then put back in window (lane) order.
            picked.sort_unstable_by(|a, b| b.cmp(a));
            let mut parts = Vec::with_capacity(picked.len());
            let mut envelopes = Vec::with_capacity(picked.len());
            for at in picked {
                if let Some(Queued {
                    item: Queue::Batch { batch, documents },
                    handle,
                    lane,
                    seq,
                }) = self.take_from_window(at)
                {
                    envelopes.push(Arc::clone(&batch.envelopes));
                    parts.push(Part {
                        handle,
                        lane,
                        position: batch.position,
                        changes: batch.changes,
                        documents,
                        seq,
                    });
                }
            }
            parts.reverse();
            envelopes.reverse();
            let Some(first) = parts.first() else {
                return Ok(());
            };
            let work = work_of(first.position);
            // Caught up when some lane of the write has nothing else queued or
            // waiting: as with one item per write, that lane's position need
            // not wait out a refresh.
            let caught_up = parts.iter().any(|part| {
                !self.window.iter().any(|queued| queued.lane == part.lane)
                    && self
                        .lanes
                        .get(part.lane)
                        .is_some_and(|run| run.consumer.is_empty())
            });
            if let Some(landed) = self.launch(parts, envelopes, work, caught_up) {
                self.landed(landed).await?;
            }
        }
        Ok(())
    }

    /// The window index of a `SnapshotComplete` with nothing before it on its
    /// lane, waiting or in flight.
    fn runnable_complete(&self) -> Option<usize> {
        let mut seen: Vec<usize> = Vec::new();
        for (at, queued) in self.window.iter().enumerate() {
            if seen.contains(&queued.lane) {
                continue;
            }
            seen.push(queued.lane);
            if matches!(queued.item, Queue::Complete { .. }) && !self.lane_in_flight(queued.lane) {
                return Some(at);
            }
        }
        None
    }

    /// The window indexes of the next write, in window order, if one item is
    /// admissible: the item lanes' turns and live-first choose, then every
    /// other admissible item of the same kind that fits, lanes in turn, up to
    /// the merge limit in documents. Merging takes what is ready and never
    /// waits for more. An ordered sink, and a lane behind a failed run's
    /// barrier, write one item alone.
    fn pick(&mut self) -> Vec<usize> {
        // One item waiting, nothing in flight: no contest, nothing to merge,
        // so it is the pick, with no hashing.
        if self.window.len() == 1
            && self.flying.is_empty()
            && matches!(
                self.window.front(),
                Some(Queued {
                    item: Queue::Batch { .. },
                    ..
                })
            )
        {
            return vec![0];
        }
        // Every admissible live and backfill item of each lane, in window
        // order. Two lanes never share a document, so only a lane's own earlier
        // items and writes count; an item admissible here shares no document
        // with any earlier item of its lane, admissible ones included, so
        // admissible items of one lane can go out together.
        let lanes = self.lanes.len();
        let mut live: Vec<Vec<usize>> = vec![Vec::new(); lanes];
        let mut backfill: Vec<Vec<usize>> = vec![Vec::new(); lanes];
        let mut blocked = vec![false; lanes];
        let mut earlier: Vec<HashSet<u64>> = vec![HashSet::new(); lanes];
        let ordered = self.ordered();
        for (at, queued) in self.window.iter().enumerate() {
            let lane = queued.lane;
            if blocked.get(lane).copied().unwrap_or(true) {
                continue;
            }
            let Queue::Batch { batch, documents } = &queued.item else {
                // A `SnapshotComplete` waits for what is before it on its
                // lane, and nothing after it on the lane starts.
                if let Some(flag) = blocked.get_mut(lane) {
                    *flag = true;
                }
                continue;
            };
            let lane_order = ordered || self.in_lane_order(lane);
            let admissible = if lane_order {
                // Only the lane's front, alone.
                !self.lane_in_flight(lane)
            } else {
                let seen = earlier.get(lane);
                self.parts_in_flight(lane)
                    .all(|part| documents.share_none_with(&part.documents))
                    && documents
                        .hashes()
                        .iter()
                        .all(|hash| seen.is_none_or(|seen| !seen.contains(hash)))
            };
            if admissible {
                let list = if batch.position.is_some() {
                    live.get_mut(lane)
                } else {
                    backfill.get_mut(lane)
                };
                if let Some(list) = list {
                    list.push(at);
                }
            }
            if lane_order {
                if let Some(flag) = blocked.get_mut(lane) {
                    *flag = true;
                }
                continue;
            }
            if let Some(seen) = earlier.get_mut(lane) {
                seen.extend(documents.hashes().iter().copied());
            }
        }
        let any_live = live.iter().any(|list| !list.is_empty());
        let any_backfill = backfill.iter().any(|list| !list.is_empty());
        let candidates = match (any_live, any_backfill) {
            (true, true) => {
                self.contested += 1;
                if self.contested.is_multiple_of(4) {
                    &backfill
                } else {
                    &live
                }
            }
            (true, false) => &live,
            (false, true) => &backfill,
            (false, false) => return Vec::new(),
        };
        let first = if ordered {
            // The live front with the lowest position; a backfill front in
            // lane order.
            candidates
                .iter()
                .filter_map(|list| list.first())
                .copied()
                .min_by_key(|at| match self.window.get(*at) {
                    Some(Queued {
                        item: Queue::Batch { batch, .. },
                        seq,
                        ..
                    }) => (batch.position.map(|p| p.seq), *seq),
                    _ => (None, u64::MAX),
                })
        } else {
            // Lanes take turns: the first lane with a candidate at or after
            // the one whose turn it is.
            (0..lanes)
                .map(|offset| (self.turn + offset) % lanes)
                .find_map(|lane| candidates.get(lane).and_then(|list| list.first()).copied())
        };
        let Some(first) = first else {
            return Vec::new();
        };
        let Some(first_lane) = self.window.get(first).map(|queued| queued.lane) else {
            return Vec::new();
        };
        self.turn = (first_lane + 1) % lanes.max(1);
        if ordered || self.in_lane_order(first_lane) {
            return vec![first];
        }
        let limit = self.merge_limit();
        let size = |at: usize| match self.window.get(at) {
            Some(Queued {
                item: Queue::Batch { batch, .. },
                ..
            }) => batch.envelopes.len(),
            _ => 0,
        };
        let mut picked = vec![first];
        let mut documents = size(first);
        // The other candidates, a lane at a time from the first's lane on,
        // one item per lane per round, each lane in window order.
        let mut queues: Vec<std::collections::VecDeque<usize>> = (0..lanes)
            .map(|offset| (first_lane + offset) % lanes)
            .filter(|lane| !self.in_lane_order(*lane))
            .filter_map(|lane| candidates.get(lane))
            .map(|list| list.iter().copied().filter(|at| *at != first).collect())
            .collect();
        while documents < limit && queues.iter().any(|queue| !queue.is_empty()) {
            for queue in &mut queues {
                let Some(at) = queue.pop_front() else {
                    continue;
                };
                let more = size(at);
                if documents + more <= limit {
                    documents += more;
                    picked.push(at);
                }
            }
        }
        picked
    }

    fn launch(
        &mut self,
        parts: Vec<Part>,
        envelopes: Vec<Arc<[Envelope]>>,
        work: Work,
        caught_up: bool,
    ) -> Option<Landed> {
        let flight = self.next_flight;
        self.next_flight += 1;
        let covers_below = caught_up.then(|| self.engine.lock_fences().returned());
        self.flying.push((
            flight,
            Flight {
                parts,
                work,
                caught_up: covers_below,
            },
        ));
        let sink = &self.engine.sink;
        let documents: usize = envelopes.iter().map(|part| part.len()).sum();
        let span = tracing::debug_span!(
            "sink.write",
            envelopes = documents,
            parts = envelopes.len(),
            caught_up
        );
        let write = async move {
            let started = Instant::now();
            let slices: Vec<&[Envelope]> = envelopes.iter().map(|part| &part[..]).collect();
            let result = sink.write_parts(&slices, caught_up).await;
            Landed {
                flight,
                result,
                took: started.elapsed(),
            }
        };
        // A disabled span still costs an enter and exit per poll.
        let write = if span.is_disabled() {
            write.boxed()
        } else {
            write.instrument(span).boxed()
        };
        // Start the write now rather than at the next turn of the loop: a
        // request goes out at once, and a write that completes on its first
        // poll is handled here.
        self.flights.start(write)
    }

    /// A write returned: decide its rejections per part, move the fences, and
    /// acknowledge every part that landed. A part with a rejection its index
    /// decides `stop` stays unacknowledged and the run fails, blamed on its
    /// lane; the other parts landed and are acknowledged. A write that failed
    /// fails every part, blamed on all their lanes.
    async fn landed(&mut self, landed: Landed) -> Result<()> {
        let Some(index) = self.flying.iter().position(|(id, _)| *id == landed.flight) else {
            return Ok(());
        };
        let (_, flight) = self.flying.swap_remove(index);
        self.engine.observer.on_stage(
            StageTiming::new(Stage::Write, landed.took)
                .work(flight.work)
                .sink(&self.engine.name),
        );
        let report = match landed.result {
            Ok(report) => report,
            Err(error) => {
                let lanes: Vec<usize> = flight.lanes().collect();
                self.fail(error.into(), &lanes);
                return Ok(());
            }
        };
        let stopped = self.engine.stopping_rejections(&report);
        let (failed, landed_parts): (Vec<Part>, Vec<Part>) =
            flight.parts.into_iter().partition(|part| {
                stopped.as_ref().is_some_and(|(indexes, _)| {
                    self.lanes
                        .get(part.lane)
                        .is_some_and(|run| indexes.contains(run.key.index.as_ref()))
                })
            });
        self.engine.quarantine(&report, |index| {
            stopped
                .as_ref()
                .is_none_or(|(indexes, _)| !indexes.contains(index))
        });
        if !landed_parts.is_empty() {
            let envelopes = landed_parts
                .iter()
                .map(|part| part.documents.envelopes.len())
                .sum();
            let changes = landed_parts.iter().map(|part| part.changes).sum();
            self.engine.observer.on_batch_committed(
                &self.engine.name,
                CommitStats::new(envelopes, changes, landed.took, flight.work).sent(report.bytes),
            );
        }
        let written: Vec<(LaneKey, Position)> = landed_parts
            .iter()
            .filter_map(|part| {
                let key = self.lanes.get(part.lane)?.key.clone();
                Some((key, part.position?))
            })
            .collect();
        let returned = self
            .engine
            .write_returned(&written, report.visibility, flight.caught_up);
        self.next_due = returned.next_due;
        let count = landed_parts.len();
        for (at, part) in landed_parts.into_iter().enumerate() {
            // Let go of the batch before acknowledging it, so the lane frees it
            // as the acknowledgement lands, as it did with one write at a time.
            // Freeing it here after the acknowledgement made the pre-#417
            // `live_drain` bench (an instant sink, where who frees a batch
            // decided its time) at 64 changes per batch half again slower.
            drop(part.documents);
            let oldest = self.is_oldest(part.lane, part.seq);
            let wants = if at + 1 == count {
                returned.wants_a_caught_up_write
            } else {
                false
            };
            self.acknowledge(part.handle, part.lane, oldest, Some(wants))
                .await?;
            if let Some(run) = self.lanes.get_mut(part.lane) {
                run.barrier = false;
            }
        }
        if let Some((_, error)) = stopped {
            let lanes: Vec<usize> = failed.iter().map(|part| part.lane).collect();
            self.fail(error, &lanes);
        }
        Ok(())
    }

    /// Record the run's first failure, blamed on `lanes` when they caused it
    /// (none: a failure no lane caused): the next run puts a barrier on those
    /// lanes only.
    fn fail(&mut self, error: EngineError, lanes: &[usize]) {
        if self.failure.is_none() {
            let names: Vec<String> = lanes
                .iter()
                .filter_map(|lane| self.lanes.get(*lane))
                .map(|lane| lane.key.to_string())
                .collect();
            tracing::error!(%error, lanes = ?names, writes_in_flight = self.flights.len(), "the sink engine's run failed; waiting for the writes in flight");
            self.failure = Some(error);
            self.failed_lanes = lanes.to_vec();
        }
    }

    /// Acknowledge an item. Before the acknowledgement that leaves the engine
    /// idle, a written position not yet searchable gets an empty caught-up
    /// write, so the last writes of a burst don't wait out the refresh
    /// interval (or, with automatic refresh off, for traffic that may not
    /// come). It runs before the acknowledgement so a drain waits for it.
    ///
    /// `oldest`: the item was the oldest unfinished one of its lane, so the
    /// sink moved forward (the supervisor's measure of progress), unless the
    /// run has already failed, or the last run's failure was blamed on other
    /// lanes (a healthy sibling landing is no progress past a poison item).
    /// Any failure of this run counts, an older or a younger item's: the run
    /// is ending, and an older failed item comes back first.
    ///
    /// `wants`: whether a caught-up write is wanted, when the caller already
    /// read it from the fences. The lane is asked last: its ledger lock is
    /// shared with the ingest engine.
    async fn acknowledge(
        &mut self,
        handle: Box<dyn AckHandle>,
        lane: usize,
        oldest: bool,
        wants: Option<bool>,
    ) -> Result<()> {
        let quiet = self.window.is_empty() && self.flights.is_empty() && self.failure.is_none();
        if quiet
            && wants.unwrap_or_else(|| {
                self.engine
                    .lock_fences()
                    .wants_a_caught_up_write(Instant::now())
            })
            && self.lanes.iter().all(|run| run.consumer.is_empty())
        {
            let covers_below = self.engine.lock_fences().returned();
            let started = Instant::now();
            let report = self.engine.sink.write(&[], true).await?;
            self.engine.observer.on_stage(
                StageTiming::new(Stage::Write, started.elapsed())
                    .work(Work::Live)
                    .sink(&self.engine.name),
            );
            let returned = self
                .engine
                .write_returned(&[], report.visibility, Some(covers_below));
            self.next_due = returned.next_due;
        }
        handle.ack().await?;
        if oldest && self.failure.is_none() && self.lanes.get(lane).is_some_and(|run| run.counts) {
            self.engine.advanced.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    fn refresh_due(&mut self) {
        self.next_due = self.engine.lock_fences().next_due();
    }

    /// Remove the window item at `index`, keeping the counts of what waits.
    fn take_from_window(&mut self, index: usize) -> Option<Queued> {
        let queued = self.window.remove(index)?;
        self.count(queued.lane, &queued.item, -1);
        Some(queued)
    }

    fn count(&mut self, lane: usize, item: &Queue, by: isize) {
        let live = match item {
            Queue::Batch { batch, .. } => batch.position.is_some(),
            Queue::Complete { .. } => return,
        };
        let (total, per_lane) = match (live, self.lanes.get_mut(lane)) {
            (true, Some(run)) => (&mut self.live_waiting, Some(&mut run.live_waiting)),
            (false, Some(run)) => (&mut self.backfill_waiting, Some(&mut run.backfill_waiting)),
            (true, None) => (&mut self.live_waiting, None),
            (false, None) => (&mut self.backfill_waiting, None),
        };
        *total = total.saturating_add_signed(by);
        if let Some(per_lane) = per_lane {
            *per_lane = per_lane.saturating_add_signed(by);
        }
    }

    /// Whether nothing still unfinished (waiting or in flight) on `lane` came
    /// before the item at `seq`.
    fn is_oldest(&self, lane: usize, seq: u64) -> bool {
        self.window
            .iter()
            .filter(|queued| queued.lane == lane)
            .all(|queued| queued.seq > seq)
            && self.parts_in_flight(lane).all(|part| part.seq > seq)
    }
}

impl SinkEngine {
    /// The indexes whose rejections in `report` their policy decides `stop`,
    /// with the error that ends the run; `None` when there is none. Decided
    /// before any quarantine event is emitted, so a `skip` document beside
    /// them isn't counted twice when the unacknowledged part is redelivered.
    fn stopping_rejections(
        &self,
        report: &WriteReport,
    ) -> Option<(std::collections::BTreeSet<String>, EngineError)> {
        let mut indexes = std::collections::BTreeSet::new();
        let mut stop_count = 0usize;
        let mut stop_example = String::new();
        for doc in &report.rejected {
            if self.failure_policies.resolve(&doc.index) == FailurePolicy::Stop {
                if stop_count == 0 {
                    stop_example = format!("{}/{}: {}", doc.index, shown_id(&doc.id), doc.reason);
                }
                stop_count += 1;
                indexes.insert(doc.index.clone());
            }
        }
        (stop_count > 0).then_some((
            indexes,
            EngineError::DocumentsRejected(stop_count, stop_example),
        ))
    }

    /// Quarantine the rejections of the indexes `landed` accepts (the parts
    /// acknowledged), under their `skip` policy.
    fn quarantine(&self, report: &WriteReport, landed: impl Fn(&str) -> bool) {
        for doc in report.rejected.iter().filter(|doc| landed(&doc.index)) {
            tracing::warn!(
                index = %doc.index,
                id = %shown_id(&doc.id),
                reason = %doc.reason,
                "document rejected by sink; quarantining and continuing",
            );
            self.observer
                .on_document_quarantined(&self.name, &doc.index, &doc.id, &doc.reason);
        }
    }

    /// A write returned: record it, raise each written lane's marks in the
    /// progress (before the caller acknowledges it, so a fence never waits on
    /// an acknowledgement), and tell observers what became applied and
    /// searchable, applied first. One fences lock, and an atomic per mark
    /// that moved.
    fn write_returned(
        &self,
        written: &[(LaneKey, Position)],
        visibility: Visibility,
        caught_up: Option<u64>,
    ) -> Returned {
        let now = Instant::now();
        let (applied, searchable, returned) = {
            let mut fences = self.lock_fences();
            fences.write_returned(written, visibility, caught_up, now);
            let mut applied: Vec<(LaneKey, Position)> = Vec::new();
            for (lane, _) in written {
                if applied.iter().any(|(seen, _)| seen == lane) {
                    continue;
                }
                if let Some(position) = fences.advance_applied(lane) {
                    applied.push((lane.clone(), position));
                }
            }
            let searchable = fences.advance_searchable(now);
            let returned = Returned {
                next_due: fences.next_due(),
                wants_a_caught_up_write: fences.wants_a_caught_up_write(now),
            };
            (applied, searchable, returned)
        };
        for (lane, position) in &applied {
            self.progress.applied(lane, *position);
            self.observer.on_position_applied(lane, *position);
        }
        self.report(&searchable);
        returned
    }

    /// Report the last positions whose time to be searchable has come.
    pub(super) fn report_searchable(&self) {
        let reached = self.lock_fences().advance_searchable(Instant::now());
        self.report(&reached);
    }

    /// Tell the progress and observers which lanes became searchable how
    /// far.
    fn report(&self, searchable: &[(LaneKey, Position)]) {
        for (lane, position) in searchable {
            self.progress.searchable(lane, *position);
            self.observer.on_position_searchable(lane, *position);
        }
    }

    /// Record a finished snapshot's index as seeded, unless a stop cut its
    /// backfill short.
    async fn complete_snapshot(&self, index: IndexName) -> Result<()> {
        let indexes = vec![index];
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

/// What a returned write left in the fences.
struct Returned {
    /// When the oldest position comes due, if that is a matter of time.
    next_due: Option<Instant>,
    /// Whether an idle engine should send an empty caught-up write.
    wants_a_caught_up_write: bool,
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

/// The next delivery of any lane that wants more, with its lane. The `recv`
/// futures (boxed by the port's `async_trait`) are made only when this is
/// first polled, so a `select!` branch disabled for want of room builds
/// nothing on that turn; one is made per wanting lane, and dropping the
/// others loses nothing (a channel receive is cancel-safe).
async fn next_delivery<'a>(
    lanes: &'a mut [LaneRun],
    wanting: &[bool],
) -> (usize, stream::Result<Option<Delivery<LaneItem>>>) {
    let mut receives: Vec<(usize, BoxFuture<'a, _>)> = lanes
        .iter_mut()
        .enumerate()
        .filter(|(lane, _)| wanting.get(*lane).copied().unwrap_or(false))
        .map(|(lane, run)| (lane, run.consumer.recv()))
        .collect();
    std::future::poll_fn(|cx| {
        for (lane, receive) in &mut receives {
            if let std::task::Poll::Ready(delivery) = receive.as_mut().poll(cx) {
                return std::task::Poll::Ready((*lane, delivery));
            }
        }
        std::task::Poll::Pending
    })
    .await
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
