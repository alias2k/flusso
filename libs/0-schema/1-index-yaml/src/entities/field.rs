use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use schema_core::FlussoType;
use schema_core::common;

use super::{Aggregate, Join, Transform};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Field {
    /// `- name` — a `keyword` column of the same name.
    Short(common::FieldName),
    /// `- group: name` with nested `fields` — a same-row sub-object. Its own
    /// key (`group`) keeps it unambiguous from the `field:` forms.
    Group(GroupDef),
    /// `- field: name` with a column, join, or aggregate source.
    Full(Box<FieldDef>),
}

/// A same-row sub-object: nested `fields` assembled from the parent row, with no
/// column or related table of its own. Renders as an OpenSearch `object`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDef {
    pub group: common::FieldName,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_yaml::Value>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub field: common::FieldName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<common::ColumnName>,
    /// The declared type. Required for a leaf column (defaults to `keyword`
    /// shorthand when omitted) and for `sum`/`min`/`max` aggregates; rejected on
    /// groups and joins, whose shape is structural. `text` is analyzed
    /// natural-language full text; `identifier` is analyzed identifier-style text.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<FlussoType>,
    /// Force the field non-null. Fields are nullable by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Extra OpenSearch mapping properties merged beside the derived `type`
    /// (e.g. `analyzer`, `scaling_factor`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_yaml::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<Join>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<Aggregate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transforms: Option<Vec<Transform>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_yaml::Value>,
}
