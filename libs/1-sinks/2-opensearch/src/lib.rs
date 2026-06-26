#![doc = include_str!("../README.md")]
// Benchmarks (in `benches/`) pull dev-dependencies the unit-test build doesn't
// touch; allow that only under `cfg(test)` — the normal build still enforces
// unused dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod bulk;
mod generations;
mod mapping;
mod sink;
mod transport;

use std::collections::HashMap;
use std::sync::{Arc, Mutex as SyncMutex, PoisonError};
use std::time::Duration;

use schema_core::SinkName;
use sinks_core::{Result, SinkError};
use tokio::sync::Mutex;

use crate::bulk::BulkAction;
use crate::mapping::IndexOptions;

// In scope only so the rustdoc links below to `Sink`'s methods (`ensure_index`,
// `flush`, `upsert`, …) resolve — the trait is implemented over in `sink`, not
// used by name in this file.
#[allow(unused_imports)]
use sinks_core::Sink;

/// OpenSearch index that persists seeding markers.
pub(crate) const META_INDEX: &str = "flusso_meta";

/// Writes document operations to an OpenSearch cluster using the bulk API.
///
/// Calls to [`upsert`](OpensearchSink::upsert) and [`delete`](OpensearchSink::delete) append to an
/// in-memory buffer; [`flush`](OpensearchSink::flush) drains it as one or more bulk
/// requests. Every index is addressed by its **physical** name — the logical
/// name plus the schema hash, learned at
/// [`ensure_index`](OpensearchSink::ensure_index) — so a structural schema change writes
/// to a fresh index instead of the old one.
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
    /// Literal prefix prepended to every name this sink owns — the hash alias,
    /// its generations, the `{logical}` convenience alias, and the meta index —
    /// so several deployments can share one cluster without colliding. Empty by
    /// default (no prefix). Set with [`with_index_prefix`](OpensearchSink::with_index_prefix).
    pub(crate) index_prefix: String,
    /// In-flight operations, shared across clones.
    pub(crate) buffer: Arc<Mutex<Vec<BulkAction>>>,
    /// Logical index name → physical name (logical + schema hash), learned from
    /// [`ensure_index`](OpensearchSink::ensure_index). Writes and seed markers are
    /// addressed by the physical name. Shared across clones.
    pub(crate) index_names: Arc<SyncMutex<HashMap<String, String>>>,
}

impl OpensearchSink {
    /// Build a sink from the schema's OpenSearch sink configuration. The
    /// connection URL and credentials are resolved here, in the running
    /// environment, applying the `<NAME>_OPENSEARCH_*` deployment overrides for
    /// the sink named `name`.
    pub fn from_config(name: &SinkName, config: &schema_core::OpensearchSink) -> Result<Self> {
        let mut builder =
            reqwest::Client::builder().timeout(Duration::from_secs(config.timeout_secs));

        if !config.tls_verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder
            .build()
            .map_err(|e| SinkError::Write(format!("failed to build HTTP client: {e}")))?;

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
            },
            index_prefix: String::new(),
            buffer: Arc::new(Mutex::new(Vec::new())),
            index_names: Arc::new(SyncMutex::new(HashMap::new())),
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

    /// The physical index name for a logical one, as learned from
    /// [`ensure_index`](OpensearchSink::ensure_index). Falls back to the logical name if
    /// the index was never announced, so a stray write is still addressable
    /// rather than silently misrouted.
    pub(crate) fn physical(&self, logical: &str) -> String {
        self.index_names
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(logical)
            .cloned()
            .unwrap_or_else(|| logical.to_owned())
    }
}
