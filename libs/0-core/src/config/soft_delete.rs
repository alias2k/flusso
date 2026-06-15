use serde::{Deserialize, Serialize};

use crate::common;

use super::Filter;

/// Tells the engine to treat a row as deleted rather than present, keyed off a
/// mapped [field](SoftDeleteField) or a raw [column](SoftDeleteColumn). The
/// optional `when` narrows it to rows matching a set of filters.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftDelete {
    Field(SoftDeleteField),
    Column(SoftDeleteColumn),
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct SoftDeleteField {
    pub field: common::FieldName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct SoftDeleteColumn {
    pub column: common::ColumnName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Vec<Filter>>,
}
