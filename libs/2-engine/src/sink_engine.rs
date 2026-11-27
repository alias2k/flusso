//! The sink engine: one per sink. Stage the sink's indexes, request whatever
//! backfill the sink needs, then receive → write → acknowledge, with up to
//! `writes` lane items written at once.
//!
//! Staging ([`SinkEngine::stage`]) is the sink's own seeding decision: ensure
//! every index, retire seeds the source can no longer honor (a `Fresh`
//! continuity), and send one `Backfill` request for the indexes still
//! unseeded. The daemon runs every sink engine's staging before it lets the
//! ingest engine `prepare` the source, which preserves the #120 ordering
//! without a shared engine: rebuilds are staged before the resume point exists,
//! and the resume point exists before any snapshot runs.
//!
//! The indexes are staged concurrently, a few at a time
//! ([`STAGE_CONCURRENCY`]): each index's own steps stay in order, and nothing
//! orders one index against another, since each has its own target on the
//! destination. Staging is all or nothing per pass: once an index fails no
//! further index is started, the ones in flight finish, the error names every
//! failure seen, and the daemon retries the whole stage (each step is
//! idempotent), which starts the rest.
//!
//! **Another process may hold the slot (O29).** Three rules, all about the
//! slot this process may not hold ([`crate::Rereads`]):
//!
//! - **No record is erased, or loosened, without the slot.** This process
//!   destroys or rewrites no record another process may rely on unless it
//!   holds the slot ([`Rereads::may_erase_records`](crate::Rereads::may_erase_records)),
//!   checked immediately before each such request in one place (`slot`,
//!   whose module docs hold the table of every record, marker and
//!   generation write, gated or safe and why). Each waits, never failing
//!   the run: a seed is owed (its documents written, only the marker
//!   waiting) and lands once the slot is held and the lane's records were
//!   read under the stream now held; a clear and an owed rebuild wait for a
//!   turn that may; a reindex waits, its lane restaging; a demotion or a
//!   detach found by a staging with no slot takes the lane as detached,
//!   its decision left to the first read while held; a reattach is refused
//!   with the reason. Plain writes of what the lanes hold behave as on
//!   `main`.
//! - **Another process's change stops the deployment** (the cut of
//!   2026-11-27). Every read of the records is judged against what this
//!   process last read or wrote of them (`known`): anything it did not write
//!   and cannot show unchanged is another process's, and the run ends with
//!   [`crate::EngineError::ForeignChange`],
//!   which the daemon doesn't restart. One deployment never meets it.
//! - **What the records say is read again once the slot is held.** After
//!   every opening, and only while the slot is held, the run reads each
//!   lane's records (`Run::reread`), deciding from them with the same
//!   function staging uses (`decide`, executed by `act_on_records`). A lane
//!   stored open while its records count only for an earlier stream (a
//!   restage, reindex or attach that spanned an opening: `store_lane`)
//!   makes that read due before anything is granted (`grant_once_read`, and
//!   `substituted` for a substitute, written past `grant`). A read counts
//!   for the stream it began under
//!   ([`Rereads::read_begins`](crate::Rereads::read_begins), taken before the
//!   read).
//!
//! The loop ([`SinkEngine::run`], in `writes`) writes each lane item with
//! one [`Sink::write`], several at once but never two holding the same
//! document, decides item-level rejections by the failure policies, and
//! acknowledges: write-then-ack is what makes delivery at-least-once, because
//! an item counts as landed in the engine's [`Progress`] only once its write
//! returned, and an item not acknowledged is delivered again. A `SnapshotComplete`
//! marker records its indexes as seeded once everything before it is written.
//! A reindex arrives as a control message and is staged once the writes in
//! flight returned: `reindex` + `ensure_index` on the sink, then a fresh
//! `Backfill` request.
//!
//! **A backfill cut short by a new target** (issue #428). Each request
//! carries a [`Ticket`], increasing, and every snapshot item names the ticket
//! it answers. The engine keeps, beside each request, the target
//! ([`sink::Ensured::target`]) it was asked for. Every staging of an index
//! whose backfill is outstanding compares, right after `ensure_index` and
//! before any other step that can fail, the target it finds with that one:
//! another target (or a restage, which only runs after a write found the
//! target missing) means the pages that backfill wrote are gone, and the
//! backfill is **cut**. A cut is kept on the request until a fresh one is
//! published, so a staging that fails after finding it loses nothing: from
//! the cut on, no item of the old request marks or seeds the index, and every
//! later staging asks again. Asking again forgets the checkpoint and asks
//! from the first key under a new ticket. The old snapshot's remaining pages
//! still land: each is a build of its row, delivered in lane order with the
//! live changes, so writing it is never a step back, only work the fresh
//! backfill repeats. But they record no walk mark, and the old
//! `SnapshotComplete`, naming an older ticket, seeds nothing: only the fresh
//! backfill's does. A seed the sink refuses because its target changed
//! ([`sink::SinkError::TargetChanged`]) stages the lane again the same way.
//! A seed refused because the old target can't be retired
//! ([`sink::SinkError::RetireBlocked`], #479) parks the lane `blocked` on its
//! `SnapshotComplete` instead: the seed, retried as is, is the lane's probe,
//! on the lane's backoff, and the run goes on.
//! Other lanes of the same snapshot (another sink, another index) answer
//! their own tickets and are untouched.
//!
//! ```text
//! lane     p₁ p₂ │delete│ p₃ p₄ Complete(1) q₁ q₂ q₃ q₄ Complete(2)
//! target   A  A        B  B   (not seeded)  B  B  B  B  seeded
//! ```
//!
//! The loop also reports the sink's two **fence levels**, per lane, in item
//! key order whatever order the writes finish in (`fences`): to the progress
//! by the item's [`ItemKey`](stream::ItemKey), to the observer by its position.
//! *Applied* is the last item before the oldest one still being written,
//! reported before the item's acknowledgement. *Searchable* trails it by what
//! each write reported ([`Visibility`](sink::Visibility)), so a searchable item
//! always means "this and everything before it on its lane". The engine only
//! observes; nothing here asks the sink to refresh because a reader waits.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use kernel::{IndexMapping, IndexName, SinkName};
use sink::{Sink, SinkOptions, TargetId};
use source::cdc::Continuity;
use stream::{LaneKey, Request, Stream, Ticket, WalkMark};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, sleep_until};

use self::fences::Fences;
use self::health::LaneState;
pub use self::health::{LaneHealth, ParkReason, ProbeOutcome};

use crate::error::{EngineError, Result, StagingFailures};
use crate::observer::{EngineId, NoopObserver, Observer, Stage, StageTiming, StartupStep};
use crate::policy::FailurePolicies;
use crate::progress::Progress;
use crate::stop::Stop;

/// How many of a sink's indexes are staged at once.
///
/// Staging is a few round trips per index, so overlapping them is most of the
/// gain. A destination such as OpenSearch applies index creations one
/// cluster-state update at a time, so more in flight only queues on its
/// master; the bound keeps a deployment with hundreds of indexes from
/// flooding it, or the client's connection pool, at start.
pub(crate) const STAGE_CONCURRENCY: usize = 8;

/// Why a reindex staged as the stop came is left owed (X19).
const STAGING_AT_THE_STOP: &str =
    "the reindex was staging as the stop came; the next start picks it up";

/// The longest an idle window stays open before it is reported, so a quiet
/// sink reads as idle in every scrape, not as one spike when work resumes.
const IDLE_TICK: Duration = Duration::from_secs(1);

mod fences;
pub(crate) mod flights;
mod health;
mod known;
mod slot;
mod walked;
mod writes;

/// Which lanes a failed run is blamed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Blame {
    /// The last run ended cleanly: no barrier, every landing is progress.
    Nobody,
    /// A fresh engine, or a failure no one lane caused: every lane has a
    /// barrier, every landing is progress.
    Everyone,
    /// A failure on these lanes: only they have a barrier, and only a landing
    /// on them is progress.
    Lanes(BTreeSet<LaneKey>),
}

impl Blame {
    /// Whether `lane` is blamed (or nothing is, and so everyone counts).
    pub(crate) fn covers(&self, lane: &LaneKey) -> bool {
        match self {
            Blame::Nobody | Blame::Everyone => true,
            Blame::Lanes(lanes) => lanes.contains(lane),
        }
    }
}

/// An operation the daemon hands a running sink engine.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SinkControl {
    /// Rebuild `indexes` into fresh generations: stage them and request a
    /// snapshot. Handled between two batches; nothing restarts.
    Reindex { indexes: Vec<IndexName> },
    /// Detach `index`'s lane from the slot's minimum (ADR 0012 stage 10): at
    /// its next probe the destination refuses, or its next staging, the sink
    /// records the detach and the lane stops holding the slot. Nothing sends
    /// it yet: the thresholds that will come in a later slice (there is no
    /// operator command, O12).
    Detach { index: IndexName },
    /// Attach `index`'s detached lane again (ADR 0012 stage 10, R8): the
    /// engine asks the index's builder, which quiesces, attaches the lane and
    /// publishes `Attached` on it; from there the lane is written as an open
    /// lane. Only tests send it: a rebuild and a reattach attach their lane
    /// themselves.
    Attach { index: IndexName },
    /// Reattach `index`'s detached lane on a sink that can't rebuild it,
    /// accepting the changes dropped while it was detached (O16, `flusso
    /// reattach --accept-gap`): the sink keeps the gap with the index, the
    /// record goes, and the lane is attached. `reply` hears the verdict once
    /// the record is gone, or why not: a sink that can rebuild (a reindex
    /// does that), a lane not detached, records that don't allow it. Dropped
    /// unanswered when the run ends first (a stop): nothing changed.
    Reattach {
        index: IndexName,
        reply: Reply<ReattachVerdict>,
    },
}

/// What a sink engine answers a reattach (O16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReattachVerdict {
    /// The gap is kept and the record gone: the lane is being attached.
    Started,
    /// Not done, and why, naming what works instead: nothing changed.
    Refused(String),
    /// The sink failed a step. Nothing the operator accepted is lost; the
    /// record's delete is read again when it is the step that failed, so a
    /// delete that landed goes on as `Started`. `/status` tells whether the
    /// lane is still detached (ask again) or attaching.
    Failed(String),
}

/// A one-shot answer carried in a control message: cloneable and
/// comparable (by identity), as the messages are. The first `send` wins.
#[derive(Debug, Clone)]
pub struct Reply<T>(Arc<Mutex<Option<tokio::sync::oneshot::Sender<T>>>>);

impl<T> Reply<T> {
    /// A reply and the receiver that hears it.
    pub fn new() -> (Self, tokio::sync::oneshot::Receiver<T>) {
        let (send, receive) = tokio::sync::oneshot::channel();
        (Self(Arc::new(Mutex::new(Some(send)))), receive)
    }

    /// A reply nobody hears.
    pub fn ignored() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }

    /// Answer, once; later answers are dropped.
    pub fn send(&self, answer: T) {
        let sender = self.0.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(sender) = sender {
            let _ = sender.send(answer);
        }
    }
}

impl<T> PartialEq for Reply<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Eq for Reply<T> {}

/// What staging does to one index.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// `ensure_index`, then `is_seeded` when asked.
    Ensure { ask_seeded: bool },
    /// `reindex` and `ensure_index`, then `is_seeded` when asked. `fresh`: a
    /// `Fresh` source's staging, before any slot exists (#120).
    Rebuild { ask_seeded: bool, fresh: bool },
    /// `ensure_index` after a write found the target missing, then
    /// `is_seeded` when the sink is backfilled.
    Restage,
}

/// A seed refused because this process held no slot (`owe_seed`).
#[derive(Debug, Clone, Copy)]
struct OwedSeed {
    ticket: Ticket,
    due: Instant,
    wait: Duration,
}

/// The first wait before an owed seed that failed is tried again.
const OWED_SEED_WAIT: Duration = Duration::from_secs(1);
/// The longest.
const OWED_SEED_WAIT_MAX: Duration = Duration::from_secs(60);

/// A backfill an index was asked for and not yet seeded by.
#[derive(Debug, Clone)]
struct Asked {
    /// The latest request's ticket.
    ticket: Ticket,
    /// The target the request was asked for, if the sink names it.
    target: Option<TargetId>,
    /// Staging found the index in another target since: no item of this
    /// request marks or seeds it, and the next staging asks again.
    cut: bool,
}

/// One sink's engine over its lane.
#[derive(Debug)]
pub struct SinkEngine {
    name: SinkName,
    sink: slot::Plain,
    stream: Arc<dyn Stream>,
    mappings: Vec<IndexMapping>,
    options: SinkOptions,
    observer: Arc<dyn Observer>,
    failure_policies: FailurePolicies,
    skip_backfill: bool,
    /// Indexes with a `Backfill` request published and not seeded yet, each
    /// with its latest request. Survives a restart of this engine's run, so
    /// re-staging doesn't ask for a snapshot the ingest engine is already
    /// serving.
    outstanding: Mutex<BTreeMap<IndexName, Asked>>,
    /// The last ticket a request of this engine carried.
    pub(super) tickets: AtomicU64,
    /// The last attach attempt this engine asked for (ADR 0012 stage 10,
    /// R8): increasing for its life.
    pub(super) attempts: AtomicU64,
    /// The fence levels over the live items received, per lane in key order.
    /// Survives a restart of this engine's run, so a redelivered item keeps
    /// its place and an item written before a failure is still reported once
    /// its time comes.
    fences: Mutex<Fences>,
    /// Where landings and searchable positions are reported, for
    /// confirmation and fences.
    progress: Arc<Progress>,
    stop: Stop,
    /// Indexes whose snapshot batches were acknowledged unapplied during a
    /// stop: their `SnapshotComplete` must not mark them seeded.
    skipped: Mutex<BTreeSet<IndexName>>,
    /// How often the oldest unfinished lane item landed, over this engine's
    /// life, across restarts.
    advanced: AtomicU64,
    /// Lane items written at once.
    writes: usize,
    /// The most documents one write holds when ready items are merged.
    write_documents: usize,
    /// The longest a ready live item waits for the writes out, to go out
    /// merged with what else is ready.
    pub(crate) merge_wait: std::time::Duration,
    /// A reindex taken off the control channel and not yet staged. It
    /// outlives a run, so a run failing with writes in flight doesn't lose a
    /// reindex the daemon already accepted; the next run stages it once its
    /// writes in flight returned. A reindex still pending when a stop ends
    /// the deployment is dropped with it (the daemon refuses new ones once
    /// stopping).
    pending_reindex: Mutex<Option<Vec<IndexName>>>,
    /// Whether `pending_reindex` holds one: the run asks every turn, so
    /// the answer is read without the lock.
    reindex_waiting: AtomicBool,
    /// Which lanes the last run's failure is blamed on: their next run writes
    /// its oldest item alone before anything else (see `writes`). A fresh
    /// engine blames every lane, since a previous process may have stopped
    /// on any.
    blame: Mutex<Blame>,
    /// What the last staging of each index read of its detach records, when
    /// the sink keeps any (ADR 0012 stage 10). Slice 1 only keeps it.
    detachments: Mutex<BTreeMap<IndexName, sink::Detachment>>,
    /// Seeds refused because this process held no slot, per index: the
    /// backfill's documents are written, only `mark_seeded` waits (the sixth
    /// re-check, S-1). Kept whatever the lane does meanwhile; dropped when the
    /// lane detaches or a new backfill of the index is asked for.
    owed_seeds: Mutex<BTreeMap<IndexName, OwedSeed>>,
    /// What this process knows of each index's detach records: every read
    /// is judged against it, and one that shows another process's change
    /// stops the deployment (`known`, the cut of 2026-11-27).
    known: Arc<known::KnownRecords>,
    /// The first change by another process a read found: once set, every
    /// staging and run of this engine ends with it.
    foreign: Mutex<Option<(IndexName, String)>>,
    /// Each lane's health (open, parked, restaging) and its probe backoff.
    /// Outlives a run: a run failing for another reason leaves a parked lane
    /// parked, on its schedule. A lane not in it is open.
    health: Mutex<std::collections::BTreeMap<LaneKey, LaneState>>,
    /// How long a parked lane waits between two probes, and a re-staged
    /// lane between two stagings.
    backoff: crate::Backoff,
    /// Where a lane stands in the source's own text, for a reattach's gap
    /// (S3). `None`: no gap is ever completed.
    positions: Option<Arc<dyn crate::SourcePositions>>,
    /// The slot's openings, and how far this engine read its records since
    /// (O29).
    rereads: Arc<crate::Rereads>,
    /// The slot's openings counted when the last full read of the records
    /// began: one since is read again before anything is written (O29).
    read_at: AtomicU64,
    /// A staging heard a gap end it may not settle (no slot held for it):
    /// the next read while the slot is held settles it (S-a).
    gaps_unsettled: AtomicBool,
    /// A lane opened whose records weren't read under the current stream (a
    /// restage, reindex or attach whose read counted for an earlier one):
    /// the run reads again before any grant (fourth re-check, B-1).
    opened_unread: AtomicBool,
}

impl SinkEngine {
    /// An engine for `sink`, named `name` in the config, over its lane on
    /// `stream`, maintaining `mappings`, reporting what lands to `progress`:
    /// the one the ingest engine over `stream` registers in.
    pub fn new(
        name: SinkName,
        sink: Arc<dyn Sink>,
        stream: Arc<dyn Stream>,
        mappings: Vec<IndexMapping>,
        progress: Arc<Progress>,
    ) -> Self {
        let known = Arc::new(known::KnownRecords::default());
        Self {
            name,
            sink: slot::Plain::new(sink, Arc::clone(&known)),
            known,
            foreign: Mutex::new(None),
            stream,
            mappings,
            options: SinkOptions::default(),
            observer: Arc::new(NoopObserver),
            failure_policies: FailurePolicies::default(),
            skip_backfill: false,
            outstanding: Mutex::new(BTreeMap::new()),
            tickets: AtomicU64::new(0),
            attempts: AtomicU64::new(0),
            detachments: Mutex::new(BTreeMap::new()),
            owed_seeds: Mutex::new(BTreeMap::new()),
            fences: Mutex::new(Fences::default()),
            progress,
            stop: Stop::new(),
            skipped: Mutex::new(BTreeSet::new()),
            advanced: AtomicU64::new(0),
            writes: 1,
            write_documents: crate::BatchPolicy::default().max_changes,
            merge_wait: crate::BatchPolicy::default().max_delay,
            blame: Mutex::new(Blame::Everyone),
            pending_reindex: Mutex::new(None),
            reindex_waiting: AtomicBool::new(false),
            health: Mutex::new(std::collections::BTreeMap::new()),
            backoff: crate::Backoff::default(),
            positions: None,
            rereads: Arc::new(crate::Rereads::default()),
            read_at: AtomicU64::new(0),
            gaps_unsettled: AtomicBool::new(false),
            opened_unread: AtomicBool::new(false),
        }
    }

    /// How long a parked lane waits between two probes of its front item, and
    /// a re-staged lane between two stagings: the first wait, doubling up to
    /// the longest. The daemon sets `[backoff]`; the default is 1 s doubling
    /// to 60 s.
    pub fn with_backoff(mut self, backoff: crate::Backoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Where `lane` is: open, parked, or staged again after its target went
    /// missing.
    pub fn lane_health(&self, lane: &LaneKey) -> LaneHealth {
        self.lock_health()
            .get(lane)
            .map_or(LaneHealth::Open, |state| state.health.clone())
    }

    /// What the last staging of `index` read of its detach records, when the
    /// sink keeps any (ADR 0012 stage 10): `None` for nothing kept.
    pub fn detachment(&self, index: &IndexName) -> Option<sink::Detachment> {
        self.detachments
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(index)
            .cloned()
    }

    /// Keep what a staging read of `index`'s detach records, and say loudly
    /// what this version does with a record it can't act on: stage the index
    /// as before (O24).
    fn read_detachment(&self, index: &IndexName, detachment: sink::Detachment) {
        let sink = &self.name;
        match &detachment.record {
            // Acted on by the staging step (`AtStaging`).
            None | Some(sink::DetachRecord::Detached { .. }) => {}
            Some(sink::DetachRecord::Rebuilding { live_only: true }) => {
                tracing::error!(%index, %sink, "a detach record says a rebuild was under way on a sink that can't rebuild (live only); the index is detached again beside a seed marker on the generation reads see (O30), else staged as before until this version demotes it (R14)");
            }
            Some(sink::DetachRecord::Rebuilding { .. }) => {
                tracing::error!(%index, %sink, "a detach record says a rebuild was under way; this version stages it as before when its seed marker moved to the new generation, else detaches the index again");
            }
            Some(sink::DetachRecord::Unreadable { reason }) => {
                tracing::error!(%index, %sink, %reason, "a detach record this version can't read; the index is staged as before only when its seed marker moved to a newer, unseeded generation (a rebuild), else detached again (O30)");
            }
            Some(other) => {
                tracing::error!(%index, %sink, record = ?other, "a detach record this version doesn't know; the index is staged as before only when its seed marker moved to a newer, unseeded generation, else detached again (O30)");
            }
        }
        if let Some(reason) = &detachment.unreadable_marker {
            tracing::error!(%index, %sink, %reason, "the seed marker carries a detach this version can't read; nothing was put back and the index is staged as before, as its seed marker says");
        }
        let mut detachments = self
            .detachments
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if detachment.is_empty() {
            detachments.remove(index);
        } else {
            detachments.insert(index.clone(), detachment);
        }
    }

    /// Read `mapping`'s detach records and judge them against what this
    /// process knew (`known`): another process's change is no read at all
    /// but the deployment's stop (the cut of 2026-11-27). Every read of the
    /// records goes through this.
    async fn read_records(&self, mapping: &IndexMapping) -> sink::Result<sink::Detachment> {
        let read_at = self.rereads.read_begins();
        let found = self.sink.detachment(mapping).await?;
        // No other process could write while this one held the slot, from
        // the opening the read began under to its return (or with nothing
        // here ever opening a stream: a sink engine on its own, as `main`).
        let same_opening = self.rereads.held_or_alone() && self.rereads.read_begins() == read_at;
        match self
            .known
            .judge(&mapping.index, &found, read_at, same_opening)
        {
            known::Change::Same => Ok(found),
            known::Change::Foreign(what) => Err(self.foreign_change(&mapping.index, what)),
        }
    }

    /// Note that another process changed `index`'s records (`what`): the
    /// first such stops this engine's stagings and runs, which stop the
    /// deployment. The error a step that met it fails with.
    fn foreign_change(&self, index: &IndexName, what: String) -> sink::SinkError {
        tracing::error!(%index, sink = %self.name, %what, "another flusso process changed the index's detach records while this one didn't hold the slot: stopping the deployment; no change of this index confirmed past what its records cover");
        let mut foreign = self.foreign.lock().unwrap_or_else(PoisonError::into_inner);
        if foreign.is_none() {
            *foreign = Some((index.clone(), what.clone()));
        }
        sink::SinkError::Write(format!("another process changed the records: {what}"))
    }

    /// The deployment's stop, once a read found another process's change.
    pub(super) fn foreign_error(&self) -> Option<EngineError> {
        self.foreign
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .or_else(|| self.known.foreign())
            .map(|(index, found)| EngineError::ForeignChange {
                sink: self.name.to_string(),
                index: index.to_string(),
                found,
            })
    }

    /// Staging found `index` detached (ADR 0012 stage 10): its lane is
    /// detached for `why`, its fence frozen first (the observers hear the
    /// health), then out of the slot's minimum (R5). Nothing is ensured or
    /// asked for.
    fn staged_detached(&self, index: &IndexName, why: String, through: Option<String>) -> Staged {
        let lane = LaneKey::new(index.clone(), self.name.clone());
        let mut state = self.lane_state(&lane);
        let changed = state.detach(why.clone(), through, Instant::now());
        if changed {
            tracing::warn!(%lane, %why, "staged detached: nothing of the index is written, and reads of it are frozen at the detach; the other indexes go on");
        }
        self.store_lane(&lane, &state, changed);
        self.progress.detach(&lane);
        self.forget_detached(&lane);
        Staged::Detached
    }

    /// What `act_on_records` answers when what the records ask (a demotion,
    /// a detach) waits for the slot. Only a process's start meets it in
    /// earnest (`stage_inner`); a read again runs only while the slot is
    /// held.
    fn unheld(
        &self,
        lane: &LaneKey,
        found: sink::Detachment,
        why: &str,
        through: Option<String>,
        refused: &slot::NotHeld,
    ) -> Acted {
        tracing::warn!(%lane, %why, %refused, "what the records ask waits");
        self.read_detachment(&lane.index, found);
        Acted::Unheld {
            why: why.to_owned(),
            through,
        }
    }

    /// Act on what `index`'s detach records say (`found`, just read): the
    /// one place staging and the read again decide from them ([`decide`]).
    /// Another process's rebuild is put back to detached (conditional) and
    /// the records read and decided again; one still there after it fails.
    /// `start`: no stream was opened yet (a process's first staging).
    async fn act_on_records(
        &self,
        mapping: &IndexMapping,
        mut found: sink::Detachment,
        start: bool,
    ) -> sink::Result<Acted> {
        let lane = LaneKey::new(mapping.index.clone(), self.name.clone());
        for round in 0..2 {
            let through = found.through.clone();
            let written_rebuildable = matches!(
                found.record,
                Some(sink::DetachRecord::Detached { live_only: false })
            );
            let state = self.lane_state(&lane);
            let decided = decide(
                &AtStaging::of(&found),
                written_rebuildable,
                &state,
                self.options.backfill,
                self.skip_backfill,
                start,
            );
            match decided {
                Decision::Demote { live_only } if round == 0 => {
                    let demoted = match self.erasing().demote(mapping, live_only).await {
                        Ok(demoted) => demoted,
                        Err(refused) => {
                            return Ok(self.unheld(
                                &lane,
                                found,
                                "the records say a rebuild is under way; put back to detached \
                                 once this process holds the slot",
                                through,
                                &refused,
                            ));
                        }
                    };
                    demoted?;
                    tracing::warn!(%lane, skip_backfill = self.skip_backfill, "the records say another process began a rebuild: put back to detached before anything of the lane is dropped (O29)");
                    found = self.read_records(mapping).await?;
                }
                Decision::Demote { .. } => {
                    return Err(sink::SinkError::Write(format!(
                        "{}: the records still say another process rebuilds it after the demotion",
                        mapping.index
                    )));
                }
                Decision::AsBefore => {
                    self.read_detachment(&mapping.index, found.clone());
                    return Ok(Acted::AsBefore { gaps: found.gaps });
                }
                Decision::Forget(version) => {
                    self.erasing().forget_finished(mapping, &version).await?;
                    tracing::info!(%lane, "a detach record outlived the rebuild that finished it; deleted, and the index is staged as before");
                    let gaps = found.gaps.clone();
                    self.read_detachment(&mapping.index, found);
                    return Ok(Acted::AsBefore { gaps });
                }
                Decision::Detach {
                    why,
                    live_only_first,
                } => {
                    if live_only_first {
                        let demoted = match self.erasing().demote(mapping, true).await {
                            Ok(demoted) => demoted,
                            Err(refused) => {
                                return Ok(self.unheld(&lane, found, &why, through, &refused));
                            }
                        };
                        demoted?;
                        tracing::warn!(%lane, "a detach record written while this sink could rebuild is put back to detached, live only: flusso reattach --accept-gap applies");
                    }
                    self.read_detachment(&mapping.index, found);
                    return Ok(Acted::Detached { why, through });
                }
                Decision::KeepDetached => {
                    self.read_detachment(&mapping.index, found);
                    return Ok(Acted::KeepDetached);
                }
                Decision::Ours => {
                    self.read_detachment(&mapping.index, found);
                    return Ok(Acted::Ours);
                }
                Decision::TakeOver(attempts) => {
                    self.read_detachment(&mapping.index, found);
                    return Ok(Acted::TookOver(attempts));
                }
                Decision::DetachAgain => {
                    let why = "the detach records can't be acted on as they stand (detached or \
                               rebuilding beside a seed marker on the generation reads see, or a \
                               record this version can't read); detached again";
                    return self.detach_again(mapping, &lane, found, through, why).await;
                }
            }
        }
        Err(sink::SinkError::Write(format!(
            "{}: the records kept changing while they were acted on",
            mapping.index
        )))
    }

    /// Record `lane` detached again for `why`, with this process's place
    /// (or the record's `through`): waits for the slot like any detach.
    async fn detach_again(
        &self,
        mapping: &IndexMapping,
        lane: &LaneKey,
        found: sink::Detachment,
        through: Option<String>,
        why: &str,
    ) -> sink::Result<Acted> {
        let through = self.place(lane).or(through);
        let detach = sink::Detach::new(why)
            .live_only(!self.options.backfill)
            .through(through.clone());
        let recorded = match self.erasing().detach(mapping, &detach).await {
            Ok(recorded) => recorded,
            Err(refused) => return Ok(self.unheld(lane, found, why, through, &refused)),
        };
        if recorded? != sink::Recorded::Kept {
            return Err(sink::SinkError::Write(format!(
                "{} reads detached, and the sink keeps no record of a detach again; it is not \
                 staged attached over the changes the detach dropped",
                mapping.index
            )));
        }
        self.read_detachment(&mapping.index, found);
        Ok(Acted::Detached {
            why: why.to_owned(),
            through,
        })
    }

    /// The reattach of `mapping`'s index up to its attach (O16, §6a): the
    /// records must say detached on a sink that can't rebuild, with the seed
    /// marker on the generation reads see, seeded. Then the index is
    /// ensured, the gap kept with the marker (its `from` where reads stood
    /// at the detach), and the record deleted: the commit point, after which
    /// no staging detaches the lane again (S2). The records saying otherwise:
    /// `Err` with why, naming what works; nothing changed.
    pub(super) async fn reattach(
        &self,
        mapping: &IndexMapping,
    ) -> sink::Result<std::result::Result<Vec<sink::Gap>, String>> {
        let lane = LaneKey::new(mapping.index.clone(), self.name.clone());
        // Another process's reattach (the record gone) is foreign here: the
        // read stops the deployment (the eighth re-check, B-2).
        let found = self.read_records(mapping).await?;
        // The record forgotten by this process's own tombstone: its earlier
        // try of this reattach, whose forget landed unseen. Its gap is the
        // open one starting where this lane was detached: the reattach goes
        // on to the attach (the review of the cut, B-2).
        if found.record.is_none()
            && self.known.is_mine(found.record_stamp.as_ref())
            && let LaneHealth::Detached { through, .. } = self.lane_health(&lane)
            && found
                .gaps
                .last()
                .is_some_and(|last| last.to.is_none() && last.from == through)
        {
            tracing::warn!(%lane, "the record is forgotten: this process's own earlier reattach forgot it, unseen; the attach goes on");
            return Ok(Ok(found.gaps));
        }
        let restart = "restart flusso: a start on this backfill = false sink puts the \
                       records back to detached, live only, on the generation reads see; \
                       then reattach again";
        let refusal = match (&found.record, found.marker, &found.version) {
            (
                Some(sink::DetachRecord::Detached { live_only: true }),
                Some(sink::MarkerAt::Serving { seeded: true }),
                Some(_),
            ) => None,
            (None, _, _) => {
                Some("the sink keeps no detach record of the index: it is not detached".to_owned())
            }
            (Some(sink::DetachRecord::Detached { live_only: true }), _, _) => Some(format!(
                "the seed marker is not on the seeded generation reads see (a demotion not \
                 finished); {restart}"
            )),
            (Some(sink::DetachRecord::Detached { live_only: false }), _, _) => Some(format!(
                "the record was written while this sink could rebuild; {restart}"
            )),
            (Some(record), _, _) => Some(format!(
                "the record says {record:?}, not detached; {restart}"
            )),
        };
        let version = match (refusal, found.version.clone()) {
            (None, Some(version)) => version,
            (refusal, _) => {
                let why =
                    refusal.unwrap_or_else(|| "the record was read without its version".to_owned());
                tracing::warn!(%lane, %why, "not reattached");
                return Ok(Err(why));
            }
        };
        // Every earlier gap stays as it was; this one is added (review of
        // step 4: the gap list), unless an earlier try of this same reattach
        // left it open already (review of the fixes, S-b): one gap per
        // detach, completed at its attach.
        let from = found.through.clone();
        self.sink.ensure_index(mapping).await?;
        let gaps = self
            .sink
            .record_gaps(mapping, &|kept| {
                let open_here = kept
                    .last()
                    .is_some_and(|last| last.to.is_none() && last.from == from);
                (!open_here).then(|| {
                    let mut gaps = kept.to_vec();
                    gaps.push(sink::Gap {
                        from: from.clone(),
                        to: None,
                    });
                    gaps
                })
            })
            .await?;
        // O29: the delete may erase a record another process relies on once
        // this one stopped holding the slot, checked right before it is sent
        // (`slot`). Refused, the gap just written stays open: the lane stays
        // detached, and a reattach asked again reuses that gap (S-b).
        let deleted = match self.erasing().forget_detachment(mapping, &version).await {
            Ok(deleted) => deleted,
            Err(refused) => return Ok(Err(refused.to_string())),
        };
        if let Err(error) = deleted {
            // The delete may have landed although its call failed (S-b):
            // read again before saying anything.
            match self.read_records(mapping).await {
                Ok(again) if again.record.is_some() => return Err(error),
                Ok(_) => {}
                Err(read) => {
                    tracing::warn!(%lane, %error, %read, "the record's delete failed, and reading it again failed too");
                    return Err(sink::SinkError::Write(format!(
                        "the record's delete failed ({error}) and reading it again failed too \
                         ({read}): whether the reattach took effect is unknown; the lane stays \
                         detached"
                    )));
                }
            }
            tracing::warn!(%lane, %error, "the record's delete failed, yet the record is gone: the reattach goes on");
        }
        tracing::warn!(%lane, ?from, "reattaching: the record is gone; the changes from the detach to the attach are accepted as missing");
        Ok(Ok(gaps))
    }

    /// The reattached `lane`'s gap `attached` (its `to` set at the attach)
    /// kept with its index, by a conditional write. The gap is taken by its
    /// start, open or not (review of the second fix round, S-a'): another
    /// process that believed it held the slot may have settled it at a stale
    /// place first. Its end moves to the attach's when that is later or the
    /// kept end can't be compared; it never moves earlier. A failure fails
    /// the run: the fence heard the gap already, and the next read while the
    /// slot is held settles an open end where the lane stands, which is
    /// where it was attached (nothing of it landed).
    pub(super) async fn record_reattached(
        &self,
        lane: &LaneKey,
        attached: &sink::Gap,
    ) -> Result<()> {
        let Some(mapping) = self.mappings.iter().find(|m| m.index == lane.index) else {
            return Ok(());
        };
        let later = |kept: &Option<String>| match (kept.as_deref(), attached.to.as_deref()) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(kept), Some(to)) => self
                .positions
                .as_ref()
                .and_then(|positions| positions.compare(to, kept))
                .is_none_or(|order| order.is_gt()),
        };
        let gaps = self
            .sink
            .record_gaps(mapping, &|kept| {
                let mut gaps = kept.to_vec();
                let ours = gaps
                    .iter_mut()
                    .rev()
                    .find(|gap| gap.from == attached.from)?;
                if !later(&ours.to) {
                    return None;
                }
                if ours.to.is_some() {
                    tracing::warn!(%lane, kept = ?ours.to, to = ?attached.to, "the gap was settled elsewhere at a place before the attach: its end moves to the attach");
                }
                ours.to = attached.to.clone();
                Some(gaps)
            })
            .await
            .map_err(|error| staging_failed(&lane.index, error))?;
        tracing::warn!(%lane, ?gaps, "reattached: written as usual from here; a fence refuses every position inside an accepted gap");
        Ok(())
    }

    /// The gaps reattaches accepted on `mapping`'s index, found with its
    /// seed marker (S3), heard by the fence. An end that is unknown, or that
    /// the source can't read, is settled only when `settle`: in a read made
    /// while this process's stream holds the slot (review of the fixes,
    /// S-a), never at a staging, which needs no slot; staging leaves it to
    /// that read. Settled, loudly, by a conditional write over the list kept
    /// then:
    ///
    /// - a `to` becomes where the lane stands now: everything after it
    ///   reaches the lane while this process holds the slot, and the lane has
    ///   landed nothing since its attach, so the gap still covers what was
    ///   dropped;
    /// - a `from` becomes unknown: the gap then reaches back from its `to`,
    ///   refusing only positions at or before it, all in the past.
    ///
    /// Until then an unknown `to` refuses every position past `from`.
    pub(super) async fn known_gaps(
        &self,
        mapping: &IndexMapping,
        gaps: Vec<sink::Gap>,
        settle: bool,
    ) -> sink::Result<()> {
        let lane = LaneKey::new(mapping.index.clone(), self.name.clone());
        let readable = |text: &Option<String>| {
            text.as_deref().is_some_and(|text| {
                self.positions
                    .as_ref()
                    .is_some_and(|positions| positions.parses(text))
            })
        };
        let unsettled = |gaps: &[sink::Gap]| {
            gaps.iter()
                .any(|gap| !readable(&gap.to) || (gap.from.is_some() && !readable(&gap.from)))
        };
        let held = settle && self.rereads.holding();
        let place = if held { self.place(&lane) } else { None };
        let Some(place) = place.filter(|_| unsettled(&gaps)) else {
            // Left for the next read while the slot is held; one made then
            // whose source can't place the lane leaves it (no loop).
            if unsettled(&gaps) && !held {
                self.gaps_unsettled.store(true, Ordering::Release);
            }
            self.observer.on_lane_gaps(&lane, &gaps);
            return Ok(());
        };
        let settled = self
            .sink
            .record_gaps(mapping, &|kept| {
                unsettled(kept).then(|| {
                    kept.iter()
                        .map(|gap| {
                            let mut gap = gap.clone();
                            if gap.from.is_some() && !readable(&gap.from) {
                                gap.from = None;
                            }
                            if !readable(&gap.to) {
                                gap.to = Some(place.clone());
                            }
                            gap
                        })
                        .collect()
                })
            })
            .await?;
        // The source answers a lane that routed nothing at or past where its
        // stream started (`Landed::Idle`'s contract): a settled end is never
        // below its gap's start.
        debug_assert!(
            settled.iter().all(|gap| match (&gap.from, &gap.to) {
                (Some(from), Some(to)) =>
                    self.positions
                        .as_ref()
                        .and_then(|positions| positions.compare(from, to))
                        != Some(std::cmp::Ordering::Greater),
                _ => true,
            }),
            "a settled gap ends below its start: {settled:?}"
        );
        tracing::warn!(%lane, at = %place, gaps = ?settled, "a gap's end was unknown or not a position the source reads: settled where the lane stands while this process holds the slot; a fence refuses every position inside each gap");
        self.observer.on_lane_gaps(&lane, &settled);
        Ok(())
    }

    /// Forget the detach record a failed `Sink::detach` of `index` may have
    /// left, on the version read, then read again, which puts the moved
    /// marker back (§5). Only a record this process wrote is forgotten
    /// (its stamp, the review of round 10, B-2); any other is left as it is.
    pub(super) async fn clear_failed_detach(&self, index: &IndexName) -> Result<()> {
        let Some(mapping) = self.mappings.iter().find(|m| &m.index == index) else {
            return Ok(());
        };
        let failed = |error| staging_failed(index, error);
        let found = self.read_records(mapping).await.map_err(failed)?;
        if let (Some(sink::DetachRecord::Detached { .. }), Some(version)) =
            (&found.record, &found.version)
            && self.known.is_mine(found.record_stamp.as_ref())
        {
            // O29: refused while this process holds no slot (`slot`): the
            // record stays, the lane stays parked and tries again.
            self.erasing()
                .forget_detachment(mapping, version)
                .await
                .map_err(|not_held| failed(sink::SinkError::Write(not_held.to_string())))?
                .map_err(failed)?;
            tracing::info!(%index, sink = %self.name, "a failed detach had left its record; forgotten before the lane's probe");
        }
        self.read_records(mapping).await.map_err(failed)?;
        Ok(())
    }

    /// What the engine keeps for a lane it no longer writes (R6): its fence
    /// entries, a reindex pending for its index; an outstanding backfill
    /// stays, cut, so nothing of it seeds the index.
    pub(super) fn forget_detached(&self, lane: &LaneKey) {
        self.lock_fences().forget_lane(lane);
        {
            let mut pending = self.lock_pending_reindex();
            if let Some(indexes) = pending.as_mut() {
                indexes.retain(|index| *index != lane.index);
                if indexes.is_empty() {
                    *pending = None;
                    self.reindex_waiting.store(false, Ordering::Release);
                }
            }
        }
        if let Some(asked) = self.lock_outstanding().get_mut(&lane.index) {
            asked.cut = true;
        }
        self.drop_owed_seed(&lane.index);
    }

    /// Drop `index`'s outstanding backfill when it is cut: what an attach
    /// with no backfill leaves (m5 of the review of step 3).
    pub(super) fn forget_cut(&self, index: &IndexName) {
        let mut outstanding = self.lock_outstanding();
        if outstanding.get(index).is_some_and(|asked| asked.cut) {
            outstanding.remove(index);
        }
    }

    fn lock_health(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::BTreeMap<LaneKey, LaneState>> {
        self.health.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `lane`'s F9 map, if the engine keeps state for the lane (tests).
    #[cfg(test)]
    pub(crate) fn superseded_map(&self, lane: &LaneKey) -> Option<BTreeMap<String, u64>> {
        self.lock_health()
            .get(lane)
            .map(|state| state.superseded.clone())
    }

    /// `lane`'s state as the last run left it; open when never stored.
    pub(super) fn lane_state(&self, lane: &LaneKey) -> LaneState {
        self.lock_health()
            .get(lane)
            .cloned()
            .unwrap_or_else(|| LaneState::new(self.backoff))
    }

    /// Keep `lane`'s state for the runs to come, and tell the observers when
    /// its health `changed`.
    pub(super) fn store_lane(&self, lane: &LaneKey, state: &LaneState, changed: bool) {
        let rebuild = (state.rebuilding, state.attaching);
        let before = {
            let mut health = self.lock_health();
            let before = health
                .get(lane)
                .map_or((None, None), |kept| (kept.rebuilding, kept.attaching));
            if !state.is_kept() {
                health.remove(lane);
            } else {
                health.insert(lane.clone(), state.clone());
            }
            before
        };
        if changed {
            self.observer.on_lane_health(lane, &state.health);
        }
        // The one place a lane opens (O29, B-1): opened while its records
        // count as read only for an earlier stream, it is read again before
        // anything of it is written. Taken as detached on such a read (a
        // restage whose read took its answer before the stream now held), it
        // is read again too: the records may say otherwise now.
        if (state.is_open() || state.health.is_detached()) && !self.rereads.current(lane) {
            self.opened_unread.store(true, Ordering::Release);
        }
        if before != rebuild {
            self.observer.on_lane_rebuild(lane, rebuild.0, rebuild.1);
        }
    }

    /// Stage `index` again after its target went missing: `ensure_index`
    /// (which retracts a seed whose target is gone and creates a fresh one),
    /// then, when it is unseeded, a backfill. One already outstanding is cut
    /// (what it wrote went with the missing target), so a fresh one is asked
    /// for. Runs beside writes of the other indexes, never beside one of
    /// `index`. What staging found: staged, detached, or awaiting the slot.
    pub(super) async fn restage(&self, index: &IndexName) -> Result<Restaged> {
        let Some(mapping) = self.mappings.iter().find(|m| &m.index == index) else {
            return Ok(Restaged::Staged);
        };
        let staged = self
            .step(Step::Restage, mapping)
            .await
            .map_err(|error| staging_failed(index, error))?;
        match staged {
            Staged::Detached => return Ok(Restaged::Detached),
            Staged::Unheld { .. } => return Ok(Restaged::WaitsForSlot),
            _ => {}
        }
        let Staged::Unseeded { walked, target } = staged else {
            return Ok(Restaged::Staged);
        };
        if !self.options.backfill || self.skip_backfill {
            tracing::warn!(%index, sink = %self.name, "staged again with no backfill (backfill off for this sink, or skip_backfill): it holds only what is written from now on");
            return Ok(Restaged::Staged);
        }
        if self.stop.is_requested() {
            tracing::info!(%index, sink = %self.name, "stopping: staged again, its backfill is asked for at the next start");
            return Ok(Restaged::Staged);
        }
        if self.outstanding().contains(index) {
            return self
                .ask_again(vec![(index.clone(), target)])
                .await
                .map(|()| Restaged::Staged);
        }
        self.request_backfill(vec![(index.clone(), walked, target)])
            .await
            .map(|()| Restaged::Staged)
    }

    /// Stage again `index`, which a staging left out (#471): with ensure's
    /// rules, as [`stage`](Self::stage) treats one index. An intact target
    /// keeps its outstanding backfill and its walk; a target that changed
    /// meanwhile has its backfill cut and asked again; an unseeded index not
    /// asked for yet is asked for. What staging found, as for
    /// [`restage`](Self::restage).
    pub(super) async fn ensure_again(&self, index: &IndexName) -> Result<Restaged> {
        let Some(mapping) = self.mappings.iter().find(|m| &m.index == index) else {
            return Ok(Restaged::Staged);
        };
        let backfill = self.options.backfill;
        let staged = self
            .step(
                Step::Ensure {
                    ask_seeded: backfill,
                },
                mapping,
            )
            .await
            .map_err(|error| staging_failed(index, error))?;
        match staged {
            Staged::Detached => return Ok(Restaged::Detached),
            Staged::Unheld { .. } => return Ok(Restaged::WaitsForSlot),
            _ => {}
        }
        let Staged::Unseeded { walked, target } = staged else {
            return Ok(Restaged::Staged);
        };
        if !backfill || self.skip_backfill || self.stop.is_requested() {
            return Ok(Restaged::Staged);
        }
        if self.outstanding().contains(index) {
            if self.is_cut(index) {
                return self
                    .ask_again(vec![(index.clone(), target)])
                    .await
                    .map(|()| Restaged::Staged);
            }
            return Ok(Restaged::Staged);
        }
        if self
            .sink
            .is_rebuilding(index)
            .await
            .map_err(|error| staging_failed(index, error))?
        {
            self.observer.on_index_rebuilding(&self.name, index);
        }
        self.request_backfill(vec![(index.clone(), walked, target)])
            .await
            .map(|()| Restaged::Staged)
    }

    /// `indexes` were cut: staged into a target their outstanding backfill
    /// was not asked for, so it can't seed them. Forget their checkpoints, so
    /// a restart never resumes the old walk, and ask for each again from the
    /// first key, for `target`; the new ticket is the one their seed now
    /// waits for. A failure leaves them cut, for the next staging.
    async fn ask_again(&self, indexes: Vec<(IndexName, Option<TargetId>)>) -> Result<()> {
        for (index, _) in &indexes {
            self.sink
                .forget_walked(index)
                .await
                .map_err(|error| staging_failed(index, error))?;
        }
        self.request_backfill(
            indexes
                .into_iter()
                .map(|(index, target)| (index, None, target))
                .collect(),
        )
        .await
    }

    /// `index` was just staged into `ensured` (`restaged`: after a write
    /// found its target missing). Cut its outstanding backfill when the
    /// target is not the one it was asked for: named otherwise, or, when
    /// either is unnamed, created anew or staged again after going missing.
    /// The cut stays until a fresh request replaces it.
    fn check_target(&self, index: &IndexName, ensured: &sink::Ensured, restaged: bool) {
        let mut outstanding = self.lock_outstanding();
        let Some(asked) = outstanding.get_mut(index) else {
            return;
        };
        let changed = restaged
            || match (&asked.target, &ensured.target) {
                (Some(asked), Some(now)) => asked != now,
                _ => ensured.created,
            };
        if changed && !asked.cut {
            asked.cut = true;
            tracing::warn!(%index, sink = %self.name, ticket = %asked.ticket, "staged into a new target while its backfill was running: the rows that backfill wrote are not there, so it can't seed this one; asking for a fresh backfill");
        }
    }

    /// The deployment's stop: once requested, a snapshot batch still on the
    /// lane is acknowledged without being applied, and its index left
    /// unseeded. The backfill was abandoned with the stop and runs again on
    /// the next start, so writing it now would only hold the drain up.
    pub fn with_stop(mut self, stop: Stop) -> Self {
        self.stop = stop;
        self
    }

    /// The universal sink keys (`backfill`).
    pub fn with_options(mut self, options: SinkOptions) -> Self {
        self.options = options;
        self
    }

    /// How many lane items this engine writes at once: at least one, which
    /// is the default. The daemon sets `[concurrency] writes`.
    pub fn with_writes(mut self, writes: usize) -> Self {
        self.writes = writes.max(1);
        self
    }

    /// The most documents one write holds: items ready at once (across lanes,
    /// and several of one lane) are merged into one write up to it, never
    /// splitting an item, and below the sink's own
    /// [`write_limit`](sink::Sink::write_limit). The default is
    /// [`BatchPolicy`](crate::BatchPolicy)'s `max_changes`; the daemon sets
    /// `[batch] max_changes`, so a write holds about one batch, as with one
    /// lane per sink.
    pub fn with_write_documents(mut self, documents: usize) -> Self {
        self.write_documents = documents.max(1);
        self
    }

    /// The longest a live item ready while a write is out waits for the writes
    /// out to return, so it goes out in one write with what else is ready
    /// (fewer, larger requests, as one lane per sink wrote one batch over
    /// every index). Bounded from when the item became ready; a write out
    /// longer than this is not waited for, so a stalled or slow lane holds no
    /// other index's writes past it. Never for backfill, an ordered sink, a
    /// sink with one lane, or without merging. The default is
    /// [`BatchPolicy`](crate::BatchPolicy)'s `max_delay`, which the daemon
    /// sets from `[batch] max_delay_ms`; zero turns the wait off.
    pub fn with_merge_wait(mut self, wait: std::time::Duration) -> Self {
        self.merge_wait = wait;
        self
    }

    /// Name a lane's place in the source's own text through `positions`: the
    /// two ends of a reattach's gap (ADR 0012 stage 10, S3). The daemon
    /// hands it the source.
    pub fn with_positions(mut self, positions: Arc<dyn crate::SourcePositions>) -> Self {
        self.positions = Some(positions);
        self
    }

    /// Read every index's detach records again each time the live stream
    /// opens, before writing anything after it (ADR 0012 stage 10, O29): the
    /// `rereads` the ingest engine and the fence share.
    pub fn with_rereads(mut self, rereads: Arc<crate::Rereads>) -> Self {
        self.rereads = rereads;
        self
    }

    /// Where `lane` stands in the source's own text: what it applied, as the
    /// source answers it. `None` while the source can't say.
    pub(super) fn place(&self, lane: &LaneKey) -> Option<String> {
        let landed = self.progress.lane(lane, crate::Level::Applied)?;
        self.positions.as_ref()?.place(landed)
    }

    /// Report lifecycle and progress to `observer`.
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    /// How item-level rejections are decided, per index.
    pub fn with_failure_policies(mut self, policies: FailurePolicies) -> Self {
        self.failure_policies = policies;
        self
    }

    /// Never request a backfill (`--skip-backfill`): unseeded indexes stay so.
    pub fn skip_backfill(mut self, skip: bool) -> Self {
        self.skip_backfill = skip;
        self
    }

    /// How often this engine's oldest unfinished lane item landed since it
    /// was built, across restarts of its run: a supervisor reads it to tell a
    /// run that made progress from one that failed where the last one did.
    /// A later item landing beside an older one that then fails is not
    /// progress: the sink is still stuck at the older one.
    pub fn advanced(&self) -> u64 {
        self.advanced.load(Ordering::Relaxed)
    }

    /// The sink this engine drives.
    pub fn name(&self) -> &SinkName {
        &self.name
    }

    /// Ensure every index, retire stale seeds under a `Fresh` source, and
    /// request a backfill for what is still unseeded. Idempotent; the daemon
    /// runs it before the ingest engine prepares the source, and again before
    /// each restart of this engine (with `Resumed`).
    ///
    /// The indexes are staged concurrently, at most eight at a time, each
    /// index's own steps in order. Under `Fresh` no further index is started
    /// after a failure and the error names every failure seen; under
    /// `Resumed` an index that can't be ensured is left restaging and the rest
    /// go on (#471, `ensure_per_index`).
    #[tracing::instrument(name = "sink.stage", skip_all, fields(sink = %self.name, ?continuity))]
    pub async fn stage(&self, continuity: Continuity) -> Result<()> {
        // A staging reads each index's records in its step, which counts that
        // lane's read (O29, B1''').
        let read_at = self.rereads.read_begins();
        let staged = self.stage_inner(continuity).await;
        // Another process's change found in a step stops the deployment,
        // whatever the staging did with the step's failure.
        if let Some(foreign) = self.foreign_error() {
            return Err(foreign);
        }
        staged?;
        self.records_read(read_at);
        Ok(())
    }

    /// `lane`'s records were read with the slot held, or decided from
    /// anyway: the lane is no longer undecided.
    fn decided(&self, lane: &LaneKey) {
        let mut state = self.lane_state(lane);
        if state.undecided {
            state.undecided = false;
            self.store_lane(lane, &state, false);
        }
    }

    /// A staging or a read again began at `read_at` and is done: the run
    /// needn't read again until the next opening. Each lane read counted
    /// itself (`Rereads::read`); one left out (restaging) is read by its own
    /// restage path.
    pub(super) fn records_read(&self, read_at: u64) {
        self.read_at.fetch_max(read_at, Ordering::AcqRel);
    }

    /// Whether the slot opened since the last full read of the records.
    /// Only while it is held: with no slot the read decides nothing, and is
    /// due again once an opening returns (the cut of 2026-11-27).
    pub(super) fn reread_due(&self) -> bool {
        (self.rereads.may_erase_records()
            && (self.rereads.is_due(self.read_at.load(Ordering::Acquire))
                || self.opened_unread.load(Ordering::Acquire)))
            || (self.gaps_unsettled.load(Ordering::Acquire) && self.rereads.holding())
    }

    /// `index`'s seed, answering `ticket`, was refused because this process
    /// held no slot: owed, landed once the slot is held and the lane's
    /// records were read under that stream (O29, the sixth re-check).
    pub(super) fn owe_seed(&self, index: &IndexName, ticket: Ticket) {
        tracing::warn!(%index, sink = %self.name, "its backfill completed while this process holds no slot: the seed waits (it writes the marker whole, erasing whatever record another process wrote since), and lands once the slot is held and the records are read again");
        self.lock_owed_seeds().insert(
            index.clone(),
            OwedSeed {
                ticket,
                due: Instant::now(),
                wait: OWED_SEED_WAIT,
            },
        );
    }

    /// The seeds owed, with when each may be tried.
    /// Whether `index`'s seed is owed.
    pub(super) fn owes_seed(&self, index: &IndexName) -> bool {
        self.lock_owed_seeds().contains_key(index)
    }

    pub(super) fn owed_seeds(&self) -> Vec<(IndexName, Ticket, Instant)> {
        self.lock_owed_seeds()
            .iter()
            .map(|(index, owed)| (index.clone(), owed.ticket, owed.due))
            .collect()
    }

    /// `index`'s seed is no longer owed: it landed, its lane detached, or a
    /// new backfill decides it.
    pub(super) fn drop_owed_seed(&self, index: &IndexName) {
        self.lock_owed_seeds().remove(index);
    }

    /// `index`'s owed seed failed for now: tried again after a wait that
    /// doubles, to a minute.
    pub(super) fn owed_seed_later(&self, index: &IndexName) {
        if let Some(owed) = self.lock_owed_seeds().get_mut(index) {
            owed.due = Instant::now() + owed.wait;
            owed.wait = (owed.wait * 2).min(OWED_SEED_WAIT_MAX);
        }
    }

    fn lock_owed_seeds(&self) -> std::sync::MutexGuard<'_, BTreeMap<IndexName, OwedSeed>> {
        self.owed_seeds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `index`'s records may keep a gap (`known`).
    pub(super) fn gaps_kept(&self, index: &IndexName) -> bool {
        self.known.keeps_gaps(index)
    }

    /// The sink's requests that may erase or loosen a record, each checked
    /// against the slot right before it is sent (`slot`).
    fn erasing(&self) -> slot::Erasing<'_> {
        self.sink.erasing(&self.rereads, &self.mappings)
    }

    /// `gaps` were just kept with an end still unknown or unreadable (a
    /// corrupt entry beside a reattach's): the next read while the slot is
    /// held settles it (review of the second fix round, m-f).
    pub(super) fn flag_unsettled(&self, gaps: &[sink::Gap]) {
        let readable = |text: &Option<String>| {
            text.as_deref().is_some_and(|text| {
                self.positions
                    .as_ref()
                    .is_some_and(|positions| positions.parses(text))
            })
        };
        if gaps
            .iter()
            .any(|gap| !readable(&gap.to) || (gap.from.is_some() && !readable(&gap.from)))
        {
            self.gaps_unsettled.store(true, Ordering::Release);
        }
    }

    /// A read while the slot is held is under way: it settles every gap
    /// end it can.
    pub(super) fn settling_gaps(&self) {
        self.gaps_unsettled.store(false, Ordering::Release);
        self.opened_unread.store(false, Ordering::Release);
    }

    async fn stage_inner(&self, continuity: Continuity) -> Result<()> {
        let started = Instant::now();
        if self.stop.is_requested() {
            self.stage_while_stopping(started).await;
            return Ok(());
        }
        let backfill = self.options.backfill;
        let step = Step::Ensure {
            ask_seeded: backfill,
        };
        // A `Fresh` source needs every seeded index's rebuild staged before it
        // is prepared (#120), and an index not ensured can't be told seeded:
        // that staging waits for every index (#471, R4).
        let (mut seeded, unensured) = if continuity == Continuity::Fresh {
            (
                self.each_index(self.mappings.iter(), step).await?,
                BTreeSet::new(),
            )
        } else {
            self.ensure_per_index(step).await?
        };
        self.observer.on_indexes_ensured(&self.name, seeded.len());
        // A stop that came while ensuring makes this staging ensure only too
        // (X21): nothing asked for, no restaging lane reopened.
        if self.stop.is_requested() {
            self.staged(started);
            return Ok(());
        }

        if !backfill {
            tracing::info!("backfill disabled for this sink; live changes only");
            self.reopen_restaged(&unensured);
            self.staged(started);
            return Ok(());
        }

        if continuity == Continuity::Fresh {
            self.rebuild_stale_seeds(&mut seeded).await?;
            self.forget_walks(&mut seeded).await?;
        }

        // An index already asked for is not asked again (a restart of this
        // engine), unless its backfill was cut: then what it wrote is gone.
        let already_requested = self.outstanding();
        let mut unseeded: Vec<(IndexName, Option<WalkMark>, Option<TargetId>)> = Vec::new();
        let mut cut: Vec<(IndexName, Option<TargetId>)> = Vec::new();
        for (mapping, staged) in seeded {
            let index = mapping.index.clone();
            match staged {
                Staged::Seeded | Staged::Detached | Staged::Unheld { .. } => {}
                Staged::Unseeded { target, .. } if already_requested.contains(&index) => {
                    if self.is_cut(&index) {
                        cut.push((index, target));
                    }
                }
                Staged::Unseeded { walked, target } => unseeded.push((index, walked, target)),
            }
        }
        if !cut.is_empty() {
            self.ask_again(cut).await?;
        }
        if self.skip_backfill {
            if !unseeded.is_empty() {
                tracing::warn!(
                    indexes = ?unseeded.iter().map(|(i, _, _)| i.as_ref()).collect::<Vec<_>>(),
                    "skipping backfill (skip_backfill set); these indexes stay unseeded"
                );
            }
        } else if !unseeded.is_empty() {
            // A start in the middle of a rebuild: reads still see the old copy,
            // so a fence must vouch for nothing of it (#379).
            for (index, _, _) in &unseeded {
                if self.sink.is_rebuilding(index).await? {
                    self.observer.on_index_rebuilding(&self.name, index);
                }
            }
            self.request_backfill(unseeded).await?;
        } else {
            tracing::info!("every index is seeded; no backfill needed");
        }
        self.reopen_restaged(&unensured);
        self.staged(started);
        Ok(())
    }

    /// Staging for a run restarted during the stop (X21): every index is
    /// ensured and nothing more. No backfill is asked for and no restaging
    /// lane reopens; an index that can't be ensured, or wasn't tried after
    /// another failed, is left restaging with why, for the stop to release
    /// like any other, rather than failing the staging and holding the stop
    /// until its deadline.
    async fn stage_while_stopping(&self, started: Instant) {
        tracing::info!(sink = %self.name, "staging during the stop: the indexes are ensured only");
        let tried = self
            .try_each_index(
                self.mappings.iter(),
                Step::Ensure { ask_seeded: false },
                Spread::StopAtFirstFailure,
            )
            .await;
        self.observer
            .on_indexes_ensured(&self.name, tried.done.len());
        self.leave_restaging(tried.failed, tried.not_started, true);
        self.staged(started);
    }

    /// Ensure every index on its own (#471, R1): an index that can't be
    /// ensured, or wasn't tried after another failed, is left restaging with
    /// why, for its lane's restage path to stage again, and the rest go on.
    /// When none could be ensured the destination is down, not an index: the
    /// staging fails as a whole, for the daemon to back off and retry. What
    /// each ensured index staged, and the indexes left restaging.
    async fn ensure_per_index(
        &self,
        step: Step,
    ) -> Result<(Vec<(&IndexMapping, Staged)>, BTreeSet<IndexName>)> {
        let tried = self
            .try_each_index(self.mappings.iter(), step, Spread::OnceAnswered)
            .await;
        if tried.done.is_empty() && !tried.failed.is_empty() {
            return Err(EngineError::Staging(StagingFailures(tried.failed)));
        }
        let stopping = self.stop.is_requested();
        let unensured = self.leave_restaging(tried.failed, tried.not_started, stopping);
        Ok((tried.done, unensured))
    }

    /// Leave each index of `failed` and `not_started` restaging, with why,
    /// marked to be staged again with ensure's rules: the lanes the staging
    /// couldn't ensure. `stopping`: a staging during the stop, whose lanes the
    /// stop releases.
    fn leave_restaging(
        &self,
        failed: Vec<(IndexName, sink::SinkError)>,
        not_started: Vec<IndexName>,
        stopping: bool,
    ) -> BTreeSet<IndexName> {
        let when = if stopping { " during the stop" } else { "" };
        let unstaged = failed
            .into_iter()
            .map(|(index, error)| (index, format!("could not be ensured{when}: {error}")))
            .chain(not_started.into_iter().map(|index| {
                let why = if stopping {
                    "not ensured during the stop: not tried".to_owned()
                } else {
                    format!(
                        "not tried: {STAGE_CONCURRENCY} indexes in a row could not be ensured before it"
                    )
                };
                (index, why)
            }));
        let now = Instant::now();
        let mut left = BTreeSet::new();
        for (index, reason) in unstaged {
            let lane = LaneKey::new(index.clone(), self.name.clone());
            let mut state = self.lane_state(&lane);
            if !state.restage(reason.clone(), now) {
                state.left_out_again(reason.clone());
            } else {
                state.unensured = true;
                if stopping {
                    tracing::warn!(%lane, %reason, "not ensured during the stop; its items are released unapplied");
                } else {
                    tracing::warn!(%lane, %reason, "the index was not ensured; its lane is staged again on its own while the sink's other indexes go on");
                }
            }
            self.store_lane(&lane, &state, true);
            left.insert(index);
        }
        left
    }

    /// Every index but `unensured` was just ensured: a lane waiting to be
    /// staged again after its target went missing is staged. A lane owed a
    /// reindex is not: its retry stages the reindex (X17). Nor is one whose
    /// index wasn't ensured: its restage path stages it (#471, R3).
    fn reopen_restaged(&self, unensured: &BTreeSet<IndexName>) {
        let restaged: Vec<LaneKey> = self
            .lock_health()
            .iter()
            .filter(|(lane, state)| {
                matches!(state.health, LaneHealth::Restaging { .. })
                    && state.reindex.is_none()
                    && !unensured.contains(&lane.index)
            })
            .map(|(lane, _)| lane.clone())
            .collect();
        for lane in restaged {
            let mut state = self.lane_state(&lane);
            state.reopen(Instant::now());
            tracing::info!(%lane, "staged again with the sink; the lane is open");
            self.store_lane(&lane, &state, true);
        }
    }

    fn staged(&self, started: Instant) {
        self.observer
            .on_startup_step(StartupStep::Stage, Some(&self.name), started.elapsed());
        self.observer.on_sink_started(&self.name);
    }

    /// The source has no resume point, so every seed this sink recorded is
    /// stale: the changes between that seed and now are unobservable. Each
    /// index still reported as seeded gets a from-scratch rebuild staged (a
    /// fresh target, so rows gone from the source are dropped on the swap) and
    /// re-announced, so the backfill that follows reseeds it while the old copy
    /// keeps serving. Indexes already unseeded are left alone: they are about
    /// to be rebuilt anyway, and staging again would orphan the target already
    /// in flight. Under `skip_backfill` nothing is staged: a staged target
    /// nobody backfills would take the live writes while the old copy keeps
    /// serving reads.
    ///
    /// `seeded` is each index with whether it is seeded; a rebuilt index's
    /// answer is asked again.
    async fn rebuild_stale_seeds(&self, seeded: &mut [(&IndexMapping, Staged)]) -> Result<()> {
        let stale: Vec<&IndexMapping> = seeded
            .iter()
            .filter(|(_, staged)| *staged == Staged::Seeded)
            .map(|(mapping, _)| *mapping)
            .collect();
        if stale.is_empty() {
            return Ok(());
        }
        let names: Vec<&str> = stale.iter().map(|m| m.index.as_ref()).collect();
        tracing::warn!(
            indexes = ?names,
            skip_backfill = self.skip_backfill,
            "the source has no resume point, so the changes since these indexes were seeded are \
             lost; rebuilding them from scratch (with skip_backfill they are served as-is)",
        );
        if self.skip_backfill {
            return Ok(());
        }
        let rebuilt = self
            .each_index(
                stale,
                Step::Rebuild {
                    ask_seeded: true,
                    fresh: true,
                },
            )
            .await?;
        for (mapping, now) in rebuilt {
            if let Some((_, seeded)) = seeded.iter_mut().find(|(m, _)| m.index == mapping.index) {
                *seeded = now;
            }
        }
        Ok(())
    }

    /// The source has no resume point, so where an unseeded index's last
    /// backfill got means nothing any more: the changes since are lost. Forget
    /// it before the source prepares (the daemon's order), so that a crash
    /// before the new walk records anything never resumes the old one.
    async fn forget_walks(&self, staged: &mut [(&IndexMapping, Staged)]) -> Result<()> {
        for (mapping, state) in staged.iter_mut() {
            if let Staged::Unseeded {
                walked: walked @ Some(_),
                ..
            } = state
            {
                tracing::warn!(index = %mapping.index, "the source has no resume point, so this index's unfinished backfill starts again from the first key");
                self.sink
                    .forget_walked(&mapping.index)
                    .await
                    .map_err(|error| staging_failed(&mapping.index, error))?;
                *walked = None;
            }
        }
        Ok(())
    }

    /// Run `step` for each of `mappings`, at most [`STAGE_CONCURRENCY`] at
    /// once, and return each mapping staged with what staging learned of it
    /// (unseeded with no checkpoint when not asked), in the order given. Every
    /// mapping, unless a stop asked meanwhile left some not started: those
    /// are missing from the answer.
    ///
    /// After a failure no further index is started: a destination that is
    /// down or never answers fails every index alike, and each wave would cost
    /// one more request timeout. The ones in flight run to their end, and the
    /// error names every failure seen; the daemon retries the whole stage,
    /// which starts the rest.
    async fn each_index<'a>(
        &self,
        mappings: impl IntoIterator<Item = &'a IndexMapping>,
        step: Step,
    ) -> Result<Vec<(&'a IndexMapping, Staged)>> {
        let tried = self
            .try_each_index(mappings, step, Spread::StopAtFirstFailure)
            .await;
        if tried.failed.is_empty() {
            Ok(tried.done)
        } else {
            Err(EngineError::Staging(StagingFailures(tried.failed)))
        }
    }

    /// [`each_index`](Self::each_index), every outcome kept: what staged,
    /// what failed, and what was not started after a failure.
    async fn try_each_index<'a>(
        &self,
        mappings: impl IntoIterator<Item = &'a IndexMapping>,
        step: Step,
        spread: Spread,
    ) -> Tried<'a> {
        let mut waiting = mappings.into_iter().enumerate();
        let mut running = FuturesUnordered::new();
        let mut outcomes = Vec::new();
        let mut failed_once = false;
        let mut answered = false;
        let mut probes = 0;
        let mut in_a_row = 0;
        // A staging that began under the stop ensures every index (X21); one
        // under way starts no index once the stop is asked: what is left is
        // the stop's to release.
        let began_stopping = self.stop.is_requested();
        loop {
            loop {
                if !began_stopping && self.stop.is_requested() {
                    break;
                }
                // A wave's worth of failures in a row, even after a success:
                // the destination stopped answering (#471, second review).
                let open = (!failed_once || (spread == Spread::OnceAnswered && answered))
                    && in_a_row < STAGE_CONCURRENCY;
                let room = if open {
                    running.len() < STAGE_CONCURRENCY
                } else if answered {
                    false
                } else {
                    // Nothing ensured yet: one more index at a time, up to one
                    // more wave's worth, before reading the destination as down.
                    spread == Spread::OnceAnswered
                        && running.is_empty()
                        && probes < STAGE_CONCURRENCY
                };
                if !room {
                    break;
                }
                let Some((order, mapping)) = waiting.next() else {
                    break;
                };
                if !open {
                    probes += 1;
                }
                running.push(async move { (order, mapping, self.step(step, mapping).await) });
            }
            let Some(outcome) = running.next().await else {
                break;
            };
            if outcome.2.is_ok() {
                answered = true;
                in_a_row = 0;
            } else {
                failed_once = true;
                in_a_row += 1;
            }
            outcomes.push(outcome);
        }
        outcomes.sort_unstable_by_key(|(order, _, _)| *order);
        let mut tried = Tried {
            done: Vec::with_capacity(outcomes.len()),
            failed: Vec::new(),
            not_started: waiting.map(|(_, mapping)| mapping.index.clone()).collect(),
        };
        for (_, mapping, outcome) in outcomes {
            match outcome {
                Ok(seeded) => tried.done.push((mapping, seeded)),
                Err(error) => tried.failed.push((mapping.index.clone(), error)),
            }
        }
        tried
    }

    /// One index's staging steps, in order. Whether an outstanding backfill
    /// is cut is decided right after `ensure_index`, before any step that can
    /// fail.
    async fn step(&self, step: Step, mapping: &IndexMapping) -> sink::Result<Staged> {
        // O29, B1''': the step reads the index's records first; only a step
        // that got through counts as its lane's read, for when it began.
        let read_at = self.rereads.read_begins();
        let staged = self.step_inner(step, mapping).await?;
        if matches!(staged, Staged::Unheld { .. }) {
            return Ok(staged);
        }
        self.rereads.read(
            &LaneKey::new(mapping.index.clone(), self.name.clone()),
            read_at,
        );
        Ok(staged)
    }

    async fn step_inner(&self, step: Step, mapping: &IndexMapping) -> sink::Result<Staged> {
        // Before anything else touches the index (ADR 0012 stage 10): a
        // failed read fails the step, never reads as "no record".
        let detachment = self.read_records(mapping).await?;
        let lane = LaneKey::new(mapping.index.clone(), self.name.clone());
        let start = self.rereads.opened() == 0;
        match self.act_on_records(mapping, detachment, start).await? {
            // Told even when empty: a generation created since an earlier
            // read dropped the gaps that read heard.
            Acted::AsBefore { gaps } => self.known_gaps(mapping, gaps, false).await?,
            Acted::Detached { why, through } => {
                return Ok(self.staged_detached(&mapping.index, why, through));
            }
            Acted::KeepDetached => return Ok(Staged::Detached),
            // A staging with no slot held (a process's start, or a sink run
            // restarted while the stream is down): the records say the index
            // is detached or rebuilding, and what they ask waits for the
            // slot. Taken as detached, what reaches the lane dropped under
            // that record, and undecided: the first read while the slot is
            // held decides again from the same records (a change meanwhile
            // stops the deployment). A restage and a reindex wait instead.
            Acted::Unheld { why, through } => {
                if matches!(step, Step::Ensure { .. }) {
                    let _ = self.staged_detached(&mapping.index, why.clone(), through.clone());
                    let mut state = self.lane_state(&lane);
                    state.undecided = true;
                    self.store_lane(&lane, &state, false);
                }
                return Ok(Staged::Unheld { why, through });
            }
            Acted::Ours if self.lane_health(&lane).is_detached() => return Ok(Staged::Detached),
            Acted::Ours => {}
            Acted::TookOver(attempts) => {
                // O2: reads see the old generation until the seed; the fence
                // hears it before the backfill is asked (review of step 4,
                // m1).
                self.observer
                    .on_index_rebuilding(&self.name, &mapping.index);
                let mut state = self.lane_state(&lane);
                state.rebuilding = Some(attempts);
                tracing::info!(%lane, attempts, "a rebuild under way: the lane is attached and its walk resumes");
                self.store_lane(&lane, &state, false);
            }
        }
        if !matches!(step, Step::Restage) {
            self.decided(&lane);
        }
        let (ask_seeded, restaged) = match step {
            Step::Ensure { ask_seeded } => (ask_seeded, false),
            Step::Restage => (self.options.backfill, true),
            Step::Rebuild { ask_seeded, fresh } => {
                // Before the sink moves its writes: a fence must stop vouching
                // for this index first (#379).
                self.observer
                    .on_index_rebuilding(&self.name, &mapping.index);
                if fresh {
                    self.erasing().reindex_at_a_fresh_start(mapping).await?;
                } else {
                    // O29: owed while this process holds no slot (`slot`),
                    // retried on the restage backoff.
                    self.erasing().reindex(mapping).await.map_err(|_| {
                        sink::SinkError::Write(
                            "the reindex waits until this process holds the slot (its stream \
                             ended or its opening failed)"
                                .to_owned(),
                        )
                    })??;
                }
                (ask_seeded, false)
            }
        };
        let ensured = self.sink.ensure_index(mapping).await?;
        self.check_target(&mapping.index, &ensured, restaged);
        let target = ensured.target;
        if !ask_seeded {
            return Ok(Staged::Unseeded {
                walked: None,
                target,
            });
        }
        Ok(match self.sink.seeding(&mapping.index).await? {
            // Seeded on another target than this process's backfill asked
            // for: another process rebuilt and seeded the index meanwhile
            // (the eighth re-check, B-4a). Its own seed settles the backfill
            // first, and a target that went missing is ensured unseeded.
            sink::Seeding::Seeded
                if self
                    .lock_outstanding()
                    .get(&mapping.index)
                    .is_some_and(|asked| {
                        asked.target.is_some() && target.is_some() && asked.target != target
                    }) =>
            {
                return Err(self.foreign_change(
                    &mapping.index,
                    format!(
                        "{}: seeded on {target:?}, not the target this process's backfill was \
                         asked for",
                        mapping.index
                    ),
                ));
            }
            sink::Seeding::Seeded => Staged::Seeded,
            sink::Seeding::Unseeded { walked } => Staged::Unseeded {
                walked: walked.map(WalkMark::new),
                target,
            },
            _ => Staged::Unseeded {
                walked: None,
                target,
            },
        })
    }

    /// Ask for a backfill of each index, after where its last one durably
    /// got, if anywhere, for the target staging found, each under a new
    /// ticket that its seed then waits for. An index counts as asked for
    /// once its request is published, so a failure part way leaves the rest
    /// to be asked for again.
    async fn request_backfill(
        &self,
        requests: Vec<(IndexName, Option<WalkMark>, Option<TargetId>)>,
    ) -> Result<()> {
        tracing::info!(indexes = requests.len(), "requesting backfill");
        let producer = self.stream.requests()?.producer;
        let mut indexes = Vec::with_capacity(requests.len());
        for (index, mut after, target) in requests {
            // A walk resumes from its checkpoint only where the records keep
            // no gap: a drop kept as a gap may postdate the pages before the
            // mark, and the seed drops the gaps. It walks from the first key
            // then (found by the world at 20,000 cases, the seventh
            // re-check).
            if after.is_some() && self.gaps_kept(&index) {
                tracing::warn!(%index, sink = %self.name, "the records keep a gap over the index, which the pages before the walk's checkpoint may predate: walking from the first key");
                self.sink
                    .forget_walked(&index)
                    .await
                    .map_err(|error| staging_failed(&index, error))?;
                after = None;
            }
            if after.is_some() {
                tracing::info!(%index, sink = %self.name, "requesting a resumed backfill: its last walk recorded how far it got");
            }
            let ticket = Ticket::new(self.tickets.fetch_add(1, Ordering::Relaxed) + 1);
            producer
                .publish(Request::Backfill {
                    index: index.clone(),
                    sink: self.name.clone(),
                    after,
                    ticket,
                })
                .await?;
            self.lock_outstanding().insert(
                index.clone(),
                Asked {
                    ticket,
                    target,
                    cut: false,
                },
            );
            self.drop_owed_seed(&index);
            indexes.push(index);
        }
        self.observer.on_backfill_requested(&self.name, &indexes);
        Ok(())
    }

    fn outstanding(&self) -> BTreeSet<IndexName> {
        self.lock_outstanding().keys().cloned().collect()
    }

    /// Whether an item naming `ticket` answers `index`'s latest request: no
    /// request is outstanding, or it is not cut and the item's ticket is its
    /// own or later. A snapshot batch naming none answers nothing
    /// outstanding.
    pub(super) fn answers(&self, index: &IndexName, ticket: Option<Ticket>) -> bool {
        match self.lock_outstanding().get(index) {
            None => true,
            Some(asked) => !asked.cut && ticket.is_some_and(|ticket| ticket >= asked.ticket),
        }
    }

    /// Whether `index`'s outstanding backfill is cut.
    fn is_cut(&self, index: &IndexName) -> bool {
        self.lock_outstanding()
            .get(index)
            .is_some_and(|asked| asked.cut)
    }

    /// The outstanding request a seed answering `ticket` ends: `index`'s, when
    /// not cut and `ticket` is its own or later.
    pub(super) fn settle(&self, index: &IndexName, ticket: Ticket) {
        let mut outstanding = self.lock_outstanding();
        if outstanding
            .get(index)
            .is_some_and(|asked| !asked.cut && asked.ticket <= ticket)
        {
            outstanding.remove(index);
        }
    }

    fn lock_outstanding(&self) -> std::sync::MutexGuard<'_, BTreeMap<IndexName, Asked>> {
        self.outstanding
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Follow the lane until it closes and everything taken from it is
    /// written (`Ok`), or until a sink, stream, or `stop` policy error stops
    /// the engine (`Err`), once the writes in flight beside it returned.
    /// Control messages arriving on `control` are handled between writes.
    #[tracing::instrument(name = "sink.run", skip_all, fields(sink = %self.name))]
    pub async fn run(&self, control: &mut mpsc::Receiver<SinkControl>) -> Result<()> {
        let result = self.run_inner(control).await;
        match &result {
            Ok(()) => self.set_blame(Blame::Nobody),
            Err(_) if matches!(self.blame(), Blame::Nobody) => self.set_blame(Blame::Everyone),
            Err(_) => {}
        }
        let id = EngineId::Sink(self.name.clone());
        match &result {
            Ok(()) => {
                tracing::info!("sink engine stopped: lane closed");
                self.observer.on_engine_stopped(&id);
            }
            Err(error) => {
                tracing::error!(%error, "sink engine stopped on error");
                self.observer.on_engine_error(&id, &error.to_string());
            }
        }
        result
    }

    async fn run_inner(&self, control: &mut mpsc::Receiver<SinkControl>) -> Result<()> {
        self.progress.check_lanes(self.lane_keys().iter())?;
        let lanes = self
            .lane_keys()
            .into_iter()
            .map(|key| Ok((key.clone(), self.stream.lane(&key)?.consumer)))
            .collect::<Result<Vec<_>>>()?;
        writes::Run::new(self, lanes).run(control).await
    }

    /// The lanes this engine reads: one per index it maintains.
    fn lane_keys(&self) -> Vec<LaneKey> {
        self.mappings
            .iter()
            .map(|mapping| LaneKey::new(mapping.index.clone(), self.name.clone()))
            .collect()
    }

    pub(super) fn blame(&self) -> Blame {
        self.blame
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn set_blame(&self, blame: Blame) {
        *self.blame.lock().unwrap_or_else(PoisonError::into_inner) = blame;
    }

    /// Close an idle window (nothing in flight, nothing taken) and report it;
    /// return the moment it closed.
    pub(super) fn idle_until(&self, since: Instant) -> Instant {
        let now = Instant::now();
        self.observer
            .on_stage(StageTiming::new(Stage::Idle, now - since).sink(&self.name));
        now
    }

    fn lock_fences(&self) -> std::sync::MutexGuard<'_, Fences> {
        self.fences.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_pending_reindex(&self) -> std::sync::MutexGuard<'_, Option<Vec<IndexName>>> {
        self.pending_reindex
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn set_pending_reindex(&self, indexes: Vec<IndexName>) {
        let mut pending = self.lock_pending_reindex();
        *pending = Some(indexes);
        self.reindex_waiting.store(true, Ordering::Release);
    }

    fn take_pending_reindex(&self) -> Option<Vec<IndexName>> {
        if !self.reindex_pending() {
            return None;
        }
        let mut pending = self.lock_pending_reindex();
        self.reindex_waiting.store(false, Ordering::Release);
        pending.take()
    }

    fn reindex_pending(&self) -> bool {
        self.reindex_waiting.load(Ordering::Acquire)
    }

    fn lock_skipped(&self) -> std::sync::MutexGuard<'_, BTreeSet<IndexName>> {
        self.skipped.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stage a rebuild of `indexes` and request their snapshot: the indexes
    /// staged, and each that could not be with why (§15.3, X14, X15). The
    /// current generation keeps serving until the snapshot completes and the
    /// sink swaps. A sink with `backfill = false` has nothing to rebuild. An
    /// index whose backfill is outstanding is left to that backfill while its
    /// lane is open; with its lane parked, blocked or restaging the reindex is
    /// the way out, so the backfill is cut and asked for again from the first
    /// key (stage 9, X6). Nothing is lost on a failure: see
    /// [`stage_reindex`](Self::stage_reindex).
    async fn reindex(&self, indexes: Vec<IndexName>) -> Reindexed {
        if !self.options.backfill {
            tracing::warn!("reindex requested, but backfill is disabled for this sink; ignoring");
            return Reindexed::default();
        }
        let outstanding = self.outstanding();
        let mut chosen = Vec::new();
        let mut rebuild_now: Vec<&IndexMapping> = Vec::new();
        let mut restart: Vec<IndexName> = Vec::new();
        for index in indexes {
            let Some(mapping) = self.mappings.iter().find(|m| m.index == index) else {
                tracing::warn!(%index, "reindex requested for an index this sink does not maintain; ignoring");
                continue;
            };
            // Two chains for one index at once are what the `Sink` port forbids.
            if chosen.iter().any(|m: &&IndexMapping| m.index == index) {
                continue;
            }
            let state = self.lane_state(&LaneKey::new(index.clone(), self.name.clone()));
            let health = state.health.clone();
            if health.is_detached() {
                if state.attaching.is_some() {
                    tracing::warn!(%index, sink = %self.name, "reindex ignored: the index is being rebuilt (starting)");
                } else {
                    tracing::info!(%index, sink = %self.name, "reindex of a detached index: rebuilding it (R13)");
                    rebuild_now.push(mapping);
                }
                continue;
            }
            if state.rebuilding.is_some() {
                if health.is_open() {
                    tracing::warn!(%index, sink = %self.name, "reindex ignored: the index is being rebuilt");
                } else if let LaneHealth::Parked {
                    reason: ParkReason::SeedBlocked { reason },
                    ..
                } = &health
                {
                    // O28: restarting would delete the complete generation
                    // and meet the same block (review of step 4, S2).
                    tracing::warn!(%index, sink = %self.name, %reason, "reindex ignored: the rebuild waits for the block on the serving generation");
                } else {
                    tracing::info!(%index, sink = %self.name, "reindex of a rebuild that is parked or restaging: it starts again (O20)");
                    restart.push(index.clone());
                }
                continue;
            }
            let open = health.is_open();
            if outstanding.contains(&index) && open {
                tracing::warn!(%index, "reindex requested while its snapshot is still outstanding; ignoring");
                continue;
            }
            tracing::info!(%index, "reindex requested; staging a fresh generation");
            chosen.push(mapping);
        }
        let mut reindexed = self.stage_reindex(chosen, &outstanding).await;
        for mapping in rebuild_now {
            match self.rebuild(mapping).await {
                Ok((attempts, target)) => {
                    reindexed
                        .rebuilt
                        .push((mapping.index.clone(), attempts, target));
                }
                Err(error) => reindexed
                    .rebuild_failed
                    .push((mapping.index.clone(), error.to_string())),
            }
        }
        reindexed.restart = restart;
        reindexed
    }

    /// Begin rebuilding `mapping`'s detached index (r1, r2): the sink clears
    /// the partials, moves the marker and records the rebuild; the new
    /// generation is ensured; the fence hears the rebuild, so it keeps
    /// vouching only for what the serving generation held. Its attempts and
    /// the generation's target.
    pub(super) async fn rebuild(
        &self,
        mapping: &IndexMapping,
    ) -> sink::Result<(u64, Option<TargetId>)> {
        // O29: refused while this process holds no slot (`slot`), the rebuild
        // is owed and tried again once the slot is held.
        let attempts = self.erasing().begin_rebuild(mapping).await.map_err(|_| {
            sink::SinkError::Write(
                "the rebuild waits until this process holds the slot (its stream ended or \
                     its opening failed)"
                    .to_owned(),
            )
        })??;
        let ensured = self.sink.ensure_index(mapping).await?;
        self.observer
            .on_index_rebuilding(&self.name, &mapping.index);
        Ok((attempts, ensured.target))
    }

    /// Retry the reindex `index` is owed (X15): staged, its backfill is
    /// asked for as the reindex would have.
    pub(super) async fn reindex_again(&self, index: &IndexName) -> Reindexed {
        let Some(mapping) = self.mappings.iter().find(|m| &m.index == index) else {
            return Reindexed::default();
        };
        tracing::info!(%index, sink = %self.name, "retrying the reindex its staging failed");
        let outstanding = self.outstanding();
        self.stage_reindex(vec![mapping], &outstanding).await
    }

    /// Stage `chosen` for a reindex and ask for each one's backfill. Every
    /// index ends in `staged` or in `failed`, whatever fails and in whatever
    /// order: a failed staging, an index not started after another failed,
    /// and an index whose ask for its backfill failed (a sink write to forget
    /// its walk, or the request lane) are each owed, never dropped (§15.3).
    async fn stage_reindex(
        &self,
        chosen: Vec<&IndexMapping>,
        outstanding: &BTreeSet<IndexName>,
    ) -> Reindexed {
        let tried = self
            .try_each_index(
                chosen,
                Step::Rebuild {
                    ask_seeded: false,
                    fresh: false,
                },
                Spread::StopAtFirstFailure,
            )
            .await;
        let mut reindexed = Reindexed {
            staged: Vec::new(),
            detached: Vec::new(),
            rebuilt: Vec::new(),
            rebuild_failed: Vec::new(),
            restart: Vec::new(),
            failed: tried
                .failed
                .into_iter()
                .map(|(index, error)| (index, format!("the reindex could not stage: {error}")))
                .collect(),
        };
        let not_started = if self.stop.is_requested() {
            "the reindex could not stage: not started, the stop came"
        } else {
            "the reindex could not stage: not started after another index failed"
        };
        reindexed.failed.extend(
            tried
                .not_started
                .into_iter()
                .map(|index| (index, not_started.to_owned())),
        );
        for (mapping, staged) in tried.done {
            let index = mapping.index.clone();
            if self.stop.is_requested() {
                reindexed
                    .failed
                    .push((index, STAGING_AT_THE_STOP.to_owned()));
                continue;
            }
            let target = match staged {
                Staged::Unseeded { target, .. } => target,
                Staged::Seeded => None,
                // Detached by what staging found: the run takes the lane as
                // detached, nothing is rebuilt.
                Staged::Detached => {
                    reindexed.detached.push(index);
                    continue;
                }
                // The records ask for a demotion or a detach, which waits
                // for the slot: the reindex is owed, its lane restaging and
                // staged again on the backoff, each staging reading the
                // records (B-1).
                Staged::Unheld { .. } => {
                    reindexed.failed.push((
                        index,
                        "the records ask for a demotion or a detach, which waits until this \
                         process holds the slot"
                            .to_owned(),
                    ));
                    continue;
                }
            };
            let asked = if outstanding.contains(&index) {
                tracing::warn!(%index, sink = %self.name, "reindex during a backfill: the backfill is cut and asked for again");
                self.ask_again(vec![(index.clone(), target)]).await
            } else {
                self.request_backfill(vec![(index.clone(), None, target)])
                    .await
            };
            match asked {
                Ok(()) => reindexed.staged.push(index),
                Err(error) => reindexed.failed.push((
                    index,
                    format!("the reindex could not ask for its backfill: {error}"),
                )),
            }
        }
        reindexed
    }

    /// Whether `index`'s backfill is still outstanding (not seeded yet).
    pub(super) fn is_outstanding(&self, index: &IndexName) -> bool {
        self.lock_outstanding().contains_key(index)
    }
}

/// How a staging goes on after an index fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spread {
    /// Start no further index: a destination that is down fails every index
    /// alike, and each wave would cost one more request timeout.
    StopAtFirstFailure,
    /// Per index (#471): once one index was ensured the destination answers,
    /// so every index is still started; while none is, further indexes are
    /// tried one at a time, at most [`STAGE_CONCURRENCY`] more, before the
    /// destination is read as down.
    OnceAnswered,
}

/// Every index a staging tried: what it learned of each that staged, each
/// that failed with its error, and each not started after a failure.
struct Tried<'a> {
    done: Vec<(&'a IndexMapping, Staged)>,
    failed: Vec<(IndexName, sink::SinkError)>,
    not_started: Vec<IndexName>,
}

/// What a reindex staged, and each index it could not stage with why.
#[derive(Debug, Default)]
pub(super) struct Reindexed {
    pub(super) staged: Vec<IndexName>,
    pub(super) failed: Vec<(IndexName, String)>,
    /// Staging found these detached: nothing rebuilt, the lane detached.
    pub(super) detached: Vec<IndexName>,
    /// Detached indexes whose rebuild began (r1, r2): its attempts and the
    /// target its generation was ensured as. The run attaches each lane.
    pub(super) rebuilt: Vec<(IndexName, u64, Option<TargetId>)>,
    /// Detached indexes whose rebuild could not begin, with why: they stay
    /// detached.
    pub(super) rebuild_failed: Vec<(IndexName, String)>,
    /// Rebuilding lanes parked or restaging: detached again, then rebuilt
    /// at once (O20(c)).
    pub(super) restart: Vec<IndexName>,
}

/// What staging learned of an index.
#[derive(Debug, Clone, PartialEq)]
enum Staged {
    Seeded,
    /// Detached (ADR 0012 stage 10): not ensured, not backfilled, its lane
    /// out of the slot's minimum.
    Detached,
    /// What the records ask (a demotion, a detach) waits for the slot
    /// (`Acted::Unheld`): nothing staged, the read not counted. A process's
    /// start takes the lane as detached for `why`, frozen at `through`, and
    /// the first read while it holds the slot decides.
    Unheld {
        why: String,
        through: Option<String>,
    },
    /// Unseeded, with where its last backfill durably got, if recorded, and
    /// the target staging found, if the sink names it.
    Unseeded {
        walked: Option<WalkMark>,
        target: Option<TargetId>,
    },
}

/// What staging a lane again ([`SinkEngine::restage`],
/// [`SinkEngine::ensure_again`]) found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Restaged {
    /// Staged: the lane opens.
    Staged,
    /// The index is detached: the lane is taken as such.
    Detached,
    /// What the records ask waits for the slot: the lane stays restaging,
    /// staged again on its backoff.
    WaitsForSlot,
}

/// What a staging does with what an index's detach records say (ADR 0012
/// stage 10, §4, O30). A `rebuilding` record with its marker moved is staged
/// as before until this version rebuilds (slice 2, step 4): its marker moved
/// first, so that is a rebuild of the index.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AtStaging {
    /// No record, a `rebuilding` one whose marker moved, or an unreadable
    /// one whose marker is on a newer unseeded generation: staged as before.
    AsBefore,
    /// Detached, for this reason: not ensured, not backfilled.
    Detached(String),
    /// A rebuild seeded a generation newer than the serving one since (an
    /// older flusso's, or one that died before deleting the record): the
    /// record is deleted and the index staged as before. Never for a
    /// live-only record.
    Finished(sink::RecordVersion),
    /// A rebuild under way, its marker moved, with the attempts its record
    /// counts: staged as a rebuild (the process-start attach), or demoted
    /// under `--skip-backfill` or `backfill = false` (O26, R14).
    Rebuilding(u64),
    /// Detached or rebuilding, yet the marker is on the generation reads see
    /// (restored by hand, or a late record of a dead process), or a record
    /// this version can't read beside any marker but a newer unseeded one:
    /// detached again.
    DetachAgain,
}

impl AtStaging {
    fn of(detachment: &sink::Detachment) -> Self {
        use sink::{DetachRecord, MarkerAt};
        let Some(record) = &detachment.record else {
            return Self::AsBefore;
        };
        let (detached, live_only) = match record {
            DetachRecord::Detached { live_only } => (true, *live_only),
            DetachRecord::Rebuilding { live_only } => (false, *live_only),
            // O30: an unreadable record is a rebuild only when the marker
            // moved first to a newer, unseeded generation; any other is
            // detached again rather than read as attached and current.
            _ => {
                return match detachment.marker {
                    Some(MarkerAt::Newer { seeded: false }) => Self::AsBefore,
                    _ => Self::DetachAgain,
                };
            }
        };
        // Finished only when a newer copy than the serving one was seeded: a
        // live-only index is never rebuilt, so its record stays until an
        // operator reattaches it (O16).
        if !live_only
            && let (Some(MarkerAt::Newer { seeded: true }), Some(version)) =
                (detachment.marker, &detachment.version)
        {
            return Self::Finished(version.clone());
        }
        match (detached, live_only, detachment.marker) {
            (false, _, Some(MarkerAt::Serving { .. }))
            | (true, false, Some(MarkerAt::Serving { .. })) => Self::DetachAgain,
            (false, _, _) => Self::Rebuilding(detachment.attempts.unwrap_or(0)),
            (true, live_only, _) => Self::Detached(if live_only {
                "the detach record says detached, live only".to_owned()
            } else {
                "the detach record says detached".to_owned()
            }),
        }
    }
}

/// What this process does about an index, from what its detach records say
/// and what it holds of the lane (ADR 0012 stage 10, O29): the one decision
/// staging and the read again both take, so the two can't drift (the fifth
/// re-check, B-3). Re-run after every opening (the read again).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Decision {
    /// Nothing recorded against the lane: as before.
    AsBefore,
    /// A rebuild seeded a newer generation and its record outlived it:
    /// deleted, on the version read.
    Forget(sink::RecordVersion),
    /// Detached by the records. `live_only_first`: written while the sink
    /// could rebuild, found on one that can't, put back live only first
    /// (R14).
    Detach { why: String, live_only_first: bool },
    /// The lane is held detached here and stays so.
    KeepDetached,
    /// The rebuild this process began or took over (the attempts match).
    Ours,
    /// Another process's rebuild, taken over: only at a start (no stream
    /// opened yet), its lane not held detached, with backfill to finish it.
    TakeOver(u64),
    /// Another process's rebuild otherwise: put back to detached before the
    /// lane drops anything (B-2), then decided again.
    Demote { live_only: bool },
    /// The records can't be acted on as they stand (O30): detached again.
    DetachAgain,
}

/// [`Decision`] from what the records say (`found`, and whether a detached
/// record was written while the sink could rebuild) and the lane this
/// process holds.
fn decide(
    found: &AtStaging,
    written_rebuildable: bool,
    lane: &LaneState,
    backfill: bool,
    skip_backfill: bool,
    start: bool,
) -> Decision {
    // A lane a start took as detached while what the records asked waited
    // for the slot (`undecided`) is decided as if attached: the records are
    // the same ones (another process's change stops first), so this is what
    // the start would have done.
    let held_detached = lane.health.is_detached() && !lane.undecided;
    match found {
        AtStaging::Rebuilding(attempts) if lane.rebuilding == Some(*attempts) => Decision::Ours,
        AtStaging::Rebuilding(attempts)
            if start && !held_detached && backfill && !skip_backfill =>
        {
            Decision::TakeOver(*attempts)
        }
        AtStaging::Rebuilding(_) => Decision::Demote {
            live_only: !backfill,
        },
        _ if held_detached => Decision::KeepDetached,
        AtStaging::AsBefore => Decision::AsBefore,
        AtStaging::Finished(version) => Decision::Forget(version.clone()),
        AtStaging::Detached(why) => Decision::Detach {
            why: why.clone(),
            live_only_first: written_rebuildable && !backfill,
        },
        AtStaging::DetachAgain => Decision::DetachAgain,
    }
}

/// What [`SinkEngine::act_on_records`] did, for its caller to apply to the
/// lane.
#[derive(Debug)]
enum Acted {
    /// As before, with the gaps the records keep.
    AsBefore { gaps: Vec<sink::Gap> },
    /// The lane is to be taken as detached, for `why`, frozen at `through`.
    Detached {
        why: String,
        through: Option<String>,
    },
    /// The lane is held detached and stays so.
    KeepDetached,
    /// The records ask for a demotion or a detach, which wait for the slot:
    /// nothing done, the read not counted. Why, and where the record says
    /// reads stood.
    Unheld {
        why: String,
        through: Option<String>,
    },
    /// The rebuild this process runs.
    Ours,
    /// Another process's rebuild, taken over under its attempts.
    TookOver(u64),
}

/// A staging step of `index` failed with `error`.
fn staging_failed(index: &IndexName, error: sink::SinkError) -> EngineError {
    EngineError::Staging(StagingFailures(vec![(index.clone(), error)]))
}

/// Sleep until `at`; never, when nothing is waiting on time. Keeps the loop
/// from building a timer on every pass just to leave it unpolled.
async fn until(at: Option<Instant>) {
    match at {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// The longest id a log line or an error message carries whole, in characters.
const SHOWN_ID_CHARS: usize = 64;

/// A document id as a log line or an error message shows it: whole when
/// short, else its first [`SHOWN_ID_CHARS`] characters and its length. An id
/// has no upper bound (a `bytea` key renders two characters per byte), and a
/// `stop` rejection is logged again on every restart.
fn shown_id(id: &str) -> std::borrow::Cow<'_, str> {
    match id.char_indices().nth(SHOWN_ID_CHARS) {
        None => id.into(),
        Some((cut, _)) => {
            let head = id.get(..cut).unwrap_or(id);
            format!("{head}… ({} bytes)", id.len()).into()
        }
    }
}

#[cfg(test)]
mod shown_id_tests {
    use super::shown_id;

    #[test]
    fn a_short_id_is_shown_whole() {
        assert_eq!(shown_id("42"), "42");
        assert_eq!(shown_id(&"é".repeat(64)), "é".repeat(64));
    }

    #[test]
    fn a_long_id_is_cut_at_a_character_and_says_its_length() {
        assert_eq!(
            shown_id(&"k".repeat(600)),
            format!("{}… (600 bytes)", "k".repeat(64))
        );
        assert_eq!(
            shown_id(&"é".repeat(65)),
            format!("{}… (130 bytes)", "é".repeat(64))
        );
    }
}
