use kernel::{ColumnName, GenericValue};

/// A row's primary key, as ordered column/value pairs.
///
/// A `Vec` rather than a single value so composite keys are represented
/// naturally; values reuse [`GenericValue`] from the schema model. Shared
/// vocabulary: [`cdc`](crate::cdc) names the changed row with it, and
/// [`document`](crate::document) uses it as a document's root key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RowKey(pub Vec<(ColumnName, GenericValue)>);

/// A changed row's **pre-image**: its column values before an update or
/// delete, as far as the change feed carried them.
///
/// Used only to find the documents that embedded the row's old version (a
/// deleted or re-parented child). Never used to build a document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RowImage(pub Vec<(ColumnName, GenericValue)>);

impl RowImage {
    /// The old value of `column`, if the pre-image carried it.
    pub fn get(&self, column: &ColumnName) -> Option<&GenericValue> {
        self.0
            .iter()
            .find(|(name, _)| name == column)
            .map(|(_, value)| value)
    }
}
