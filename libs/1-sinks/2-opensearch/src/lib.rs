//! A [`Sink`] that writes documents to an OpenSearch cluster via the bulk API.
//!
//! The sink owns each index it writes to and creates it up front from an
//! explicit, fully-typed mapping ([`ensure_index`](OpensearchSink::ensure_index)):
//!
//! - **Hash alias over generations.** The addressable name `{logical}_{hash}`
//!   (hash derived from the parsed schema) is an **alias**; the data lives in a
//!   concrete *generation* index `{logical}_{hash}_{gen}` behind it. A structural
//!   schema change moves the hash — a fresh alias + generation, re-seeded from
//!   scratch. An on-demand [`reindex`](OpensearchSink::reindex) (same schema)
//!   builds the *next* generation while the current one keeps serving reads, then
//!   [`mark_seeded`](OpensearchSink::mark_seeded) atomically repoints the alias
//!   and drops the old generation — so reads never see a half-built index. flusso
//!   and the `flusso-query` client address `{logical}_{hash}` (reading through an
//!   alias is transparent); the active generation + seeded-state live in a
//!   per-index meta doc.
//! - **Convenience alias.** The logical name `{logical}` is *also* kept as an
//!   alias on the current generation, so a human or ad-hoc tool can query
//!   `{logical}` without knowing the hash. Best-effort: a failure (say, the
//!   cluster already has a real index named `{logical}`) is logged and ignored,
//!   because correctness never depends on it.
//! - **Explicit mapping.** Field types come from the schema, not OpenSearch's
//!   dynamic guesses, and the index is created `dynamic: strict` so only
//!   configured fields are accepted. An index that already exists is left
//!   untouched.
//! - **Production-ready defaults.** Every index ships the `flusso_*` analysis
//!   definitions, and (unless `auto_subfields` is off) each `text`/`keyword`
//!   field is enriched with a good case/accent-insensitive analyzer plus
//!   `keyword` (exact), `keyword_lowercase` (sortable), and `text` (searchable)
//!   subfields. A field's explicit mapping always wins. See the crate-private
//!   `build_analysis` and `build_property`.
//! - **Refresh adapts to the pipeline's backlog.** The index is created with
//!   auto-refresh disabled (`refresh_interval: -1`) for fast bulk seeding;
//!   writes during backfill accumulate without per-flush refresh churn. When
//!   seeding completes ([`mark_seeded`](OpensearchSink::mark_seeded)) the index
//!   is refreshed once and handed the configured `refresh_interval` (default
//!   `"10s"`) — the steady-state visibility ceiling. On top of that,
//!   [`flush`](OpensearchSink::flush) forces an immediate refresh whenever it is
//!   told the pipeline has *caught up* (no backlog behind the batch), so search
//!   is fresh when idle but indexing stays cheap while a backlog drains. The
//!   `refresh_interval` only bounds staleness during sustained backlog, when a
//!   caught-up flush never happens.
//!
//! Operations are buffered in memory until `flush` is called. Large flushes are
//! chunked by `batch_size` to stay within OpenSearch request limits.
//!
//! Seeding state is persisted in a hidden `flusso_meta` index so restarts skip
//! a completed backfill.
//!
//! ## Module layout
//!
//! This file holds the [`OpensearchSink`] type, its constructor, and the few
//! shared helpers (`maybe_auth`, `physical`). The rest is split by concern:
//!
//! - `transport` — the HTTP plumbing: the bulk request, the small request
//!   helpers, and the generic per-index operations (create/exists/delete/refresh).
//! - `generations` — the alias-over-generations addressing: the aliases, the
//!   meta doc, generation discovery, and the pure naming functions.
//! - `sink` — the [`Sink`] trait implementation tying it all together.
//! - `mapping` — building the `dynamic: strict` index body and analysis.
//! - `bulk` — the bulk wire format, request chunking, and rejection parsing.

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
            buffer: Arc::new(Mutex::new(Vec::new())),
            index_names: Arc::new(SyncMutex::new(HashMap::new())),
        })
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
