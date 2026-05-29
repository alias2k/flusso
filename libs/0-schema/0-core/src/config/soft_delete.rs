use crate::common;

use super::Filter;

/// Tells the engine to treat a row as deleted rather than present, keyed off a
/// mapped [field](SoftDeleteField) or a raw [column](SoftDeleteColumn). The
/// optional `when` narrows it to rows matching a set of filters.
#[derive(Debug, Clone, Hash)]
pub enum SoftDelete {
    Field(SoftDeleteField),
    Column(SoftDeleteColumn),
}

#[derive(Debug, Clone, Hash)]
pub struct SoftDeleteField {
    pub field: common::FieldName,
    pub when: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Hash)]
pub struct SoftDeleteColumn {
    pub column: common::ColumnName,
    pub when: Option<Vec<Filter>>,
}
