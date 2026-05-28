use crate::common;

#[derive(Debug, Clone, Hash)]
pub enum Filter {
    Raw(RawFilter),
    NullCheck(NullCheckFilter),
    ValueOp(ValueOpFilter),
}

#[derive(Debug, Clone, Hash)]
pub struct RawFilter {
    pub raw: common::RawFilterValue,
}

/// `is_null` / `is_not_null` — no value operand.
#[derive(Debug, Clone, Hash)]
pub struct NullCheckFilter {
    pub column: common::ColumnName,
    pub op: NullOp,
}

/// All other filter ops — value operand matches the operator's arity.
#[derive(Debug, Clone, Hash)]
pub struct ValueOpFilter {
    pub column: common::ColumnName,
    pub op: FilterOp,
    pub value: FilterValue,
}

/// Typed filter value — arity matches the operator.
/// `In`/`NotIn` → `List`, `Between` → `Range`, everything else → `Single`.
#[derive(Debug, Clone, Hash)]
pub enum FilterValue {
    Single(String),
    List(Vec<String>),
    Range(String, String),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum NullOp {
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
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
