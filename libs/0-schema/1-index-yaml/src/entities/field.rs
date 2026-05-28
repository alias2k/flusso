use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use schema_core::common;

use super::{Aggregate, Join, Transform};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Field {
    Short(common::FieldName),
    Full(Box<FieldDef>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub field: common::FieldName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<common::ColumnName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping: Option<Mapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<Join>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<Aggregate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transforms: Option<Vec<Transform>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<Field>>,
}

/// OpenSearch mapping. `type` is required; all other properties are passed through as-is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    #[serde(rename = "type")]
    pub mapping_type: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}
