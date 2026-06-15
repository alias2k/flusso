use serde::{Deserialize, Serialize};

use crate::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    pub name: common::IndexName,
    pub schema: common::SchemaPath,
    pub enabled: bool,
    /// Per-index override of the global [`on_error`](crate::ConfigToml::on_error)
    /// policy. Omitted inherits the global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<schema_core::FailurePolicy>,
}
