use crate::common;

use super::{Filter, JoinKey};

/// Reduces rows from a related `table` to a single value — a count, sum, or
/// extreme. The `key` connects the tables; `filters` restrict which rows count.
#[derive(Debug, Clone, Hash)]
pub struct Aggregate {
    pub table: common::TableName,
    pub op: AggregateOp,
    pub key: JoinKey,
    pub filters: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Hash)]
pub enum AggregateOp {
    Count,
    Sum(common::ColumnName),
    Avg(common::ColumnName),
    Min(common::ColumnName),
    Max(common::ColumnName),
}
