use serde::{Deserialize, Serialize};

use crate::common;

use super::Filter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SoftDelete {
    Field(SoftDeleteField),
    Column(SoftDeleteColumn),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftDeleteField {
    pub field: common::FieldName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftDeleteColumn {
    pub column: common::ColumnName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Vec<Filter>>,
}
