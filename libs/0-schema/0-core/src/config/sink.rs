use serde::Serialize;

/// A destination for built documents: an OpenSearch cluster, or `stdout` for
/// inspecting output during development.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sink {
    Opensearch(OpensearchSink),
    Stdout(StdoutSink),
}

#[derive(Debug, Clone, Serialize)]
pub struct OpensearchSink {
    pub url: crate::common::HttpUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    // Redacted on serialize — the value is a secret, but its presence is worth
    // showing, so `Some` becomes `"***"` and `None` is omitted.
    #[serde(
        serialize_with = "redact_secret",
        skip_serializing_if = "Option::is_none"
    )]
    pub password: Option<String>,
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
    pub pipeline: Option<String>,
    /// Primary shards for each created index. Default: 1.
    pub number_of_shards: u32,
    /// Replica shards for each created index. Default: 1.
    pub number_of_replicas: u32,
    /// Which analysis backend the built-in `flusso_*` analyzers use. Default:
    /// [`Builtin`](TextAnalysis::Builtin).
    pub text_analysis: TextAnalysis,
    /// Whether the sink automatically enriches `text`/`keyword` fields with a
    /// good analyzer and the `keyword` / `keyword_lowercase` / `text` subfields.
    /// A field's explicit mapping always wins. Default: true.
    pub auto_subfields: bool,
}

/// Which analyzer toolkit the sink wires its `flusso_*` analyzers onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct StdoutSink {
    /// Pretty-print JSON output. Default: false.
    pub pretty: bool,
}

/// Serialize a present secret as `"***"`. Paired with
/// `skip_serializing_if = "Option::is_none"`, so it only ever sees `Some`.
fn redact_secret<S: serde::Serializer>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(_) => serializer.serialize_str("***"),
        None => serializer.serialize_none(),
    }
}
