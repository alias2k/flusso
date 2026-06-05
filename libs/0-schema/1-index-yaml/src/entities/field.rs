use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use schema_core::FlussoType;
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
    /// The declared type. Required for a leaf column (defaults to `keyword`
    /// shorthand when omitted) and for `sum`/`min`/`max` aggregates; rejected on
    /// groups and joins, whose shape is structural.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<FlussoType>,
    /// Force the field non-null. Fields are nullable by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Extra OpenSearch mapping properties merged beside the derived `type`
    /// (e.g. `analyzer`, `scaling_factor`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_yaml::Value>,
    /// Shorthand for full-text text fields. Sugar for `type: text` plus the
    /// matching `flusso_*` analyzer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FieldKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<Join>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<Aggregate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transforms: Option<Vec<Transform>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_yaml::Value>,
}

/// Full-text shorthand for a text field: `code` (the `flusso_code` analyzer,
/// for identifier-like short text) or `prose` (the `flusso_text` analyzer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Code,
    Prose,
}
