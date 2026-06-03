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

use std::collections::HashMap;
use std::sync::{Arc, Mutex as SyncMutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use schema_core::{GenericValue, IndexMapping, IndexName, MappingType, ResolvedField};
use serde_json::{Map, Value, json};
use sinks_core::{Result, Sink, SinkError, to_json};
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// OpenSearch index that persists seeding markers.
const META_INDEX: &str = "flusso_meta";

/// A buffered write destined for OpenSearch.
#[derive(Debug)]
enum BulkAction {
    Index { index: String, id: String, doc: Value },
    Delete { index: String, id: String },
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
    max_retries: u32,
    pipeline: Option<String>,
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
            max_retries: config.max_retries,
            pipeline: config.pipeline.clone(),
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

    /// Send a slice of bulk actions as a single request.
    ///
    /// When `refresh` is `true`, appends `?refresh=true` to the URL so
    /// OpenSearch performs a segment refresh after the request completes and
    /// all documents become immediately searchable. Pass `false` for
    /// intermediate chunks; the final chunk in a flush carries the refresh.
    ///
    /// Retries on transient failures with exponential backoff.
    async fn send_bulk_chunk(&self, actions: &[BulkAction], refresh: bool) -> Result<()> {
        if actions.is_empty() {
            return Ok(());
        }

        let body = build_bulk_body(actions)?;
        let url = build_bulk_url(&self.base_url, self.pipeline.as_deref(), refresh);

        let mut last_err: Option<SinkError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff =
                    Duration::from_millis(200u64.saturating_mul(1u64 << (attempt - 1)));
                tokio::time::sleep(backoff).await;
                warn!(attempt, "retrying OpenSearch bulk request");
            }

            let req = self
                .client
                .post(&url)
                .header("Content-Type", "application/x-ndjson")
                .body(body.clone());

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

                    debug!(count = actions.len(), refresh, "bulk request succeeded");
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
            let body: Value = resp.json().await.map_err(|e| {
                SinkError::Write(format!("failed to parse meta response: {e}"))
            })?;
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
            .json(&build_index_body(&mapping.fields));

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
    /// Large batches are split into `batch_size` chunks. No refresh is forced:
    /// visibility is governed by the index's refresh interval — disabled during
    /// backfill and automatic in steady state (see the module docs).
    async fn flush(&self) -> Result<()> {
        let actions = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };

        if actions.is_empty() {
            return Ok(());
        }

        for chunk in actions.chunks(self.batch_size) {
            self.send_bulk_chunk(chunk, false).await?;
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

/// Build the `PUT /{index}` request body: a `dynamic: strict` mapping with one
/// typed property per field, plus `refresh_interval: -1` for bulk seeding.
fn build_index_body(fields: &[ResolvedField]) -> Value {
    json!({
        "settings": { "index": { "refresh_interval": "-1" } },
        "mappings": {
            "dynamic": "strict",
            "properties": build_properties(fields),
        },
    })
}

/// Translate resolved fields into an OpenSearch `properties` object.
fn build_properties(fields: &[ResolvedField]) -> Value {
    let mut props = Map::new();
    for field in fields {
        props.insert(field.name.as_ref().to_owned(), build_property(field));
    }
    Value::Object(props)
}

/// Translate one resolved field into its OpenSearch property: the mapped type,
/// any passthrough `extra` settings, and — for `object`/`nested` — its nested
/// `properties`.
fn build_property(field: &ResolvedField) -> Value {
    let mut prop = Map::new();
    prop.insert(
        "type".to_owned(),
        Value::String(opensearch_type(&field.mapping.mapping_type)),
    );
    for (key, value) in &field.mapping.extra {
        prop.insert(key.clone(), to_json(value));
    }
    if !field.children.is_empty() {
        prop.insert("properties".to_owned(), build_properties(&field.children));
    }
    Value::Object(prop)
}

/// The OpenSearch type string for a [`MappingType`].
fn opensearch_type(mapping_type: &MappingType) -> String {
    match mapping_type {
        MappingType::Text => "text",
        MappingType::Keyword => "keyword",
        MappingType::Boolean => "boolean",
        MappingType::Byte => "byte",
        MappingType::Short => "short",
        MappingType::Integer => "integer",
        MappingType::Long => "long",
        MappingType::Float => "float",
        MappingType::Double => "double",
        MappingType::HalfFloat => "half_float",
        MappingType::ScaledFloat => "scaled_float",
        MappingType::Date => "date",
        MappingType::Object => "object",
        MappingType::Nested => "nested",
        MappingType::Other(name) => name.as_str(),
    }
    .to_owned()
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

/// Serialize a slice of [`BulkAction`]s into an NDJSON bulk request body.
fn build_bulk_body(actions: &[BulkAction]) -> Result<String> {
    let mut body = String::new();
    for action in actions {
        match action {
            BulkAction::Index { index, id, doc } => {
                let meta =
                    serde_json::to_string(&json!({ "index": { "_index": index, "_id": id } }))
                        .map_err(|e| SinkError::Serialize(e.to_string()))?;
                let source = serde_json::to_string(doc)
                    .map_err(|e| SinkError::Serialize(e.to_string()))?;
                body.push_str(&meta);
                body.push('\n');
                body.push_str(&source);
                body.push('\n');
            }
            BulkAction::Delete { index, id } => {
                let meta =
                    serde_json::to_string(&json!({ "delete": { "_index": index, "_id": id } }))
                        .map_err(|e| SinkError::Serialize(e.to_string()))?;
                body.push_str(&meta);
                body.push('\n');
            }
        }
    }
    Ok(body)
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
            children,
        }
    }

    #[test]
    fn index_body_is_dynamic_strict_with_disabled_refresh() {
        let body = build_index_body(&[field("email", MappingType::Keyword, vec![])]);
        assert_eq!(body["mappings"]["dynamic"], "strict");
        assert_eq!(body["settings"]["index"]["refresh_interval"], "-1");
        assert_eq!(body["mappings"]["properties"]["email"]["type"], "keyword");
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
        let body = build_index_body(&[orders]);
        let orders = &body["mappings"]["properties"]["orders"];
        assert_eq!(orders["type"], "nested");
        assert_eq!(orders["properties"]["id"]["type"], "long");
        assert_eq!(orders["properties"]["total"]["type"], "double");
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
            children: vec![],
        };
        let body = build_index_body(&[amount]);
        let amount = &body["mappings"]["properties"]["amount"];
        assert_eq!(amount["type"], "scaled_float");
        assert_eq!(amount["scaling_factor"], 100);
    }

    #[test]
    fn other_mapping_type_uses_its_raw_name() {
        assert_eq!(opensearch_type(&MappingType::Other("binary".to_owned())), "binary");
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
        assert_eq!(build_bulk_url("http://localhost:9200", None, false), "http://localhost:9200/_bulk");
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
}
