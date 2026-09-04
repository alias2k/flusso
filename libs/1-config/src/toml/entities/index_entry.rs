use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use kernel::common;

/// One `[[index]]` entry: an index to build from a schema file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    /// The index's logical name; the OpenSearch alias and the `flusso-query`
    /// binding use it.
    pub name: common::IndexName,
    /// Path to the index's `*.schema.yml`, relative to the config file.
    pub schema: common::SchemaPath,
    /// Whether this run builds and follows the index. A disabled index keeps
    /// its entry but is neither seeded nor updated.
    pub enabled: bool,
    /// Per-index override of the global [`on_error`](crate::toml::ConfigToml::on_error)
    /// policy. Omitted inherits the global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<kernel::FailurePolicy>,
}
