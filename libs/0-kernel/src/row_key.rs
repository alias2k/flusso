//! A row's primary key: what names a changed row and a document's root
//! row, from the source through the stream (a lane item carries the keys
//! of its documents, so a sink can ask for them to be built again).
//!
//! ```
//! use kernel::{ColumnName, GenericValue, RowKey};
//!
//! let key = RowKey::from_text(ColumnName::try_new("id").unwrap(), "42");
//! assert_eq!(key.0[0].1, GenericValue::BigInt(42));
//! assert!(!key.has_null());
//! ```

use crate::{ColumnName, GenericValue};

/// A row's primary key, as ordered column/value pairs.
///
/// A `Vec` rather than a single value so composite keys are represented
/// naturally; values reuse [`GenericValue`] from the schema model. Shared
/// vocabulary: the source port's change capture names the changed row with it,
/// its document builder uses it as a document's root key, and a lane item
/// carries the keys of its documents.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RowKey(pub Vec<(ColumnName, GenericValue)>);

impl RowKey {
    /// Whether some part of the key is null. Such a key matches no stored row
    /// by equality, so it addresses no document. Two ways to get one: a table
    /// with a primary key whose change feed left the key out, or a table
    /// without one, keyed by its replica identity's columns (every column
    /// under `FULL`), where a null is ordinary row data.
    pub fn has_null(&self) -> bool {
        self.0
            .iter()
            .any(|(_, value)| matches!(value, GenericValue::Null))
    }

    /// A one-column key from text a person typed, typed the way the database
    /// most likely holds it: an integer, else a uuid, else text.
    pub fn from_text(column: ColumnName, text: &str) -> Self {
        let value = if let Ok(int) = text.parse::<i64>() {
            GenericValue::BigInt(int)
        } else if let Ok(uuid) = uuid::Uuid::parse_str(text) {
            GenericValue::Uuid(uuid)
        } else {
            GenericValue::String(text.to_owned())
        };
        Self(vec![(column, value)])
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_typed_key_is_an_integer_a_uuid_or_text() {
        let id = ColumnName::try_new("id").unwrap();
        let value = |text| RowKey::from_text(id.clone(), text).0.remove(0).1;
        assert_eq!(value("42"), GenericValue::BigInt(42));
        assert!(matches!(
            value("00000000-0000-0000-0000-000000000000"),
            GenericValue::Uuid(_)
        ));
        assert_eq!(value("sku-1"), GenericValue::String("sku-1".to_owned()));
    }
}
