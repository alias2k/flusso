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
//! - `Idle`: nothing routed to the lane yet.
//!
//! Several lanes answer as the one that covers least: what the source may
//! confirm is every lane's answer at once. The source turns the case into its
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
//! **Why no answer claims too much.** A lane's answer covers only what is
//! registered on it, and an item is registered only after its send succeeded,
//! so a lane never answers past an item it doesn't hold. Until the item is
//! registered the lane answers `Through` its previous newest (or `Idle`):
//! the batch's changes were already emitted, so the source's own `next_seq`
//! test keeps that `Through` short of them, and an `Idle` after an emission
//! moves nothing. Several lanes take the least answer, so a lane still behind
//! a registration holds the others back.
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
//! unbuilt caps every catch-up until the next step closes
//! ([`Progress::forget_all`]).
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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

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
}

/// Whether a running build holds an index's document: what a take leaves
/// waiting.
pub type Holds<'a> = &'a dyn Fn(&IndexName, &RowKey) -> bool;

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
    /// While set, no lane's catch-up passes it: one below the least read the
    /// last ended engine run left unbuilt, until a live step closes after it
    /// (§3.9 D2).
    restart: Option<u64>,
}

/// One lane's routed items not yet covered, as `(key, oldest, step)` in
/// routing order, the newest position routed to it and the steps routed to
/// it from the newest it has committed.
#[derive(Debug, Default)]
struct Registered {
    unlanded: VecDeque<(ItemKey, Position, Option<Step>)>,
    unsearchable: VecDeque<(ItemKey, Position)>,
    /// Every change through it that the lane needs is registered on it.
    newest: Option<Position>,
    /// The steps routed to the lane that can still answer what it has
    /// committed, in order: the front is the newest it has committed, once
    /// it has one; after it, each step holding an unlanded item and the
    /// newest step after each of those, a later skip replacing an earlier
    /// one. See [`Registered::record`] for the bound.
    steps: VecDeque<Step>,
    /// The most it has answered as committed: it never answers less.
    committed: u64,
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
                restart: None,
            }),
            marks,
            keys,
            pending,
            waiting: AtomicUsize::new(0),
            waiting_on,
            read: AtomicU64::new(0),
            unmarked: AtomicU64::new(u64::MAX),
            listener: OnceLock::new(),
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

    /// `lane` took the item keyed `key` of a batch covering `oldest..=last`.
    /// Call it once the lane's own send succeeded, in key order per lane.
    /// It counts toward no [`Step`]: a live build's item is registered with
    /// its step ([`published_build`](Self::published_build)).
    pub fn published(&self, lane: &LaneKey, key: ItemKey, oldest: Position, last: Position) {
        self.register(lane, Some((key, oldest)), last, None);
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
        self.register(lane, Some((key, oldest)), last, Some(step));
        self.registered();
    }

    /// A batch through `last` had nothing for `lane`: the lane has every
    /// change through `last` it needs, once its earlier items are registered.
    pub fn skipped(&self, lane: &LaneKey, last: Position) {
        self.register(lane, None, last, None);
        self.registered();
    }

    /// [`skipped`](Self::skipped), for the live build of `step`: the lane has
    /// every change of the step it needs, once its earlier items are.
    pub fn skipped_step(&self, lane: &LaneKey, last: Position, step: Step) {
        self.register(lane, None, last, Some(step));
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
        self.register(lane, Some((key, oldest)), last, Some(step));
        self.settle_build(lane, token);
        self.registered();
    }

    /// The live build of `step`, through `last`, had nothing for `lane`
    /// (it touched no document of the index, or every one it made failed
    /// under `skip`): [`skipped_step`](Self::skipped_step), and the index's
    /// pending build `token`, if it took one, no longer holds that lane
    /// back, before the one notification.
    pub fn skipped_build(&self, lane: &LaneKey, last: Position, token: Option<Token>, step: Step) {
        self.register(lane, None, last, Some(step));
        if let Some(token) = token {
            self.settle_build(lane, token);
        }
        self.registered();
    }

    fn register(
        &self,
        lane: &LaneKey,
        item: Option<(ItemKey, Position)>,
        last: Position,
        step: Option<Step>,
    ) {
        let mut registry = lock(&self.registry);
        if let (Some(registered), Some(marks)) =
            (registry.lanes.get_mut(lane), self.marks.get(lane))
        {
            if let Some(step) = step {
                registered.committed(marks);
                registered.record(step, item.is_some());
            }
            if let Some((key, oldest)) = item {
                registered.unlanded.push_back((key, oldest, step));
                registered.unsearchable.push_back((key, oldest));
            }
            registered.newest = Some(registered.newest.map_or(last, |n| n.max(last)));
        }
    }

    fn settle_build(&self, lane: &LaneKey, token: Token) {
        if let Some(pending) = self.pending.get(&lane.index) {
            lock(pending).registered(token, &lane.sink);
        }
    }

    fn registered(&self) {
        if let Some(listener) = self.listener.get() {
            listener();
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

    /// What `lane` has reached at `level`; `None` for a lane this progress
    /// doesn't know.
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
        for lane in lanes {
            let marks = self.marks.get(lane)?;
            let registered = registry.lanes.get_mut(lane)?;
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
        least
    }

    /// The oldest of what `lane` itself holds back: its items not yet landed
    /// and a batch being published that it hasn't taken yet. What its stall
    /// is measured by. Its index's backlog (documents waiting or building)
    /// doesn't count: it is on no lane yet. `None` for nothing outstanding or
    /// an unknown lane.
    pub fn own_oldest(&self, lane: &LaneKey) -> Option<Position> {
        let marks = self.marks.get(lane)?;
        let mut registry = lock(&self.registry);
        let own = registry
            .lanes
            .get_mut(lane)?
            .outstanding(marks, Level::Applied);
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
        let read = self.changes_read();
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

    fn take_from(
        &self,
        index: &IndexName,
        max: usize,
        through: Option<Position>,
        held: &dyn Fn(&RowKey) -> bool,
    ) -> Option<Taken> {
        let read = self.changes_read();
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
            let held = held.map(|held| move |key: &RowKey| held(index, key));
            lock(pending).any_held(
                held.as_ref().map(|held| held as &dyn Fn(&RowKey) -> bool),
                None,
            )
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

    /// Forget every pending document: the run that marked it ended before
    /// publishing it, and confirmation never passed it, so the next run's
    /// replayed stream marks it again. Call it as a run starts, before
    /// anything is read. Idempotent.
    ///
    /// What the ended run routed goes with it: until the new run routes its
    /// own, a lane answers through what was registered on it. The least read
    /// it leaves unbuilt (a document waiting, one in a build never released,
    /// a read counted before its marks) caps every lane's catch-up until a
    /// live step closes after the restart (ADR 0012 stage 8a, §3.9 D2).
    pub fn forget_all(&self) {
        let mut least = match self.unmarked.swap(u64::MAX, Ordering::AcqRel) {
            u64::MAX => None,
            read => Some(read),
        };
        for (index, pending) in &self.pending {
            let (forgotten, unbuilt) = lock(pending).forget();
            self.stopped_waiting(index, forgotten);
            least = least.into_iter().chain(unbuilt).min();
        }
        let mut registry = lock(&self.registry);
        registry.routed = None;
        if let Some(least) = least {
            let cap = least.saturating_sub(1);
            registry.restart = Some(registry.restart.map_or(cap, |held| held.min(cap)));
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
    /// before its marks): counted as read, and unbuilt until their replay
    /// (see [`forget_all`](Self::forget_all)). Returns how many have been
    /// read in all.
    pub fn read_unmarked(&self, changes: u64) -> u64 {
        let before = self.read.fetch_add(changes, Ordering::Release);
        if changes > 0 {
            self.unmarked.fetch_min(before + 1, Ordering::AcqRel);
        }
        before + changes
    }

    /// How many live changes have been read, over every engine run.
    pub fn changes_read(&self) -> u64 {
        self.read.load(Ordering::Acquire)
    }

    /// Close a step through every change read, for every index: right only
    /// while every document of every index read through it was taken
    /// (nothing waits). The engine closes [`close_steps`](Self::close_steps).
    pub fn close_step(&self) -> Step {
        let read = self.changes_read();
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
        registry.restart = None;
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
        let cap = registry.restart;
        let mut least: Option<u64> = None;
        for (lane, marks) in &self.marks {
            if lane.sink != *sink {
                continue;
            }
            let Some(registered) = registry.lanes.get_mut(lane) else {
                continue;
            };
            let landed = registered.committed(marks).map_or(0, |step| step.read);
            let idle = registered.unlanded.is_empty()
                && self
                    .pending
                    .get(&lane.index)
                    .is_none_or(|pending| lock(pending).is_idle());
            let caught_up = if idle {
                cap.map_or(read, |cap| cap.min(read))
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
        let mut waiting: Vec<(Position, IndexName, usize)> = Vec::new();
        for (index, pending) in &self.pending {
            let held = held.map(|held| move |key: &RowKey| held(index, key));
            let pending = lock(pending);
            let count = pending.scan(
                through,
                held.as_ref().map(|held| held as &dyn Fn(&RowKey) -> bool),
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
