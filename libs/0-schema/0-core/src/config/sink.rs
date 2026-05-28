#[derive(Debug, Clone)]
pub enum Sink {
    Opensearch(OpensearchSink),
    Stdout(StdoutSink),
}

#[derive(Debug, Clone)]
pub struct OpensearchSink {
    /// Cluster endpoint, e.g. `https://localhost:9200`
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Verify TLS certificates. Set false for local dev. Default: true.
    pub tls_verify: bool,
    /// Documents per bulk request. Default: 1000.
    pub batch_size: u32,
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
