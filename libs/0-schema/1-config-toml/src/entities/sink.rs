use serde::{Deserialize, Serialize};

use crate::EnvOrValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Sink {
    Opensearch(OpensearchSink),
    Stdout(StdoutSink),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpensearchSink {
    pub url: EnvOrValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<EnvOrValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<EnvOrValue>,
    #[serde(default = "default_tls_verify")]
    pub tls_verify: bool,
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StdoutSink {
    #[serde(default)]
    pub pretty: bool,
}

fn default_tls_verify() -> bool {
    true
}

fn default_batch_size() -> u32 {
    1000
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_max_retries() -> u32 {
    3
}
