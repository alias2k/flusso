//! The HTTP plumbing against an OpenSearch cluster: the bulk request (with
//! retry and item-level rejection handling), the small request helpers, and the
//! generic per-index operations (create, exists, delete, refresh, settings,
//! listing). Higher layers ([`generations`](crate) and the [`Sink`](crate) impl)
//! build on these.

use std::time::Duration;

use schema_core::IndexMapping;
use serde_json::{Value, json};
use sinks_core::{RejectedDocument, Result, SinkError};
use tracing::{debug, warn};

use crate::OpensearchSink;
use crate::bulk::{BulkAction, build_bulk_url, bulk_rejected};
use crate::mapping::build_index_body;

impl OpensearchSink {
    /// Send one prebuilt NDJSON `body` (the actions in `actions`, in order) as a
    /// single bulk request. The caller (`flush`) is responsible for keeping
    /// `body` within the count and byte caps; this just transmits it.
    ///
    /// When `refresh` is `true`, appends `?refresh=true` to the URL so
    /// OpenSearch performs a segment refresh after the request completes and
    /// all documents become immediately searchable. Pass `false` for
    /// intermediate chunks; the final chunk in a flush carries the refresh.
    ///
    /// **Transport and request-wide failures** (connection error, a non-2xx
    /// status for the whole request) are transient and **retried** with
    /// exponential backoff. **Item-level rejections** — a 2xx bulk response in
    /// which OpenSearch accepted some actions and refused others (a mapping
    /// conflict, a malformed value) — are **not** retried: re-sending the same
    /// document yields the same rejection. They are returned as
    /// [`RejectedDocument`]s for the engine's failure policy to handle, while
    /// the accepted actions in the same request stay applied.
    #[tracing::instrument(
        name = "os.bulk",
        level = "debug",
        skip_all,
        fields(count = actions.len(), bytes = body.len(), refresh),
        err,
    )]
    pub(crate) async fn send_bulk_chunk(
        &self,
        body: &str,
        actions: &[BulkAction],
        refresh: bool,
    ) -> Result<Vec<RejectedDocument>> {
        if actions.is_empty() {
            return Ok(Vec::new());
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

                    let rejected = bulk_rejected(&result, actions);
                    debug!(
                        count = actions.len(),
                        rejected = rejected.len(),
                        refresh,
                        "bulk request applied",
                    );
                    return Ok(rejected);
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

    /// Turn a non-success response into a `Write` error, draining its body for
    /// diagnostics. `context` names the operation (e.g. `"refresh failed"`).
    pub(crate) async fn status_error(resp: reqwest::Response, context: &str) -> SinkError {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        SinkError::Write(format!("{context}: HTTP {status}: {text}"))
    }

    /// Send `req` (with auth applied) and require a 2xx response, returning it.
    /// Both the transport failure and the non-success status are reported with
    /// the `context` prefix.
    pub(crate) async fn send_ok(
        &self,
        req: reqwest::RequestBuilder,
        context: &str,
    ) -> Result<reqwest::Response> {
        let resp = self
            .maybe_auth(req)
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("{context}: {e}")))?;
        if resp.status().is_success() {
            Ok(resp)
        } else {
            Err(Self::status_error(resp, context).await)
        }
    }

    /// Whether `index` already exists in the cluster.
    pub(crate) async fn index_exists(&self, index: &str) -> Result<bool> {
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
        Err(Self::status_error(resp, "index check failed").await)
    }

    /// `PUT /{index}` with the explicit mapping body. Tolerates losing a
    /// creation race — a concurrent creator winning is fine, the index exists.
    pub(crate) async fn create_index(&self, index: &str, mapping: &IndexMapping) -> Result<()> {
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
        if text.contains("resource_already_exists_exception") {
            return Ok(());
        }
        Err(SinkError::Write(format!(
            "index create failed: HTTP {status}: {text}"
        )))
    }

    /// Force a one-off refresh so everything written to `index` so far becomes
    /// searchable, regardless of the index's refresh interval.
    pub(crate) async fn refresh_index(&self, index: &str) -> Result<()> {
        let url = format!("{}/{index}/_refresh", self.base_url);
        self.send_ok(self.client.post(&url), "refresh failed")
            .await?;
        debug!(index, "refreshed index");
        Ok(())
    }

    /// Set `index`'s steady-state `refresh_interval` (the configured value, e.g.
    /// `"10s"`), called once seeding completes — it was `-1` (refresh off) at
    /// creation for fast bulk seeding. This is the visibility ceiling under
    /// load; a caught-up `flush` refreshes sooner on top of it.
    pub(crate) async fn restore_auto_refresh(&self, index: &str) -> Result<()> {
        let url = format!("{}/{index}/_settings", self.base_url);
        let req = self
            .client
            .put(&url)
            .header("Content-Type", "application/json")
            .json(&json!({ "index": { "refresh_interval": self.refresh_interval } }));

        self.send_ok(req, "restore refresh failed").await?;
        debug!(
            index,
            refresh_interval = self.refresh_interval,
            "set steady-state refresh interval on index"
        );
        Ok(())
    }

    /// Concrete index names matching `pattern` (a `_cat/indices` glob, e.g.
    /// `users_ab12_*`). An alias in the pattern resolves to the indexes behind
    /// it; a no-match (404) is an empty list, not an error.
    pub(crate) async fn list_indices(&self, pattern: &str) -> Result<Vec<String>> {
        let url = format!(
            "{}/_cat/indices/{pattern}?h=index&format=json",
            self.base_url
        );
        let resp = self
            .maybe_auth(self.client.get(&url))
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("listing indices failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !resp.status().is_success() {
            return Err(Self::status_error(resp, "listing indices failed").await);
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| SinkError::Write(format!("failed to parse _cat/indices: {e}")))?;
        Ok(body
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row.get("index").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Delete an index. A 404 (already gone) is success — dropping a superseded
    /// generation must be idempotent across retries.
    pub(crate) async fn delete_index(&self, index: &str) -> Result<()> {
        let url = format!("{}/{index}", self.base_url);
        let resp = self
            .maybe_auth(self.client.delete(&url))
            .send()
            .await
            .map_err(|e| SinkError::Write(format!("index delete failed: {e}")))?;
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND {
            debug!(index, "dropped superseded generation");
            Ok(())
        } else {
            Err(Self::status_error(resp, "index delete failed").await)
        }
    }
}
