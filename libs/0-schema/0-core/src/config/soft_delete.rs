use crate::common;

use super::Filter;

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
