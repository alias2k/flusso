//! The address of one document: which index, and its root row's key. Shared
//! by the source (which builds it), the engine (which carries a build's ids
//! with the items it publishes) and a sink engine (which hands them back to
//! have a parked item's documents built again).
//!
//! ```
//! use kernel::{ColumnName, DocumentId, GenericValue, IndexName, RowKey};
//!
//! let id = DocumentId {
//!     index: IndexName::try_new("orders").unwrap(),
//!     key: RowKey(vec![(ColumnName::try_new("id").unwrap(), GenericValue::BigInt(7))]),
//! };
//! assert_eq!(id.rendered(), "7");
//! ```

use crate::{GenericValue, IndexName, RowKey};

/// Addresses one document in a target index: which index, and the root row's
/// key within it. The same source row can map to documents in several indexes,
/// so the [`index`](Self::index) is part of the identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentId {
    pub index: IndexName,
    /// The root table's primary key — the natural identifier of the document.
    pub key: RowKey,
}

impl DocumentId {
    /// The key as the sink's document `_id`: each part as text, composite keys
    /// joined with `:`.
    pub fn rendered(&self) -> String {
        self.key
            .0
            .iter()
            .map(|(_, value)| key_part(value))
            .collect::<Vec<_>>()
            .join(":")
    }
}

fn key_part(value: &GenericValue) -> String {
    match value {
        GenericValue::Bool(b) => b.to_string(),
        GenericValue::SmallInt(i) => i.to_string(),
        GenericValue::Int(i) => i.to_string(),
        GenericValue::BigInt(i) => i.to_string(),
        GenericValue::Float(f) => f.to_string(),
        GenericValue::Double(f) => f.to_string(),
        GenericValue::Decimal(d) => d.to_string(),
        GenericValue::BigDecimal(d) => d.as_str().to_owned(),
        GenericValue::String(s) => s.clone(),
        GenericValue::Uuid(u) => u.to_string(),
        GenericValue::Date(d) => d.to_string(),
        GenericValue::Time(t) => t.to_string(),
        GenericValue::Timestamp(ts) => ts.to_string(),
        GenericValue::TimestampTz(ts) => ts.to_rfc3339(),
        // `\x`-prefixed lowercase hex, matching Postgres's `bytea` text output,
        // so a snapshot key and a WAL key for the same row agree.
        GenericValue::Bytes(bytes) => {
            let mut out = String::with_capacity(2 + bytes.len() * 2);
            out.push_str("\\x");
            for byte in bytes {
                out.push_str(&format!("{byte:02x}"));
            }
            out
        }
        GenericValue::Null => "null".to_owned(),
        GenericValue::Array(_) | GenericValue::Map(_) => String::new(),
    }
}
