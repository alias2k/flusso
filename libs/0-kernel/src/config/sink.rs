use serde::{Deserialize, Serialize};

use crate::common::{HttpUrl, SinkName};

use super::{ResolveError, Secret, http_url, resolve_optional, resolve_required, sink_var_prefix};

/// Per-backend configuration for an OpenSearch destination. The `Sink` enum that
/// selects between this and [`StdoutSink`] is a composition concern and lives in
/// the `schema` crate; the backend sinks read these settings directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpensearchSink {
    /// Cluster URL, resolved at runtime (`<NAME>_OPENSEARCH_URL` overrides).
    pub url: Secret,
    /// Basic-auth user, resolved at runtime (`<NAME>_OPENSEARCH_USERNAME`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<Secret>,
    /// Basic-auth password, resolved at runtime (`<NAME>_OPENSEARCH_PASSWORD`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Secret>,
    /// Verify TLS certificates. Set false for local dev. Default: true.
    pub tls_verify: bool,
    /// Documents per bulk request. Default: 1000.
    pub batch_size: u32,
    /// Maximum serialized size of a single bulk request, in bytes. A flush is
    /// split so no request exceeds this, independent of `batch_size`, keeping
    /// requests under OpenSearch's `http.max_content_length` (100 MB default).
    /// Default: 10 MiB. A single document larger than this is sent on its own.
    pub max_bytes: u64,
    /// Request timeout in seconds. Default: 30.
    pub timeout_secs: u64,
    /// Transient failure retries. Default: 3.
    pub max_retries: u32,
    /// Optional ingest pipeline applied on index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    /// Primary shards for each created index. Default: 1.
    pub number_of_shards: u32,
    /// Replica shards for each created index. Default: 1.
    pub number_of_replicas: u32,
    /// OpenSearch `refresh_interval` applied to each index once its backfill
    /// completes — the steady-state visibility ceiling (e.g. `"10s"`, `"1s"`,
    /// or `"-1"` to disable automatic refresh). Indexes are seeded with refresh
    /// off (`-1`) and handed this value afterwards. flusso forces an immediate
    /// refresh on any flush that catches the pipeline up, so this only bounds
    /// search staleness while a backlog is draining. Default: `"10s"`.
    pub refresh_interval: String,
    /// Which analysis backend the built-in `flusso_*` analyzers use. Default:
    /// [`Builtin`](TextAnalysis::Builtin).
    pub text_analysis: TextAnalysis,
    /// Whether the sink automatically enriches `text`/`keyword` fields with a
    /// good analyzer and the `keyword` / `keyword_lowercase` / `text` subfields.
    /// A field's explicit mapping always wins. Default: true.
    pub auto_subfields: bool,
}

impl OpensearchSink {
    /// Resolve the cluster URL in the current environment, applying the
    /// `<NAME>_OPENSEARCH_URL` deployment override for the sink named `name`.
    pub fn resolve_url(&self, name: &SinkName) -> Result<HttpUrl, ResolveError> {
        let var = format!("{}_OPENSEARCH_URL", sink_var_prefix(name));
        http_url(resolve_required(&self.url, &var)?)
    }

    /// Resolve the basic-auth username, applying `<NAME>_OPENSEARCH_USERNAME`.
    pub fn resolve_username(&self, name: &SinkName) -> Result<Option<String>, ResolveError> {
        let var = format!("{}_OPENSEARCH_USERNAME", sink_var_prefix(name));
        resolve_optional(self.username.as_ref(), &var)
    }

    /// Resolve the basic-auth password, applying `<NAME>_OPENSEARCH_PASSWORD`.
    pub fn resolve_password(&self, name: &SinkName) -> Result<Option<String>, ResolveError> {
        let var = format!("{}_OPENSEARCH_PASSWORD", sink_var_prefix(name));
        resolve_optional(self.password.as_ref(), &var)
    }
}

/// Which analyzer toolkit the sink wires its `flusso_*` analyzers onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAnalysis {
    /// Built-in OpenSearch components only — works on any cluster with no
    /// plugins. Accent/case folding via `asciifolding` + `lowercase`.
    Builtin,
    /// Use the `analysis-icu` plugin (`icu_tokenizer` / `icu_folding` /
    /// `icu_normalizer`) for stronger multilingual handling. Requires the plugin
    /// to be installed on every node, or index creation fails.
    Icu,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdoutSink {
    /// Pretty-print JSON output. Default: false.
    pub pretty: bool,
}
