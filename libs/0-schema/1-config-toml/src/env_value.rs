use serde::{Deserialize, Serialize};

/// A config value given either literally or as `{ env = "VAR" }`. Parsing keeps
/// the distinction; the core model carries it as a [`Secret`](schema_core::Secret)
/// and resolves it at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvOrValue {
    Env { env: String },
    Value(String),
}
