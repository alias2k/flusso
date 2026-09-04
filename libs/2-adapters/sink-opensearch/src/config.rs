//! The OpenSearch sink's own configuration: a `[sinks.<name>]` table with
//! `type = "opensearch"`.
//!
//! [`OpensearchConfig`] is what the composition root deserializes the entry's
//! options into ([`kernel::AdapterConfig`]). The URL and credentials are
//! [`Secret`]s, resolved at run time with the sink's override variables
//! (`<NAME>_OPENSEARCH_URL`, `_USERNAME`, `_PASSWORD`).
//!
//! ```
//! use kernel::{AdapterConfig, Options, Secret};
//! use sink_opensearch::{OpensearchConfig, TextAnalysis};
//!
//! let options: Options = toml::from_str(r#"
//!     url = "https://search:9200"
//!     password = { env = "OS_PASSWORD" }
//!     text_analysis = "icu"
//! "#).unwrap();
//! let config = OpensearchConfig::from_options(options).unwrap();
//! assert_eq!(config.url, Secret::Value("https://search:9200".into()));
//! assert_eq!(config.batch_size, 1000);
//! assert_eq!(config.text_analysis, TextAnalysis::Icu);
//! ```

use kernel::{AdapterConfig, HttpUrl, ResolveError, Secret, SinkName, override_var};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The `[sinks.<name>]` options for `type = "opensearch"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[serde(deny_unknown_fields)]
#[adapter(port = sink, kind = "opensearch")]
pub struct OpensearchConfig {
    /// Base URL of the cluster, literal or `{ env = "VAR" }`.
    #[adapter(example = "https://search.example.com:9200")]
    pub url: Secret,
    /// HTTP Basic auth user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[adapter(example = "indexer")]
    pub username: Option<Secret>,
    /// HTTP Basic auth password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[adapter(example = Secret::env("OPENSEARCH_PASSWORD"))]
    pub password: Option<Secret>,
    /// Verify TLS certificates. Set `false` for a local cluster with a
    /// self-signed certificate.
    #[serde(default = "default_tls_verify")]
    pub tls_verify: bool,
    /// Documents per bulk request.
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,
    /// Maximum serialized size of one bulk request, in bytes. A flush is split
    /// so no request exceeds this, independent of `batch_size`, keeping
    /// requests under OpenSearch's `http.max_content_length` (100 MB by
    /// default). A single document larger than this is sent on its own.
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    /// Request timeout, in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Retries for a transient failure of a whole request.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// An ingest pipeline applied to every indexed document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[adapter(example = "enrich")]
    pub pipeline: Option<String>,
    /// Primary shards for each index flusso creates.
    #[serde(default = "default_number_of_shards")]
    pub number_of_shards: u32,
    /// Replica shards for each index flusso creates.
    #[serde(default = "default_number_of_replicas")]
    pub number_of_replicas: u32,
    /// The `refresh_interval` applied to each index once its backfill completes:
    /// the steady-state visibility ceiling (`"10s"`, `"1s"`, or `"-1"` to
    /// disable automatic refresh). Indexes are seeded with refresh off and
    /// handed this value afterwards; a flush that catches the pipeline up
    /// forces an immediate refresh, so this only bounds staleness while a
    /// backlog drains.
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: String,
    /// Which analysis toolkit the built-in `flusso_*` analyzers use.
    #[serde(default)]
    pub text_analysis: TextAnalysis,
    /// Whether `text`/`keyword` fields are enriched with a good analyzer and the
    /// `keyword` / `keyword_lowercase` / `text` subfields. A field's explicit
    /// mapping always wins.
    #[serde(default = "default_auto_subfields")]
    pub auto_subfields: bool,
}

fn default_tls_verify() -> bool {
    true
}

fn default_batch_size() -> u32 {
    1000
}

/// 10 MiB: within OpenSearch's recommended 5–15 MB bulk range and well under the
/// 100 MB `http.max_content_length` default.
fn default_max_bytes() -> u64 {
    10 * 1024 * 1024
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_max_retries() -> u32 {
    3
}

fn default_number_of_shards() -> u32 {
    1
}

fn default_number_of_replicas() -> u32 {
    1
}

/// A 10s steady-state ceiling: under sustained backlog documents are visible
/// within 10s while bulk indexing stays cheap.
fn default_refresh_interval() -> String {
    "10s".to_owned()
}

fn default_auto_subfields() -> bool {
    true
}

impl OpensearchConfig {
    /// Resolve the cluster URL in the current environment, applying
    /// `<NAME>_OPENSEARCH_URL` for the sink named `name`.
    pub fn resolve_url(&self, name: &SinkName) -> Result<HttpUrl, ResolveError> {
        let value = self.url.resolve(&self.var(name, "url"))?;
        HttpUrl::try_new(value).map_err(|e| ResolveError::Invalid(format!("invalid HTTP URL: {e}")))
    }

    /// Resolve the basic-auth username, applying `<NAME>_OPENSEARCH_USERNAME`.
    pub fn resolve_username(&self, name: &SinkName) -> Result<Option<String>, ResolveError> {
        Secret::resolve_optional(self.username.as_ref(), &self.var(name, "username"))
    }

    /// Resolve the basic-auth password, applying `<NAME>_OPENSEARCH_PASSWORD`.
    pub fn resolve_password(&self, name: &SinkName) -> Result<Option<String>, ResolveError> {
        Secret::resolve_optional(self.password.as_ref(), &self.var(name, "password"))
    }

    fn var(&self, name: &SinkName, field: &str) -> String {
        override_var(name.as_ref(), Self::KIND, field)
    }
}

/// Which analyzer toolkit the sink wires its `flusso_*` analyzers onto.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextAnalysis {
    /// Built-in OpenSearch components only: works on any cluster with no
    /// plugins. Accent and case folding via `asciifolding` + `lowercase`.
    #[default]
    Builtin,
    /// The `analysis-icu` plugin (`icu_tokenizer` / `icu_folding` /
    /// `icu_normalizer`) for stronger multilingual handling. Requires the plugin
    /// on every node, or index creation fails.
    Icu,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use kernel::Options;

    #[test]
    fn missing_url_is_named() {
        let options: Options = toml::from_str("batch_size = 5").unwrap();
        let error = OpensearchConfig::from_options(options)
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing field `url`"), "{error}");
    }

    #[test]
    fn example_round_trips_and_describes_secrets() {
        let example = OpensearchConfig::example();
        let options = Options::from_serialize(&example).unwrap();
        assert_eq!(OpensearchConfig::from_options(options).unwrap(), example);
        let description = OpensearchConfig::description();
        assert_eq!(description.secrets, vec!["password", "url", "username"]);
        let vars: Vec<String> = description.override_vars("primary").collect();
        assert_eq!(
            vars,
            [
                "PRIMARY_OPENSEARCH_PASSWORD",
                "PRIMARY_OPENSEARCH_URL",
                "PRIMARY_OPENSEARCH_USERNAME"
            ]
        );
    }
}
