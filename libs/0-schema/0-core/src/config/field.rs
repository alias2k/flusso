use std::collections::HashMap;

use crate::common;

use super::{Aggregate, Join, Transform};

#[derive(Debug, Clone)]
pub enum Field {
    Short(common::FieldName),
    Full(Box<FieldDef>),
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub field: common::FieldName,
    pub column: Option<common::ColumnName>,
    pub mapping: Option<Mapping>,
    pub join: Option<Join>,
    pub aggregate: Option<Aggregate>,
    pub transforms: Option<Vec<Transform>>,
    pub default: Option<String>,
    pub fields: Option<Vec<Field>>,
}

/// OpenSearch mapping. `type` is required; all other properties are passed through as-is.
#[derive(Debug, Clone)]
pub struct Mapping {
    pub mapping_type: String,
    pub extra: HashMap<String, String>,
}
