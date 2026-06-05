use serde::{Deserialize, Serialize};

use crate::{Field, common};

use super::Filter;

/// Folds rows from a related `table` into the document. The `key` says how the
/// tables connect; `filters`, `order_by`, and `limit` narrow and shape the
/// rows that come back.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Join {
    pub table: common::TableName,
    pub join_type: JoinType,
    pub primary_key: common::ColumnName,
    pub key: JoinKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_by: Option<Vec<OrderBy>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    pub fields: Vec<Field>,
}

/// How the join condition is expressed — direct FK or through a junction table.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinKey {
    Direct(common::ColumnName),
    Through(Through),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    OneToOne,
    OneToMany,
    ManyToMany,
}

/// A junction table linking two sides of a many-to-many relation.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Through {
    pub table: common::TableName,
    pub left_key: common::ColumnName,
    pub right_key: common::ColumnName,
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct OrderBy {
    pub column: common::ColumnName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    Desc,
}
