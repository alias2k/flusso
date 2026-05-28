use serde::{Deserialize, Serialize};

use crate::common;

use super::{Filter, Through};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aggregate {
    pub table: common::TableName,
    pub op: AggregateOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<common::ColumnName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign_key: Option<common::ColumnName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through: Option<Through>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}
