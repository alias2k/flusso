use serde::{Deserialize, Serialize};

use schema_core::common;

/// The cardinality of a join — written as the field's type key
/// (`one_to_one:` / `one_to_many:` / `many_to_many:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    OneToOne,
    OneToMany,
    ManyToMany,
}

/// A junction table for a `many_to_many` join.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Through {
    pub table: common::TableName,
    pub left_key: common::ColumnName,
    pub right_key: common::ColumnName,
}

/// One `order_by` entry for a to-many join.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderBy {
    pub column: common::ColumnName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    Desc,
}
