use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::common;

use super::{AggregateKey, Filter, FlussoType};

/// Reduces rows from a related `table` to a single value — a count, sum,
/// extreme, or existence flag. The `key` connects the tables; `filters`
/// restrict which rows of `table` count; `via` reaches `table` across the
/// tables in between.
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
    /// The tables between the parent and `table`, in order from the parent.
    /// Empty for a one-hop aggregate. When set, the [`key`](Self::key) points
    /// at the last hop's table instead of the parent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via: Vec<ViaHop>,
}

/// One table an aggregate passes through on its way to the aggregated table: a
/// downward foreign-key hop. `foreign_key` is on `table` and points at the
/// table before it (the parent, for the first hop).
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct ViaHop {
    /// The table this hop passes through.
    pub table: common::TableName,
    /// The column on `table` pointing at the table before it.
    pub foreign_key: common::ColumnName,
    /// Which rows of `table` the path passes through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
}

/// Hashes `distinct` only when set and `via` only when non-empty, so a schema
/// that never opts in keeps the content hash (and so the physical index) it had
/// before the fields existed.
impl Hash for Aggregate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self {
            table,
            op,
            key,
            value_type,
            filters,
            distinct,
            via,
        } = self;
        table.hash(state);
        op.hash(state);
        key.hash(state);
        value_type.hash(state);
        filters.hash(state);
        if *distinct {
            distinct.hash(state);
        }
        if !via.is_empty() {
            "via".hash(state);
            via.hash(state);
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
    /// Whether any related row exists: a non-null `boolean`, `false` over zero
    /// rows. Last, so the derived `Hash` keeps every earlier variant's
    /// discriminant.
    Exists,
}
