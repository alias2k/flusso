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
    /// `- field: name` with a `join:` — a related table folded in. Its presence
    /// of the `join` key distinguishes it; `required` is not allowed (a relation's
    /// nullability is structural, not declared).
    Join(Box<JoinField>),
    /// `- field: name` with an `aggregate:` — a rollup over a related table.
    Aggregate(Box<AggregateField>),
    /// `- field: name` reading a single column. A leaf field must declare
    /// `required` explicitly; the other forms above are matched first by their
    /// `group`/`join`/`aggregate` keys.
    Column(Box<ColumnField>),
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

/// A field whose source is a join. The `join` key is the discriminator. A
/// stray `aggregate` is accepted here only so the conversion can report the
/// conflict; `type` is accepted so the conversion can reject it on a structural
/// field with a clear error rather than an opaque parse failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinField {
    pub field: common::FieldName,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<FlussoType>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_yaml::Value>,
    pub join: Join,
    /// Only present to detect a `join` + `aggregate` conflict during conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<Aggregate>,
}

/// A field whose source is an aggregate over a related table. The `aggregate`
/// key is the discriminator. `type` is required for `sum`/`min`/`max` (their
/// result mirrors the column) and rejected otherwise during conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateField {
    pub field: common::FieldName,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<FlussoType>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_yaml::Value>,
    pub aggregate: Aggregate,
}

/// A leaf field reading a single column. `required` is mandatory: every leaf
/// field states its nullability explicitly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnField {
    pub field: common::FieldName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<common::ColumnName>,
    /// The declared type. Defaults to the `keyword` shorthand when omitted.
    /// `text` is analyzed natural-language full text; `identifier` is analyzed
    /// identifier-style text.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<FlussoType>,
    /// Force the field non-null.
    pub required: bool,
    /// Extra OpenSearch mapping properties merged beside the derived `type`
    /// (e.g. `analyzer`, `scaling_factor`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transforms: Option<Vec<Transform>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_yaml::Value>,
}
