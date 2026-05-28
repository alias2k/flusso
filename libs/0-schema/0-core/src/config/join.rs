use crate::common;

use super::Filter;

#[derive(Debug, Clone)]
pub struct Join {
    pub table: common::TableName,
    pub join_type: JoinType,
    pub foreign_key: Option<common::ColumnName>,
    pub through: Option<Through>,
    pub filters: Option<Vec<Filter>>,
    pub order_by: Option<Vec<OrderBy>>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    OneToOne,
    OneToMany,
    ManyToMany,
}

#[derive(Debug, Clone)]
pub struct Through {
    pub table: common::TableName,
    pub left_key: common::ColumnName,
    pub right_key: common::ColumnName,
}

#[derive(Debug, Clone)]
pub struct OrderBy {
    pub column: common::ColumnName,
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}
