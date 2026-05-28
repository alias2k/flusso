use serde::{Deserialize, Serialize};

use schema_core::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Filter {
    Raw(RawFilter),
    NullCheck(NullCheckFilter),
    ValueOp(ValueOpFilter),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFilter {
    pub raw: common::RawFilterValue,
}

/// `is_null` / `is_not_null` — no value operand.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NullCheckFilter {
    pub column: common::ColumnName,
    pub op: NullOp,
}

/// All other filter ops — value operand required.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueOpFilter {
    pub column: common::ColumnName,
    pub op: FilterOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NullOp {
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
