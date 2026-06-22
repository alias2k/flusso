use serde::{Deserialize, Serialize};

use crate::{Field, common};

use super::Filter;

/// Folds rows from a related `table` into the document. The [`kind`](Self::kind)
/// names the relationship — which side carries the key, and whether one row or
/// many fold in; `filters`, `order_by`, and `limit` narrow and shape the rows
/// that come back.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Join {
    pub table: common::TableName,
    pub kind: JoinKind,
    pub primary_key: common::ColumnName,
    /// Whether the folded-in object may be absent. Only meaningful for a to-one
    /// join (`belongs_to`/`has_one`); a to-many join is always a non-null array.
    /// A to-one join defaults to nullable unless the schema marks it `required`.
    #[serde(default)]
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_by: Option<Vec<OrderBy>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    pub fields: Vec<Field>,
}

/// The relationship a join expresses. The verb carries both the cardinality
/// and — the part that matters — **which table holds the key**:
///
/// - [`BelongsTo`](Self::BelongsTo): *this* row points at the related row
///   (`column` is on the parent table) → a single object.
/// - [`HasOne`](Self::HasOne) / [`HasMany`](Self::HasMany): the related rows
///   point back at this one (`foreign_key` is on the related table) → a single
///   object / a nested array.
/// - [`ManyToMany`](Self::ManyToMany): both sides are keyed through a junction
///   table → a nested array.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinKind {
    BelongsTo { column: common::ColumnName },
    HasOne { foreign_key: common::ColumnName },
    HasMany { foreign_key: common::ColumnName },
    ManyToMany { through: Through },
}

impl JoinKind {
    /// Whether this join folds in many rows (a nested array) rather than one
    /// (an object).
    pub fn is_to_many(&self) -> bool {
        matches!(self, JoinKind::HasMany { .. } | JoinKind::ManyToMany { .. })
    }
}

/// How an aggregate's related rows tie back to the parent — a direct FK on the
/// aggregated table, or a junction table. (Joins carry their key inside
/// [`JoinKind`]; aggregates are inherently over-many, so `belongs_to` has no
/// aggregate counterpart.)
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateKey {
    Direct(common::ColumnName),
    Through(Through),
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
