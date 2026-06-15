use serde::{Deserialize, Serialize};

use crate::common;

/// A condition on which rows a join or aggregate sees. Either a structured
/// comparison ([`NullCheckFilter`], [`ValueOpFilter`]) or a [`RawFilter`] of
/// verbatim SQL for cases the structured forms don't cover.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    Raw(RawFilter),
    NullCheck(NullCheckFilter),
    ValueOp(ValueOpFilter),
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct RawFilter {
    pub raw: common::RawFilterValue,
}

/// `is_null` / `is_not_null` — no value operand.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct NullCheckFilter {
    pub column: common::ColumnName,
    pub op: NullOp,
}

/// All other filter ops — value operand matches the operator's arity.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct ValueOpFilter {
    pub column: common::ColumnName,
    pub op: FilterOp,
    pub value: FilterValue,
}

/// Typed filter value — arity matches the operator.
/// `In`/`NotIn` → `List`, `Between` → `Range`, everything else → `Single`.
#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterValue {
    Single(String),
    List(Vec<String>),
    Range(String, String),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NullOp {
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    In,
    NotIn,
    Like,
    Ilike,
    Between,
}
