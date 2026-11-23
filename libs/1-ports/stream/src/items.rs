//! What travels on the stream: [`LaneItem`]s down each `(index, sink)` lane
//! ([`LaneKey`]), [`Request`]s up the request lane, and the [`Stream`] port
//! that hands out their ends.
//!
//! ```
//! use kernel::Position;
//! use stream::{Batch, ItemKey, LaneItem, Ticket};
//!
//! let live = LaneItem::Batch(Batch { key: Some(ItemKey::new(0)), position: Some(Position::new(7)), ..Batch::default() });
//! let snapshot = LaneItem::Batch(Batch { ticket: Some(Ticket::new(1)), ..Batch::default() });
//! assert_eq!(live.position(), Some(Position::new(7)));
//! assert_eq!(live.key(), Some(ItemKey::new(0)));
//! assert_eq!(snapshot.position(), None, "snapshot rows carry no position");
//! ```

use std::sync::Arc;

use kernel::{Envelope, IndexName, Position, RowKey, SinkName};
use serde::{Deserialize, Serialize};

/// The unit a lane carries down to one sink engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneItem {
    /// The envelopes one ingest commit built, in build order.
    Batch(Batch),
    /// The snapshot the lane requested for its index is fully published; the
    /// sink engine may now record the index as seeded, unless `ticket` names
    /// an older request than the one its seed waits for.
    SnapshotComplete {
        index: IndexName,
        /// The lane's request this snapshot answers: the greatest of those it
        /// served.
        #[serde(default)]
        ticket: Ticket,
    },
}

/// Which lane: one index, as one sink receives it. Ordered by index, then
/// sink.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LaneKey {
    /// The index whose documents the lane carries.
    pub index: IndexName,
    /// The sink that receives them.
    pub sink: SinkName,
}

impl LaneKey {
    /// The lane of `index` into `sink`.
    pub fn new(index: IndexName, sink: SinkName) -> Self {
        Self { index, sink }
    }
}

impl std::fmt::Display for LaneKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.index, self.sink)
    }
}

/// One index's envelopes from one ingest commit, in build order, with the
/// position of the last change the commit covers. A live batch is known on
/// its lane by its [`ItemKey`], numbered per index in publication order; an
/// index the commit built nothing for gets no item. A snapshot batch carries
/// neither a key nor a position.
///
/// The envelopes are shared, not owned: one build is published to every lane
/// and held for redelivery without copying the documents, so a batch is
/// immutable once built — a sink stamps nothing into it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    /// The item's number on its index's lanes: increasing per index in the
    /// order the items are published, the same on every lane of the index.
    /// What a sink engine reports landed and searchable. `None` for snapshot
    /// rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<ItemKey>,
    /// The position of the last change in the batch; `None` for snapshot rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// The documents, in build order.
    pub envelopes: Arc<[Envelope]>,
    /// What keeps the batch's documents counted against the producer's
    /// memory budget: released when the last copy of the batch is dropped,
    /// every lane and redelivery ledger included. Never serialized.
    #[serde(skip)]
    pub hold: Hold,
    /// On a snapshot batch, how far its index's walk got: the greatest key
    /// of the batch's documents. `None` on a live batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walked: Option<WalkMark>,
    /// On a snapshot batch, the lane's request its snapshot answers (as
    /// [`LaneItem::SnapshotComplete`] names it). `None` on a live batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<Ticket>,
    /// Its build's number on its index: one counter per index, taken as the
    /// item is published, shared by live, snapshot and fresh builds. Two
    /// builds of one document never overlap, so for one document a larger
    /// number is a later build. `None` when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<u64>,
}

/// A fresh build of a parked lane's item, sent back to that lane's sink
/// engine on its sink's substitute lane ([`Stream::substitutes`]), apart from
/// the lane it replaces an item of: that lane is held up by the very item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Substitute {
    /// The lane whose parked item it replaces.
    pub lane: LaneKey,
    /// The [`Request::Rebuild`] it answers.
    pub ticket: Ticket,
    /// The documents, built again; neither a key nor a position.
    pub batch: Batch,
}

/// A live item's number on its index's lanes (see [`Batch::key`]): what
/// orders the items of a lane, where several may end on the same change.
/// Opaque to a sink.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ItemKey(u64);

impl ItemKey {
    /// The key numbered `number`.
    pub const fn new(number: u64) -> Self {
        Self(number)
    }

    /// Its number.
    pub const fn number(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ItemKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Names one backfill request of a lane. A sink engine numbers its requests
/// in increasing order, and every snapshot item a lane receives names the
/// greatest ticket of that lane's requests its snapshot serves. So a sink
/// engine that asked again (its target was created anew in the middle of a
/// backfill) tells the items of the snapshot that can no longer seed it from
/// the ones that can.
///
/// Tickets are numbered from one in each process. That is enough while a
/// stream lives no longer than the process (the in-process channel): no item
/// of an earlier process can reach a lane. A stream adapter that outlives it
/// needs tickets unique across processes.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Ticket(u64);

impl Ticket {
    /// The ticket numbered `number`.
    pub const fn new(number: u64) -> Self {
        Self(number)
    }
}

impl std::fmt::Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A point in a backfill's walk over an index's root keys: opaque to the
/// stream and the sink, written and read back only by the source, which
/// checks it still fits the index's key before resuming after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WalkMark(Arc<str>);

impl WalkMark {
    /// A mark holding `text`, as the source wrote it.
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self(text.into())
    }

    /// The text the source wrote.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque guard a [`Batch`] carries while any copy of it lives, shared by
/// every copy: the producer counts the batch's bytes until its last copy is
/// dropped. Equal to any other, so it never changes a batch's equality.
#[derive(Clone, Default)]
pub struct Hold(Option<Arc<dyn std::any::Any + Send + Sync>>);

impl Hold {
    /// Keep `guard` alive as long as any copy of the batch.
    pub fn new(guard: Arc<dyn std::any::Any + Send + Sync>) -> Self {
        Self(Some(guard))
    }

    /// Whether it keeps a guard.
    pub fn is_held(&self) -> bool {
        self.0.is_some()
    }
}

impl std::fmt::Debug for Hold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.is_held() { "Hold" } else { "Hold(none)" })
    }
}

impl PartialEq for Hold {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl LaneItem {
    /// The position acknowledging this item confirms, if any.
    pub fn position(&self) -> Option<Position> {
        match self {
            LaneItem::Batch(batch) => batch.position,
            LaneItem::SnapshotComplete { .. } => None,
        }
    }

    /// The item's key on its lane, if it is a live batch.
    pub fn key(&self) -> Option<ItemKey> {
        match self {
            LaneItem::Batch(batch) => batch.key,
            LaneItem::SnapshotComplete { .. } => None,
        }
    }
}

/// What a sink engine sends up the request lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// Snapshot `index` into its lane to `sink`, ending with a
    /// [`LaneItem::SnapshotComplete`]. Acknowledged by the ingest engine only
    /// after that marker is published, so a crash mid-snapshot redelivers it.
    /// `after`: where the sink's last walk of the index durably got, if it
    /// recorded one; the snapshot resumes after it. `ticket`: this request's
    /// number, which every item of the snapshot names back.
    Backfill {
        index: IndexName,
        sink: SinkName,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<WalkMark>,
        #[serde(default)]
        ticket: Ticket,
    },
    /// Build the documents of `keys` of `index` again now, and send them to
    /// `sink` as a [`Substitute`] naming `ticket`, on its substitute lane
    /// only. Asked by a lane parked on a refused item, for that item's
    /// documents. Acknowledged by the ingest engine once the substitute is
    /// published.
    Rebuild {
        index: IndexName,
        sink: SinkName,
        keys: Vec<RowKey>,
        ticket: Ticket,
    },
}

impl Request {
    /// The index the request is for.
    pub fn index(&self) -> &IndexName {
        match self {
            Request::Backfill { index, .. } | Request::Rebuild { index, .. } => index,
        }
    }

    /// The sink that asked.
    pub fn sink(&self) -> &SinkName {
        match self {
            Request::Backfill { sink, .. } | Request::Rebuild { sink, .. } => sink,
        }
    }
}

/// The two ends of one sink's substitute lane.
#[derive(Debug)]
pub struct SubstituteLane {
    /// Where the ingest engine publishes fresh builds for the sink.
    pub producer: Box<dyn crate::Producer<Substitute>>,
    /// Where the sink's engine receives them.
    pub consumer: Box<dyn crate::Consumer<Substitute>>,
}

/// The two ends of one `(index, sink)` lane.
#[derive(Debug)]
pub struct Lane {
    /// Where the ingest engine publishes.
    pub producer: Box<dyn crate::Producer<LaneItem>>,
    /// Where that sink's engine receives.
    pub consumer: Box<dyn crate::Consumer<LaneItem>>,
}

/// The two ends of the request lane.
#[derive(Debug)]
pub struct RequestLane {
    /// Where a sink engine publishes its requests.
    pub producer: Box<dyn crate::Producer<Request>>,
    /// Where the ingest engine receives them.
    pub consumer: Box<dyn crate::Consumer<Request>>,
}

/// The stream port: one lane per `(index, sink)` downward, one request lane
/// upward, and nothing else: how far each lane has landed is the engine's
/// `Progress`.
///
/// `lane` and `requests` are idempotent: calling them again returns fresh ends
/// over the *same* lane, which is how the daemon re-attaches a restarted engine
/// (the previous consumer's unacknowledged item is redelivered to the new one).
pub trait Stream: std::fmt::Debug + Send + Sync {
    /// The lane `key`. An unknown key is an error: the set of lanes is fixed
    /// when the stream is built.
    fn lane(&self, key: &LaneKey) -> crate::Result<Lane>;

    /// Every lane into `sink`, in index order.
    fn lanes(&self, sink: &SinkName) -> Vec<LaneKey>;

    /// The request lane.
    fn requests(&self) -> crate::Result<RequestLane>;

    /// `sink`'s substitute lane: fresh builds of its parked lanes' items, at
    /// most one per lane outstanding, delivered apart from the lanes. Fresh
    /// ends over the same lane on each call, as [`lane`](Self::lane). A stream
    /// without one answers [`QueueError::Unsupported`](crate::QueueError),
    /// and a parked lane over it writes its own item again instead.
    fn substitutes(&self, sink: &SinkName) -> crate::Result<SubstituteLane> {
        let _ = sink;
        Err(crate::QueueError::Unsupported("substitute lanes"))
    }

    /// Whether every lane is drained: nothing queued and nothing delivered but
    /// unacknowledged. The daemon waits for this before it stops the sink
    /// engines after the source stream ended.
    fn is_idle(&self) -> bool;
}
