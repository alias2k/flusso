/// A destination for built documents: an OpenSearch cluster, or `stdout` for
/// inspecting output during development.
#[derive(Debug, Clone)]
pub enum Sink {
    Opensearch(OpensearchSink),
    Stdout(StdoutSink),
}

#[derive(Debug, Clone)]
pub struct OpensearchSink {
    pub url: crate::common::HttpUrl,
    pub username: Option<String>,
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
}

#[derive(Debug, Clone)]
pub struct StdoutSink {
    /// Pretty-print JSON output. Default: false.
    pub pretty: bool,
}
