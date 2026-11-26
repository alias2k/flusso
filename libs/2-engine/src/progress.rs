//! How far each lane has landed the changes routed to it: the one place
//! confirmation and fences are computed from (ADR 0012).
//!
//! The ingest engine **routes** a batch: it numbers each index's item with
//! the next [`ItemKey`] of that index ([`Progress::next_key`]), and as each
//! lane's send succeeds, registers the item there under that key with the
//! batch's oldest position and its last; a lane the batch has nothing for is
//! told it was skipped. A lane's items are known by key, not by position: two
//! items of one lane may end on the same change. Each sink engine reports how far, in lane order, its
//! writes have **landed** (returned, nothing refused for good) and are
//! **searchable**: the last item written with every item before it, by key. Each
//! lane answers a [`Landed`] case:
//!
//! - `Before(o)`: `o` is the oldest position the lane hasn't landed;
//! - `Through(p)`: nothing outstanding, `p` the newest routed to the lane;
//! - `Idle`: nothing routed to the lane yet;
//! - `Free`: the lane is detached (ADR 0012 stage 10): it needs no change.
//!
//! Several lanes answer as the one that covers least: what the source may
//! confirm is every lane's answer at once. A detached lane
//! ([`Progress::detach`]) is left out, its index's backlog with it, so with
//! every lane detached the source confirms through its own horizon. The source turns the case into its
//! own terms (`ChangeCapture::confirm`, `ChangeCapture::reached`), so it keeps
//! no map of the changes it emitted.
//!
//! ```
//! use engine::{Level, Progress};
//! use kernel::{IndexName, Position, SinkName};
//! use source::cdc::Landed;
//! use stream::{ItemKey, LaneKey};
//!
//! let primary = LaneKey::new(
//!     IndexName::try_new("users").unwrap(),
//!     SinkName::try_new("primary").unwrap(),
//! );
//! let progress = Progress::new([primary.clone()]);
//! assert_eq!(progress.landed(), Landed::Idle);
//!
//! let key = progress.next_key(&primary.index).unwrap();
//! assert_eq!(key, ItemKey::new(0));
//! progress.published(&primary, key, Position::new(1), Position::new(3));
//! assert_eq!(progress.landed(), Landed::Before(Position::new(1)));
//!
//! progress.applied(&primary, key);
//! assert_eq!(progress.landed(), Landed::Through(Position::new(3)));
//! assert_eq!(
//!     progress.lane(&primary, Level::Searchable),
//!     Some(Landed::Before(Position::new(1))),
//!     "written, not yet searchable",
//! );
//! ```
//!
//! **Why no answer claims too much.** A lane answers `Before` the oldest of
//! its unlanded items and its index's **backlog**: a document waiting, or in
//! a build not yet registered on the lane. Only with neither does it answer
//! `Through(max(newest, routed))`: the newest position registered on it, or
//! the newest the reader routed. The reader raises `routed` only after the
//! run's marks, and a reader of `Progress` takes the registry lock before
//! any pending lock, so whoever sees a routed position sees every document
//! it marked: a change routed and not landed is in the backlog or on the
//! lane. An item is registered only after its send succeeded, before its
//! build leaves the backlog, so between the two both hold the lane back.
//! An engine restart forgets the run's documents and its `routed` in one
//! step under the registry lock ([`Progress::forget_all`]). Several lanes take
//! the least answer, so a lane still behind holds the others back.
//!
//! **Who writes what.** The ingest engine is the only writer of the
//! registrations, under one lock it takes once per lane per batch; the sink
//! engines never take it. A lane's sink engine is the only writer of its two
//! marks, one atomic each, which it raises without a lock. Lane order is
//! registration order and key order, so "through key k" covers every
//! registered item up to k. The key counters are one atomic per index,
//! outliving every run, so a restarted engine never numbers an item like one
//! still unacknowledged on a lane. A reader (the ingest engine's confirmation, a fence) takes the
//! registrations' lock, reads every lane's marks under it, and drops what they
//! cover: no registration lands between two lanes' answers, and a mark read
//! early only makes its lane's answer older. A mark may run ahead of the
//! registrations (a lane lands an item before it is registered): it is kept,
//! and covers the item once it is.
//!
//! **How many changes a sink has committed.** The ingest engine numbers each
//! change it reads ([`Progress::read_changes`]) and marks every document it
//! touched with that read number ([`Progress::mark_reads`]). At each live grant
//! it closes one [`Step`] per index at the index's **frontier**: one below the
//! oldest read still waiting on the index, or every read with none waiting
//! ([`Progress::close_steps`]); its build's items carry it on every lane of the
//! index, published or skipped. A lane has committed the frontier of the
//! newest step it has landed every item through, and every read while it has
//! nothing unlanded and its index nothing waiting or building (its catch-up);
//! never less than it answered before. A sink has committed the least over its
//! lanes ([`Progress::committed`]), every read with no lane. A change a
//! replaying source reads again is a second read, and a redelivered item that
//! had landed moves nothing. After an engine run ends, the least read it left
//! unbuilt caps each lane's catch-up until a step is registered on that lane
//! after the restart ([`Progress::forget_all`]).
//!
//! **What the reader routed.** After a run's marks and count the reader
//! raises [`Progress::routed`]: a lane with nothing outstanding answers through
//! it, so a quiet index holds back neither confirmation nor a fence.
//!
//! ```
//! use engine::Progress;
//! use kernel::{IndexName, Position, SinkName};
//! use stream::LaneKey;
//!
//! let primary = SinkName::try_new("primary").unwrap();
//! let users = LaneKey::new(IndexName::try_new("users").unwrap(), primary.clone());
//! let orders = LaneKey::new(IndexName::try_new("orders").unwrap(), primary.clone());
//! let progress = Progress::new([users.clone(), orders.clone()]);
//!
//! progress.change_read();
//! let step = progress.close_step();
//! let key = progress.next_key(&users.index).unwrap();
//! progress.published_step(&users, key, Position::new(1), Position::new(1), step);
//! progress.skipped_step(&orders, Position::new(1), step);
//! assert_eq!(progress.committed(&primary), 0, "the users item hasn't landed");
//!
//! progress.applied(&users, key);
//! assert_eq!(progress.committed(&primary), 1);
//! ```

mod pending;

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::task::{Context, Poll};

use futures::task::AtomicWaker;

use kernel::{IndexName, Position, SinkName};
use source::RowKey;
use source::cdc::Landed;
use source::document::DocumentId;
use stream::{ItemKey, LaneKey};

use pending::Pending;
pub use pending::{Taken, Token};

/// What holds the source's resume point back the most
/// ([`Progress::pinned_by`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    /// A lane: its own oldest item not landed, or a batch being published
    /// that it hasn't taken yet (a full lane).
    Lane(LaneKey),
    /// An index's backlog: documents marked and waiting, or building, that
    /// no lane has been sent yet.
    Backlog(IndexName),
}

/// One index's part of a live grant: what its lanes' committed count moves
/// to once they land it ([`Progress::committed`]). Numbered in grant order
/// for the life of the progress; the indexes of one grant share the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Step {
    number: u64,
    /// The index's frontier at the grant: how many changes had been read
    /// through the last one all of whose documents of the index were taken
    /// by this grant or an earlier one. What a lane that has committed the
    /// step has committed.
    read: u64,
}

/// Which of a lane's two levels to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// The item's write returned: confirmation and an *applied* fence.
    Applied,
    /// A search sees the item too: a *searchable* fence.
    Searchable,
}

/// The landing state of every lane. One per deployment; it outlives every
/// engine run.
pub struct Progress {
    registry: Mutex<Registry>,
    marks: BTreeMap<LaneKey, Marks>,
    /// Per index, the number of its next item.
    keys: BTreeMap<IndexName, AtomicU64>,
    /// Per index, its documents marked and not yet published.
    pending: BTreeMap<IndexName, Mutex<Pending>>,
    /// How many ids wait, over every index.
    waiting: AtomicUsize,
    /// How many ids wait on each index: what the reader's lead reads
    /// without taking an index's lock.
    waiting_on: BTreeMap<IndexName, AtomicUsize>,
    /// How many live changes have been read, over every engine run. Raised
    /// with release after a run's marks, read with acquire before anything
    /// that the marks decide.
    read: AtomicU64,
    /// The least read number counted without its marks since the last
    /// `forget_all` (a run cut before its marks); `u64::MAX` for none.
    unmarked: AtomicU64,
    /// Called after every registration: a fence waiter may pass now.
    listener: OnceLock<Listener>,
    /// Raised after every registration, for the engine run's dispatcher to
    /// confirm (ADR 0012 stage 8a, §3.9 §2): the listener's one slot is the
    /// fence's.
    registrations: Registrations,
}

/// A flag raised after every registration, and the waker of whoever waits on
/// it. Only the first raise since the waiter last looked wakes it.
#[derive(Debug, Default)]
struct Registrations {
    raised: AtomicBool,
    waiter: AtomicWaker,
}

/// The documents of an index running builds hold outside its pending set (a
/// backfill build's): offered one by one to `found` until it answers
/// `true`; whether it did. What a take leaves waiting, read from the builds'
/// side so a check costs one lookup per held document, not a walk of what
/// waits.
pub type Holds<'a> = &'a dyn Fn(&IndexName, &mut dyn FnMut(&RowKey) -> bool) -> bool;

/// What [`Progress::on_registered`] calls.
pub type Listener = Arc<dyn Fn() + Send + Sync>;

impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("registry", &self.registry)
            .field("marks", &self.marks)
            .field("keys", &self.keys)
            .finish_non_exhaustive()
    }
}

/// What the ingest engine routed to each lane and no mark covers yet.
#[derive(Debug, Default)]
struct Registry {
    lanes: BTreeMap<LaneKey, Registered>,
    /// The number of the next step.
    next_step: u64,
    /// The newest position the reader routed in this engine run: a lane with
    /// nothing outstanding answers through it (§3.9 D1).
    routed: Option<Position>,
}

/// One lane's routed items not yet covered, as `(key, oldest, step)` in
/// routing order, the newest position routed to it and the steps routed to
/// it from the newest it has committed.
#[derive(Debug, Default)]
struct Registered {
    unlanded: VecDeque<(ItemKey, Position, Option<Step>)>,
    unsearchable: VecDeque<(ItemKey, Position)>,
    /// Every change through it that the lane needs is registered on it:
    /// raised only by an item (or skip) whose index had every document
    /// marked through it taken by then (see [`Progress::register`]).
    newest: Option<Position>,
    /// The steps routed to the lane that can still answer what it has
    /// committed, in order: the front is the newest it has committed, once
    /// it has one; after it, each step holding an unlanded item and the
    /// newest step after each of those, a later skip replacing an earlier
    /// one. See [`Registered::record`] for the bound.
    steps: VecDeque<Step>,
    /// The most it has answered as committed: it never answers less.
    committed: u64,
    /// While set, its catch-up never passes it: one below the least read an
    /// ended engine run left unbuilt, until a step is registered on the lane
    /// after the restart (§3.9 D2).
    restart: Option<u64>,
    /// Detached from the minimum (ADR 0012 stage 10, R5): nothing registered
    /// on it counts, and no answer over several lanes reads it.
    detached: bool,
}

/// How far one lane's sink engine has written, in lane order: the key's
/// number `+ 1`, `0` for nothing.
#[derive(Debug, Default)]
struct Marks {
    applied: AtomicU64,
    searchable: AtomicU64,
}

impl Progress {
    /// Nothing routed, over `lanes`.
    pub fn new(lanes: impl IntoIterator<Item = LaneKey>) -> Self {
        let marks: BTreeMap<LaneKey, Marks> = lanes
            .into_iter()
            .map(|lane| (lane, Marks::default()))
            .collect();
        let keys = marks
            .keys()
            .map(|lane| (lane.index.clone(), AtomicU64::new(0)))
            .collect();
        let pending = marks
            .keys()
            .map(|lane| (lane.index.clone(), Mutex::new(Pending::default())))
            .collect();
        let waiting_on = marks
            .keys()
            .map(|lane| (lane.index.clone(), AtomicUsize::new(0)))
            .collect();
        Self {
            registry: Mutex::new(Registry {
                lanes: marks
                    .keys()
                    .map(|lane| (lane.clone(), Registered::default()))
                    .collect(),
                next_step: 0,
                routed: None,
            }),
            marks,
            keys,
            pending,
            waiting: AtomicUsize::new(0),
            waiting_on,
            read: AtomicU64::new(0),
            unmarked: AtomicU64::new(u64::MAX),
            listener: OnceLock::new(),
            registrations: Registrations::default(),
        }
    }

    /// Nothing routed, over one lane per pair of `indexes` and `sinks`.
    pub fn of(
        indexes: impl IntoIterator<Item = IndexName>,
        sinks: impl IntoIterator<Item = SinkName> + Clone,
    ) -> Self {
        Self::new(indexes.into_iter().flat_map(|index| {
            sinks
                .clone()
                .into_iter()
                .map(move |sink| LaneKey::new(index.clone(), sink))
        }))
    }

    /// Whether `lane` is one this progress keeps.
    pub fn knows(&self, lane: &LaneKey) -> bool {
        lock(&self.registry).lanes.contains_key(lane)
    }

    /// Take `lane` out of the minimum (ADR 0012 stage 10, R5): its
    /// registrations and steps are dropped, every later registration on it is
    /// ignored, and no answer over several lanes reads it, whatever it or its
    /// index's backlog holds. With no lane left attached, [`landed`](Self::landed)
    /// answers [`Landed::Free`]. Outlives [`forget_all`](Self::forget_all).
    /// Whether `lane` is one this progress keeps.
    pub fn detach(&self, lane: &LaneKey) -> bool {
        let mut registry = lock(&self.registry);
        let Some(registered) = registry.lanes.get_mut(lane) else {
            return false;
        };
        *registered = Registered {
            detached: true,
            ..Registered::default()
        };
        drop(registry);
        self.registered();
        true
    }

    /// Whether `lane` is detached from the minimum.
    pub fn is_detached(&self, lane: &LaneKey) -> bool {
        lock(&self.registry)
            .lanes
            .get(lane)
            .is_some_and(|registered| registered.detached)
    }

    /// Fail unless every lane in `lanes` is one this progress keeps: an engine
    /// publishing to a lane it doesn't would confirm past that lane's items.
    pub(crate) fn check_lanes<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a LaneKey>,
    ) -> crate::Result<()> {
        let registry = lock(&self.registry);
        match lanes
            .into_iter()
            .find(|lane| !registry.lanes.contains_key(*lane))
        {
            Some(lane) => Err(crate::EngineError::LaneNotInProgress(lane.to_string())),
            None => Ok(()),
        }
    }

    /// Call `listener` after every registration (one listener: the fence's).
    pub fn on_registered(&self, listener: Listener) {
        let _ = self.listener.set(listener);
    }

    /// The key of `index`'s next item, the same on every lane of the index:
    /// one more than the last, for the life of this progress. `None` for an
    /// index with no lane here.
    pub fn next_key(&self, index: &IndexName) -> Option<ItemKey> {
        self.keys
            .get(index)
            .map(|next| ItemKey::new(next.fetch_add(1, Ordering::Relaxed)))
    }

    /// How many items of `index` have been numbered: it moves once a live
    /// item of the index is published (a restarted builder that published
    /// again starts its backoff over, ADR 0012 stage 8a, slice 8).
    pub(crate) fn numbered(&self, index: &IndexName) -> u64 {
        self.keys
            .get(index)
            .map_or(0, |next| next.load(Ordering::Relaxed))
    }

    /// `lane` took the item keyed `key` of a batch covering `oldest..=last`.
    /// Call it once the lane's own send succeeded, in key order per lane.
    /// It counts toward no [`Step`]: a live build's item is registered with
    /// its step ([`published_build`](Self::published_build)).
    pub fn published(&self, lane: &LaneKey, key: ItemKey, oldest: Position, last: Position) {
        self.register(lane, Some((key, oldest)), last, None, None);
        self.registered();
    }

    /// [`published`](Self::published), for an item of the live build of
    /// `step`.
    pub fn published_step(
        &self,
        lane: &LaneKey,
        key: ItemKey,
        oldest: Position,
        last: Position,
        step: Step,
    ) {
        self.register(lane, Some((key, oldest)), last, Some(step), None);
        self.registered();
    }

    /// A batch through `last` had nothing for `lane`: the lane has every
    /// change through `last` it needs, once its earlier items are registered.
    pub fn skipped(&self, lane: &LaneKey, last: Position) {
        self.register(lane, None, last, None, None);
        self.registered();
    }

    /// [`skipped`](Self::skipped), for the live build of `step`: the lane has
    /// every change of the step it needs, once its earlier items are.
    pub fn skipped_step(&self, lane: &LaneKey, last: Position, step: Step) {
        self.register(lane, None, last, Some(step), None);
        self.registered();
    }

    /// `lane` took the item keyed `key` of the pending build `token`, which
    /// covers `oldest..=last` and is the live build of `step`:
    /// [`published_step`](Self::published_step), and the build no longer
    /// holds that lane back, both before the one notification, so a fence
    /// waiter woken by it sees the lane's new answer.
    pub fn published_build(
        &self,
        lane: &LaneKey,
        key: ItemKey,
        oldest: Position,
        last: Position,
        token: Token,
        step: Step,
    ) {
        self.register(lane, Some((key, oldest)), last, Some(step), Some(token));
        self.settle_build(lane, token);
        self.registered();
    }

    /// The live build of `step`, through `last`, had nothing for `lane`
    /// (it touched no document of the index, or every one it made failed
    /// under `skip`): [`skipped_step`](Self::skipped_step), and the index's
    /// pending build `token`, if it took one, no longer holds that lane
    /// back, before the one notification.
    pub fn skipped_build(&self, lane: &LaneKey, last: Position, token: Option<Token>, step: Step) {
        self.register(lane, None, last, Some(step), token);
        if let Some(token) = token {
            self.settle_build(lane, token);
        }
        self.registered();
    }

    /// Register `item` (or a skip) through `last` on `lane`, the item of
    /// the pending build `token` if any. The lane's `newest` moves to `last`
    /// only while its index has nothing marked at or before `last` that
    /// waits or is in another build: a take cut by the cap (O2) or the
    /// fold's backlog take leaves some of `last`'s documents behind, and
    /// after a restart forgets them a lane answering through `last` would
    /// let confirmation and a fence pass documents never written (the
    /// invariant: a lane's `newest` names only positions whose every
    /// document of the index has been taken).
    fn register(
        &self,
        lane: &LaneKey,
        item: Option<(ItemKey, Position)>,
        last: Position,
        step: Option<Step>,
        token: Option<Token>,
    ) {
        let mut registry = lock(&self.registry);
        let whole = self
            .pending
            .get(&lane.index)
            .is_none_or(|pending| lock(pending).all_taken_through(last, token));
        if let (Some(registered), Some(marks)) =
            (registry.lanes.get_mut(lane), self.marks.get(lane))
            && !registered.detached
        {
            if let Some(step) = step {
                registered.committed(marks);
                registered.record(step, item.is_some());
                registered.restart = None;
            }
            if let Some((key, oldest)) = item {
                registered.unlanded.push_back((key, oldest, step));
                registered.unsearchable.push_back((key, oldest));
            }
            if whole {
                registered.newest = Some(registered.newest.map_or(last, |n| n.max(last)));
            }
        }
    }

    fn settle_build(&self, lane: &LaneKey, token: Token) {
        if let Some(pending) = self.pending.get(&lane.index) {
            lock(pending).registered(token, &lane.sink);
        }
    }

    fn registered(&self) {
        if !self.registrations.raised.swap(true, Ordering::AcqRel) {
            self.registrations.waiter.wake();
        }
        if let Some(listener) = self.listener.get() {
            listener();
        }
    }

    /// Ready once something was registered since it was last ready: a run
    /// routed, a build's lane registered, a build released. The engine run's
    /// dispatcher confirms then, after the future that registered (ADR 0012
    /// stage 8a, §3.9 §2). One waiter at a time.
    pub(crate) fn poll_registered(&self, cx: &mut Context<'_>) -> Poll<()> {
        self.registrations.waiter.register(cx.waker());
        if self.registrations.raised.swap(false, Ordering::AcqRel) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }

    /// `lane` has landed, in lane order, every item through the one keyed
    /// `through`. Never moves back.
    pub fn applied(&self, lane: &LaneKey, through: ItemKey) {
        if let Some(marks) = self.marks.get(lane) {
            raise(&marks.applied, through);
        }
    }

    /// Every item `lane` routed through the one keyed `through` is
    /// searchable. Never moves back.
    pub fn searchable(&self, lane: &LaneKey, through: ItemKey) {
        if let Some(marks) = self.marks.get(lane) {
            raise(&marks.searchable, through);
        }
    }

    /// What every lane has landed: what the source may confirm. The lane
    /// that covers least decides.
    pub fn landed(&self) -> Landed {
        self.over(self.marks.keys(), Level::Applied)
            .unwrap_or(Landed::Idle)
    }

    /// What decides [`landed`](Self::landed) while it holds something back
    /// (`Before`): a lane (what it holds itself, see
    /// [`own_oldest`](Self::own_oldest)), or an index's backlog, which no
    /// lane has been sent yet. On a tie, within a lane or across lanes, the
    /// lane is named. `None` when everything routed has landed.
    pub fn pinned_by(&self) -> Option<Pin> {
        match self.least_of(self.marks.keys(), Level::Applied) {
            Some((lane, Landed::Before(_), true)) => Some(Pin::Backlog(lane.index.clone())),
            Some((lane, Landed::Before(_), false)) => Some(Pin::Lane(lane.clone())),
            _ => None,
        }
    }

    /// What `lane` has reached at `level`: `Free` while it is detached;
    /// `None` for a lane this progress doesn't know.
    pub fn lane(&self, lane: &LaneKey, level: Level) -> Option<Landed> {
        self.over([lane], level)
    }

    /// What every one of `lanes` has reached at `level`: the answer of the
    /// lane that covers least. `None` when one is a lane this progress
    /// doesn't know, or `lanes` is empty.
    ///
    /// Every lane is read under the one registry lock, so no registration
    /// lands between two lanes' answers; a mark read early only makes that
    /// lane's answer older, never wider.
    pub fn over<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a LaneKey>,
        level: Level,
    ) -> Option<Landed> {
        self.least_of(lanes, level).map(|(_, landed, _)| landed)
    }

    /// [`over`](Self::over), with the lane whose answer it is (on a tie, the
    /// first of `lanes`) and whether its index's pending documents, not its
    /// own items, decide it.
    ///
    /// A lane answers `Before` the oldest of its own unlanded items, a batch
    /// being published it hasn't taken yet (both its own), and its index's
    /// backlog (the backlog's). The writers order their steps so the reader
    /// can't miss a change: an item is registered on a lane before its build
    /// stops counting there, and documents are pending before anything routes
    /// past them. So the registry is locked first and each index's pending set
    /// under it; no writer holds a pending lock while it takes the registry's.
    fn least_of<'a>(
        &self,
        lanes: impl IntoIterator<Item = &'a LaneKey>,
        level: Level,
    ) -> Option<(&'a LaneKey, Landed, bool)> {
        let mut registry = lock(&self.registry);
        let routed = registry.routed;
        let mut least: Option<(&'a LaneKey, Landed, bool)> = None;
        let mut detached: Option<&'a LaneKey> = None;
        for lane in lanes {
            let marks = self.marks.get(lane)?;
            let registered = registry.lanes.get_mut(lane)?;
            if registered.detached {
                detached = detached.or(Some(lane));
                continue;
            }
            let (sent, backlog) = self
                .pending
                .get(&lane.index)
                .map_or((None, None), |pending| {
                    let pending = lock(pending);
                    (pending.sent_to(&lane.sink), pending.backlog_for(&lane.sink))
                });

            let own = registered
                .outstanding(marks, level)
                .into_iter()
                .chain(sent)
                .min();
            let (answer, from_backlog) = match (own, backlog) {
                (Some(own), Some(backlog)) if backlog < own => (Landed::Before(backlog), true),
                (Some(own), _) => (Landed::Before(own), false),
                (None, Some(backlog)) => (Landed::Before(backlog), true),
                (None, None) => (
                    registered
                        .newest
                        .into_iter()
                        .chain(routed)
                        .max_by_key(|position| position.seq)
                        .map_or(Landed::Idle, Landed::Through),
                    false,
                ),
            };
            // On a tie the lane is named: its own item holds the slot as much
            // as the backlog does, and it is the one to look at.
            least = Some(match least {
                Some(least) if (bound(least.1), least.2) <= (bound(answer), from_backlog) => least,
                _ => (lane, answer, from_backlog),
            });
        }
        least.or_else(|| detached.map(|lane| (lane, Landed::Free, false)))
    }

    /// The oldest of what `lane` itself holds back: its items not yet landed
    /// and a batch being published that it hasn't taken yet. What its stall
    /// is measured by. Its index's backlog (documents waiting or building)
    /// doesn't count: it is on no lane yet. `None` for nothing outstanding or
    /// an unknown lane.
    pub fn own_oldest(&self, lane: &LaneKey) -> Option<Position> {
        let marks = self.marks.get(lane)?;
        let mut registry = lock(&self.registry);
        let registered = registry.lanes.get_mut(lane)?;
        if registered.detached {
            return None;
        }
        let own = registered.outstanding(marks, Level::Applied);
        let sent = self
            .pending
            .get(&lane.index)
            .and_then(|pending| lock(pending).sent_to(&lane.sink));
        own.into_iter().chain(sent).min()
    }

    /// Mark each id pending on its index, touched by the change at its
    /// position, as the next change read (see [`mark_reads`](Self::mark_reads)
    /// for a run of several). An id of an index with no lane here is dropped:
    /// nothing would ever publish it. Returns how many ids were kept.
    pub fn mark(&self, ids: impl IntoIterator<Item = (DocumentId, Position)>) -> usize {
        let read = self.changes_read() + 1;
        self.mark_reads(ids.into_iter().map(|(id, at)| (id, at, read)))
    }

    /// Mark each id pending on its index, touched by the change at its
    /// position, the change read as the `read`th (its read number: what
    /// [`read_changes`](Self::read_changes) will have counted through it).
    /// An id already waiting keeps its older change. An id of an index with
    /// no lane here is dropped. Returns how many ids were kept.
    pub fn mark_reads(&self, ids: impl IntoIterator<Item = (DocumentId, Position, u64)>) -> usize {
        let mut kept = 0;
        let mut current: Option<(&IndexName, MutexGuard<'_, Pending>, usize)> = None;
        for (id, at, read) in ids {
            let same = current
                .as_ref()
                .is_some_and(|(index, _, _)| **index == id.index);
            if !same {
                if let Some((index, pending, started)) = current.take() {
                    drop(pending);
                    self.started_waiting(index, started);
                }
                if let Some((index, pending)) = self.pending.get_key_value(&id.index) {
                    current = Some((index, lock(pending), 0));
                }
            }
            let Some((_, pending, started)) = current.as_mut() else {
                continue;
            };
            kept += 1;
            if pending.mark(id.key, at, read) {
                *started += 1;
            }
        }
        if let Some((index, pending, started)) = current {
            drop(pending);
            self.started_waiting(index, started);
        }
        kept
    }

    /// The reader routed every change through `position`: their documents are
    /// marked and they are counted. A lane with nothing outstanding answers
    /// through it (never further back than it answered). Fires the listener.
    pub fn routed(&self, position: Position) {
        {
            let mut registry = lock(&self.registry);
            if registry
                .routed
                .is_none_or(|routed| routed.seq < position.seq)
            {
                registry.routed = Some(position);
            }
        }
        self.registered();
    }

    /// The frontier of `index` now: one below the read number of its oldest
    /// waiting document, or every change read with none waiting. What a live
    /// grant that takes nothing of the index registers on its lanes.
    pub fn frontier(&self, index: &IndexName) -> u64 {
        let read = self.readable();
        match self.pending.get(index) {
            Some(pending) => frontier(&lock(pending), read),
            None => read,
        }
    }

    fn started_waiting(&self, index: &IndexName, started: usize) {
        if started == 0 {
            return;
        }
        self.waiting.fetch_add(started, Ordering::Relaxed);
        if let Some(waiting) = self.waiting_on.get(index) {
            waiting.fetch_add(started, Ordering::Relaxed);
        }
    }

    fn stopped_waiting(&self, index: &IndexName, stopped: usize) {
        if stopped == 0 {
            return;
        }
        self.waiting.fetch_sub(stopped, Ordering::Relaxed);
        if let Some(waiting) = self.waiting_on.get(index) {
            waiting.fetch_sub(stopped, Ordering::Relaxed);
        }
    }

    /// Take up to `max` of `index`'s waiting ids for one build, oldest
    /// first, skipping ids building and ids `held` says a running build holds.
    /// `None` when nothing could be taken.
    pub fn take(
        &self,
        index: &IndexName,
        max: usize,
        held: &dyn Fn(&RowKey) -> bool,
    ) -> Option<Taken> {
        self.take_from(index, max, None, held)
    }

    /// [`take`](Self::take) every waiting id of `index` marked at or before
    /// `through`, and none marked after: a build of the runs through
    /// `through` takes no newer run's document (ADR 0012 stage 8a, R4).
    pub fn take_through(
        &self,
        index: &IndexName,
        through: Position,
        held: &dyn Fn(&RowKey) -> bool,
    ) -> Option<Taken> {
        self.take_from(index, usize::MAX, Some(through), held)
    }

    /// [`take_through`](Self::take_through) at most `max` ids, oldest first
    /// (one index's build takes at most `max_changes` documents, O2).
    pub fn take_through_at_most(
        &self,
        index: &IndexName,
        through: Position,
        max: usize,
        held: &dyn Fn(&RowKey) -> bool,
    ) -> Option<Taken> {
        self.take_from(index, max, Some(through), held)
    }

    /// The newest position among `index`'s documents waiting: where a take
    /// of its backlog ends.
    pub fn newest_waiting(&self, index: &IndexName) -> Option<Position> {
        self.pending
            .get(index)
            .and_then(|pending| lock(pending).newest_waiting())
    }

    /// How many of `index`'s documents wait to be taken.
    pub fn waiting_on(&self, index: &IndexName) -> usize {
        self.waiting_on
            .get(index)
            .map_or(0, |waiting| waiting.load(Ordering::Relaxed))
    }

    fn take_from(
        &self,
        index: &IndexName,
        max: usize,
        through: Option<Position>,
        held: &dyn Fn(&RowKey) -> bool,
    ) -> Option<Taken> {
        let read = self.readable();
        let mut pending = lock(self.pending.get(index)?);
        let mut taken = pending.take(index, max, through, held)?;
        taken.frontier = frontier(&pending, read);
        drop(pending);
        self.stopped_waiting(index, taken.ids.len());
        Some(taken)
    }

    /// `index`'s build `token` returned: its documents leave admission, so
    /// a later change to one of them can be taken into a later build.
    pub fn build_landed(&self, index: &IndexName, token: Token) {
        if let Some(pending) = self.pending.get(index) {
            lock(pending).land(token);
        }
    }

    /// `index`'s build `token` is being published: from now on a lane that
    /// hasn't registered its item answers for it (stall age, `pinned_by`).
    pub fn build_sending(&self, index: &IndexName, token: Token) {
        if let Some(pending) = self.pending.get(index) {
            lock(pending).sending(token);
        }
    }

    /// Whether some waiting document is building, or `held` says a running
    /// build holds it (`None`: none is held), so a take now would leave it
    /// behind.
    pub fn any_waiting_held(&self, held: Option<Holds<'_>>) -> bool {
        self.pending.iter().any(|(index, pending)| {
            let held =
                held.map(|held| move |found: &mut dyn FnMut(&RowKey) -> bool| held(index, found));
            lock(pending).any_held(held.as_ref().map(|held| held as pending::Held<'_>), None)
        })
    }

    /// The build under `token` is published on every lane of `index` (call
    /// it after every lane's registration): its ids no longer hold anything
    /// back.
    pub fn release(&self, index: &IndexName, token: Token) {
        if let Some(pending) = self.pending.get(index) {
            lock(pending).release(token);
        }
        self.registered();
    }

    /// Give every build of `index` taken and not yet released back to its
    /// pending set (ADR 0012 stage 8a, slice 8): the index's builder failed
    /// before publishing them and restarts alone, so nothing is read again.
    /// Each id waits again at its own change, so the index's lanes stay
    /// before the oldest of them, as they were while the builds held them.
    /// Call it once none of the builds still runs.
    pub fn give_back(&self, index: &IndexName) {
        let Some(pending) = self.pending.get(index) else {
            return;
        };
        let started = lock(pending).give_back();
        self.started_waiting(index, started);
    }

    /// Forget every pending document: the run that marked it ended before
    /// publishing it, and confirmation never passed it, so the next run's
    /// replayed stream marks it again. Call it as a run starts, before
    /// anything is read. Idempotent.
    ///
    /// What the ended run routed goes with it: until the new run routes its
    /// own, a lane answers through what was registered on it. The least read
    /// it leaves unbuilt (a document waiting, one in a build never released,
    /// a read counted before its marks) caps each lane's catch-up until a
    /// live step is registered on that lane after the restart (ADR 0012
    /// stage 8a, §3.9 D2): a lane the next grant passes by (its index
    /// deferred) keeps its cap. That never strands a cap while every grant
    /// registers every lane not deferred, with an item or a skip; once a
    /// builder registers only its own index's lanes (slice 6b-1c), the first
    /// grant after a restart must also lift the cap of every lane whose index
    /// has nothing waiting or building, or a quiet index stays capped.
    pub fn forget_all(&self) {
        // One step for a reader: it holds the registry lock before any
        // pending lock, so it sees the run whole or forgotten whole, never
        // its documents gone while `routed` and no cap still answer for it.
        let mut registry = lock(&self.registry);
        let mut least = match self.unmarked.swap(u64::MAX, Ordering::AcqRel) {
            u64::MAX => None,
            read => Some(read),
        };
        for (index, pending) in &self.pending {
            let forgotten = lock(pending).forget();
            self.stopped_waiting(index, forgotten.waiting);
            least = least.into_iter().chain(forgotten.least_read).min();
        }
        #[cfg(test)]
        tests::while_forgetting();
        registry.routed = None;
        if let Some(least) = least {
            let cap = least.saturating_sub(1);
            for registered in registry.lanes.values_mut() {
                registered.restart = Some(registered.restart.map_or(cap, |held| held.min(cap)));
            }
        }
    }

    /// One more live change was read.
    pub fn change_read(&self) {
        self.read_changes(1);
    }

    /// `changes` more live changes were read, their documents marked:
    /// returns how many have been read in all.
    pub fn read_changes(&self, changes: u64) -> u64 {
        self.read.fetch_add(changes, Ordering::Release) + changes
    }

    /// `changes` more live changes were read and never marked (a run cut
    /// before its marks): counted as read, and unbuilt until their replay.
    /// From now on no frontier and no catch-up passes the first of them,
    /// until the next run's first step (see [`forget_all`](Self::forget_all)).
    /// Returns how many have been read in all.
    pub fn read_unmarked(&self, changes: u64) -> u64 {
        if changes == 0 {
            return self.changes_read();
        }
        // The cut is recorded before the count moves (release), so whoever
        // reads the count (acquire) sees it.
        let first = self.read.load(Ordering::Acquire) + 1;
        self.unmarked.fetch_min(first, Ordering::AcqRel);
        self.read.fetch_add(changes, Ordering::Release) + changes
    }

    /// How many live changes have been read, over every engine run.
    pub fn changes_read(&self) -> u64 {
        self.read.load(Ordering::Acquire)
    }

    /// The reads a frontier may cover: every read, or those before the first
    /// read this run cut before its marks.
    fn readable(&self) -> u64 {
        let read = self.changes_read();
        Self::short_of_the_cut(read, self.unmarked.load(Ordering::Acquire))
    }

    fn short_of_the_cut(read: u64, unmarked: u64) -> u64 {
        match unmarked {
            u64::MAX => read,
            first => read.min(first.saturating_sub(1)),
        }
    }

    /// Close a step through every change read, for every index: right only
    /// while every document of every index read through it was taken
    /// (nothing waits). The engine closes [`close_steps`](Self::close_steps).
    pub fn close_step(&self) -> Step {
        let read = self.readable();
        let mut registry = lock(&self.registry);
        Self::next_step(&mut registry, read)
    }

    /// Close one live grant's step for each index, at the frontier given for
    /// it (a take's [`Taken::frontier`], or [`frontier`](Self::frontier) for
    /// an index the grant took nothing of), all under one number: register
    /// each index's lanes with its own.
    pub fn close_steps(
        &self,
        frontiers: impl IntoIterator<Item = (IndexName, u64)>,
    ) -> BTreeMap<IndexName, Step> {
        let mut registry = lock(&self.registry);
        let number = Self::next_step(&mut registry, 0).number;
        // A step closed after a restart lifts the cap of every lane whose
        // index has nothing waiting or building: no builder of it would
        // ever register a step of its own (the rule in `forget_all`'s doc).
        for (lane, registered) in registry.lanes.iter_mut() {
            if registered.restart.is_some()
                && self
                    .pending
                    .get(&lane.index)
                    .is_none_or(|pending| lock(pending).is_idle())
            {
                registered.restart = None;
            }
        }
        frontiers
            .into_iter()
            .map(|(index, read)| (index, Step { number, read }))
            .collect()
    }

    fn next_step(registry: &mut Registry, read: u64) -> Step {
        let step = Step {
            number: registry.next_step,
            read,
        };
        registry.next_step += 1;
        step
    }

    /// How many live changes `sink` has committed: the least over its lanes.
    /// A lane has committed the frontier of the newest step it has landed
    /// every item through, or had nothing for; while it has nothing unlanded
    /// and its index nothing waiting or building, every change read (its
    /// catch-up, capped after an engine run ended: see
    /// [`forget_all`](Self::forget_all)); never less than it answered
    /// before. A sink with no lane has every read.
    pub fn committed(&self, sink: &SinkName) -> u64 {
        let read = self.changes_read();
        let mut registry = lock(&self.registry);
        // Read under the registry lock: `forget_all` turns the cut into the
        // restart caps inside it.
        let readable = Self::short_of_the_cut(read, self.unmarked.load(Ordering::Acquire));
        let mut least: Option<u64> = None;
        for (lane, marks) in &self.marks {
            if lane.sink != *sink {
                continue;
            }
            let Some(registered) = registry.lanes.get_mut(lane) else {
                continue;
            };
            if registered.detached {
                continue;
            }
            let landed = registered.committed(marks).map_or(0, |step| step.read);
            // With nothing unlanded, every read below the index's oldest
            // unbuilt one (a document waiting, a build not yet released) is
            // landed on the lane or had nothing for it (ADR 0012 stage 8a,
            // 6b-1c: a lane is registered only with its own index's steps).
            let caught_up = if registered.unlanded.is_empty() {
                let unbuilt = self
                    .pending
                    .get(&lane.index)
                    .and_then(|pending| lock(pending).least_unbuilt());
                let reach = unbuilt.map_or(readable, |least| least.saturating_sub(1).min(readable));
                registered.restart.map_or(reach, |cap| cap.min(reach))
            } else {
                0
            };
            let value = landed.max(caught_up).max(registered.committed);
            registered.committed = value;
            least = Some(least.map_or(value, |least| least.min(value)));
        }
        least.unwrap_or(read)
    }

    /// How many steps `lane` keeps to answer [`committed`](Self::committed),
    /// beside how many items it has unlanded, both read under one lock: the
    /// first is at most the second plus two, however many grants pass the
    /// lane. `None` for an unknown lane.
    pub fn steps_kept(&self, lane: &LaneKey) -> Option<(usize, usize)> {
        let marks = self.marks.get(lane)?;
        let mut registry = lock(&self.registry);
        registry.lanes.get_mut(lane).map(|registered| {
            registered.committed(marks);
            (registered.steps.len(), registered.unlanded.len())
        })
    }

    /// The read number of `index`'s oldest change not built yet: a document
    /// waiting, or in a build no lane has been sent yet; `None` while none.
    /// Compared with [`changes_read`](Self::changes_read) over time, it says
    /// how long ago that change was read: an index's backlog age (slice 7).
    /// One lock.
    pub fn oldest_unbuilt(&self, index: &IndexName) -> Option<u64> {
        self.pending
            .get(index)
            .and_then(|pending| lock(pending).oldest_unbuilt())
    }

    /// Every index that can have pending documents: one with a lane here.
    pub fn indexes(&self) -> impl Iterator<Item = &IndexName> {
        self.pending.keys()
    }

    /// Each index with ids waiting to be taken, how many, oldest waiting
    /// first (by the position of its oldest waiting id; ties by name).
    pub fn waiting_by_index(&self) -> Vec<(IndexName, usize)> {
        let mut waiting: Vec<(Position, IndexName, usize)> = self
            .pending
            .iter()
            .filter_map(|(index, pending)| {
                let pending = lock(pending);
                let count = pending.waiting();
                pending
                    .oldest_waiting()
                    .filter(|_| count > 0)
                    .map(|oldest| (oldest, index.clone(), count))
            })
            .collect();
        waiting.sort();
        waiting
            .into_iter()
            .map(|(_, index, count)| (index, count))
            .collect()
    }

    /// How many ids wait to be taken, over every index.
    pub fn waiting(&self) -> usize {
        self.waiting.load(Ordering::Relaxed)
    }

    /// The most ids waiting on any one index: what the reader's lead per
    /// index reads. One atomic load per index, no lock.
    pub fn most_waiting(&self) -> usize {
        self.waiting_on
            .values()
            .map(|waiting| waiting.load(Ordering::Relaxed))
            .max()
            .unwrap_or(0)
    }

    /// Each index's documents waiting that were marked at or before
    /// `through`, how many, oldest waiting first: what a take through it
    /// would reach. `None` when one of them is building or `held` says a
    /// running build holds it (a take now would leave it behind). One lock
    /// and at most one walk per index.
    pub fn waiting_through(
        &self,
        through: Position,
        held: Option<Holds<'_>>,
    ) -> Option<Vec<(IndexName, usize)>> {
        self.waiting_of(None, through, held, usize::MAX)
    }

    /// [`waiting_through`](Self::waiting_through) on `index` alone, or on
    /// every index when `None`, each count stopped at `most`: what one
    /// index's builder takes. Its work is one lookup per document building
    /// or held and at most `most` waiting entries per index, never a walk of
    /// everything waiting.
    pub fn waiting_of(
        &self,
        only: Option<&IndexName>,
        through: Position,
        held: Option<Holds<'_>>,
        most: usize,
    ) -> Option<Vec<(IndexName, usize)>> {
        let mut waiting: Vec<(Position, IndexName, usize)> = Vec::new();
        for (index, pending) in self
            .pending
            .iter()
            .filter(|(index, _)| only.is_none_or(|only| *index == only))
        {
            let held =
                held.map(|held| move |found: &mut dyn FnMut(&RowKey) -> bool| held(index, found));
            let pending = lock(pending);
            let count = pending.scan(
                through,
                held.as_ref().map(|held| held as pending::Held<'_>),
                most,
            )?;
            if count > 0
                && let Some(oldest) = pending.oldest_waiting()
            {
                waiting.push((oldest, index.clone(), count));
            }
        }
        waiting.sort();
        Some(
            waiting
                .into_iter()
                .map(|(_, index, count)| (index, count))
                .collect(),
        )
    }

    /// Whether `index` (any index for `None`) has a document marked at or
    /// before `through` waiting: one look at each set's oldest.
    pub fn waits_through(&self, only: Option<&IndexName>, through: Position) -> bool {
        self.pending
            .iter()
            .filter(|(index, _)| only.is_none_or(|only| *index == only))
            .any(|(_, pending)| lock(pending).waits_through(through))
    }

    /// How many items routed to `lane` it hasn't landed.
    pub fn outstanding(&self, lane: &LaneKey) -> usize {
        let Some(marks) = self.marks.get(lane) else {
            return 0;
        };
        let mut registry = lock(&self.registry);
        registry.lanes.get_mut(lane).map_or(0, |registered| {
            registered.outstanding(marks, Level::Applied);
            registered.unlanded.len()
        })
    }

    /// Whether `lane` has nothing routed to it outstanding at `level`.
    pub fn is_clear(&self, lane: &LaneKey, level: Level) -> bool {
        self.lane(lane, level)
            .is_none_or(|landed| !matches!(landed, Landed::Before(_)))
    }
}

/// The seq just past what `landed` covers; on a tie `Before` is the lower,
/// since it is what a lane with an item outstanding answers.
fn bound(landed: Landed) -> (u64, u8) {
    match landed {
        Landed::Before(oldest) => (oldest.seq, 0),
        Landed::Through(last) => (last.seq.saturating_add(1), 1),
        Landed::Idle => (0, 0),
        Landed::Free => (u64::MAX, 2),
    }
}

impl Registered {
    /// Drop what the marks cover, then the oldest position still outstanding
    /// at `level`.
    fn outstanding(&mut self, marks: &Marks, level: Level) -> Option<Position> {
        let applied = marks.applied.load(Ordering::Acquire);
        while self
            .unlanded
            .front()
            .is_some_and(|(key, _, _)| key.number() < applied)
        {
            self.unlanded.pop_front();
        }
        let searchable = marks.searchable.load(Ordering::Acquire);
        while self
            .unsearchable
            .front()
            .is_some_and(|(key, _)| key.number() < searchable)
        {
            self.unsearchable.pop_front();
        }
        let unlanded = self.unlanded.front().map(|(_, oldest, _)| *oldest);
        match level {
            Level::Applied => unlanded,
            Level::Searchable => {
                let unsearchable = self.unsearchable.front().map(|(_, oldest)| *oldest);
                unlanded.into_iter().chain(unsearchable).min()
            }
        }
    }
}

impl Registered {
    /// The newest step routed to the lane that it has landed every item
    /// of, through it, or had nothing for; `None` before the first.
    fn committed(&mut self, marks: &Marks) -> Option<Step> {
        self.outstanding(marks, Level::Applied);
        let unlanded = self.unlanded.iter().find_map(|(_, _, step)| *step);
        let behind = |step: &Step| unlanded.is_none_or(|unlanded| *step < unlanded);
        while self.steps.get(1).is_some_and(behind) {
            self.steps.pop_front();
        }
        self.steps.front().copied().filter(behind)
    }

    /// Record that `step` was routed to the lane, with an item or skipped.
    /// Call it after [`committed`](Self::committed) pruned the front and
    /// before the step's own item is registered.
    ///
    /// A skipped step replaces the newest entry: steps reach a lane in
    /// order, so whenever that entry could answer as committed the skip
    /// answers too, and it is the newer. **The bound:** items land in key
    /// order, so every item after the first unlanded one is unlanded too;
    /// past the front, an entry was pushed by one of the `k` unlanded items.
    /// So a lane keeps at most `k + 1` entries after a registration, `k + 2`
    /// with the step being recorded, however many grants pass it.
    fn record(&mut self, step: Step, item: bool) {
        let Some(&newest) = self.steps.back() else {
            self.steps.push_back(step);
            return;
        };
        if newest >= step {
            return;
        }
        if !item && let Some(back) = self.steps.back_mut() {
            *back = step;
        } else {
            self.steps.push_back(step);
        }
    }
}

/// Raise a mark to cover `through`. A lane's sink engine is its marks' only
/// writer (one run at a time), so a plain load and store are enough: no
/// read-modify-write on the path every lane item takes.
fn raise(mark: &AtomicU64, through: ItemKey) {
    let covered = through.number().saturating_add(1);
    if covered > mark.load(Ordering::Relaxed) {
        mark.store(covered, Ordering::Release);
    }
}

/// An index's frontier given its pending set and the changes read before
/// the set was locked: one below its oldest waiting document's read number,
/// or every change read with none waiting.
fn frontier(pending: &Pending, read: u64) -> u64 {
    pending
        .least_waiting_read()
        .map_or(read, |least| least.saturating_sub(1).min(read))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
