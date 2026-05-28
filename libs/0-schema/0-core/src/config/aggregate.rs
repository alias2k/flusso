use crate::common;

use super::{Filter, JoinKey};

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
