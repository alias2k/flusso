use serde::{Deserialize, Serialize};

use crate::common;

use super::{Filter, FlussoType, JoinKey};

/// Reduces rows from a related `table` to a single value — a count, sum, or
/// extreme. The `key` connects the tables; `filters` restrict which rows count.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Aggregate {
    pub table: common::TableName,
    pub op: AggregateOp,
    pub key: JoinKey,
    /// The declared result type. Fixed for `count` (`long`) and `avg` (`double`)
    /// and left `None`; required for `sum` / `min` / `max`, whose result mirrors
    /// the aggregated column and so must be stated to stay database-free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_type: Option<FlussoType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Count,
    Sum(common::ColumnName),
    Avg(common::ColumnName),
    Min(common::ColumnName),
    Max(common::ColumnName),
}
