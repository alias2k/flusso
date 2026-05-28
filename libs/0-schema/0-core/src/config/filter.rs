use crate::common;

#[derive(Debug, Clone)]
pub enum Filter {
    Raw(RawFilter),
    NullCheck(NullCheckFilter),
    ValueOp(ValueOpFilter),
}

#[derive(Debug, Clone)]
pub struct RawFilter {
    pub raw: common::RawFilterValue,
}

/// `is_null` / `is_not_null` — no value operand.
#[derive(Debug, Clone)]
pub struct NullCheckFilter {
    pub column: common::ColumnName,
    pub op: NullOp,
}

/// All other filter ops — value operand required.
#[derive(Debug, Clone)]
pub struct ValueOpFilter {
    pub column: common::ColumnName,
    pub op: FilterOp,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullOp {
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
