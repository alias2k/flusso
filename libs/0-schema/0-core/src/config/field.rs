use std::collections::BTreeMap;

use crate::common;

use super::{Aggregate, Join, Transform};

#[derive(Debug, Clone, Hash)]
pub struct Field {
    pub field: common::FieldName,
    pub column: Option<common::ColumnName>,
    pub mapping: Option<Mapping>,
    pub relation: Option<FieldRelation>,
    pub transforms: Option<Vec<Transform>>,
    pub default: Option<common::GenericValue>,
    pub fields: Option<Vec<Field>>,
}

#[derive(Debug, Clone, Hash)]
pub enum FieldRelation {
    Join(Join),
    Aggregate(Aggregate),
}

/// OpenSearch mapping. `type` is required; all other properties are passed through as-is.
#[derive(Debug, Clone, Hash)]
pub struct Mapping {
    pub mapping_type: String,
    pub extra: BTreeMap<String, common::GenericValue>,
}
