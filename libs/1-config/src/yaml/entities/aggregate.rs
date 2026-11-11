use serde::{Deserialize, Serialize};

use kernel::common;

use super::{ColumnPair, Filter};

/// The aggregate operation — written as the field's type key
/// (`count:` / `sum:` / `avg:` / `min:` / `max:` / `ids:` / `exists:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Ids,
    Exists,
}

/// One `via` entry of an aggregate: a table between the parent and the
/// aggregated table, whose `foreign_key` points at the table before it.
///
/// `column` and `through` are parsed only to be rejected at conversion with a
/// pointed message: a `belongs_to`-direction hop and a junction in the middle
/// are not supported.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViaHop {
    pub table: common::TableName,
    #[serde(default)]
    pub foreign_key: Option<common::ColumnName>,
    #[serde(default)]
    pub filters: Option<Vec<Filter>>,
    #[serde(default)]
    pub on: Option<Vec<ColumnPair>>,
    #[serde(default)]
    pub column: Option<serde_yaml::Value>,
    #[serde(default)]
    pub through: Option<serde_yaml::Value>,
}
