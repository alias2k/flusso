#![doc = include_str!("../README.md")]
// Benchmarks (in `benches/`) pull dev-dependencies the unit-test build doesn't
// touch; allow that only under `cfg(test)` — the normal build still enforces
// unused dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod bulk;
mod config;
mod generations;
mod mapping;
mod settings;
mod sink_impl;
mod transport;

pub use config::{OpensearchConfig, OpensearchIndexConfig, TextAnalysis};
pub use settings::IndexSettings;

/// Benchmark entry point: render `envelopes` into the NDJSON bulk body exactly
/// as `apply` (document → JSON) and `flush` (action → fragment) would, without
/// a cluster. Used by the `render` bench; gated behind the `bench` feature and
/// not part of the stable API.
#[cfg(feature = "bench")]
pub fn bench_render_bulk(envelopes: &[kernel::Envelope]) -> Result<String> {
    let mut body = String::new();
    for envelope in envelopes {
        let action = match (envelope.op, &envelope.document) {
            (kernel::Op::Upsert, Some(document)) => BulkAction::Index {
                index: envelope.index.to_string(),
                id: envelope.id.clone(),
                doc: sink::to_json(document),
                require_alias: false,
            },
            (kernel::Op::Upsert, None) => {
                return Err(SinkError::Write(format!(
                    "upsert of {}/{} carries no document",
                    envelope.index, envelope.id
                )));
            }
            (kernel::Op::Delete, _) => BulkAction::Delete {
                index: envelope.index.to_string(),
                id: envelope.id.clone(),
            },
        };
        if bulk::refused_id(&action).is_some() {
            continue;
        }
        body.push_str(&bulk::bulk_action_fragment(&action)?);
    }
    Ok(body)
}

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex as SyncMutex, PoisonError};
use std::time::Duration;

use kernel::SinkName;
use sink::{Result, SinkError};
use tokio::sync::Mutex;

use crate::bulk::BulkAction;
use crate::mapping::IndexOptions;

// In scope only so the rustdoc links below to `Sink`'s methods (`ensure_index`,
// `flush`, `upsert`, …) resolve — the trait is implemented over in `sink_impl`, not
// used by name in this file.
#[allow(unused_imports)]
use sink::Sink;

/// OpenSearch index that persists seeding markers.
pub(crate) const META_INDEX: &str = "flusso_meta";

/// Writes document operations to an OpenSearch cluster using the bulk API.
///
/// Each [`apply`](OpensearchSink::apply) appends the envelope's operation to an
/// in-memory buffer; [`flush`](OpensearchSink::flush) drains it as one or more bulk
/// requests. Every index is addressed by its schema hash, learned at
/// [`ensure_index`](OpensearchSink::ensure_index), so a structural schema
/// change writes to a fresh index instead of the old one. A seeded index is
/// written through its hash alias (with `require_alias`), an unseeded one
/// through its concrete generation.
#[derive(Debug, Clone)]
pub struct OpensearchSink {
    pub(crate) client: reqwest::Client,
    pub(crate) base_url: String,
    pub(crate) auth: Option<(String, String)>,
    pub(crate) batch_size: usize,
    /// Maximum serialized bytes per bulk request — a flush is split so no
    /// request exceeds this, keeping it under OpenSearch's
    /// `http.max_content_length`.
    pub(crate) max_bytes: usize,
    pub(crate) max_retries: u32,
    pub(crate) pipeline: Option<String>,
    /// `refresh_interval` handed to each index once seeded — the steady-state
    /// visibility ceiling (see [`flush`](OpensearchSink::flush) for how a caught-up flush
    /// forces an immediate refresh on top of this).
    pub(crate) refresh_interval: String,
    /// Settings that shape every index this sink creates: shard counts, the
    /// analysis backend, and whether `text`/`keyword` fields are auto-enriched.
    pub(crate) index_options: IndexOptions,
    /// Logical index name → that index's own `index_settings`, laid over the
    /// sink-wide ones when its generation is created. Set with
    /// [`with_index_settings`](OpensearchSink::with_index_settings).
    pub(crate) per_index_settings: Arc<HashMap<kernel::IndexName, IndexSettings>>,
    /// Literal prefix prepended to every name this sink owns — the hash alias,
    /// its generations, the `{logical}` convenience alias, and the meta index —
    /// so several deployments can share one cluster without colliding. Empty by
    /// default (no prefix). Set with [`with_index_prefix`](OpensearchSink::with_index_prefix).
    pub(crate) index_prefix: String,
    /// In-flight operations, shared across clones.
    pub(crate) buffer: Arc<Mutex<Vec<BulkAction>>>,
    /// Indexes written by a flush that forced no refresh while automatic
    /// refresh is off (`refresh_interval = "-1"`): nothing else will make them
    /// searchable, so the next caught-up flush refreshes them. Empty whenever a
    /// refresh interval is set. Shared across clones.
    pub(crate) unrefreshed: Arc<SyncMutex<BTreeSet<String>>>,
    /// Logical index name → where it lives and where its writes go, learned
    /// from [`ensure_index`](OpensearchSink::ensure_index) and updated by
    /// [`mark_seeded`](OpensearchSink::mark_seeded). Shared across clones.
    pub(crate) targets: Arc<SyncMutex<HashMap<String, IndexTarget>>>,
}

/// Where one logical index lives, and so where its bulk writes are addressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexTarget {
    /// The active generation `{hash_alias}_{n}`.
    pub(crate) generation: String,
    /// The hash alias `{prefix}{logical}_{hash}` the generation sits behind.
    pub(crate) hash_alias: String,
    /// Whether the active generation is seeded.
    pub(crate) seeded: bool,
}

impl IndexTarget {
    /// The bulk `_index` for a write, and whether it must already be an alias
    /// (`require_alias`): the hash alias once seeded, else the generation,
    /// since the alias may still serve the previous one during a rebuild.
    pub(crate) fn write_target(&self) -> (&str, bool) {
        if self.seeded {
            (&self.hash_alias, true)
        } else {
            (&self.generation, false)
        }
    }
}

/// The Mozilla root bundle, which the sink has always verified against: reqwest
/// 0.13 would otherwise read the OS trust store.
fn mozilla_roots() -> impl Iterator<Item = reqwest::Certificate> {
    webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|der| reqwest::Certificate::from_der(der).ok())
}

/// The certificates in the PEM bundle at `path`, which must hold at least one.
fn ca_file_roots(path: &Path) -> Result<Vec<reqwest::Certificate>> {
    let shown = path.display();
    let pem = std::fs::read(path)
        .map_err(|e| SinkError::Write(format!("reading ca_file {shown}: {e}")))?;
    let certificates = reqwest::Certificate::from_pem_bundle(&pem)
        .map_err(|e| transport::request_error(&format!("parsing ca_file {shown}"), e))?;
    if certificates.is_empty() {
        return Err(SinkError::Write(format!(
            "ca_file {shown} holds no PEM certificate (`-----BEGIN CERTIFICATE-----`)"
        )));
    }
    Ok(certificates)
}

impl OpensearchSink {
    /// Build a sink from the schema's OpenSearch sink configuration. The
    /// connection URL and credentials are resolved here, in the running
    /// environment, applying the `<NAME>_OPENSEARCH_*` deployment overrides for
    /// the sink named `name`.
    pub fn from_config(name: &SinkName, config: &OpensearchConfig) -> Result<Self> {
        let private_roots = match &config.ca_file {
            Some(path) => ca_file_roots(path)?,
            None => Vec::new(),
        };
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .tls_certs_only(mozilla_roots().chain(private_roots));

        if !config.tls_verify {
            if config.ca_file.is_some() {
                tracing::warn!(
                    sink = %name,
                    "tls_verify = false skips certificate verification, so ca_file has no effect"
                );
            }
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build().map_err(|e| {
            let context = match &config.ca_file {
                Some(path) => format!(
                    "failed to build HTTP client with ca_file {}",
                    path.display()
                ),
                None => "failed to build HTTP client".to_owned(),
            };
            transport::request_error(&context, e)
        })?;

        let url = config
            .resolve_url(name)
            .map_err(|e| SinkError::Write(format!("resolving OpenSearch URL: {e}")))?;
        let username = config
            .resolve_username(name)
            .map_err(|e| SinkError::Write(format!("resolving OpenSearch username: {e}")))?;
        let password = config
            .resolve_password(name)
            .map_err(|e| SinkError::Write(format!("resolving OpenSearch password: {e}")))?;

        let auth = match (username, password) {
            (Some(u), Some(p)) => Some((u, p)),
            (Some(u), None) => Some((u, String::new())),
            _ => None,
        };

        Ok(Self {
            client,
            base_url: url.as_ref().trim_end_matches('/').to_owned(),
            auth,
            // `chunks(0)` panics, so a zero batch size would crash the first
            // non-empty flush; clamp it to at least one document per request.
            batch_size: (config.batch_size as usize).max(1),
            // At least one byte so the byte cap can never wedge a flush; a doc
            // larger than the cap is still sent (alone, with a warning).
            max_bytes: (config.max_bytes as usize).max(1),
            max_retries: config.max_retries,
            pipeline: config.pipeline.clone(),
            refresh_interval: config.refresh_interval.clone(),
            index_options: IndexOptions {
                // At least one primary shard — zero is not a valid index.
                number_of_shards: config.number_of_shards.max(1),
                number_of_replicas: config.number_of_replicas,
                text_analysis: config.text_analysis,
                auto_subfields: config.auto_subfields,
                settings: config.index_settings.clone(),
            },
            per_index_settings: Arc::new(HashMap::new()),
            index_prefix: String::new(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            unrefreshed: Arc::new(SyncMutex::new(BTreeSet::new())),
            targets: Arc::new(SyncMutex::new(HashMap::new())),
        })
    }

    /// Set the literal prefix prepended to every name this sink owns. Empty
    /// (the default) means no prefix. The matching `flusso-query` client must
    /// be given the same prefix to read the indexes back.
    #[must_use]
    pub fn with_index_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.index_prefix = prefix.into();
        self
    }

    /// Set each index's own `index_settings` (an `[[index]]` entry's
    /// `[index.sinks.<name>]` table), by logical index name. They are laid over
    /// the sink-wide `index_settings` when that index's generation is created.
    #[must_use]
    pub fn with_index_settings(
        mut self,
        settings: impl IntoIterator<Item = (kernel::IndexName, IndexSettings)>,
    ) -> Self {
        self.per_index_settings = Arc::new(settings.into_iter().collect());
        self
    }

    /// The index options for creating a generation of `logical`: the sink's,
    /// with that index's own settings laid over the sink-wide ones.
    pub(crate) fn index_options_for(&self, logical: &kernel::IndexName) -> IndexOptions {
        let mut options = self.index_options.clone();
        if let Some(own) = self.per_index_settings.get(logical) {
            options.settings = options.settings.layered(own);
        }
        options
    }

    /// How often OpenSearch refreshes a seeded index by itself: the
    /// configured `refresh_interval`. `None` when automatic refresh is off, or
    /// the value is not a duration this sink can read (which must not be
    /// reported as sooner than it is).
    pub(crate) fn refresh_period(&self) -> Option<Duration> {
        parse_time_value(&self.refresh_interval)
    }

    /// The stable hash alias `{prefix}{logical}_{hash}` an index is addressed
    /// by — the load-bearing name the generations sit behind. Prefixing here is
    /// the single chokepoint: generations (`hash_alias_*`) and the meta doc key
    /// all derive from this.
    pub(crate) fn hash_alias(&self, logical: &str, hash: &str) -> String {
        format!("{}{logical}_{hash}", self.index_prefix)
    }

    /// The `{prefix}{logical}` convenience alias kept on the live generation.
    pub(crate) fn convenience_alias(&self, logical: &str) -> String {
        format!("{}{logical}", self.index_prefix)
    }

    /// The meta index name, `{prefix}flusso_meta` — prefixed too, so two
    /// prefixed deployments on one cluster keep independent seed/generation
    /// state.
    pub(crate) fn meta_index(&self) -> String {
        format!("{}{META_INDEX}", self.index_prefix)
    }

    /// Apply basic auth to a request builder if credentials are configured.
    pub(crate) fn maybe_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Some((user, pass)) => req.basic_auth(user, Some(pass)),
            None => req,
        }
    }

    /// What [`ensure_index`](OpensearchSink::ensure_index) learned about a
    /// logical index; `None` if it was never announced.
    pub(crate) fn target(&self, logical: &str) -> Option<IndexTarget> {
        self.targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(logical)
            .cloned()
    }

    /// Every name a write may have targeted (generation or hash alias) →
    /// its logical index.
    pub(crate) fn logical_names(&self) -> HashMap<String, String> {
        self.targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .flat_map(|(logical, target)| {
                [
                    (target.generation.clone(), logical.clone()),
                    (target.hash_alias.clone(), logical.clone()),
                ]
            })
            .collect()
    }

    /// Record what is known about a logical index.
    pub(crate) fn set_target(&self, logical: &str, target: IndexTarget) {
        self.targets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(logical.to_owned(), target);
    }
}

/// An OpenSearch time value (`10s`, `500ms`, `1m`) as a duration. `None` for
/// `-1` (disabled) and for anything unreadable.
pub(crate) fn parse_time_value(value: &str) -> Option<Duration> {
    let value = value.trim();
    let split = value.find(|c: char| !c.is_ascii_digit())?;
    let (digits, unit) = value.split_at(split);
    let amount: u64 = digits.parse().ok()?;
    match unit {
        "nanos" => Some(Duration::from_nanos(amount)),
        "micros" => Some(Duration::from_micros(amount)),
        "ms" => Some(Duration::from_millis(amount)),
        "s" => Some(Duration::from_secs(amount)),
        "m" => amount.checked_mul(60).map(Duration::from_secs),
        "h" => amount.checked_mul(3_600).map(Duration::from_secs),
        "d" => amount.checked_mul(86_400).map(Duration::from_secs),
        _ => None,
    }
}

#[cfg(test)]
mod time_value_tests {
    use super::*;

    #[test]
    fn reads_opensearch_time_units() {
        assert_eq!(parse_time_value("10s"), Some(Duration::from_secs(10)));
        assert_eq!(parse_time_value("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_time_value("2m"), Some(Duration::from_secs(120)));
        assert_eq!(parse_time_value(" 1h "), Some(Duration::from_secs(3_600)));
    }

    #[test]
    fn disabled_or_unreadable_is_none() {
        assert_eq!(parse_time_value("-1"), None);
        assert_eq!(parse_time_value("10"), None);
        assert_eq!(parse_time_value("ten seconds"), None);
        assert_eq!(parse_time_value(""), None);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use kernel::AdapterConfig;

    fn build(ca_file: &Path) -> Result<OpensearchSink> {
        let toml = format!(
            "url = \"https://search:9200\"\nca_file = {:?}",
            ca_file.display().to_string()
        );
        let config = OpensearchConfig::from_options(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(config.ca_file.as_deref(), Some(ca_file));
        OpensearchSink::from_config(&SinkName::try_new("primary").unwrap(), &config)
    }

    fn write(name: &str, contents: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("flusso-ca-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn a_missing_ca_file_stops_the_start_and_names_the_path() {
        let path = std::env::temp_dir().join("flusso-no-such-ca.pem");
        let error = build(&path).unwrap_err().to_string();
        assert!(error.contains("reading ca_file"), "{error}");
        assert!(error.contains("flusso-no-such-ca.pem"), "{error}");
    }

    #[test]
    fn a_ca_file_without_a_certificate_stops_the_start() {
        let path = write("empty.pem", "not a certificate\n");
        let error = build(&path).unwrap_err().to_string();
        assert!(error.contains("holds no PEM certificate"), "{error}");
    }

    #[test]
    fn a_ca_file_with_a_broken_certificate_stops_the_start() {
        let path = write(
            "broken.pem",
            "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
        );
        let error = build(&path).unwrap_err().to_string();
        assert!(error.contains("ca_file"), "{error}");
        assert!(error.contains("broken.pem"), "{error}");
    }
}
