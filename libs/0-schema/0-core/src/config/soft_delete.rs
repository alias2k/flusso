use serde::Serialize;

use crate::common;

use super::Filter;

/// Tells the engine to treat a row as deleted rather than present, keyed off a
/// mapped [field](SoftDeleteField) or a raw [column](SoftDeleteColumn). The
/// optional `when` narrows it to rows matching a set of filters.
#[derive(Debug, Clone, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftDelete {
    Field(SoftDeleteField),
    Column(SoftDeleteColumn),
}

#[derive(Debug, Clone, Hash, Serialize)]
pub struct SoftDeleteField {
    pub field: common::FieldName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Hash, Serialize)]
pub struct SoftDeleteColumn {
    pub column: common::ColumnName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<Vec<Filter>>,
}
