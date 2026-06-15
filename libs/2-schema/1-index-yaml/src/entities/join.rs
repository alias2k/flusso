use serde::{Deserialize, Serialize};

use schema_core::common;

/// The relationship verb of a join — written as the field's type key
/// (`belongs_to:` / `has_one:` / `has_many:` / `many_to_many:`). The verb names
/// which side carries the key: `belongs_to` follows a column on *this* table;
/// `has_one`/`has_many` follow a `foreign_key` on the *related* table;
/// `many_to_many` goes `through` a junction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinVerb {
    BelongsTo,
    HasOne,
    HasMany,
    ManyToMany,
}

impl JoinVerb {
    /// The verb as written in YAML, for error messages.
    pub fn as_str(self) -> &'static str {
        match self {
            JoinVerb::BelongsTo => "belongs_to",
            JoinVerb::HasOne => "has_one",
            JoinVerb::HasMany => "has_many",
            JoinVerb::ManyToMany => "many_to_many",
        }
    }
}

/// A junction table for a `many_to_many` join.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Through {
    pub table: common::TableName,
    pub left_key: common::ColumnName,
    pub right_key: common::ColumnName,
}

/// One `order_by` entry for a to-many join.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderBy {
    pub column: common::ColumnName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    Desc,
}
