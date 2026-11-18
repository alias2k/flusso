//! What travels on the stream: [`LaneItem`]s down each `(index, sink)` lane
//! ([`LaneKey`]), [`Request`]s up the request lane, and the [`Stream`] port
//! that hands out their ends.
//!
//! ```
//! use kernel::Position;
//! use stream::{Batch, LaneItem};
//!
//! let live = LaneItem::Batch(Batch { position: Some(Position::new(7)), changes: 3, envelopes: Vec::new().into(), hold: Default::default() });
//! let snapshot = LaneItem::Batch(Batch { position: None, changes: 0, envelopes: Vec::new().into(), hold: Default::default() });
//! assert_eq!(live.position(), Some(Position::new(7)));
//! assert_eq!(snapshot.position(), None, "snapshot rows carry no position");
//! ```

use std::sync::Arc;

use kernel::{Envelope, IndexName, Position, SinkName};
use serde::{Deserialize, Serialize};

/// The unit a lane carries down to one sink engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneItem {
    /// The envelopes one ingest commit built, in build order.
    Batch(Batch),
    /// The snapshot the lane requested for its index is fully published; the
    /// sink engine may now record the index as seeded.
    SnapshotComplete { index: IndexName },
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
/// position of the last change the commit covers (its key on the lane: one
/// commit puts at most one item on a lane). An index the commit built nothing
/// for gets no item. A snapshot batch carries no position.
///
/// The envelopes are shared, not owned: one build is published to every lane
/// and held for redelivery without copying the documents, so a batch is
/// immutable once built — a sink stamps nothing into it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    /// The position of the last change in the batch; `None` for snapshot rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// How many source changes the batch was built from, for progress
    /// accounting; `0` for a snapshot batch.
    #[serde(default)]
    pub changes: usize,
    /// The documents, in build order.
    pub envelopes: Arc<[Envelope]>,
    /// What keeps the batch's documents counted against the producer's
    /// memory budget: released when the last copy of the batch is dropped,
    /// every lane and redelivery ledger included. Never serialized.
    #[serde(skip)]
    pub hold: Hold,
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
}

/// What a sink engine sends up the request lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// Snapshot `index` into its lane to `sink`, ending with a
    /// [`LaneItem::SnapshotComplete`]. Acknowledged by the ingest engine only
    /// after that marker is published, so a crash mid-snapshot redelivers it.
    Backfill { index: IndexName, sink: SinkName },
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

    /// Whether every lane is drained: nothing queued and nothing delivered but
    /// unacknowledged. The daemon waits for this before it stops the sink
    /// engines after the source stream ended.
    fn is_idle(&self) -> bool;
}
