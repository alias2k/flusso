use serde::{Deserialize, Serialize};

use schema_core::common;

use crate::Field;

use super::Filter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Join {
    pub table: common::TableName,
    #[serde(rename = "type")]
    pub join_type: JoinType,
    pub primary_key: common::ColumnName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign_key: Option<common::ColumnName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through: Option<Through>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<Filter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_by: Option<Vec<OrderBy>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinType {
    OneToOne,
    OneToMany,
    ManyToMany,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Through {
    pub table: common::TableName,
    pub left_key: common::ColumnName,
    pub right_key: common::ColumnName,
}

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
