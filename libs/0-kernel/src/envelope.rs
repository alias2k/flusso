//! What flows through the stream: the position in the source's change feed
//! and the envelope a lane carries for one document.
//!
//! A [`Position`] is assigned by the source, monotonic within one run, and
//! meaningful only to the source that issued it (the Postgres adapter maps it
//! back to an LSN). The stream orders positions to compute its watermark; the
//! ingest engine hands the watermark back to the source as confirmation.
//!
//! An [`Envelope`] is one document operation as every sink receives it: the
//! index, the operation, the document id, the document itself (absent on a
//! delete), the position of the change that produced it (absent for a snapshot
//! row), and when it was built. Emitting sinks forward it as-is; a future
//! consumer library deserializes the same type.
//!
//! ```
//! use kernel::{Envelope, Op, Position};
//!
//! let position = Position(42);
//! assert_eq!(position.to_string(), "42");
//! assert!(Position(41) < position);
//!
//! let json = serde_json::to_string(&Op::Upsert).unwrap();
//! assert_eq!(json, "\"upsert\"");
//! ```

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{GenericValue, IndexName};

/// An offset in the source's change feed: opaque to everyone but the source,
/// ordered so the stream can take a minimum across lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Position(pub u64);

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What happened to the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    /// The document was inserted or replaced; `document` carries its state.
    Upsert,
    /// The document no longer exists at the source.
    Delete,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Op::Upsert => "upsert",
            Op::Delete => "delete",
        })
    }
}

/// The envelope format version, bumped when a field changes meaning.
pub const ENVELOPE_VERSION: u8 = 1;

/// One document operation as a lane carries it and a sink receives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// [`ENVELOPE_VERSION`] at the time of writing.
    pub version: u8,
    /// When the ingest engine built this document.
    pub ts: DateTime<Utc>,
    /// The position of the change that produced it; `None` for a snapshot row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// The logical index the document belongs to.
    pub index: IndexName,
    /// Upsert or delete.
    pub op: Op,
    /// The document id within the index: the root primary key as a string.
    pub id: String,
    /// The built document; absent on a delete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<GenericValue>,
}

impl Envelope {
    /// An upsert of `document` under `id` in `index`.
    pub fn upsert(
        index: IndexName,
        id: impl Into<String>,
        document: GenericValue,
        position: Option<Position>,
        ts: DateTime<Utc>,
    ) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            ts,
            position,
            index,
            op: Op::Upsert,
            id: id.into(),
            document: Some(document),
        }
    }

    /// A delete of `id` from `index`.
    pub fn delete(
        index: IndexName,
        id: impl Into<String>,
        position: Option<Position>,
        ts: DateTime<Utc>,
    ) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            ts,
            position,
            index,
            op: Op::Delete,
            id: id.into(),
            document: None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn envelope_round_trips_through_json() {
        let ts = DateTime::parse_from_rfc3339("2026-06-03T10:20:30.123Z")
            .unwrap()
            .with_timezone(&Utc);
        let envelope = Envelope::delete(
            IndexName::try_new("users").unwrap(),
            "7",
            Some(Position(9)),
            ts,
        );
        let json = serde_json::to_value(&envelope).unwrap();
        assert_eq!(json["op"], "delete");
        assert_eq!(json["position"], 9);
        assert_eq!(json["ts"], "2026-06-03T10:20:30.123Z");
        assert!(json.get("document").is_none());
        let back: Envelope = serde_json::from_value(json).unwrap();
        assert_eq!(back, envelope);
    }

    #[test]
    fn snapshot_envelopes_carry_no_position() {
        let ts = DateTime::parse_from_rfc3339("2026-06-03T10:20:30Z")
            .unwrap()
            .with_timezone(&Utc);
        let envelope = Envelope::upsert(
            IndexName::try_new("users").unwrap(),
            "1",
            GenericValue::Null,
            None,
            ts,
        );
        let json = serde_json::to_value(&envelope).unwrap();
        assert!(json.get("position").is_none());
        assert_eq!(json["version"], ENVELOPE_VERSION);
    }
}
