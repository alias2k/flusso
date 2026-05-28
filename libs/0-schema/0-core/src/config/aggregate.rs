use crate::common;

use super::{Filter, Through};

#[derive(Debug, Clone)]
pub struct Aggregate {
    pub table: common::TableName,
    pub op: AggregateOp,
    pub column: Option<common::ColumnName>,
    pub foreign_key: Option<common::ColumnName>,
    pub through: Option<Through>,
    pub filters: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}
