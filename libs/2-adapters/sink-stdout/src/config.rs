//! The stdout sink's own configuration: a `[sinks.<name>]` table with
//! `type = "stdout"`.

use kernel::AdapterConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The `[sinks.<name>]` options for `type = "stdout"`.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, AdapterConfig,
)]
#[serde(deny_unknown_fields)]
#[adapter(port = sink, kind = "stdout")]
pub struct StdoutConfig {
    /// Pretty-print each envelope over several lines instead of one compact
    /// JSON line per operation.
    #[serde(default)]
    pub pretty: bool,
}
