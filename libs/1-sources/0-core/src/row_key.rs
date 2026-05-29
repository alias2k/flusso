use schema_core::{ColumnName, GenericValue};

/// A row's primary key, as ordered column/value pairs.
///
/// A `Vec` rather than a single value so composite keys are represented
/// naturally; values reuse [`GenericValue`] from the schema model. Shared
/// vocabulary: [`cdc`](crate::cdc) names the changed row with it, and
/// [`document`](crate::document) uses it as a document's root key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RowKey(pub Vec<(ColumnName, GenericValue)>);
