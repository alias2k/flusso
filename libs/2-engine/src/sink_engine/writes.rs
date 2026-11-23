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
//! items beside its lanes' own capacity, plus one per lane waiting for its
//! lane's caught-up write, and with live traffic alone about `writes`. While there is room, the run waits on the `recv` of every lane
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
//! admissible item. A `SnapshotComplete` is a barrier on its lane: once
//! everything before it on the lane is written, it seeds its index
//! ([`mark_seeded`](sink::Sink::mark_seeded)) as a flight holding one write
//! slot, and nothing after it on the lane starts until the seed returned;
//! other lanes go on writing and seeding meanwhile, so the seeds at the end
//! of a backfill run at once. A `SnapshotComplete` naming an older ticket
//! than its index's latest request (the backfill was cut short by a new
//! target) is acknowledged without seeding, and a snapshot batch naming one
//! records no walk mark. A reindex waits for the writes and seeds in flight,
//! then is staged.
//!
//! **Merging.** A write holds every admissible item of the chosen kind that
//! is ready, live with live and backfill with backfill, the first item as
//! above, then the others a lane at a time from its lane on, up to the merge
//! limit in documents (`[batch] max_changes`, and the sink's
//! [`write_limit`](sink::Sink::write_limit)), never splitting an item. So a
//! burst over many indexes goes out in writes of about a batch, as with one
//! lane per sink, and several small items of one lane go out together
//! (#397). A lone item on a quiet engine is written at once.
//!
//! **The merge wait.** With several lanes, a live item ready while a write
//! is out waits for the writes out to return, so it goes out in one write
//! with what else is ready rather than alone beside them (a burst over three
//! indexes is otherwise three times the requests, each refreshing). The
//! wait is bounded hard by `merge_wait` (`[batch] max_delay_ms`, 50 ms by
//! default), counted from when the item became ready and from when the
//! youngest write out was sent, whichever ends first: a write out longer
//! than the bound is not waited for, so a stalled or slow lane holds no
//! other index's writes past it. Backfill is never held (with a backfill
//! item waiting nothing is), nor is anything on an ordered sink, one lane, or
//! without merging. The slot cap and a quiet lane's one item still apply.
//!
//! An item behind a `SnapshotComplete` of its lane is not
//! admissible; a lane behind a failed run's barrier, a parked lane's probe,
//! and an ordered sink write one item alone. Two items of one lane in a write
//! share no document (each is admissible against every earlier item of its
//! lane). The write takes one slot; when it returns each part is decided on
//! its own: a part whose index the sink reports blocked or missing, or whose
//! rejection its index decides `stop`, goes back in the window unacknowledged
//! and its lane parks or is staged again (below); the other parts land, move
//! their lanes' fences and are acknowledged in lane order. A write that fails
//! fails every part, blamed on all their lanes.
//!
//! **Caught up.** A write is granted as caught up when some lane of it has
//! nothing else queued or waiting in the window. Its refresh can't cover a write in
//! flight beside it that returns later, so before the acknowledgement that
//! leaves the engine idle, a position not yet searchable gets an empty
//! caught-up write.
//!
//! **One lane's failure parks that lane** (ADR 0012 stage 9, see `health`).
//! A `stop` refusal or a write block parks the lane: it is no longer granted,
//! takes nothing more from its lane, and its unacknowledged items hold its
//! marks, its fences and the source's resume point. Its front item is written
//! again alone (a probe) when its backoff is due, at once when a stop is
//! requested; landed, the lane opens. A missing target stages the lane's index
//! again once nothing of the lane is in flight, beside the other lanes'
//! writes, then the lane opens. A parked lane's items neither hold nor end a
//! merge wait. A parked lane stays parked across runs.
//!
//! **Failure.** A write that fails as a whole, or any other error of the run
//! (the lane failing to deliver or acknowledge, a seed, staging a reindex)
//! ends the run: no new item starts, the writes in flight are awaited (the
//! sink bounds each by its own timeouts and retries) and acknowledged when
//! they succeeded, then the first error is returned. Every unfinished item is released with its handle, and
//! the lane redelivers it, in lane order, to the restarted engine.
//!
//! **An ordered sink** ([`Sink::ordered`](sink::Sink::ordered), an emitting
//! one such as stdout) gets its items one write at a time, whatever `writes`
//! says, each lane in exact lane order: the next item is the live lane front
//! with the lowest position, backfill fronts taking every fourth turn when
//! live ones wait. So a reader sees each document's builds in order, and
//! each index's live changes in position order. Across indexes the order is
//! the lowest front's, not the changes' own: each index has its own builder
//! (ADR 0012 stage 8a, 6b-1c), so one index's item can reach its lane after
//! a newer change of another index's, and a change the document cap split
//! reaches a lane as several items of one position.
//!
//! **After a failed run, and on a fresh engine,** each lane the failure is
//! blamed on (every lane, for a fresh engine or a failure no lane caused)
//! writes its oldest item alone, as a barrier, and opens only once a write on
//! it landed; other lanes write freely.
//! A fresh engine counts because a previous process may have stopped on that
//! item. So a write that fails as a whole every time (a destination refusing
//! the request it makes) halts the sink after its first run: on every restart
//! of the engine nothing behind it lands, no progress resets the backoff, and
//! the lane fills. In the run that first meets it, writes already granted beside
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
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use kernel::{Envelope, IndexName, Position};
use sink::{CaughtUp, Visibility, WriteReport};
use stream::{
    AckHandle, Batch, Consumer, Delivery, ItemKey, LaneItem, LaneKey, Request, Substitute, Ticket,
};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::Instrument;

use super::flights::Flights;
use super::health::{HeldForSeed, LaneState, supersedes};
use super::walked::Walked;
use super::{
    Blame, IDLE_TICK, LaneHealth, ParkReason, ProbeOutcome, SinkControl, SinkEngine, shown_id,
    until,
};
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
    /// When it was taken off the lane: a merge wait is bounded from here.
    ready_at: Instant,
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
    /// How far its backfill has durably got, and when to record it.
    walked: Walked,
    /// Open, parked or being staged again; outlives the run (the engine
    /// keeps it).
    health: LaneState,
    /// The largest build number received in this run, noted while the lane
    /// has F9 entries (`prune_superseded`).
    newest: Option<u64>,
    /// The item a reindex left the lane holding until its index is seeded
    /// (stage 9, X2): neither written nor acknowledged meanwhile.
    held: Option<Queued>,
    /// While the lane has F9 entries: every item of it received in this run
    /// and not acknowledged, by receive order, with its build number (none:
    /// unknown). What decides when an entry may go (the note's 14.10).
    account: Option<BTreeMap<u64, Option<u64>>>,
}

impl LaneRun {
    fn waiting(&self) -> usize {
        self.live_waiting + self.backfill_waiting
    }
}

enum Queue {
    Batch { batch: Batch, documents: Documents },
    Complete { index: IndexName, ticket: Ticket },
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
    /// When it was sent: a write out longer than the merge wait is not
    /// waited for.
    started: Instant,
    /// For a caught-up write, how many writes had returned at its grant, and
    /// the lanes it is caught up for.
    caught_up: Option<(u64, Vec<LaneKey>)>,
    /// The parked lane this write probes: its front item, alone.
    probe: Option<usize>,
    /// The fresh build the probe writes in its item's place (F5).
    substitute: Option<Box<Substituted>>,
}

/// A held item to acknowledge: its handle, lane, receive order and build.
type HeldAck = (Box<dyn AckHandle>, usize, u64, Option<u64>);

/// A fresh build written in a parked item's place: its delivery, its build
/// number and its documents' ids (what it supersedes once it lands, F9).
struct Substituted {
    handle: Box<dyn AckHandle>,
    build: Option<u64>,
    ids: Vec<String>,
}

/// One lane item of a write.
struct Part {
    handle: Box<dyn AckHandle>,
    lane: usize,
    key: Option<ItemKey>,
    position: Option<Position>,
    documents: Documents,
    seq: u64,
    /// The item itself, put back in the window if its lane parks.
    batch: Batch,
    /// When it was first taken off the lane.
    ready_at: Instant,
}

impl Flight {
    fn lanes(&self) -> impl Iterator<Item = usize> + '_ {
        self.parts.iter().map(|part| part.lane)
    }
}

/// A seed in flight: a lane's `SnapshotComplete`, marking its index seeded.
struct Seeding {
    index: IndexName,
    /// The request the `SnapshotComplete` answers.
    ticket: Ticket,
    handle: Box<dyn AckHandle>,
    lane: usize,
    seq: u64,
}

/// What a flight returned.
enum Outcome {
    Write(sink::Result<WriteReport>),
    Seed(sink::Result<()>),
    Record(sink::Result<()>),
    /// A quiet lane's empty caught-up write.
    Refresh(sink::Result<WriteReport>),
    /// A lane's index staged again after its target went missing.
    Restage(Result<()>),
}

/// A lane waiting for its empty caught-up write: the item whose
/// acknowledgement waits for it, if any (a run's opening caught-up write has
/// none). One flight may cover several lanes.
struct Refreshing {
    handle: Option<Box<dyn AckHandle>>,
    lane: usize,
    /// The waiting item's lane order.
    seq: Option<u64>,
    /// The waiting item's build number.
    build: Option<u64>,
    oldest: bool,
    covers_below: u64,
    /// Something of the lane landed while this was out: its position may be
    /// one this write doesn't cover, so the lane is checked again on return.
    again: bool,
}

/// A flight that returned.
struct Landed {
    flight: u64,
    outcome: Outcome,
    took: Duration,
}

/// The state of one run.
pub(super) struct Run<'a> {
    engine: &'a SinkEngine,
    lanes: Vec<LaneRun>,
    window: VecDeque<Queued>,
    flights: Flights<'a, Landed>,
    flying: Vec<(u64, Flight)>,
    seeding: Vec<(u64, Seeding)>,
    /// Backfill checkpoints in flight, by lane: they hold no write slot.
    recording: Vec<(u64, usize)>,
    /// The empty caught-up writes in flight, by flight.
    refreshing: Vec<(u64, Refreshing)>,
    /// Lanes being staged again, by flight: they hold no write slot.
    restaging: Vec<(u64, usize)>,
    /// The sink's substitute lane, when the stream has one: fresh builds of
    /// parked lanes' items (stage 9). Without it a refused lane writes its
    /// own item again.
    substitutes: Option<Box<dyn Consumer<Substitute>>>,
    /// Lanes whose F9 map has entries: while none has, a received item is
    /// not looked at for them.
    superseding: usize,
    /// Lanes holding an item for a reindex's seed (X2): while none does, a
    /// received item is not looked at for it.
    holding: usize,
    /// Held items settled (their index seeded), to acknowledge on the run's
    /// next step: lane, receive order, build number.
    unacknowledged_held: Vec<HeldAck>,
    /// Lanes not open (parked or restaging): while none is, nothing of the
    /// parking is looked at.
    unhealthy: usize,
    /// The stop's one probe of every parked lane was scheduled.
    stop_probed: bool,
    /// While live items wait for the writes out to merge: when the wait ends.
    hold_until: Option<Instant>,
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
        let lanes: Vec<LaneRun> = lanes
            .into_iter()
            .map(|(key, consumer)| {
                let blamed = blame.covers(&key);
                let health = engine.lane_state(&key);
                // A run starts its F9 account empty (14.10).
                let account = (!health.superseded.is_empty()).then(BTreeMap::new);
                LaneRun {
                    health,
                    barrier: blamed && !matches!(blame, Blame::Nobody),
                    counts: blamed,
                    key,
                    consumer,
                    closed: false,
                    live_waiting: 0,
                    backfill_waiting: 0,
                    walked: Walked::default(),
                    newest: None,
                    held: None,
                    account,
                }
            })
            .collect();
        let unhealthy = lanes.iter().filter(|lane| !lane.health.is_open()).count();
        // A restarted run keeps applying what the last one's substitutes
        // supersede (B2): the lanes it redelivers may hold older builds.
        let superseding = lanes
            .iter()
            .filter(|lane| !lane.health.superseded.is_empty())
            .count();
        let holding = lanes
            .iter()
            .filter(|lane| lane.health.held_for_seed.is_some())
            .count();
        let substitutes = engine
            .stream
            .substitutes(&engine.name)
            .ok()
            .map(|lane| lane.consumer);
        Self {
            engine,
            lanes,
            restaging: Vec::new(),
            substitutes,
            superseding,
            holding,
            unacknowledged_held: Vec::new(),
            unhealthy,
            stop_probed: false,
            window: VecDeque::new(),
            flights: Flights::new(),
            flying: Vec::new(),
            seeding: Vec::new(),
            recording: Vec::new(),
            refreshing: Vec::new(),
            hold_until: None,
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
        if let Err(error) = self.owe_opening_refreshes().await {
            self.fail(error, &[]);
        }
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
            let hold_until = self.hold_until;
            let lane_wake = self.lane_wake(Instant::now());
            // A stop changes nothing for the loop but the backfill waiting
            // (skipped unwritten) and the parked lanes (probed at once), so it
            // is watched only while some wait.
            let engine = self.engine;
            let stop = &engine.stop;
            let watch_stop =
                (self.backfill_waiting > 0 || self.unhealthy > 0) && !stop.is_requested();
            let awaiting = self.awaits_a_substitute();
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
                    woke = lane_event(lane_wake, &mut self.substitutes, awaiting), if lane_wake.is_some() || awaiting => {
                        if let Some(substitute) = woke
                            && let Err(error) = self.substituted(substitute).await
                        {
                            self.fail(error, &[]);
                        }
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
                        Ok(Some(delivery)) => {
                            if let Err(error) = self.queue(lane, delivery) {
                                self.fail(error, &[lane]);
                            }
                        }
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
                () = until(hold_until), if hold_until.is_some() => {
                    // A failed run or a pending reindex grants nothing, so
                    // nothing else ends this hold: left set, its past instant
                    // is ready on every turn.
                    self.hold_until = None;
                    self.close_idle();
                }
                woke = lane_event(lane_wake, &mut self.substitutes, awaiting), if lane_wake.is_some() || awaiting => {
                    self.close_idle();
                    if let Some(substitute) = woke
                        && let Err(error) = self.substituted(substitute).await
                    {
                        self.fail(error, &[]);
                    }
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
        if !self.unacknowledged_held.is_empty() {
            self.acknowledge_held().await?;
        }
        if self.engine.stop.is_requested() {
            self.skip_backfill().await?;
            self.probe_on_stop();
        }
        let pending = if self.flights.is_empty() {
            self.engine.take_pending_reindex()
        } else {
            None
        };
        if let Some(indexes) = pending {
            let started = Instant::now();
            match self.engine.reindex(indexes.clone()).await {
                Ok(staged) => self.reindexed(&staged).await?,
                Err(error) => {
                    self.engine.set_pending_reindex(indexes);
                    return Err(error);
                }
            }
            self.engine.observer.on_stage(
                StageTiming::new(Stage::Reindex, started.elapsed()).sink(&self.engine.name),
            );
        }
        if !self.engine.reindex_pending() {
            if self.unhealthy > 0 {
                self.start_restages();
                self.ask_rebuilds().await?;
            }
            // An ordered sink first takes every lane's front, so the pick by
            // position sees an earlier item a lane held back while its write
            // was in flight.
            if self.ordered() && self.wants_more() {
                self.fill()?;
            }
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
            // A write that returned at its first poll, inside a grant, may
            // have found its target missing: stage it now, not at the next
            // wake-up.
            if self.unhealthy > 0 {
                self.start_restages();
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
    /// a lane behind the barrier of a failed run, or a parked lane's probe.
    fn in_lane_order(&self, lane: usize) -> bool {
        self.lanes
            .get(lane)
            .is_some_and(|lane| lane.barrier || !lane.health.is_open())
    }

    /// Whether `lane` is waiting to be staged again, or being staged.
    fn lane_restaging(&self, lane: usize) -> bool {
        self.lanes
            .get(lane)
            .is_some_and(|run| matches!(run.health.health, LaneHealth::Restaging { .. }))
    }

    /// Whether `lane` is written as usual (not parked, not being staged).
    fn lane_open(&self, lane: usize) -> bool {
        self.lanes.get(lane).is_some_and(|run| run.health.is_open())
    }

    /// Whether `lane` may be granted now: open, or parked with its probe due
    /// and nothing of it in flight.
    fn grantable(&self, lane: usize, now: Instant) -> bool {
        let Some(run) = self.lanes.get(lane) else {
            return false;
        };
        match &run.health.health {
            LaneHealth::Open => true,
            LaneHealth::Parked { .. } => run.health.due <= now && !self.lane_in_flight(lane),
            _ => false,
        }
    }

    /// The next time a parked lane's probe or a restaging lane's staging comes
    /// due, if any is still ahead: a due one starts on the next pass, or once
    /// what holds it back (a write of the lane, every slot) returns.
    fn lane_wake(&self, now: Instant) -> Option<Instant> {
        if self.unhealthy == 0 || self.failure.is_some() {
            return None;
        }
        self.lanes
            .iter()
            .filter(|run| !run.health.is_open() && run.health.due > now)
            .map(|run| run.health.due)
            .min()
    }

    /// Once a stop is requested, every parked lane is probed at once: a block
    /// lifted meanwhile still drains, without waiting out the backoff.
    fn probe_on_stop(&mut self) {
        if self.stop_probed || self.unhealthy == 0 {
            return;
        }
        self.stop_probed = true;
        let now = Instant::now();
        for run in &mut self.lanes {
            if run.health.probe_now(now) {
                tracing::info!(lane = %run.key, "stopping: probing the parked lane once more now");
                self.engine.store_lane(&run.key, &run.health, true);
            }
        }
    }

    /// Stage again each lane whose target went missing, once nothing of it
    /// is in flight, as a flight holding no write slot.
    fn start_restages(&mut self) {
        let now = Instant::now();
        let due: Vec<usize> = (0..self.lanes.len())
            .filter(|lane| {
                self.lanes.get(*lane).is_some_and(|run| {
                    matches!(run.health.health, LaneHealth::Restaging { .. })
                        && run.health.due <= now
                })
            })
            .filter(|lane| !self.lane_in_flight(*lane))
            .collect();
        for lane in due {
            let Some(index) = self.lanes.get(lane).map(|run| run.key.index.clone()) else {
                continue;
            };
            tracing::info!(%index, sink = %self.engine.name, "staging the index again: its target is gone");
            let flight = self.next_flight;
            self.next_flight += 1;
            self.restaging.push((flight, lane));
            let engine = self.engine;
            let restage = async move {
                let started = Instant::now();
                let result = engine.restage(&index).await;
                Landed {
                    flight,
                    outcome: Outcome::Restage(result),
                    took: started.elapsed(),
                }
            };
            if let Some(landed) = self.flights.start(restage.boxed()) {
                self.restage_returned(
                    landed.flight,
                    match landed.outcome {
                        Outcome::Restage(result) => result,
                        _ => Ok(()),
                    },
                );
            }
        }
    }

    /// A lane's staging returned: staged, the lane opens and its items are
    /// written again; failed, it is tried again after its backoff. Either
    /// way, how far the lane's walk got in the missing target means nothing
    /// in the new one.
    fn restage_returned(&mut self, flight: u64, result: Result<()>) {
        let Some(at) = self.restaging.iter().position(|(id, _)| *id == flight) else {
            return;
        };
        let (_, lane) = self.restaging.swap_remove(at);
        let now = Instant::now();
        let Some(run) = self.lanes.get_mut(lane) else {
            return;
        };
        run.walked.reset();
        match result {
            Ok(()) => {
                if let Some(down) = run.health.reopen(now) {
                    self.unhealthy = self.unhealthy.saturating_sub(1);
                    tracing::info!(lane = %run.key, after = ?down, "the index is staged again; the lane is open");
                    self.engine.store_lane(&run.key, &run.health, true);
                }
            }
            Err(error) => {
                run.health.restage_failed(now);
                tracing::warn!(lane = %run.key, %error, retry_in = ?run.health.due.saturating_duration_since(now), "could not stage the index again; trying again later");
                self.engine.store_lane(&run.key, &run.health, false);
            }
        }
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
        if run.barrier || !run.health.is_open() || self.ordered() {
            return run.waiting() == 0 && !self.lane_in_flight(lane);
        }
        // With several lanes, a lane with nothing waiting always takes one
        // item, so a quiet index's next change is seen by live-first and the
        // lanes' turns while a busy one keeps every slot: at most one more
        // delivered item per lane.
        if self.lanes.len() > 1 && run.waiting() == 0 {
            return true;
        }
        run.waiting() < LOOKAHEAD
            && (self.writing() < self.slots()
                || (run.live_waiting == 0 && run.backfill_waiting > 0))
    }

    /// Whether a write, the seed, a checkpoint or a staging of `lane` is in
    /// flight.
    fn lane_in_flight(&self, lane: usize) -> bool {
        self.flying
            .iter()
            .any(|(_, flight)| flight.lanes().any(|at| at == lane))
            || self.lane_seeding(lane)
            || self.recording.iter().any(|(_, at)| *at == lane)
            || self.refreshing.iter().any(|(_, at)| at.lane == lane)
            || self.restaging.iter().any(|(_, at)| *at == lane)
    }

    /// The flights holding a write slot: writes and seeds, not checkpoints;
    /// a quiet lane's caught-up write only for an ordered sink, which writes
    /// one thing at a time.
    fn writing(&self) -> usize {
        let refreshes = if self.ordered() {
            0
        } else {
            self.refresh_flights()
        };
        self.flights.len() - self.recording.len() - self.restaging.len() - refreshes
    }

    /// The caught-up write flights out: one may cover several lanes.
    fn refresh_flights(&self) -> usize {
        let mut flights: Vec<u64> = self.refreshing.iter().map(|(flight, _)| *flight).collect();
        flights.sort_unstable();
        flights.dedup();
        flights.len()
    }

    /// Whether `lane`'s seed is in flight: nothing after it on the lane
    /// starts until it returned.
    fn lane_seeding(&self, lane: usize) -> bool {
        self.seeding.iter().any(|(_, seeding)| seeding.lane == lane)
    }

    /// The writes (and the seed) of `lane` in flight: the slots it holds.
    fn lane_writes(&self, lane: usize) -> usize {
        self.flying
            .iter()
            .filter(|(_, flight)| flight.lanes().any(|at| at == lane))
            .count()
            + self
                .seeding
                .iter()
                .filter(|(_, seeding)| seeding.lane == lane)
                .count()
    }

    /// The most slots one lane may hold: with several lanes and several
    /// slots, all but one, so an index whose writes never return can't take
    /// every slot from its siblings. One slot, or one lane, can't be shared.
    fn lane_cap(&self) -> usize {
        let slots = self.slots();
        if self.lanes.len() > 1 && slots > 1 {
            slots - 1
        } else {
            slots
        }
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
                            self.queue(lane, delivery)?;
                            took = true;
                        }
                    },
                }
            }
        }
        Ok(took)
    }

    /// Take a delivered item into the window. A live item (it carries a
    /// position) must carry its key: without one it could be neither marked
    /// in the progress nor fenced, and a later item's mark would cover it, so
    /// it fails the run, left unacknowledged.
    fn queue(&mut self, lane: usize, delivery: Delivery<LaneItem>) -> Result<()> {
        self.close_idle();
        let (item, handle) = delivery.into_parts();
        let item = match item {
            LaneItem::Batch(batch) => {
                if batch.position.is_some() && batch.key.is_none() {
                    let lane = self
                        .lanes
                        .get(lane)
                        .map_or_else(String::new, |run| run.key.to_string());
                    return Err(EngineError::UnkeyedLiveItem(lane));
                }
                if let Some(key) = batch.key
                    && let Some(position) = batch.position
                    && let Some(run) = self.lanes.get(lane)
                {
                    let mut fences = self.engine.lock_fences();
                    fences.received(&run.key, key, position);
                    self.next_due = fences.next_due();
                }
                let batch = if self.superseding > 0 {
                    self.past_superseded(lane, batch)
                } else {
                    batch
                };
                let documents = Documents::new(&batch.envelopes);
                Queue::Batch { batch, documents }
            }
            LaneItem::SnapshotComplete { index, ticket } => Queue::Complete { index, ticket },
        };
        let seq = self.next_seq;
        self.next_seq += 1;
        // Received: into its lane's F9 account (14.10), while it keeps one.
        if self.superseding > 0
            && let Queue::Batch { batch, .. } = &item
            && let Some(run) = self.lanes.get_mut(lane)
            && let Some(account) = run.account.as_mut()
        {
            account.insert(seq, batch.build);
            run.newest = run.newest.max(batch.build);
        }
        if let Queue::Complete { .. } = &item
            && let Some(run) = self.lanes.get_mut(lane)
        {
            run.walked.boundary(seq);
        }
        if let Queue::Batch { batch, .. } = &item
            && batch.position.is_none()
            && let Some(run) = self.lanes.get_mut(lane)
        {
            // A page of a backfill cut short counts in the walk's order but
            // marks nothing: its walk is not the one filling this target.
            let mark = if self.engine.answers(&run.key.index, batch.ticket) {
                batch.walked.clone()
            } else {
                None
            };
            run.walked.received(seq, mark, Instant::now());
        }
        if self.holding > 0
            && let Queue::Batch { batch, .. } = &item
            && let Some(run) = self.lanes.get(lane)
            && let Some(held) = run.health.held_for_seed
            && held.key == batch.key
            && held.build == batch.build
        {
            // The item a reindex left the lane holding, redelivered (X7):
            // held again, or, its index seeded since, acknowledged unwritten.
            let queued = Queued {
                item,
                handle,
                lane,
                seq,
                ready_at: Instant::now(),
            };
            if held.seeded {
                return self.settle_held(lane, queued);
            }
            if let Some(run) = self.lanes.get_mut(lane) {
                run.held = Some(queued);
            }
            return Ok(());
        }
        self.count(lane, &item, 1);
        self.window.push_back(Queued {
            item,
            handle,
            lane,
            seq,
            ready_at: Instant::now(),
        });
        Ok(())
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
                ..
            }) = self.take_from_window(index)
            else {
                continue;
            };
            self.engine
                .lock_skipped()
                .extend(batch.envelopes.iter().map(|e| e.index.clone()));
            if let Some(run) = self.lanes.get_mut(lane) {
                run.walked.hold(seq);
            }
            let oldest = self.is_oldest(lane, seq);
            self.acknowledge(handle, lane, seq, batch.build, oldest, None)
                .await?;
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
            if self.writing() >= slots {
                return Ok(());
            }
            if let Some(at) = self.runnable_complete() {
                if let Some(Queued {
                    item: Queue::Complete { index, ticket },
                    handle,
                    lane,
                    seq,
                    ..
                }) = self.take_from_window(at)
                {
                    if let Some(run) = self.lanes.get_mut(lane) {
                        run.walked.ended(seq, Instant::now());
                    }
                    if self.engine.cut_short(&index) || self.engine.superseded(&index, ticket) {
                        self.acknowledge(handle, lane, seq, None, true, None)
                            .await?;
                    } else if let Some(landed) = self.seed(index, ticket, handle, lane, seq) {
                        self.landed(landed).await?;
                    }
                }
                continue;
            }
            if self.holds_for_merge() {
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
                    ready_at,
                }) = self.take_from_window(at)
                {
                    envelopes.push(Arc::clone(&batch.envelopes));
                    parts.push(Part {
                        handle,
                        lane,
                        key: batch.key,
                        position: batch.position,
                        documents,
                        seq,
                        batch,
                        ready_at,
                    });
                }
            }
            parts.reverse();
            envelopes.reverse();
            let Some(first) = parts.first() else {
                return Ok(());
            };
            let work = work_of(first.position);
            let probe = Some(first.lane).filter(|lane| !self.lane_open(*lane));
            if let Some(lane) = probe
                && let Some(run) = self.lanes.get_mut(lane)
            {
                // The item itself is the probe: a fresh build still awaited
                // is no longer (it would come too late, F15), and the ask
                // already counted this probe.
                if run.health.rebuild.take().is_none() {
                    run.health.probes += 1;
                }
                if run.health.probes > 1 {
                    tracing::info!(lane = %run.key, probe = run.health.probes, "probing the parked lane again: its front item, alone");
                }
            }
            // Caught up for each lane of the write with nothing else queued or
            // waiting: as with one item per write, that lane's position need
            // not wait out a refresh. Only those lanes' indexes are refreshed.
            let mut caught_up: Vec<usize> = parts
                .iter()
                .map(|part| part.lane)
                .filter(|lane| {
                    !self.window.iter().any(|queued| queued.lane == *lane)
                        && self
                            .lanes
                            .get(*lane)
                            .is_some_and(|run| run.consumer.is_empty())
                })
                .collect();
            caught_up.dedup();
            if let Some(landed) = self.launch(parts, envelopes, work, caught_up, probe, None) {
                self.landed(landed).await?;
            }
        }
        Ok(())
    }

    /// Whether the live items waiting are held for the writes out, so they go
    /// out merged with what else is ready once those return. Only while every
    /// waiting item is live (backfill is never held), on a merging, unordered
    /// sink with several lanes, and only for a write out shorter than the
    /// merge wait: a stalled or slow write is not waited for. The hold ends
    /// when the oldest waiting item has waited the merge wait, or when every
    /// write out reaches that age, whichever comes first; a write returning
    /// ends it sooner, when nothing younger is out.
    fn holds_for_merge(&mut self) -> bool {
        self.hold_until = self.merge_hold(Instant::now());
        self.hold_until.is_some()
    }

    fn merge_hold(&self, now: Instant) -> Option<Instant> {
        let bound = self.engine.merge_wait;
        if bound.is_zero() || self.lanes.len() < 2 || self.ordered() || self.merge_limit() < 2 {
            return None;
        }
        let oldest_ready = if self.unhealthy == 0 {
            if self.backfill_waiting > 0 || self.live_waiting == 0 {
                return None;
            }
            self.window
                .iter()
                .filter(|queued| matches!(queued.item, Queue::Batch { .. }))
                .map(|queued| queued.ready_at)
                .min()?
        } else {
            // A parked lane's items wait for its probe, not for a merge: they
            // neither hold nor end a hold, and a due probe is never held.
            if (0..self.lanes.len()).any(|lane| !self.lane_open(lane) && self.grantable(lane, now))
            {
                return None;
            }
            let mut oldest: Option<Instant> = None;
            for queued in self
                .window
                .iter()
                .filter(|queued| self.lane_open(queued.lane))
            {
                match &queued.item {
                    Queue::Batch { batch, .. } if batch.position.is_none() => return None,
                    Queue::Batch { .. } => {
                        oldest = Some(oldest.map_or(queued.ready_at, |at| at.min(queued.ready_at)));
                    }
                    Queue::Complete { .. } => {}
                }
            }
            oldest?
        };
        let youngest_write = self.flying.iter().map(|(_, flight)| flight.started).max()?;
        let until = oldest_ready.min(youngest_write) + bound;
        (now < until).then_some(until)
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
            // A lane being staged again holds its completion: the staging
            // decides whether the backfill it ends was cut. A parked lane's
            // runs once nothing is in front of it, as any other lane's.
            if matches!(queued.item, Queue::Complete { .. })
                && !self.lane_restaging(queued.lane)
                && !self.lane_in_flight(queued.lane)
            {
                return Some(at);
            }
        }
        None
    }

    /// The window indexes of the next write, in window order, if one item is
    /// admissible: the item lanes' turns and live-first choose, then every
    /// other admissible item of the same kind that fits, lanes in turn, up to
    /// the merge limit in documents. Merging takes what is ready; the merge
    /// wait, if any, has already been decided. An ordered sink, and a lane
    /// behind a failed run's barrier, write one item alone.
    fn pick(&mut self) -> Vec<usize> {
        // One lane, nothing in flight, one kind waiting: no contest, no turn
        // to take, so the front is the pick. Unless another item could join it
        // in the write, nothing needs hashing, as with one item per write.
        if self.lanes.len() == 1
            && self.unhealthy == 0
            && self.flying.is_empty()
            && self.seeding.is_empty()
            && !(self.live_waiting > 0 && self.backfill_waiting > 0)
            && matches!(
                self.window.front(),
                Some(Queued {
                    item: Queue::Batch { .. },
                    ..
                })
            )
            && !self.could_join_the_front()
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
        // A seeding lane starts nothing, nor does a lane holding its share of
        // the slots.
        let cap = self.lane_cap();
        let now = Instant::now();
        let mut blocked: Vec<bool> = (0..lanes)
            .map(|lane| {
                self.lane_seeding(lane)
                    || self.lane_writes(lane) >= cap
                    || (self.unhealthy > 0 && !self.grantable(lane, now))
            })
            .collect();
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
        // one item per lane per round, each lane in window order. A write is
        // as slow as its slowest part, so lanes merge only while none of them
        // has a write out (it may be stuck): a lane with one writes alone,
        // several of its own items at most.
        let alone = self.lane_writes(first_lane) > 0;
        let mut queues: Vec<std::collections::VecDeque<usize>> = (0..lanes)
            .map(|offset| (first_lane + offset) % lanes)
            .filter(|lane| *lane == first_lane || (!alone && self.lane_writes(*lane) == 0))
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

    /// Whether another waiting item could join the front item in one write:
    /// merging is on for its lane, and some item before the lane's first
    /// `SnapshotComplete` is small enough to fit beside it.
    fn could_join_the_front(&self) -> bool {
        let Some(Queued {
            item: Queue::Batch { batch, .. },
            lane,
            ..
        }) = self.window.front()
        else {
            return false;
        };
        if self.ordered() || self.in_lane_order(*lane) {
            return false;
        }
        let room = self.merge_limit().saturating_sub(batch.envelopes.len());
        room > 0
            && self
                .window
                .iter()
                .skip(1)
                .take_while(|queued| matches!(queued.item, Queue::Batch { .. }))
                .any(|queued| match &queued.item {
                    Queue::Batch { batch, .. } => batch.envelopes.len() <= room,
                    Queue::Complete { .. } => false,
                })
    }

    fn launch(
        &mut self,
        parts: Vec<Part>,
        envelopes: Vec<Arc<[Envelope]>>,
        work: Work,
        caught_up: Vec<usize>,
        probe: Option<usize>,
        substitute: Option<Box<Substituted>>,
    ) -> Option<Landed> {
        let flight = self.next_flight;
        self.next_flight += 1;
        let lanes: Vec<LaneKey> = caught_up
            .iter()
            .filter_map(|lane| self.lanes.get(*lane))
            .map(|run| run.key.clone())
            .collect();
        let indexes: Vec<IndexName> = lanes.iter().map(|lane| lane.index.clone()).collect();
        let caught_up = !lanes.is_empty();
        let covers_below = caught_up.then(|| (self.engine.lock_fences().returned(), lanes));
        self.flying.push((
            flight,
            Flight {
                parts,
                work,
                started: Instant::now(),
                caught_up: covers_below,
                probe,
                substitute,
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
            let scope = if indexes.is_empty() {
                CaughtUp::No
            } else {
                CaughtUp::Indexes(&indexes)
            };
            let result = sink.write_parts(&slices, scope).await;
            Landed {
                flight,
                outcome: Outcome::Write(result),
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

    /// Seed `index` as a flight holding one write slot, its lane blocked
    /// until it returns. Like a write, it starts now, and one that completes
    /// on its first poll is handed back.
    fn seed(
        &mut self,
        index: IndexName,
        ticket: Ticket,
        handle: Box<dyn AckHandle>,
        lane: usize,
        seq: u64,
    ) -> Option<Landed> {
        let flight = self.next_flight;
        self.next_flight += 1;
        let sink = &self.engine.sink;
        let marking = index.clone();
        self.seeding.push((
            flight,
            Seeding {
                index,
                ticket,
                handle,
                lane,
                seq,
            },
        ));
        let seed = async move {
            let started = Instant::now();
            let result = sink.mark_seeded(&marking).await;
            Landed {
                flight,
                outcome: Outcome::Seed(result),
                took: started.elapsed(),
            }
        };
        self.flights.start(seed.boxed())
    }

    /// A seed returned: once it succeeded the index is seeded and its
    /// `SnapshotComplete` acknowledged; a failed one fails the run blamed on
    /// its lane, and the lane hands the `SnapshotComplete` back.
    async fn seed_returned(
        &mut self,
        flight: u64,
        result: sink::Result<()>,
        took: Duration,
    ) -> Result<()> {
        let Some(at) = self.seeding.iter().position(|(id, _)| *id == flight) else {
            return Ok(());
        };
        let (_, seeding) = self.seeding.swap_remove(at);
        match result {
            Ok(()) => {}
            // The target is not the one staged: the lane is staged again,
            // which cuts the backfill, and the `SnapshotComplete` waits for
            // it, seeding nothing.
            Err(sink::SinkError::TargetChanged(reason)) => {
                let lane = seeding.lane;
                self.put_back_complete(seeding);
                self.hold(lane, Held::Missing(reason), false);
                return Ok(());
            }
            Err(error) => {
                self.fail(error.into(), &[seeding.lane]);
                return Ok(());
            }
        }
        self.engine.seeded(&seeding.index, seeding.ticket, took);
        if self.holding > 0 && !self.engine.is_outstanding(&seeding.index) {
            self.release_held(seeding.lane).await?;
        }
        let oldest = self.is_oldest(seeding.lane, seeding.seq);
        self.acknowledge(
            seeding.handle,
            seeding.lane,
            seeding.seq,
            None,
            oldest,
            None,
        )
        .await?;
        if let Some(run) = self.lanes.get_mut(seeding.lane) {
            run.barrier = false;
        }
        Ok(())
    }

    /// A write returned: decide each part, move the fences, and acknowledge
    /// every part that landed. A part whose index was refused under `stop`,
    /// blocked, or found missing goes back in the window unacknowledged and
    /// its lane parks (or is staged again); the run goes on. The other parts
    /// landed and are acknowledged. A write that failed fails every part,
    /// blamed on all their lanes, and ends the run.
    async fn landed(&mut self, landed: Landed) -> Result<()> {
        let result = match landed.outcome {
            Outcome::Write(result) => result,
            Outcome::Seed(result) => {
                return self.seed_returned(landed.flight, result, landed.took).await;
            }
            Outcome::Record(result) => {
                self.record_returned(landed.flight, result);
                return Ok(());
            }
            Outcome::Refresh(result) => {
                return self
                    .refresh_returned(landed.flight, result, landed.took)
                    .await;
            }
            Outcome::Restage(result) => {
                self.restage_returned(landed.flight, result);
                return Ok(());
            }
        };
        let Some(index) = self.flying.iter().position(|(id, _)| *id == landed.flight) else {
            return Ok(());
        };
        let (_, mut flight) = self.flying.swap_remove(index);
        let mut substitute = flight.substitute.take();
        self.engine.observer.on_stage(
            StageTiming::new(Stage::Write, landed.took)
                .work(flight.work)
                .sink(&self.engine.name),
        );
        let report = match result {
            Ok(report) => report,
            Err(error) => {
                let lanes: Vec<usize> = flight.lanes().collect();
                if let Some(lane) = flight.probe {
                    self.probed(lane, ProbeOutcome::Failed);
                }
                self.hold_walks(&flight.parts);
                self.fail(error.into(), &lanes);
                return Ok(());
            }
        };
        let held = self.engine.held_indexes(&report);
        if let Some(stray) = held.keys().find(|index| {
            !flight.parts.iter().any(|part| {
                self.lanes
                    .get(part.lane)
                    .is_some_and(|run| run.key.index.as_ref() == index.as_str())
            })
        }) {
            // A refusal naming an index no part holds can't be placed: every
            // part of the write is redelivered, as for a write that failed.
            let lanes: Vec<usize> = flight.lanes().collect();
            self.hold_walks(&flight.parts);
            self.fail(
                EngineError::Sink(sink::SinkError::Write(format!(
                    "the sink refused index {stray}, which no part of the write holds"
                ))),
                &lanes,
            );
            return Ok(());
        }
        let held_lane = |run: Option<&LaneRun>| {
            run.is_some_and(|run| held.contains_key(run.key.index.as_ref()))
        };
        let (failed, landed_parts): (Vec<Part>, Vec<Part>) = std::mem::take(&mut flight.parts)
            .into_iter()
            .partition(|part| held_lane(self.lanes.get(part.lane)));
        self.engine
            .quarantine(&report, |index| !held.contains_key(index));
        let mut parked: Vec<usize> = Vec::new();
        for mut part in failed {
            if !parked.contains(&part.lane) {
                parked.push(part.lane);
            }
            // A substitute refused again is done with (F6): the item goes
            // back as it was, and the next probe asks for another. Its
            // documents that were not refused landed, at its newer build: the
            // parked item, older, is written without them from now on (F9).
            let mut landed_anyway = None;
            let mut done_with = None;
            if flight.probe == Some(part.lane)
                && let Some(substitute) = substitute.take()
            {
                part.documents = Documents::new(&part.batch.envelopes);
                done_with = Some(substitute.handle);
                let refused = self
                    .lanes
                    .get(part.lane)
                    .and_then(|run| held.get(run.key.index.as_ref()))
                    .and_then(|why| match why {
                        Held::Refused { .. } => Some(run_rejected(&report, part.lane, &self.lanes)),
                        _ => None,
                    });
                if let (Some(refused), Some(build)) = (refused, substitute.build) {
                    let landed: Vec<String> = substitute
                        .ids
                        .into_iter()
                        .filter(|id| !refused.contains(id))
                        .collect();
                    landed_anyway = Some((part.lane, build, landed));
                }
            }
            self.put_back(part);
            if let Some((lane, build, landed)) = landed_anyway {
                self.supersede(lane, build, landed);
            }
            // Acknowledged once its landed documents are in the map: a
            // failing acknowledgement can't leave them out (m4).
            if let Some(handle) = done_with {
                handle.ack().await?;
            }
        }
        for &lane in &parked {
            let Some(index) = self.lanes.get(lane).map(|run| run.key.index.to_string()) else {
                continue;
            };
            if let Some(why) = held.get(index.as_str()) {
                self.hold(lane, why.clone(), flight.probe == Some(lane));
            }
        }
        if !landed_parts.is_empty() {
            let envelopes = landed_parts
                .iter()
                .map(|part| part.documents.envelopes.len())
                .sum();
            self.engine.observer.on_batch_committed(
                &self.engine.name,
                CommitStats::new(envelopes, landed.took, flight.work).sent(report.bytes),
            );
        }
        let written: Vec<(LaneKey, ItemKey)> = landed_parts
            .iter()
            .filter_map(|part| {
                let lane = self.lanes.get(part.lane)?.key.clone();
                Some((lane, part.key?))
            })
            .collect();
        if report.refreshes > 0 {
            self.engine
                .observer
                .on_refreshes(&self.engine.name, report.refreshes);
        }
        let returned = self.engine.write_returned(
            &written,
            report.visibility,
            flight
                .caught_up
                .as_ref()
                .map(|(covers_below, lanes)| (*covers_below, lanes.as_slice())),
        );
        self.next_due = returned.next_due;
        // A lane's last part of the write is the one whose acknowledgement may
        // leave the lane quiet.
        let last_of_lane: Vec<bool> = landed_parts
            .iter()
            .enumerate()
            .map(|(at, part)| {
                !landed_parts
                    .iter()
                    .skip(at + 1)
                    .any(|later| later.lane == part.lane)
            })
            .collect();
        let mut walked: Vec<usize> = Vec::new();
        for part in &landed_parts {
            if part.position.is_none()
                && let Some(run) = self.lanes.get_mut(part.lane)
            {
                run.walked.landed(part.seq);
                if !walked.contains(&part.lane) {
                    walked.push(part.lane);
                }
            }
        }
        if let Some(lane) = flight.probe
            && !parked.contains(&lane)
        {
            self.probed(lane, ProbeOutcome::Landed);
        }
        let mut acknowledged = Ok(());
        for (part, last) in landed_parts.into_iter().zip(last_of_lane) {
            let build = part.batch.build;
            // Let go of the batch before acknowledging it, so the lane frees it
            // as the acknowledgement lands.
            drop(part.documents);
            drop(part.batch);
            let oldest = self.is_oldest(part.lane, part.seq);
            // `wanting` names only lanes this write gave a keyed item: a
            // backfill part landing last on its lane asks the fences, since an
            // earlier live write of the lane may still want a caught-up write.
            let wants = if !last {
                Some(false)
            } else if part.key.is_none() {
                None
            } else {
                Some(
                    self.lanes
                        .get(part.lane)
                        .is_some_and(|run| returned.wanting.contains(&run.key)),
                )
            };
            if let Err(error) = self
                .acknowledge(part.handle, part.lane, part.seq, build, oldest, wants)
                .await
            {
                acknowledged = Err(error);
                break;
            }
            if let Some(run) = self.lanes.get_mut(part.lane) {
                run.barrier = false;
                if run.health.landed() {
                    self.engine.store_lane(&run.key, &run.health, false);
                }
            }
        }
        acknowledged?;
        // The item landed through its substitute (F5): the substitute is done
        // with, and the builds it is newer than are left out (F9).
        if let (Some(substitute), Some(lane)) = (substitute.take(), flight.probe) {
            if let Some(build) = substitute.build {
                self.supersede(lane, build, substitute.ids);
            }
            substitute.handle.ack().await?;
        }
        for lane in walked {
            if let Some(landed) = self.record_walked(lane) {
                Box::pin(self.landed(landed)).await?;
            }
        }
        Ok(())
    }

    /// A reindex staged `staged` (stage 9, 15.1): each of their lanes not
    /// open opens; a parked one holds its front item, unwritten and
    /// unacknowledged, until the index is seeded (X2), while the items behind
    /// it go to the new generation; a restaging one writes its items (X5).
    async fn reindexed(&mut self, staged: &[IndexName]) -> Result<()> {
        let now = Instant::now();
        for lane in 0..self.lanes.len() {
            let Some(run) = self.lanes.get(lane) else {
                continue;
            };
            if !staged.contains(&run.key.index) || run.health.is_open() {
                continue;
            }
            let parked = matches!(run.health.health, LaneHealth::Parked { .. });
            let front = parked
                .then(|| {
                    self.window
                        .iter()
                        .position(|queued| queued.lane == lane)
                        .filter(|at| {
                            matches!(
                                self.window.get(*at),
                                Some(Queued {
                                    item: Queue::Batch { .. },
                                    ..
                                })
                            )
                        })
                })
                .flatten();
            let held = front.and_then(|at| self.take_from_window(at));
            let Some(run) = self.lanes.get_mut(lane) else {
                continue;
            };
            if let Some(down) = run.health.open(now) {
                self.unhealthy = self.unhealthy.saturating_sub(1);
                tracing::info!(lane = %run.key, after = ?down, "reindexed: the lane is open; its parked item waits for the new generation's backfill");
            }
            if let Some(held) = held {
                if let Queue::Batch { batch, .. } = &held.item {
                    run.health.held_for_seed = Some(HeldForSeed {
                        key: batch.key,
                        build: batch.build,
                        seeded: false,
                    });
                    self.holding += 1;
                }
                run.held = Some(held);
            }
            self.engine.store_lane(&run.key, &run.health, true);
        }
        Ok(())
    }

    /// `lane`'s index is seeded: the item a reindex left it holding is
    /// acknowledged, its position applied (X2). Not received yet in this run,
    /// it will be acknowledged unwritten when it comes.
    async fn release_held(&mut self, lane: usize) -> Result<()> {
        let Some(run) = self.lanes.get_mut(lane) else {
            return Ok(());
        };
        let Some(held) = run.health.held_for_seed.as_mut() else {
            return Ok(());
        };
        held.seeded = true;
        match run.held.take() {
            Some(queued) => {
                self.settle_held(lane, queued)?;
                self.acknowledge_held().await
            }
            None => {
                self.engine.store_lane(&run.key, &run.health, false);
                Ok(())
            }
        }
    }

    /// Acknowledge the held items settled since the last step.
    async fn acknowledge_held(&mut self) -> Result<()> {
        for (handle, lane, seq, build) in std::mem::take(&mut self.unacknowledged_held) {
            let oldest = self.is_oldest(lane, seq);
            self.acknowledge(handle, lane, seq, build, oldest, None)
                .await?;
        }
        Ok(())
    }

    /// Acknowledge a held item unwritten, its position applied: its
    /// documents are in the seeded generation.
    fn settle_held(&mut self, lane: usize, queued: Queued) -> Result<()> {
        let Queued {
            item, handle, seq, ..
        } = queued;
        let Queue::Batch { batch, .. } = item else {
            return Ok(());
        };
        if let Some(run) = self.lanes.get_mut(lane) {
            run.health.held_for_seed = None;
            self.holding = self.holding.saturating_sub(1);
            self.engine.store_lane(&run.key, &run.health, false);
        }
        if let (Some(key), Some(run)) = (batch.key, self.lanes.get(lane)) {
            let written = [(run.key.clone(), key)];
            let returned = self
                .engine
                .write_returned(&written, Visibility::Immediate, None);
            self.next_due = returned.next_due;
        }
        self.unacknowledged_held
            .push((handle, lane, seq, batch.build));
        Ok(())
    }

    /// Whether some lane waits for its fresh build's substitute.
    fn awaits_a_substitute(&self) -> bool {
        // No probe starts once the run failed (review of #458, m2).
        self.unhealthy > 0
            && self.failure.is_none()
            && self.substitutes.is_some()
            && self.lanes.iter().any(|run| run.health.rebuild.is_some())
    }

    /// Ask, for each lane parked on a refusal whose probe is due, for a fresh
    /// build of its front item's documents (F1): the item goes unwritten until
    /// the substitute comes. Not once the stop is asked (F13), not without a
    /// substitute lane, and not for an item that doesn't carry its keys: those
    /// write the item itself (F2).
    async fn ask_rebuilds(&mut self) -> Result<()> {
        if self.substitutes.is_none() || self.engine.stop.is_requested() {
            return Ok(());
        }
        let now = Instant::now();
        for lane in 0..self.lanes.len() {
            let wants = self
                .lanes
                .get(lane)
                .is_some_and(|run| run.health.wants_rebuild(now));
            if !wants || self.lane_in_flight(lane) {
                continue;
            }
            let keys = self
                .window
                .iter()
                .find(|queued| queued.lane == lane)
                .and_then(|queued| match &queued.item {
                    // Every document's key, or none can be built again.
                    Queue::Batch { batch, .. } if !batch.envelopes.is_empty() => batch
                        .envelopes
                        .iter()
                        .map(|envelope| envelope.key.clone())
                        .collect::<Option<Vec<_>>>(),
                    _ => None,
                });
            let (Some(keys), Some(run)) = (keys, self.lanes.get_mut(lane)) else {
                continue;
            };
            let ticket = Ticket::new(self.engine.tickets.fetch_add(1, Ordering::Relaxed) + 1);
            let producer = self.engine.stream.requests()?.producer;
            let request = Request::Rebuild {
                index: run.key.index.clone(),
                sink: run.key.sink.clone(),
                keys,
                ticket,
            };
            // Offered, never waited for: a request lane nobody reads (the
            // ingest engine stopping) would stall every lane of the sink.
            // Not taken, the probe writes the item itself (F16).
            match producer.publish(request).now_or_never() {
                Some(published) => published?,
                None => continue,
            }
            tracing::info!(lane = %run.key, %ticket, "asking for the parked item's documents to be built again");
            run.health.asked(ticket, now);
            self.engine.store_lane(&run.key, &run.health, false);
        }
        Ok(())
    }

    /// A substitute arrived: written alone, as its lane's probe, in its
    /// parked item's place (F5); one the lane no longer waits for is
    /// acknowledged and dropped (F8).
    async fn substituted(
        &mut self,
        delivery: Option<stream::Result<Option<Delivery<Substitute>>>>,
    ) -> Result<()> {
        let Some(delivery) = delivery else {
            return Ok(());
        };
        let Some(delivery) = delivery? else {
            tracing::warn!("the substitute lane closed: parked lanes write their own items again");
            self.substitutes = None;
            for run in &mut self.lanes {
                run.health.rebuild = None;
            }
            return Ok(());
        };
        let (substitute, handle) = delivery.into_parts();
        let lane = self.lanes.iter().position(|run| run.key == substitute.lane);
        let front = lane.and_then(|lane| {
            let awaited = self.lanes.get(lane).is_some_and(|run| {
                !run.health.is_open() && run.health.rebuild == Some(substitute.ticket)
            });
            if !awaited || self.lane_in_flight(lane) {
                return None;
            }
            self.window
                .iter()
                .position(|queued| queued.lane == lane)
                .filter(|at| {
                    matches!(
                        self.window.get(*at),
                        Some(Queued {
                            item: Queue::Batch { .. },
                            ..
                        })
                    )
                })
        });
        let (Some(lane), Some(at)) = (lane, front) else {
            tracing::debug!(lane = %substitute.lane, ticket = %substitute.ticket, "a substitute no lane waits for; dropped");
            handle.ack().await?;
            return Ok(());
        };
        let Some(Queued {
            item: Queue::Batch { batch, .. },
            handle: item_handle,
            seq,
            ready_at,
            ..
        }) = self.take_from_window(at)
        else {
            return Ok(());
        };
        if let Some(run) = self.lanes.get_mut(lane) {
            run.health.rebuild = None;
        }
        let fresh = substitute.batch;
        let substituted = Box::new(Substituted {
            handle,
            build: fresh.build,
            ids: fresh
                .envelopes
                .iter()
                .map(|envelope| envelope.id.clone())
                .collect(),
        });
        let envelopes = Arc::clone(&fresh.envelopes);
        let part = Part {
            handle: item_handle,
            lane,
            key: batch.key,
            position: batch.position,
            documents: Documents::new(&fresh.envelopes),
            seq,
            batch,
            ready_at,
        };
        let work = work_of(part.position);
        let quiet = !self.window.iter().any(|queued| queued.lane == lane)
            && self
                .lanes
                .get(lane)
                .is_some_and(|run| run.consumer.is_empty());
        let caught_up = if quiet { vec![lane] } else { Vec::new() };
        if let Some(landed) = self.launch(
            vec![part],
            vec![envelopes],
            work,
            caught_up,
            Some(lane),
            Some(substituted),
        ) {
            self.landed(landed).await?;
        }
        Ok(())
    }

    /// `batch` of `lane` without the documents a landed substitute built later
    /// (F9), noting how far the lane's deliveries got.
    fn past_superseded(&mut self, lane: usize, batch: Batch) -> Batch {
        let Some(run) = self.lanes.get_mut(lane) else {
            return batch;
        };
        if run.health.superseded.is_empty() {
            return batch;
        }
        let superseded = &run.health.superseded;
        let build = batch.build;
        without(batch, |id| {
            superseded
                .get(id)
                .is_some_and(|substitute| supersedes(*substitute, build))
        })
    }

    /// `lane`'s substitute numbered `build` landed with the documents `ids`:
    /// merge them into the lane's map and leave them out of the older items
    /// already waiting. A lane without entries yet starts its account (14.10)
    /// from every place a received, unacknowledged item of it can be: the
    /// window, the writes in flight, the items waiting for a caught-up write.
    fn supersede(&mut self, lane: usize, build: u64, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        let mut found: BTreeMap<u64, Option<u64>> = BTreeMap::new();
        for queued in self.window.iter_mut().filter(|queued| queued.lane == lane) {
            if let Queue::Batch { batch, documents } = &mut queued.item {
                found.insert(queued.seq, batch.build);
                if supersedes(build, batch.build)
                    && batch.envelopes.iter().any(|e| ids.contains(&e.id))
                {
                    *batch = without(std::mem::take(batch), |id| ids.iter().any(|own| own == id));
                    *documents = Documents::new(&batch.envelopes);
                }
            }
        }
        for part in self.parts_in_flight(lane) {
            found.insert(part.seq, part.batch.build);
        }
        for (_, refreshing) in &self.refreshing {
            if refreshing.lane == lane
                && refreshing.handle.is_some()
                && let Some(seq) = refreshing.seq
            {
                found.insert(seq, refreshing.build);
            }
        }
        let Some(run) = self.lanes.get_mut(lane) else {
            return;
        };
        let was_empty = run.health.superseded.is_empty();
        for id in ids {
            let entry = run.health.superseded.entry(id).or_insert(build);
            *entry = (*entry).max(build);
        }
        if run.account.is_none() {
            run.newest = found.values().copied().max().flatten();
            run.account = Some(found);
        }
        if was_empty && !run.health.superseded.is_empty() {
            self.superseding += 1;
        }
        self.engine.store_lane(&run.key, &run.health, false);
    }

    /// Drop the F9 entries of `lane` no item can be written over any more
    /// (14.10): those numbered at most the smallest build number in the
    /// lane's account, or, with the account empty, at most one past the
    /// newest received in this run. Nothing once the run has failed, nor
    /// while an item of unknown number is unacknowledged.
    fn prune_superseded(&mut self, lane: usize) {
        if self.failure.is_some() {
            return;
        }
        let Some(run) = self.lanes.get_mut(lane) else {
            return;
        };
        let Some(account) = &run.account else {
            return;
        };
        let floor = if account.is_empty() {
            match run.newest {
                Some(newest) => newest.saturating_add(1),
                None => return,
            }
        } else {
            let mut least = u64::MAX;
            for build in account.values() {
                match build {
                    Some(build) => least = least.min(*build),
                    None => return,
                }
            }
            least
        };
        let before = run.health.superseded.len();
        run.health
            .superseded
            .retain(|_, substitute| *substitute > floor);
        if run.health.superseded.len() == before {
            return;
        }
        if run.health.superseded.is_empty() {
            run.account = None;
            run.newest = None;
            self.superseding = self.superseding.saturating_sub(1);
        }
        self.engine.store_lane(&run.key, &run.health, false);
    }

    /// Put a seed's `SnapshotComplete` back in the window, at its place in
    /// receive order, still unacknowledged: it runs again once its lane is
    /// staged again.
    fn put_back_complete(&mut self, seeding: Seeding) {
        let at = self
            .window
            .partition_point(|queued| queued.seq < seeding.seq);
        self.window.insert(
            at,
            Queued {
                item: Queue::Complete {
                    index: seeding.index,
                    ticket: seeding.ticket,
                },
                handle: seeding.handle,
                lane: seeding.lane,
                seq: seeding.seq,
                ready_at: Instant::now(),
            },
        );
    }

    /// Put a part of a returned write back in the window, at its place in
    /// receive order, still unacknowledged: its lane parked or is staged
    /// again, and writes it again later, before anything after it.
    fn put_back(&mut self, part: Part) {
        let at = self.window.partition_point(|queued| queued.seq < part.seq);
        let item = Queue::Batch {
            batch: part.batch,
            documents: part.documents,
        };
        self.count(part.lane, &item, 1);
        self.window.insert(
            at,
            Queued {
                item,
                handle: part.handle,
                lane: part.lane,
                seq: part.seq,
                ready_at: part.ready_at,
            },
        );
    }

    /// `lane`'s part of a write was held back by `why`: park the lane (a
    /// refusal, a block) or stage it again (a missing target). `probe`: the
    /// write was the lane's probe, so a park doubles its backoff.
    fn hold(&mut self, lane: usize, why: Held, probe: bool) {
        let now = Instant::now();
        let Some(run) = self.lanes.get_mut(lane) else {
            return;
        };
        let was_open = run.health.is_open();
        let (changed, outcome) = match why {
            Held::Missing(reason) => (run.health.restage(reason, now), ProbeOutcome::Missing),
            Held::Blocked(reason) => (
                run.health.park(ParkReason::Blocked { reason }, probe, now),
                ProbeOutcome::Blocked,
            ),
            Held::Refused { ids, reason } => (
                run.health
                    .park(ParkReason::Refused { ids, reason }, probe, now),
                ProbeOutcome::Refused,
            ),
        };
        if was_open && !run.health.is_open() {
            self.unhealthy += 1;
        }
        if changed {
            let retry_in = run.health.due.saturating_duration_since(now);
            match &run.health.health {
                LaneHealth::Parked { reason, .. } if was_open => tracing::warn!(
                    index = %run.key.index,
                    sink = %run.key.sink,
                    %reason,
                    backoff = ?retry_in,
                    "the lane is parked: its item is written again alone after the backoff; the other indexes go on"
                ),
                LaneHealth::Parked { reason, .. } => tracing::info!(
                    index = %run.key.index,
                    sink = %run.key.sink,
                    %reason,
                    backoff = ?retry_in,
                    "the parked lane's probe did not land; probing again after the backoff"
                ),
                LaneHealth::Restaging { reason, .. } => tracing::warn!(
                    index = %run.key.index,
                    sink = %run.key.sink,
                    %reason,
                    retry_in = ?retry_in,
                    "the index's target is gone: staging it again; the other indexes go on"
                ),
                LaneHealth::Open => {}
            }
        }
        self.engine.store_lane(&run.key, &run.health, changed);
        if probe {
            self.engine.observer.on_lane_probe(&run.key, outcome);
        }
    }

    /// A parked lane's probe returned `outcome` without being held: landed,
    /// it opens; failed with the write, it stays parked.
    fn probed(&mut self, lane: usize, outcome: ProbeOutcome) {
        let Some(run) = self.lanes.get_mut(lane) else {
            return;
        };
        self.engine.observer.on_lane_probe(&run.key, outcome);
        if outcome != ProbeOutcome::Landed {
            return;
        }
        if let Some(parked) = run.health.open(Instant::now()) {
            self.unhealthy = self.unhealthy.saturating_sub(1);
            tracing::info!(index = %run.key.index, sink = %run.key.sink, after = ?parked, "the parked lane's probe landed; the lane is open");
            self.engine.store_lane(&run.key, &run.health, true);
        }
    }

    /// `parts` go back unlanded: their walks' marks move no further.
    fn hold_walks(&mut self, parts: &[Part]) {
        for part in parts {
            if let Some(run) = self.lanes.get_mut(part.lane) {
                run.walked.hold(part.seq);
            }
        }
    }

    /// Record how far `lane`'s backfill got, when a record is due, as a
    /// flight beside the writes, holding no slot. Like a write, it starts now,
    /// and one that completes on its first poll is handed back.
    fn record_walked(&mut self, lane: usize) -> Option<Landed> {
        let run = self.lanes.get_mut(lane)?;
        let mark = run.walked.due(Instant::now())?;
        let index = run.key.index.clone();
        let flight = self.next_flight;
        self.next_flight += 1;
        self.recording.push((flight, lane));
        let sink = &self.engine.sink;
        let record = async move {
            let started = Instant::now();
            let result = sink.record_walked(&index, mark.as_str()).await;
            Landed {
                flight,
                outcome: Outcome::Record(result),
                took: started.elapsed(),
            }
        };
        self.flights.start(record.boxed())
    }

    /// A checkpoint returned. A failed one costs only redone work after a
    /// restart: it is logged and tried again when the next is due.
    fn record_returned(&mut self, flight: u64, result: sink::Result<()>) {
        let Some(at) = self.recording.iter().position(|(id, _)| *id == flight) else {
            return;
        };
        let (_, lane) = self.recording.swap_remove(at);
        let Some(run) = self.lanes.get_mut(lane) else {
            return;
        };
        run.walked.record_returned(result.is_ok());
        match result {
            Ok(()) => self.engine.observer.on_walk_recorded(&run.key),
            Err(error) => {
                tracing::warn!(lane = %run.key, %error, "could not record how far the backfill got; it is tried again in a while")
            }
        }
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

    /// Acknowledge an item. A lane it leaves quiet with a written position not
    /// yet searchable gets an empty caught-up write first, so the last writes
    /// of a burst don't wait out the refresh interval (or, with automatic
    /// refresh off, for traffic that may not come); the acknowledgement waits
    /// for it, so a drain does too.
    ///
    /// `oldest`: the item was the oldest unfinished one in lane order, so the
    /// sink moved forward (the supervisor's measure of progress), unless the
    /// run has already failed.
    ///
    /// `wants`: whether a caught-up write is wanted, when the caller already
    /// read it from the fences.
    async fn acknowledge(
        &mut self,
        handle: Box<dyn AckHandle>,
        lane: usize,
        seq: u64,
        build: Option<u64>,
        oldest: bool,
        wants: Option<bool>,
    ) -> Result<()> {
        if let Some(key) = self.owed_refresh(lane, wants) {
            let waiting = Refreshing {
                handle: Some(handle),
                lane,
                seq: Some(seq),
                build,
                oldest,
                covers_below: 0,
                again: false,
            };
            return self.start_refresh(vec![(waiting, key)]).await;
        }
        // A caught-up write of the lane already out may not cover what just
        // landed: the lane is checked again when it returns.
        if self.failure.is_none() {
            for (_, refreshing) in &mut self.refreshing {
                if refreshing.lane == lane {
                    refreshing.again = true;
                }
            }
        }
        self.acknowledged(handle, lane, Some(seq), oldest).await
    }

    /// The lane's key, if it is quiet (nothing of it waiting, in flight or on
    /// the lane, the run not failed) with a written position a caught-up write
    /// would make searchable. `wants` short-cuts the fences when known.
    fn owed_refresh(&self, lane: usize, wants: Option<bool>) -> Option<LaneKey> {
        let run = self.lanes.get(lane)?;
        let quiet = self.failure.is_none()
            && !self.window.iter().any(|queued| queued.lane == lane)
            && !self.lane_in_flight(lane)
            && run.consumer.is_empty();
        let wanted = quiet
            && wants.unwrap_or_else(|| {
                self.engine
                    .lock_fences()
                    .wants_a_caught_up_write(&run.key, Instant::now())
            });
        wanted.then(|| run.key.clone())
    }

    /// At the start of a run, one empty caught-up write over every quiet lane
    /// with a written position not yet searchable: the run before may have
    /// ended before sending it (a failure stops them), and nothing else would
    /// until new traffic. A new process has no such position; the sink's own
    /// start covers what an earlier process left.
    async fn owe_opening_refreshes(&mut self) -> Result<()> {
        let owed: Vec<(Refreshing, LaneKey)> = (0..self.lanes.len())
            .filter_map(|lane| {
                let key = self.owed_refresh(lane, None)?;
                Some((
                    Refreshing {
                        handle: None,
                        lane,
                        seq: None,
                        build: None,
                        oldest: false,
                        covers_below: 0,
                        again: false,
                    },
                    key,
                ))
            })
            .collect();
        if owed.is_empty() {
            return Ok(());
        }
        self.start_refresh(owed).await
    }

    /// Start one empty caught-up write covering the lanes of `waiting`, as a
    /// flight, not awaited here: another lane's caught-up write may hold the
    /// sink's one refresh, and only this loop drives it.
    async fn start_refresh(&mut self, waiting: Vec<(Refreshing, LaneKey)>) -> Result<()> {
        let covers_below = self.engine.lock_fences().returned();
        let flight = self.next_flight;
        self.next_flight += 1;
        let mut indexes: Vec<IndexName> = Vec::with_capacity(waiting.len());
        for (mut refreshing, key) in waiting {
            refreshing.covers_below = covers_below;
            if !indexes.contains(&key.index) {
                indexes.push(key.index);
            }
            self.refreshing.push((flight, refreshing));
        }
        let sink = &self.engine.sink;
        let refresh = async move {
            let started = Instant::now();
            let result = sink.write_parts(&[], CaughtUp::Indexes(&indexes)).await;
            Landed {
                flight,
                outcome: Outcome::Refresh(result),
                took: started.elapsed(),
            }
        };
        if let Some(landed) = self.flights.start(refresh.boxed()) {
            Box::pin(self.landed(landed)).await?;
        }
        Ok(())
    }

    /// Acknowledge an item; a landing at the front of its lane is progress.
    async fn acknowledged(
        &mut self,
        handle: Box<dyn AckHandle>,
        lane: usize,
        seq: Option<u64>,
        oldest: bool,
    ) -> Result<()> {
        handle.ack().await?;
        // Acknowledged: the item leaves its lane's F9 account (14.10), the
        // one place it does.
        if self.superseding > 0
            && let Some(seq) = seq
            && let Some(account) = self
                .lanes
                .get_mut(lane)
                .and_then(|run| run.account.as_mut())
        {
            account.remove(&seq);
            self.prune_superseded(lane);
        }
        if oldest && self.failure.is_none() && self.lanes.get(lane).is_some_and(|run| run.counts) {
            self.engine.advanced.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// An empty caught-up write returned: its lanes' written positions before
    /// it are searchable, and the items that waited for it are acknowledged. A
    /// lane on which something landed meanwhile is checked again: a write that
    /// returned after this one started is not covered, and may now be its
    /// lane's last (it would wait forever under `Deferred`). A failed one
    /// fails the run; the items come back.
    async fn refresh_returned(
        &mut self,
        flight: u64,
        result: sink::Result<WriteReport>,
        took: Duration,
    ) -> Result<()> {
        let mut waiting: Vec<Refreshing> = Vec::new();
        let mut at = 0;
        while at < self.refreshing.len() {
            if self.refreshing.get(at).is_some_and(|(id, _)| *id == flight) {
                waiting.push(self.refreshing.swap_remove(at).1);
            } else {
                at += 1;
            }
        }
        let Some(covers_below) = waiting.first().map(|refreshing| refreshing.covers_below) else {
            return Ok(());
        };
        let report = match result {
            Ok(report) => report,
            Err(error) => {
                self.fail(error.into(), &[]);
                return Ok(());
            }
        };
        self.engine.observer.on_stage(
            StageTiming::new(Stage::Write, took)
                .work(Work::Live)
                .sink(&self.engine.name),
        );
        if report.refreshes > 0 {
            self.engine
                .observer
                .on_refreshes(&self.engine.name, report.refreshes);
        }
        let keys: Vec<LaneKey> = waiting
            .iter()
            .filter_map(|refreshing| self.lanes.get(refreshing.lane))
            .map(|run| run.key.clone())
            .collect();
        let returned =
            self.engine
                .write_returned(&[], report.visibility, Some((covers_below, &keys)));
        self.next_due = returned.next_due;
        for refreshing in waiting {
            if refreshing.again
                && let Some(key) = self.owed_refresh(refreshing.lane, None)
            {
                let again = Refreshing {
                    again: false,
                    ..refreshing
                };
                self.start_refresh(vec![(again, key)]).await?;
                continue;
            }
            if let Some(handle) = refreshing.handle {
                self.acknowledged(handle, refreshing.lane, refreshing.seq, refreshing.oldest)
                    .await?;
            }
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
            && self
                .seeding
                .iter()
                .filter(|(_, seeding)| seeding.lane == lane)
                .all(|(_, seeding)| seeding.seq > seq)
            && self
                .refreshing
                .iter()
                .filter(|(_, refreshing)| refreshing.lane == lane)
                .all(|(_, refreshing)| refreshing.seq.is_none_or(|waiting| waiting > seq))
    }
}

impl SinkEngine {
    /// The indexes `report` holds back, by name, with why: missing, else
    /// blocked, else refused under a `stop` policy (with the refused ids, a
    /// few). Their parts stay unacknowledged; a `skip` rejection is not held.
    fn held_indexes(&self, report: &WriteReport) -> BTreeMap<String, Held> {
        let mut held: BTreeMap<String, Held> = BTreeMap::new();
        for doc in &report.rejected {
            if self.failure_policies.resolve(&doc.index) != FailurePolicy::Stop {
                continue;
            }
            match held
                .entry(doc.index.clone())
                .or_insert_with(|| Held::Refused {
                    ids: Vec::new(),
                    reason: doc.reason.clone(),
                }) {
                Held::Refused { ids, .. } if ids.len() < SHOWN_IDS => {
                    ids.push(shown_id(&doc.id).into_owned());
                }
                _ => {}
            }
        }
        for blocked in &report.blocked {
            held.insert(
                blocked.index.to_string(),
                Held::Blocked(blocked.reason.clone()),
            );
        }
        for missing in &report.missing {
            held.insert(
                missing.index.to_string(),
                Held::Missing(missing.reason.clone()),
            );
        }
        held
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
        written: &[(LaneKey, ItemKey)],
        visibility: Visibility,
        caught_up: Option<(u64, &[LaneKey])>,
    ) -> Returned {
        let now = Instant::now();
        let (applied, searchable, returned) = {
            let mut fences = self.lock_fences();
            fences.write_returned(written, visibility, caught_up, now);
            let mut applied: Vec<(LaneKey, ItemKey, Position)> = Vec::new();
            for (lane, _) in written {
                if applied.iter().any(|(seen, _, _)| seen == lane) {
                    continue;
                }
                if let Some((key, position)) = fences.advance_applied(lane) {
                    applied.push((lane.clone(), key, position));
                }
            }
            let searchable = fences.advance_searchable(now);
            let returned = Returned {
                next_due: fences.next_due(),
                wanting: written
                    .iter()
                    .map(|(lane, _)| lane)
                    .filter(|lane| fences.wants_a_caught_up_write(lane, now))
                    .cloned()
                    .collect(),
            };
            (applied, searchable, returned)
        };
        for (lane, key, position) in &applied {
            self.progress.applied(lane, *key);
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
    fn report(&self, searchable: &[(LaneKey, ItemKey, Position)]) {
        for (lane, key, position) in searchable {
            self.progress.searchable(lane, *key);
            self.observer.on_position_searchable(lane, *position);
        }
    }

    /// Whether a stop cut `index`'s backfill short: its `SnapshotComplete`
    /// then seeds nothing, and the index is backfilled on the next start.
    fn cut_short(&self, index: &IndexName) -> bool {
        let cut = self.lock_skipped().contains(index);
        if cut {
            tracing::info!(%index, "stopping: its backfill was cut short, so it stays unseeded and is backfilled on the next start");
        }
        cut
    }

    /// Whether a `SnapshotComplete` of `index` naming `ticket` answers an
    /// older request than the one its seed waits for: its backfill was cut
    /// short by a new target, and seeds nothing.
    fn superseded(&self, index: &IndexName, ticket: Ticket) -> bool {
        let superseded = !self.answers(index, Some(ticket));
        if superseded {
            tracing::info!(%index, sink = %self.name, %ticket, "a backfill cut short by a new target completed; it seeds nothing, the fresh one does");
        }
        superseded
    }

    /// Record that `index`'s seed, answering `ticket`, returned: it is
    /// seeded, and no longer waits for its backfill.
    fn seeded(&self, index: &IndexName, ticket: Ticket, took: Duration) {
        self.observer.on_stage(
            StageTiming::new(Stage::Seed, took)
                .work(Work::Backfill)
                .sink(&self.name)
                .index(index),
        );
        self.observer.on_index_seeded(&self.name, index);
        self.settle(index, ticket);
        tracing::info!(%index, "backfill complete");
    }
}

/// The most refused ids a parked lane names.
const SHOWN_IDS: usize = 5;

/// Why a part of a returned write is held back.
#[derive(Debug, Clone)]
enum Held {
    /// Its index's target is gone.
    Missing(String),
    /// Its index refuses writes for now.
    Blocked(String),
    /// Documents of it were refused, and its index's `on_error` is `stop`.
    Refused { ids: Vec<String>, reason: String },
}

/// What a returned write left in the fences.
struct Returned {
    /// When the oldest position comes due, if that is a matter of time.
    next_due: Option<Instant>,
    /// The written lanes that want an empty caught-up write once quiet.
    wanting: Vec<LaneKey>,
}

/// `batch` without the documents `left_out` names; the batch itself when it
/// holds none of them.
fn without(batch: Batch, left_out: impl Fn(&str) -> bool) -> Batch {
    if !batch
        .envelopes
        .iter()
        .any(|envelope| left_out(&envelope.id))
    {
        return batch;
    }
    let envelopes: Vec<Envelope> = batch
        .envelopes
        .iter()
        .filter(|envelope| !left_out(&envelope.id))
        .cloned()
        .collect();
    Batch {
        envelopes: envelopes.into(),
        ..batch
    }
}

/// The ids `report` refused of `lane`'s index.
fn run_rejected(report: &WriteReport, lane: usize, lanes: &[LaneRun]) -> Vec<String> {
    let Some(run) = lanes.get(lane) else {
        return Vec::new();
    };
    report
        .rejected
        .iter()
        .filter(|doc| doc.index == run.key.index.as_ref())
        .map(|doc| doc.id.clone())
        .collect()
}

/// A parked or restaging lane's due time (`None`), or the next delivery of
/// the substitute lane while a lane `awaits` one: one branch of the run's
/// loop for both, so a sink with every lane open polls what `main` polled.
async fn lane_event(
    wake: Option<Instant>,
    substitutes: &mut Option<Box<dyn Consumer<Substitute>>>,
    awaits: bool,
) -> Option<Option<stream::Result<Option<Delivery<Substitute>>>>> {
    match substitutes {
        Some(consumer) if awaits => tokio::select! {
            () = until(wake) => None,
            delivery = consumer.recv() => Some(Some(delivery)),
        },
        _ => {
            until(wake).await;
            None
        }
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
