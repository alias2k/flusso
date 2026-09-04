//! What flows through the stream: the position in the source's change feed
//! and the envelope a lane carries for one document.
//!
//! A [`Position`] is assigned by the source, monotonic for the life of the
//! source instance (across reopened streams, so a restarted ingest engine
//! never reuses one), and meaningful only to the source that issued it (the Postgres adapter maps it
//! back to an LSN). The stream orders positions to compute its watermark; the
//! ingest engine hands the watermark back to the source as confirmation.
//!
//! An [`Envelope`] is one document operation as every sink receives it: the
//! index, the operation, the document id, the document itself (absent on a
//! delete), the position of the change that produced it (absent for a snapshot
//! row, `seq` on the wire), when it was built, and — stamped on the sink side —
//! the emitting sink's name and a `meta` summary. Emitting sinks forward it
//! as-is; a consumer deserializes the same type over its own document type.
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

use crate::common::{GenericValue, IndexName, SinkName};

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
///
/// Generic over the document so one shape serves the pipeline
/// (`Envelope<GenericValue>`, the default) and a consumer of an emitting sink's
/// output (`Envelope<MyDocument>` deserialized from the same JSON). On the
/// wire the position is `seq`, an opaque string; `sink` and `meta` are set by
/// the sink side and absent inside the pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope<D = GenericValue> {
    /// The sink this copy is for; stamped by the sink engine before the sink
    /// sees it, so an emitting sink forwards its own name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sink: Option<SinkName>,
    /// [`ENVELOPE_VERSION`] at the time of writing.
    pub version: u8,
    /// When the ingest engine built this document.
    pub ts: DateTime<Utc>,
    /// The position of the change that produced it; `None` for a snapshot row.
    /// Serialized as `seq`, the position rendered as an opaque string.
    #[serde(
        rename = "seq",
        default,
        skip_serializing_if = "Option::is_none",
        with = "seq_string"
    )]
    pub position: Option<Position>,
    /// The logical index the document belongs to.
    pub index: IndexName,
    /// Upsert or delete.
    pub op: Op,
    /// The document id within the index: the root primary key as a string.
    pub id: String,
    /// A summary of the serialized document, filled by an emitting sink (it
    /// knows the wire format); absent inside the pipeline and on a delete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<EnvelopeMeta>,
    /// The built document; absent on a delete.
    #[serde(default = "Option::default", skip_serializing_if = "Option::is_none")]
    pub document: Option<D>,
}

/// At-a-glance facts about a serialized document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopeMeta {
    /// Top-level field count; `None` when the document is not an object.
    pub fields: Option<usize>,
    /// Size of the compact serialized document, in bytes.
    pub bytes: usize,
}

impl<D> Envelope<D> {
    /// The same envelope over a document of another type: how an emitting sink
    /// translates the pipeline's value into its wire format.
    pub fn map_document<E>(self, f: impl FnOnce(D) -> E) -> Envelope<E> {
        Envelope {
            sink: self.sink,
            version: self.version,
            ts: self.ts,
            position: self.position,
            index: self.index,
            op: self.op,
            id: self.id,
            meta: self.meta,
            document: self.document.map(f),
        }
    }
}

/// `seq` on the wire: the position as an opaque string.
mod seq_string {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::Position;

    pub(super) fn serialize<S: Serializer>(
        position: &Option<Position>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        position
            .map(|position| position.0.to_string())
            .serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Position>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|raw| {
                raw.parse::<u64>()
                    .map(Position)
                    .map_err(serde::de::Error::custom)
            })
            .transpose()
    }
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
            sink: None,
            version: ENVELOPE_VERSION,
            ts,
            position,
            index,
            op: Op::Upsert,
            id: id.into(),
            meta: None,
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
            sink: None,
            version: ENVELOPE_VERSION,
            ts,
            position,
            index,
            op: Op::Delete,
            id: id.into(),
            meta: None,
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
        assert_eq!(
            json["seq"], "9",
            "the position is an opaque string on the wire"
        );
        assert_eq!(json["ts"], "2026-06-03T10:20:30.123Z");
        assert!(json.get("document").is_none());
        assert!(json.get("sink").is_none());
        assert!(json.get("meta").is_none());
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
        assert!(json.get("seq").is_none());
        assert_eq!(json["version"], ENVELOPE_VERSION);
    }

    #[test]
    fn a_consumer_reads_the_wire_shape_over_its_own_document_type() {
        let ts = DateTime::parse_from_rfc3339("2026-06-03T10:20:30Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut envelope = Envelope::upsert(
            IndexName::try_new("users").unwrap(),
            "1",
            GenericValue::Map(Default::default()),
            Some(Position(3)),
            ts,
        );
        envelope.sink = Some(SinkName::try_new("audit").unwrap());
        let wire = envelope.map_document(|_| serde_json::json!({ "email": "ada@x.io" }));
        let json = serde_json::to_string(&wire).unwrap();

        #[derive(Debug, PartialEq, serde::Deserialize)]
        struct User {
            email: String,
        }
        let back: Envelope<User> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sink.as_ref().map(AsRef::as_ref), Some("audit"));
        assert_eq!(back.position, Some(Position(3)));
        assert_eq!(back.document.unwrap().email, "ada@x.io");
    }
}
