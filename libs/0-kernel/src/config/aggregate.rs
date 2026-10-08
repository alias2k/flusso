use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::common;

use super::{AggregateKey, Filter, FlussoType};

/// Reduces rows from a related `table` to a single value — a count, sum, or
/// extreme. The `key` connects the tables; `filters` restrict which rows of
/// `table` count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregate {
    pub table: common::TableName,
    pub op: AggregateOp,
    pub key: AggregateKey,
    /// The declared result type. Fixed for `count` (`long`) and `avg` (`double`)
    /// and left `None`; required for `sum` / `min` / `max`, whose result mirrors
    /// the aggregated column and so must be stated to stay database-free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_type: Option<FlussoType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
    /// Over a junction: count each target row once instead of once per
    /// junction row. Only valid with an [`AggregateKey::Through`] key.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub distinct: bool,
}

/// Hashes `distinct` only when set, so a schema that never opts in keeps the
/// content hash (and so the physical index) it had before the field existed.
impl Hash for Aggregate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.table.hash(state);
        self.op.hash(state);
        self.key.hash(state);
        self.value_type.hash(state);
        self.filters.hash(state);
        if self.distinct {
            self.distinct.hash(state);
        }
    }
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateOp {
    Count,
    Sum(common::ColumnName),
    Avg(common::ColumnName),
    Min(common::ColumnName),
    Max(common::ColumnName),
    /// Collect the related table's primary keys into a flat scalar array. The
    /// element type is stated explicitly (the schema names it) so the array's
    /// mapping is known without touching the database.
    Ids {
        element_type: FlussoType,
    },
}
