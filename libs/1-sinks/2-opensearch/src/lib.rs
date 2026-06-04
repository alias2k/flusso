//! A [`Sink`] that writes documents to an OpenSearch cluster via the bulk API.
//!
//! The sink owns each index it writes to and creates it up front from an
//! explicit, fully-typed mapping ([`ensure_index`](OpensearchSink::ensure_index)):
//!
//! - **Hashed physical name.** The actual index is named `{logical}_{hash}`,
//!   where the hash is derived from the parsed index schema. A structural
//!   change to the schema changes the hash, so the sink writes to a fresh
//!   index (re-seeded from scratch) rather than into the old, now-mismatched
//!   shape. The logical name remains the pipeline's identity; the sink
//!   translates it to the physical name on every call.
//! - **Explicit mapping.** Field types come from the schema, not OpenSearch's
//!   dynamic guesses, and the index is created `dynamic: strict` so only
//!   configured fields are accepted. An index that already exists is left
//!   untouched.
//! - **Production-ready defaults.** Every index ships the `flusso_*` analysis
//!   definitions, and (unless `auto_subfields` is off) each `text`/`keyword`
//!   field is enriched with a punctuation/case/accent-insensitive analyzer plus
//!   `keyword` (exact), `keyword_lowercase` (sortable), and `text` (searchable)
//!   subfields. A field's explicit mapping always wins. See
//!   [`build_analysis`] and [`build_property`].
//! - **Refresh follows the index lifecycle, not every flush.** The index is
//!   created with auto-refresh disabled (`refresh_interval: -1`) for fast bulk
//!   seeding; writes during backfill accumulate without per-flush refresh churn.
//!   When seeding completes ([`mark_seeded`](OpensearchSink::mark_seeded)) the
//!   index is refreshed once and handed back to automatic refresh (the interval
//!   is reset to the cluster default). In steady state, visibility is automatic;
//!   [`flush`](OpensearchSink::flush) does not force a refresh.
//!
//! Operations are buffered in memory until `flush` is called. Large flushes are
//! chunked by `batch_size` to stay within OpenSearch request limits.
//!
//! Seeding state is persisted in a hidden `flusso_meta` index so restarts skip
//! a completed backfill.

// Benchmarks (in `benches/`) pull dev-dependencies the unit-test build doesn't
// touch; allow that only under `cfg(test)` — the normal build still enforces
// unused dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

use std::collections::HashMap;
use std::sync::{Arc, Mutex as SyncMutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use schema_core::{
    GenericValue, IndexMapping, IndexName, MappingType, ResolvedField, TextAnalysis,
};
use serde_json::{Map, Value, json};
use sinks_core::{Result, Sink, SinkError, to_json};
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// OpenSearch index that persists seeding markers.
const META_INDEX: &str = "flusso_meta";

/// A buffered write destined for OpenSearch.
#[derive(Debug)]
enum BulkAction {
    Index {
        index: String,
        id: String,
        doc: Value,
    },
    Delete {
        index: String,
        id: String,
    },
}

/// Writes document operations to an OpenSearch cluster using the bulk API.
///
/// Calls to [`upsert`](Self::upsert) and [`delete`](Self::delete) append to an
/// in-memory buffer; [`flush`](Self::flush) drains it as one or more bulk
/// requests. Every index is addressed by its **physical** name — the logical
/// name plus the schema hash, learned at
/// [`ensure_index`](Self::ensure_index) — so a structural schema change writes
/// to a fresh index instead of the old one.
#[derive(Debug, Clone)]
pub struct OpensearchSink {
    client: reqwest::Client,
    base_url: String,
    auth: Option<(String, String)>,
    batch_size: usize,
    /// Maximum serialized bytes per bulk request — a flush is split so no
    /// request exceeds this, keeping it under OpenSearch's
    /// `http.max_content_length`.
    max_bytes: usize,
    max_retries: u32,
    pipeline: Option<String>,
    /// Settings that shape every index this sink creates: shard counts, the
    /// analysis backend, and whether `text`/`keyword` fields are auto-enriched.
    index_options: IndexOptions,
    /// In-flight operations, shared across clones.
    buffer: Arc<Mutex<Vec<BulkAction>>>,
    /// Logical index name → physical name (logical + schema hash), learned from
    /// [`ensure_index`](Self::ensure_index). Writes and seed markers are
    /// addressed by the physical name. Shared across clones.
    index_names: Arc<SyncMutex<HashMap<String, String>>>,
}

impl OpensearchSink {
    /// Build a sink from the schema's OpenSearch sink configuration.
    pub fn from_config(config: &schema_core::OpensearchSink) -> Result<Self> {
        let mut builder =
            reqwest::Client::builder().timeout(Duration::from_secs(config.timeout_secs));

        if !config.tls_verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder
            .build()
            .map_err(|e| SinkError::Write(format!("failed to build HTTP client: {e}")))?;

        let auth = match (&config.username, &config.password) {
            (Some(u), Some(p)) => Some((u.clone(), p.clone())),
            (Some(u), None) => Some((u.clone(), String::new())),
            _ => None,
        };

        Ok(Self {
            client,
            base_url: config.url.as_ref().trim_end_matches('/').to_owned(),
            auth,
            // `chunks(0)` panics, so a zero batch size would crash the first
            // non-empty flush; clamp it to at least one document per request.
            batch_size: (config.batch_size as usize).max(1),
            // At least one byte so the byte cap can never wedge a flush; a doc
            // larger than the cap is still sent (alone, with a warning).
            max_bytes: (config.max_bytes as usize).max(1),
            max_retries: config.max_retries,
            pipeline: config.pipeline.clone(),
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
    fn maybe_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Some((user, pass)) => req.basic_auth(user, Some(pass)),
            None => req,
        }
    }

    /// The physical index name for a logical one, as learned from
    /// [`ensure_index`](Self::ensure_index). Falls back to the logical name if
    /// the index was never announced, so a stray write is still addressable
    /// rather than silently misrouted.
    fn physical(&self, logical: &str) -> String {
        self.index_names
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(logical)
            .cloned()
            .unwrap_or_else(|| logical.to_owned())
    }

    /// Send one prebuilt NDJSON `body` (`count` actions) as a single bulk
    /// request. The caller (`flush`) is responsible for keeping `body` within
    /// the count and byte caps; this just transmits it.
    ///
    /// When `refresh` is `true`, appends `?refresh=true` to the URL so
    /// OpenSearch performs a segment refresh after the request completes and
    /// all documents become immediately searchable. Pass `false` for
    /// intermediate chunks; the final chunk in a flush carries the refresh.
    ///
    /// Retries on transient failures with exponential backoff.
    #[tracing::instrument(
        name = "os.bulk",
        level = "debug",
        skip_all,
        fields(count, bytes = body.len(), refresh),
        err,
    )]
    async fn send_bulk_chunk(&self, body: &str, count: usize, refresh: bool) -> Result<()> {
        if count == 0 {
            return Ok(());
        }

        let url = build_bulk_url(&self.base_url, self.pipeline.as_deref(), refresh);

        let mut last_err: Option<SinkError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = Duration::from_millis(200u64.saturating_mul(1u64 << (attempt - 1)));
                tokio::time::sleep(backoff).await;
                warn!(attempt, "retrying OpenSearch bulk request");
            }

            let req = self
                .client
                .post(&url)
                .header("Content-Type", "application/x-ndjson")
                .body(body.to_owned());

            match self.maybe_auth(req).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let result: Value = resp.json().await.map_err(|e| {
                        SinkError::Write(format!("failed to read bulk response: {e}"))
                    })?;

                    if bulk_has_errors(&result) {
                        last_err = Some(SinkError::Write(
                            "bulk request completed with item-level errors".to_owned(),
                        ));
                        continue;
                    }

                    debug!(count, refresh, "bulk request succeeded");
                    return Ok(());
                }
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    last_err = Some(SinkError::Write(format!(
                        "bulk request failed: HTTP {status}: {text}"
                    )));
                }
                Err(e) => {
                    last_err = Some(SinkError::Write(format!("bulk request error: {e}")));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            SinkError::Write("bulk request failed with no error detail".to_owned())
        }))
    }

    /// Whether `index` already exists in the cluster.
    async fn index_exists(&self, index: &str) -> Result<bool> {
        let url = format!("{}/{index}", self.base_url);
        let resp = self
            .maybe_auth(self.client.head(&url))
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("index check failed: {e}")))?;

        if resp.status().is_success() {
            return Ok(true);
        }
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Err(SinkError::Write(format!(
            "index check failed: HTTP {status}: {text}"
        )))
    }

    /// Force a one-off refresh so everything written to `index` so far becomes
    /// searchable, regardless of the index's refresh interval.
    async fn refresh_index(&self, index: &str) -> Result<()> {
        let url = format!("{}/{index}/_refresh", self.base_url);
        let resp = self
            .maybe_auth(self.client.post(&url))
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("refresh failed: {e}")))?;

        if resp.status().is_success() {
            debug!(index, "refreshed index");
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(SinkError::Write(format!(
                "refresh failed: HTTP {status}: {text}"
            )))
        }
    }

    /// Hand `index` back to automatic refresh by resetting `refresh_interval`
    /// to the cluster default (it was set to `-1` at creation for bulk seeding).
    async fn restore_auto_refresh(&self, index: &str) -> Result<()> {
        let url = format!("{}/{index}/_settings", self.base_url);
        let req = self
            .client
            .put(&url)
            .header("Content-Type", "application/json")
            .json(&json!({ "index": { "refresh_interval": null } }));

        let resp = self
            .maybe_auth(req)
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("restore refresh failed: {e}")))?;

        if resp.status().is_success() {
            debug!(index, "restored automatic refresh on index");
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(SinkError::Write(format!(
                "restore refresh failed: HTTP {status}: {text}"
            )))
        }
    }

    /// Write a document to `META_INDEX` under the given id.
    async fn put_meta(&self, id: &str, doc: Value) -> Result<()> {
        let url = format!("{}/{META_INDEX}/_doc/{id}", self.base_url);
        let req = self
            .client
            .put(&url)
            .header("Content-Type", "application/json")
            .json(&doc);

        let resp = self
            .maybe_auth(req)
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("meta put failed: {e}")))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(SinkError::Write(format!(
                "meta put failed: HTTP {status}: {text}"
            )))
        }
    }

    /// Fetch a document from `META_INDEX` by id. Returns `None` on 404.
    async fn get_meta(&self, id: &str) -> Result<Option<Value>> {
        let url = format!("{}/{META_INDEX}/_doc/{id}", self.base_url);
        let req = self.client.get(&url);

        let resp = self
            .maybe_auth(req)
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("meta get failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if resp.status().is_success() {
            let body: Value = resp
                .json()
                .await
                .map_err(|e| SinkError::Write(format!("failed to parse meta response: {e}")))?;
            Ok(Some(body))
        } else {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            Err(SinkError::Write(format!(
                "meta get failed: HTTP {status}: {text}"
            )))
        }
    }
}

#[async_trait]
impl Sink for OpensearchSink {
    /// Create the index from its explicit mapping if it does not already exist.
    ///
    /// The index is created `dynamic: strict` (only configured fields are
    /// accepted) with `refresh_interval: -1` (auto-refresh off) for fast bulk
    /// seeding — [`mark_seeded`](Self::mark_seeded) restores automatic refresh
    /// once the backfill completes. An existing index is left untouched, so its
    /// mapping is never silently rewritten.
    #[tracing::instrument(
        name = "os.ensure_index",
        skip_all,
        fields(index = mapping.index.as_ref()),
        err,
    )]
    async fn ensure_index(&self, mapping: &IndexMapping) -> Result<()> {
        let logical = mapping.index.as_ref();
        // Physical name = logical name + schema hash, so a structural schema
        // change yields a new name (and thus a fresh, re-seeded index).
        let index = format!("{logical}_{}", mapping.hash);
        self.index_names
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(logical.to_owned(), index.clone());

        if self.index_exists(&index).await? {
            debug!(index, "index exists; leaving its mapping untouched");
            return Ok(());
        }

        let url = format!("{}/{index}", self.base_url);
        let req = self
            .client
            .put(&url)
            .header("Content-Type", "application/json")
            .json(&build_index_body(&mapping.fields, &self.index_options));

        let resp = self
            .maybe_auth(req)
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("index create failed: {e}")))?;

        if resp.status().is_success() {
            debug!(index, "created index with explicit mapping");
            return Ok(());
        }

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // A concurrent creator winning the race is fine — the index now exists.
        if text.contains("resource_already_exists_exception") {
            return Ok(());
        }
        Err(SinkError::Write(format!(
            "index create failed: HTTP {status}: {text}"
        )))
    }

    async fn upsert(&self, index: &IndexName, id: &str, document: &GenericValue) -> Result<()> {
        let action = BulkAction::Index {
            index: self.physical(index.as_ref()),
            id: id.to_owned(),
            doc: to_json(document),
        };
        self.buffer.lock().await.push(action);
        Ok(())
    }

    async fn delete(&self, index: &IndexName, id: &str) -> Result<()> {
        let action = BulkAction::Delete {
            index: self.physical(index.as_ref()),
            id: id.to_owned(),
        };
        self.buffer.lock().await.push(action);
        Ok(())
    }

    /// Drain the buffer and send all buffered operations to OpenSearch.
    ///
    /// The drained operations are split into bulk requests bounded by **both**
    /// caps: at most `batch_size` documents *and* at most `max_bytes` serialized
    /// bytes per request, so a few large documents can't push a request past
    /// OpenSearch's `http.max_content_length`. A single document larger than
    /// `max_bytes` is sent on its own (it can't be split) with a warning.
    ///
    /// No refresh is forced: visibility is governed by the index's refresh
    /// interval — disabled during backfill and automatic in steady state (see
    /// the module docs).
    #[tracing::instrument(name = "os.flush", skip_all, err)]
    async fn flush(&self) -> Result<()> {
        let actions = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };

        if actions.is_empty() {
            return Ok(());
        }

        // Serialize each action's NDJSON fragment once, then group fragments
        // into requests honoring the count and byte caps (see `plan_chunks`).
        let mut fragments = Vec::with_capacity(actions.len());
        for action in &actions {
            let fragment = bulk_action_fragment(action)?;
            if fragment.len() > self.max_bytes {
                warn!(
                    bytes = fragment.len(),
                    max_bytes = self.max_bytes,
                    "a single document exceeds the bulk byte cap; sending it in its own request",
                );
            }
            fragments.push(fragment);
        }

        let sizes: Vec<usize> = fragments.iter().map(String::len).collect();
        let total_bytes: usize = sizes.iter().sum();
        let plan = plan_chunks(&sizes, self.batch_size, self.max_bytes);
        debug!(
            documents = actions.len(),
            requests = plan.len(),
            bytes = total_bytes,
            "flushing buffered operations",
        );

        let mut fragments = fragments.into_iter();
        for &count in &plan {
            let mut body = String::new();
            for _ in 0..count {
                if let Some(fragment) = fragments.next() {
                    body.push_str(&fragment);
                }
            }
            self.send_bulk_chunk(&body, count, false).await?;
        }

        Ok(())
    }

    async fn is_seeded(&self, index: &IndexName) -> Result<bool> {
        match self.get_meta(&self.physical(index.as_ref())).await? {
            Some(doc) => {
                let seeded = doc
                    .get("_source")
                    .and_then(|s| s.get("seeded"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Ok(seeded)
            }
            None => Ok(false),
        }
    }

    /// Record that `index` has been seeded.
    ///
    /// First makes the backfilled documents searchable (one refresh) and hands
    /// the index back to automatic refresh, *then* writes the seed marker. The
    /// ordering matters: if either step fails, the marker is not written, so the
    /// next run re-runs the (idempotent) backfill and retries rather than
    /// stranding the index at `refresh_interval: -1`.
    async fn mark_seeded(&self, index: &IndexName) -> Result<()> {
        let physical = self.physical(index.as_ref());
        self.refresh_index(&physical).await?;
        self.restore_auto_refresh(&physical).await?;
        self.put_meta(&physical, json!({ "seeded": true })).await
    }
}

/// The settings that shape every index this sink creates. Held by the sink and
/// threaded into [`build_index_body`] so the body builder stays a pure function
/// of `(fields, options)` — easy to unit-test without a live sink.
#[derive(Debug, Clone)]
struct IndexOptions {
    number_of_shards: u32,
    number_of_replicas: u32,
    text_analysis: TextAnalysis,
    auto_subfields: bool,
}

/// The subfield key holding the exact, case-sensitive value of a string field —
/// for aggregations, exact-term filters, and exact sort.
const KEYWORD_SUBFIELD: &str = "keyword";
/// The subfield key holding the lowercased, accent-folded value — for
/// case-insensitive sort and exact lookup.
const KEYWORD_LOWERCASE_SUBFIELD: &str = "keyword_lowercase";
/// The subfield key holding the full-text-analyzed value of a `keyword` field,
/// so a `keyword` is still searchable in a search box.
const TEXT_SUBFIELD: &str = "text";
/// The analyzer attached to `text` fields (and `keyword` text subfields) by
/// default — punctuation-splitting, case- and accent-insensitive. Tuned for
/// short identifier-like text (names, codes, SKUs, statuses).
const CODE_ANALYZER: &str = "flusso_code";
/// The prose analyzer (`kind: prose` points fields here). Plain
/// tokenize + fold, no code-splitting.
const TEXT_ANALYZER: &str = "flusso_text";
/// The normalizer attached to lowercase keyword subfields.
const LOWERCASE_NORMALIZER: &str = "flusso_lowercase";
/// Strings longer than this are not indexed in a `keyword` subfield (they are
/// still stored). Matches OpenSearch's own dynamic-mapping default.
const KEYWORD_IGNORE_ABOVE: u32 = 256;

/// Build the `PUT /{index}` request body: a `dynamic: strict` mapping with one
/// typed property per field, the shard counts, `refresh_interval: -1` for bulk
/// seeding, and the `flusso_*` analysis definitions the field shapes reference.
fn build_index_body(fields: &[ResolvedField], options: &IndexOptions) -> Value {
    json!({
        "settings": {
            "index": {
                "refresh_interval": "-1",
                "number_of_shards": options.number_of_shards,
                "number_of_replicas": options.number_of_replicas,
            },
            // Always emitted so an explicit `analyzer: flusso_text` works even
            // when `auto_subfields` is off; an unused analyzer is harmless.
            "analysis": build_analysis(options.text_analysis),
        },
        "mappings": {
            "dynamic": "strict",
            "properties": build_properties(fields, options),
        },
    })
}

/// The `analysis` block defining the `flusso_*` analyzers, the code-splitting
/// token filter, and the lowercase normalizer. The folding components swap
/// between built-in (`asciifolding`) and ICU (`icu_folding`) per `mode`.
fn build_analysis(mode: TextAnalysis) -> Value {
    // `flusso_code`: split on punctuation / case / letter-digit boundaries
    // (so `C-01234` → `c`, `01234`, `c01234`, `c-01234`), then lowercase + fold.
    // `flatten_graph` is required after `word_delimiter_graph` at index time.
    let code_fold = match mode {
        TextAnalysis::Builtin => "asciifolding",
        TextAnalysis::Icu => "icu_folding",
    };
    let code_analyzer = json!({
        "type": "custom",
        "tokenizer": "whitespace",
        "filter": ["flusso_word_delimiter", "flatten_graph", "lowercase", code_fold],
    });

    // `flusso_text`: prose. Built-in standard tokenizer + fold, or the ICU
    // tokenizer/normalizer/folding which segment CJK/Thai and fold every script.
    let text_analyzer = match mode {
        TextAnalysis::Builtin => json!({
            "type": "custom",
            "tokenizer": "standard",
            "filter": ["lowercase", "asciifolding"],
        }),
        TextAnalysis::Icu => json!({
            "type": "custom",
            "tokenizer": "icu_tokenizer",
            "filter": ["icu_normalizer", "icu_folding"],
        }),
    };

    // Normalizers accept only a restricted filter set; `icu_normalizer` is the
    // ICU member that qualifies (it lowercases and folds), while built-in mode
    // uses `lowercase` + `asciifolding`.
    let normalizer_filters = match mode {
        TextAnalysis::Builtin => json!(["lowercase", "asciifolding"]),
        TextAnalysis::Icu => json!(["icu_normalizer"]),
    };

    let mut analyzers = Map::new();
    analyzers.insert(CODE_ANALYZER.to_owned(), code_analyzer);
    analyzers.insert(TEXT_ANALYZER.to_owned(), text_analyzer);

    let mut normalizers = Map::new();
    normalizers.insert(
        LOWERCASE_NORMALIZER.to_owned(),
        json!({ "type": "custom", "filter": normalizer_filters }),
    );

    json!({
        "filter": {
            "flusso_word_delimiter": {
                "type": "word_delimiter_graph",
                "catenate_all": true,
                "preserve_original": true,
            },
        },
        "analyzer": Value::Object(analyzers),
        "normalizer": Value::Object(normalizers),
    })
}

/// Translate resolved fields into an OpenSearch `properties` object.
fn build_properties(fields: &[ResolvedField], options: &IndexOptions) -> Value {
    let mut props = Map::new();
    for field in fields {
        props.insert(
            field.name.as_ref().to_owned(),
            build_property(field, options),
        );
    }
    Value::Object(props)
}

/// Translate one resolved field into its OpenSearch property.
///
/// For a scalar `text`/`keyword` field (and `auto_subfields` on) this starts
/// from a production-ready default — a good analyzer plus exact / sortable /
/// searchable subfields — then overlays the field's own `extra` on top, so an
/// explicit `analyzer`, `fields`, etc. always wins. `object`/`nested` recurse
/// into their children; other types pass through with just their `extra`.
fn build_property(field: &ResolvedField, options: &IndexOptions) -> Value {
    let mut prop = Map::new();
    prop.insert(
        "type".to_owned(),
        Value::String(opensearch_type(&field.mapping.mapping_type)),
    );

    // Auto-enrichment applies only to scalar string fields; container types
    // (object/nested, which carry children) and numerics are left as-is.
    if options.auto_subfields && field.children.is_empty() {
        match field.mapping.mapping_type {
            MappingType::Text => {
                prop.insert("analyzer".to_owned(), json!(CODE_ANALYZER));
                prop.insert("fields".to_owned(), text_subfields());
            }
            MappingType::Keyword => {
                prop.insert("fields".to_owned(), keyword_subfields());
            }
            _ => {}
        }
    }

    // The field's explicit mapping wins, key by key — overriding the analyzer,
    // replacing the auto subfields wholesale, etc.
    for (key, value) in &field.mapping.extra {
        prop.insert(key.clone(), to_json(value));
    }

    if !field.children.is_empty() {
        prop.insert(
            "properties".to_owned(),
            build_properties(&field.children, options),
        );
    }
    Value::Object(prop)
}

/// The case/accent-insensitive `keyword_lowercase` subfield, shared by the
/// `text` and `keyword` defaults — for case-insensitive sort and exact lookup.
fn keyword_lowercase_subfield() -> Value {
    json!({
        "type": "keyword",
        "normalizer": LOWERCASE_NORMALIZER,
        "ignore_above": KEYWORD_IGNORE_ABOVE,
    })
}

/// Default subfields for a `text` field: an exact `keyword` and a
/// case/accent-insensitive `keyword_lowercase` (both for filter/sort/agg).
fn text_subfields() -> Value {
    let mut fields = Map::new();
    fields.insert(
        KEYWORD_SUBFIELD.to_owned(),
        json!({ "type": "keyword", "ignore_above": KEYWORD_IGNORE_ABOVE }),
    );
    fields.insert(
        KEYWORD_LOWERCASE_SUBFIELD.to_owned(),
        keyword_lowercase_subfield(),
    );
    Value::Object(fields)
}

/// Default subfields for a `keyword` field: a full-text `text` (so it is still
/// searchable) and a case/accent-insensitive `keyword_lowercase` for sort.
fn keyword_subfields() -> Value {
    let mut fields = Map::new();
    fields.insert(
        TEXT_SUBFIELD.to_owned(),
        json!({ "type": "text", "analyzer": CODE_ANALYZER }),
    );
    fields.insert(
        KEYWORD_LOWERCASE_SUBFIELD.to_owned(),
        keyword_lowercase_subfield(),
    );
    Value::Object(fields)
}

/// The OpenSearch type string for a [`MappingType`] — the canonical name from
/// [`MappingType::name`], which is also what the type serializes as.
fn opensearch_type(mapping_type: &MappingType) -> String {
    mapping_type.name().to_owned()
}

/// Build the `/_bulk` URL with optional pipeline and refresh parameters.
fn build_bulk_url(base_url: &str, pipeline: Option<&str>, refresh: bool) -> String {
    let mut params: Vec<String> = Vec::new();
    if let Some(p) = pipeline {
        params.push(format!("pipeline={p}"));
    }
    if refresh {
        params.push("refresh=true".to_owned());
    }
    if params.is_empty() {
        format!("{base_url}/_bulk")
    } else {
        format!("{base_url}/_bulk?{}", params.join("&"))
    }
}

/// Serialize a slice of [`BulkAction`]s into one NDJSON bulk body. Production
/// code builds bodies fragment-by-fragment in [`flush`](OpensearchSink::flush)
/// (to honor the byte cap), so this whole-slice form is now a test convenience.
#[cfg(test)]
fn build_bulk_body(actions: &[BulkAction]) -> Result<String> {
    let mut body = String::new();
    for action in actions {
        body.push_str(&bulk_action_fragment(action)?);
    }
    Ok(body)
}

/// Serialize one [`BulkAction`] into its NDJSON fragment — the metadata line
/// and, for an index op, the source line, each newline-terminated. This is the
/// single place the bulk wire format is produced; [`build_bulk_body`] and the
/// byte-aware chunking in [`flush`](OpensearchSink::flush) both go through it.
fn bulk_action_fragment(action: &BulkAction) -> Result<String> {
    let mut fragment = String::new();
    match action {
        BulkAction::Index { index, id, doc } => {
            let meta = serde_json::to_string(&json!({ "index": { "_index": index, "_id": id } }))
                .map_err(|e| SinkError::Serialize(e.to_string()))?;
            let source =
                serde_json::to_string(doc).map_err(|e| SinkError::Serialize(e.to_string()))?;
            fragment.push_str(&meta);
            fragment.push('\n');
            fragment.push_str(&source);
            fragment.push('\n');
        }
        BulkAction::Delete { index, id } => {
            let meta = serde_json::to_string(&json!({ "delete": { "_index": index, "_id": id } }))
                .map_err(|e| SinkError::Serialize(e.to_string()))?;
            fragment.push_str(&meta);
            fragment.push('\n');
        }
    }
    Ok(fragment)
}

/// Group action `sizes` (serialized NDJSON byte lengths, in order) into bulk
/// requests, returning the action count for each request. A new request starts
/// before an action that would push the current one past **either** cap:
/// `batch_size` documents or `max_bytes` bytes.
///
/// An action larger than `max_bytes` lands in a request of its own — a bulk
/// action is atomic and can't be split — so the byte cap is best-effort for a
/// single oversized document (the caller warns when that happens).
fn plan_chunks(sizes: &[usize], batch_size: usize, max_bytes: usize) -> Vec<usize> {
    let mut chunks = Vec::new();
    let mut count = 0usize;
    let mut bytes = 0usize;
    for &size in sizes {
        if count > 0 && (count >= batch_size || bytes.saturating_add(size) > max_bytes) {
            chunks.push(count);
            count = 0;
            bytes = 0;
        }
        count += 1;
        bytes = bytes.saturating_add(size);
    }
    if count > 0 {
        chunks.push(count);
    }
    chunks
}

/// Returns `true` if the bulk response indicates at least one item-level error
/// (HTTP 4xx/5xx on an individual operation).
fn bulk_has_errors(response: &Value) -> bool {
    let has_errors = response
        .get("errors")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !has_errors {
        return false;
    }

    response
        .get("items")
        .and_then(|v| v.as_array())
        .map(|items| {
            items.iter().any(|item| {
                let op = item
                    .get("index")
                    .or_else(|| item.get("create"))
                    .or_else(|| item.get("delete"))
                    .or_else(|| item.get("update"));
                op.and_then(|o| o.get("status"))
                    .and_then(|s| s.as_u64())
                    .map(|status| status >= 400)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::collections::BTreeMap;

    use schema_core::{FieldName, Mapping};

    use super::*;

    fn field(name: &str, mapping_type: MappingType, children: Vec<ResolvedField>) -> ResolvedField {
        ResolvedField {
            name: FieldName::try_new(name).unwrap(),
            mapping: Mapping {
                mapping_type,
                extra: BTreeMap::new(),
            },
            nullable: true,
            children,
        }
    }

    /// Default options: auto-subfields on, built-in analysis, 1 shard / 1 replica.
    fn opts() -> IndexOptions {
        IndexOptions {
            number_of_shards: 1,
            number_of_replicas: 1,
            text_analysis: TextAnalysis::Builtin,
            auto_subfields: true,
        }
    }

    fn opts_no_subfields() -> IndexOptions {
        IndexOptions {
            auto_subfields: false,
            ..opts()
        }
    }

    #[test]
    fn index_body_is_dynamic_strict_with_disabled_refresh_and_shards() {
        let body = build_index_body(&[field("email", MappingType::Keyword, vec![])], &opts());
        assert_eq!(body["mappings"]["dynamic"], "strict");
        assert_eq!(body["settings"]["index"]["refresh_interval"], "-1");
        assert_eq!(body["settings"]["index"]["number_of_shards"], 1);
        assert_eq!(body["settings"]["index"]["number_of_replicas"], 1);
        assert_eq!(body["mappings"]["properties"]["email"]["type"], "keyword");
    }

    #[test]
    fn analysis_block_defines_the_flusso_analyzers() {
        let body = build_index_body(&[], &opts());
        let analysis = &body["settings"]["analysis"];
        assert_eq!(
            analysis["filter"]["flusso_word_delimiter"]["type"],
            "word_delimiter_graph"
        );
        assert_eq!(
            analysis["analyzer"]["flusso_code"]["tokenizer"],
            "whitespace"
        );
        // Built-in mode folds with asciifolding, not ICU.
        let code_filters = &analysis["analyzer"]["flusso_code"]["filter"];
        assert!(
            code_filters
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "asciifolding")
        );
        assert_eq!(analysis["analyzer"]["flusso_text"]["tokenizer"], "standard");
        assert_eq!(analysis["normalizer"]["flusso_lowercase"]["type"], "custom");
    }

    #[test]
    fn icu_mode_swaps_in_icu_components() {
        let icu = IndexOptions {
            text_analysis: TextAnalysis::Icu,
            ..opts()
        };
        let body = build_index_body(&[], &icu);
        let analysis = &body["settings"]["analysis"];
        let code_filters = &analysis["analyzer"]["flusso_code"]["filter"];
        assert!(
            code_filters
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "icu_folding")
        );
        assert_eq!(
            analysis["analyzer"]["flusso_text"]["tokenizer"],
            "icu_tokenizer"
        );
        assert_eq!(
            analysis["normalizer"]["flusso_lowercase"]["filter"][0],
            "icu_normalizer"
        );
    }

    #[test]
    fn text_field_gets_code_analyzer_and_subfields() {
        let body = build_index_body(&[field("name", MappingType::Text, vec![])], &opts());
        let name = &body["mappings"]["properties"]["name"];
        assert_eq!(name["type"], "text");
        assert_eq!(name["analyzer"], "flusso_code");
        assert_eq!(name["fields"]["keyword"]["type"], "keyword");
        assert_eq!(name["fields"]["keyword"]["ignore_above"], 256);
        assert_eq!(
            name["fields"]["keyword_lowercase"]["normalizer"],
            "flusso_lowercase"
        );
    }

    #[test]
    fn keyword_field_gets_text_and_lowercase_subfields() {
        let body = build_index_body(&[field("email", MappingType::Keyword, vec![])], &opts());
        let email = &body["mappings"]["properties"]["email"];
        assert_eq!(email["type"], "keyword");
        assert_eq!(email["fields"]["text"]["type"], "text");
        assert_eq!(email["fields"]["text"]["analyzer"], "flusso_code");
        assert_eq!(
            email["fields"]["keyword_lowercase"]["normalizer"],
            "flusso_lowercase"
        );
    }

    #[test]
    fn auto_subfields_off_leaves_string_fields_bare() {
        let body = build_index_body(
            &[field("name", MappingType::Text, vec![])],
            &opts_no_subfields(),
        );
        let name = &body["mappings"]["properties"]["name"];
        assert_eq!(name["type"], "text");
        assert!(name.get("fields").is_none());
        assert!(name.get("analyzer").is_none());
    }

    #[test]
    fn explicit_extra_overrides_the_auto_shape() {
        // A field that sets its own analyzer (e.g. via `kind: prose`) keeps it,
        // and explicit `fields` replace the auto subfields wholesale.
        let mut extra = BTreeMap::new();
        extra.insert(
            "analyzer".to_owned(),
            GenericValue::String("flusso_text".to_owned()),
        );
        let name = ResolvedField {
            name: FieldName::try_new("bio").unwrap(),
            mapping: Mapping {
                mapping_type: MappingType::Text,
                extra,
            },
            nullable: true,
            children: vec![],
        };
        let body = build_index_body(&[name], &opts());
        let bio = &body["mappings"]["properties"]["bio"];
        assert_eq!(bio["analyzer"], "flusso_text");
        // The auto subfields are still present (only `analyzer` was overridden).
        assert_eq!(bio["fields"]["keyword"]["type"], "keyword");
    }

    #[test]
    fn nested_field_recurses_into_properties() {
        let orders = field(
            "orders",
            MappingType::Nested,
            vec![
                field("id", MappingType::Long, vec![]),
                field("total", MappingType::Double, vec![]),
            ],
        );
        let body = build_index_body(&[orders], &opts());
        let orders = &body["mappings"]["properties"]["orders"];
        assert_eq!(orders["type"], "nested");
        assert_eq!(orders["properties"]["id"]["type"], "long");
        assert_eq!(orders["properties"]["total"]["type"], "double");
        // Numeric children get no string subfields.
        assert!(orders["properties"]["id"].get("fields").is_none());
    }

    #[test]
    fn extra_mapping_settings_pass_through() {
        let mut extra = BTreeMap::new();
        extra.insert("scaling_factor".to_owned(), GenericValue::Int(100));
        let amount = ResolvedField {
            name: FieldName::try_new("amount").unwrap(),
            mapping: Mapping {
                mapping_type: MappingType::ScaledFloat,
                extra,
            },
            nullable: true,
            children: vec![],
        };
        let body = build_index_body(&[amount], &opts());
        let amount = &body["mappings"]["properties"]["amount"];
        assert_eq!(amount["type"], "scaled_float");
        assert_eq!(amount["scaling_factor"], 100);
    }

    #[test]
    fn other_mapping_type_uses_its_raw_name() {
        assert_eq!(
            opensearch_type(&MappingType::Other("binary".to_owned())),
            "binary"
        );
    }

    #[test]
    fn bulk_body_index_produces_two_ndjson_lines() {
        let doc = json!({ "email": "ada@x.io" });
        let actions = vec![BulkAction::Index {
            index: "users".to_owned(),
            id: "42".to_owned(),
            doc,
        }];
        let body = build_bulk_body(&actions).unwrap();
        let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 2);

        let meta: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(meta["index"]["_index"], "users");
        assert_eq!(meta["index"]["_id"], "42");

        let source: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(source["email"], "ada@x.io");
    }

    #[test]
    fn bulk_body_delete_produces_one_ndjson_line() {
        let actions = vec![BulkAction::Delete {
            index: "users".to_owned(),
            id: "7".to_owned(),
        }];
        let body = build_bulk_body(&actions).unwrap();
        let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
        assert_eq!(lines.len(), 1);

        let meta: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(meta["delete"]["_index"], "users");
        assert_eq!(meta["delete"]["_id"], "7");
    }

    #[test]
    fn bulk_body_mixed_operations_are_ordered() {
        let actions = vec![
            BulkAction::Index {
                index: "users".to_owned(),
                id: "1".to_owned(),
                doc: json!({ "name": "alice" }),
            },
            BulkAction::Delete {
                index: "users".to_owned(),
                id: "2".to_owned(),
            },
        ];
        let body = build_bulk_body(&actions).unwrap();
        let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
        // index: 2 lines, delete: 1 line
        assert_eq!(lines.len(), 3);
        let first_meta: Value = serde_json::from_str(lines[0]).unwrap();
        assert!(first_meta.get("index").is_some());
        let delete_meta: Value = serde_json::from_str(lines[2]).unwrap();
        assert!(delete_meta.get("delete").is_some());
    }

    #[test]
    fn bulk_url_no_pipeline_no_refresh() {
        assert_eq!(
            build_bulk_url("http://localhost:9200", None, false),
            "http://localhost:9200/_bulk"
        );
    }

    #[test]
    fn bulk_url_refresh_only() {
        assert_eq!(
            build_bulk_url("http://localhost:9200", None, true),
            "http://localhost:9200/_bulk?refresh=true"
        );
    }

    #[test]
    fn bulk_url_pipeline_and_refresh() {
        assert_eq!(
            build_bulk_url("http://localhost:9200", Some("my-pipeline"), true),
            "http://localhost:9200/_bulk?pipeline=my-pipeline&refresh=true"
        );
    }

    #[test]
    fn bulk_has_errors_returns_false_when_no_errors_flag() {
        let resp = json!({ "errors": false, "items": [] });
        assert!(!bulk_has_errors(&resp));
    }

    #[test]
    fn bulk_has_errors_returns_true_when_item_has_4xx_status() {
        let resp = json!({
            "errors": true,
            "items": [{ "index": { "_index": "x", "_id": "1", "status": 429 } }]
        });
        assert!(bulk_has_errors(&resp));
    }

    #[test]
    fn bulk_has_errors_returns_false_when_all_items_succeed() {
        let resp = json!({
            "errors": true,
            "items": [{ "index": { "_index": "x", "_id": "1", "status": 200 } }]
        });
        assert!(!bulk_has_errors(&resp));
    }

    #[test]
    fn build_bulk_body_is_empty_for_no_actions() {
        let body = build_bulk_body(&[]).unwrap();
        assert!(body.is_empty());
    }

    #[test]
    fn plan_chunks_splits_on_the_count_cap() {
        // 5 small actions, cap of 2 per request → 2 + 2 + 1.
        let sizes = [10, 10, 10, 10, 10];
        assert_eq!(plan_chunks(&sizes, 2, 1_000), vec![2, 2, 1]);
    }

    #[test]
    fn plan_chunks_splits_on_the_byte_cap_before_the_count_cap() {
        // Count cap is generous (100), but 30 bytes per request fits only two
        // 12-byte actions; the third would reach 36 > 30, so it starts a new one.
        let sizes = [12, 12, 12, 12];
        assert_eq!(plan_chunks(&sizes, 100, 30), vec![2, 2]);
    }

    #[test]
    fn plan_chunks_isolates_an_oversized_action() {
        // The 50-byte action exceeds the 30-byte cap: it can't be split, so it
        // gets its own request, and the neighbors pack around it.
        let sizes = [10, 50, 10, 10];
        assert_eq!(plan_chunks(&sizes, 100, 30), vec![1, 1, 2]);
    }

    #[test]
    fn plan_chunks_applies_whichever_cap_is_hit_first() {
        // Count cap 3 and byte cap 100: the byte cap bites first at 40+40+40.
        let sizes = [40, 40, 40, 5, 5];
        assert_eq!(plan_chunks(&sizes, 3, 100), vec![2, 3]);
    }

    #[test]
    fn plan_chunks_of_nothing_is_no_requests() {
        assert!(plan_chunks(&[], 10, 100).is_empty());
    }
}
