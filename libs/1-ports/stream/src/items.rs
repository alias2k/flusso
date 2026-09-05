//! What travels on the stream: [`LaneItem`]s down each sink's lane,
//! [`Request`]s up the request lane, and the [`Stream`] port that hands out
//! their ends and owns the watermark.
//!
//! ```
//! use kernel::Position;
//! use stream::{Batch, LaneItem};
//!
//! let live = LaneItem::Batch(Batch { position: Some(Position(7)), changes: 3, envelopes: Vec::new().into() });
//! let snapshot = LaneItem::Batch(Batch { position: None, changes: 0, envelopes: Vec::new().into() });
//! assert_eq!(live.position(), Some(Position(7)));
//! assert_eq!(snapshot.position(), None, "snapshot rows never move the watermark");
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
    /// The snapshot the lane requested for `indexes` is fully published; the
    /// sink engine may now record them as seeded.
    SnapshotComplete { indexes: Vec<IndexName> },
}

/// The envelopes one ingest commit built, in build order, with the position of
/// the last change the batch covers. A live batch with no envelopes (every
/// change resolved to nothing this sink indexes) still carries its position,
/// so acknowledging it moves the watermark. A snapshot batch carries none.
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
    /// Snapshot `indexes` into `sink`'s lane, ending with a
    /// [`LaneItem::SnapshotComplete`]. Acknowledged by the ingest engine only
    /// after that marker is published, so a crash mid-snapshot redelivers it.
    Backfill {
        sink: SinkName,
        indexes: Vec<IndexName>,
    },
}

/// The two ends of one sink's lane.
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

/// The stream port: one lane per sink downward, one request lane upward, and
/// the watermark that ties every lane's acknowledgements together.
///
/// `lane` and `requests` are idempotent: calling them again returns fresh ends
/// over the *same* lane, which is how the daemon re-attaches a restarted engine
/// (the previous consumer's unacknowledged item is redelivered to the new one).
pub trait Stream: std::fmt::Debug + Send + Sync {
    /// The lane for `sink`. An unknown sink is an error: the set of lanes is
    /// fixed when the stream is built.
    fn lane(&self, sink: &SinkName) -> crate::Result<Lane>;

    /// The request lane.
    fn requests(&self) -> crate::Result<RequestLane>;

    /// The lowest position every lane has acknowledged: what the source may
    /// confirm. `None` until every lane has acknowledged a positioned batch.
    fn watermark(&self) -> Option<Position>;

    /// Whether every lane is drained: nothing queued and nothing delivered but
    /// unacknowledged. The daemon waits for this before it stops the sink
    /// engines after the source stream ended.
    fn is_idle(&self) -> bool;
}
