use crate::common;

use super::Filter;

#[derive(Debug, Clone, Hash)]
pub struct Join {
    pub table: common::TableName,
    pub join_type: JoinType,
    pub key: JoinKey,
    pub filters: Option<Vec<Filter>>,
    pub order_by: Option<Vec<OrderBy>>,
    pub limit: Option<u64>,
}

/// How the join condition is expressed — direct FK or through a junction table.
#[derive(Debug, Clone, Hash)]
pub enum JoinKey {
    Direct(common::ColumnName),
    Through(Through),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum JoinType {
    OneToOne,
    OneToMany,
    ManyToMany,
}

#[derive(Debug, Clone, Hash)]
pub struct Through {
    pub table: common::TableName,
    pub left_key: common::ColumnName,
    pub right_key: common::ColumnName,
}

#[derive(Debug, Clone, Hash)]
pub struct OrderBy {
    pub column: common::ColumnName,
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}
