//! The HTTP plumbing against an OpenSearch cluster: the bulk request (with
//! retry, resending items refused as busy, and item-level rejection handling), the small request helpers, and the
//! generic per-index operations (create, exists, delete, refresh, settings,
//! listing). Higher layers ([`generations`](crate) and the [`Sink`](crate) impl)
//! build on these.
//!
//! Every `reqwest` failure becomes a [`SinkError::Write`] through
//! [`request_error`]: the operation, reqwest's own line, then each cause in its
//! source chain, with any credentials in the URL removed. reqwest's `Display`
//! alone stops at "error sending request for url (…)", so a refused
//! certificate, a refused connection, and a DNS failure would read the same:
//!
//! ```text
//! listing indices failed: error sending request for url (https://127.0.0.1:9200/_cat/indices/…): client error (Connect): invalid peer certificate: UnknownIssuer
//! ```

use std::collections::BTreeSet;
use std::time::Duration;

use kernel::IndexMapping;
use serde_json::{Value, json};
use sink::{RejectedDocument, Result, SinkError};
use tracing::{debug, warn};

use crate::OpensearchSink;
use crate::bulk::{BulkAction, build_bulk_url, bulk_action_fragment, read_bulk_response};
use crate::mapping::build_index_body;

/// A `reqwest` failure as a [`SinkError::Write`]: `context`, the error, and
/// every cause in its source chain, joined with `": "`. A cause whose text its
/// parent already printed is skipped.
///
/// The URL reqwest prints loses its userinfo first, username included:
/// userinfo in a configured `url` is a credential of some kind (a password, or
/// a token passed as the user) and must not reach the logs or `/status`.
/// reqwest already moves it into an `Authorization` header when it builds a
/// request; this keeps that true for any error whose URL came from elsewhere.
pub(crate) fn request_error(context: &str, mut error: reqwest::Error) -> SinkError {
    if let Some(url) = error.url_mut() {
        // Both fail only for a URL that cannot carry userinfo, which then has none.
        let _ = url.set_password(None);
        let _ = url.set_username("");
    }
    let mut message = format!("{context}: {error}");
    let mut parent = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !parent.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        parent = text;
        source = cause.source();
    }
    SinkError::Write(message)
}

/// Whether `error` is the server's certificate being refused: an untrusted
/// issuer, a name mismatch, an expired or missing certificate. No retry can
/// fix one. Read from the rustls error in the source chain.
pub(crate) fn is_certificate_error(error: &reqwest::Error) -> bool {
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if matches!(
            rustls_error(cause),
            Some(rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented)
        ) {
            return true;
        }
        source = cause.source();
    }
    false
}

/// The rustls error `cause` is or wraps. tokio-rustls hands it up inside an
/// `io::Error`, which hyper may wrap in another, and `io::Error::source` skips
/// the error it wraps, so each one is opened with `get_ref`.
fn rustls_error<'a>(cause: &'a (dyn std::error::Error + 'static)) -> Option<&'a rustls::Error> {
    if let Some(tls) = cause.downcast_ref::<rustls::Error>() {
        return Some(tls);
    }
    let inner = cause.downcast_ref::<std::io::Error>()?.get_ref()?;
    rustls_error(inner)
}

/// Whether no two of `actions` write the same document (index and `_id`).
fn one_write_per_document(actions: &[BulkAction]) -> bool {
    let mut seen = std::collections::HashSet::with_capacity(actions.len());
    actions
        .iter()
        .all(|action| seen.insert((action.index(), action.id())))
}

impl OpensearchSink {
    /// Send one prebuilt NDJSON `body` (the actions in `actions`, in order) as a
    /// single bulk request. The caller (`write`) is responsible for keeping
    /// `body` within the count and byte caps; this just transmits it.
    ///
    /// When `refresh` is `true`, appends `?refresh=true` to the URL so
    /// OpenSearch performs a segment refresh after the request completes and
    /// all documents become immediately searchable. Pass `false` for
    /// intermediate chunks; the final chunk in a write carries the refresh.
    ///
    /// **Transport and request-wide failures** (connection error, a non-2xx
    /// status for the whole request) are transient and **retried** with
    /// exponential backoff, except a refused server certificate, which fails
    /// at once: it is configuration (`ca_file`, `tls_verify`), not a blip.
    ///
    /// **Items refused as busy** (429 or 503 inside a 2xx response, see the
    /// `bulk` module docs) are resent alone, from the same retry budget and
    /// backoff: the accepted items are applied and never sent twice. Items
    /// still busy once `max_retries` is spent fail the write with an `Err`, so
    /// the sink engine restarts with its own backoff and the batch is
    /// redelivered; a busy item never reaches `on_error`.
    ///
    /// **Item-level rejections** — a mapping conflict, a malformed value — are
    /// **not** retried: re-sending the same document yields the same rejection.
    /// They are returned as [`RejectedDocument`]s, each once, for the engine's
    /// failure policy to handle, except a missing target, which is an `Err`
    /// (see the `bulk` module docs).
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

        let mut sending: Vec<&BulkAction> = actions.iter().collect();
        let mut resend_body: Option<String> = None;
        let mut rejected = Vec::new();
        let mut last_err: Option<SinkError> = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                let backoff = Duration::from_millis(200u64.saturating_mul(1u64 << (attempt - 1)));
                tokio::time::sleep(backoff).await;
                warn!(
                    attempt,
                    documents = sending.len(),
                    "retrying OpenSearch bulk request"
                );
            }

            let req = self
                .client
                .post(&url)
                .header("Content-Type", "application/x-ndjson")
                .body(resend_body.as_deref().unwrap_or(body).to_owned());

            match self.maybe_auth(req).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let result: Value = resp
                        .json()
                        .await
                        .map_err(|e| request_error("failed to read bulk response", e))?;

                    let read = read_bulk_response(&result, &sending)?;
                    debug!(
                        count = sending.len(),
                        rejected = read.rejected.len(),
                        busy = read.busy.len(),
                        refresh,
                        "bulk request applied",
                    );
                    rejected.extend(read.rejected);
                    if read.busy.is_empty() {
                        return Ok(rejected);
                    }
                    sending = read
                        .busy
                        .iter()
                        .filter_map(|&i| sending.get(i).copied())
                        .collect();
                    // Resending a subset reorders writes: it is safe only while
                    // a request holds one write per document (the ingest engine
                    // dedups ids per batch, and the sink engine never runs two
                    // writes holding the same document at once).
                    debug_assert!(
                        one_write_per_document(actions),
                        "a bulk request writes one document twice"
                    );
                    let mut body = String::new();
                    for action in &sending {
                        body.push_str(&bulk_action_fragment(action)?);
                    }
                    resend_body = Some(body);
                    let reason = read.busy_reason.unwrap_or_default();
                    warn!(
                        documents = sending.len(),
                        %reason,
                        "OpenSearch refused documents as busy; resending them",
                    );
                    let documents = sending.len();
                    let plural = if documents == 1 { "" } else { "s" };
                    last_err = Some(SinkError::Write(format!(
                        "{documents} document{plural} still refused as busy after {} retries \
                         (last: {reason}); failing the write so the batch is redelivered",
                        self.max_retries
                    )));
                }
                Ok(resp) => {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    last_err = Some(SinkError::Write(format!(
                        "bulk request failed: HTTP {status}: {text}"
                    )));
                }
                Err(e) if is_certificate_error(&e) => {
                    return Err(request_error(
                        "bulk request refused the server certificate (not retried)",
                        e,
                    ));
                }
                Err(e) => {
                    last_err = Some(request_error("bulk request error", e));
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
            .map_err(|e| request_error(context, e))?;
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
            .map_err(|e| request_error("index check failed", e))?;

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
            .json(&build_index_body(
                &mapping.fields,
                &self.index_options_for(&mapping.index),
            ));

        let resp = self
            .maybe_auth(req)
            .send()
            .await
            .map_err(|e| request_error("index create failed", e))?;

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

    /// Refresh several indexes in one request. A name that no longer exists (a
    /// generation a reindex dropped) is skipped: there is nothing left of it
    /// to make searchable.
    /// The number of primary shards `index` has, if the cluster says.
    pub(crate) async fn primary_shards(&self, index: &str) -> Result<Option<u32>> {
        let url = format!("{}/{index}/_settings/index.number_of_shards", self.base_url);
        let body: Value = self
            .send_ok(self.client.get(&url), "shard count lookup failed")
            .await?
            .json()
            .await
            .map_err(|e| request_error("failed to parse the shard count", e))?;
        Ok(body
            .as_object()
            .and_then(|indexes| indexes.values().next())
            .and_then(|settings| settings.pointer("/settings/index/number_of_shards"))
            .and_then(|shards| match shards {
                Value::String(text) => text.parse().ok(),
                other => other.as_u64().and_then(|n| u32::try_from(n).ok()),
            }))
    }

    pub(crate) async fn refresh_indexes(&self, indexes: &BTreeSet<String>) -> Result<()> {
        let names: Vec<&str> = indexes.iter().map(String::as_str).collect();
        let url = format!(
            "{}/{}/_refresh?ignore_unavailable=true",
            self.base_url,
            names.join(",")
        );
        self.send_ok(self.client.post(&url), "refresh failed")
            .await?;
        debug!(
            indexes = names.len(),
            "refreshed indexes written without a refresh"
        );
        Ok(())
    }

    /// Set `index`'s steady-state `refresh_interval` (the configured value, e.g.
    /// `"10s"`), called once seeding completes — it was `-1` (refresh off) at
    /// creation for fast bulk seeding. This is the visibility ceiling under
    /// load; a caught-up `write` refreshes sooner on top of it.
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
            .map_err(|e| request_error("listing indices failed", e))?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !resp.status().is_success() {
            return Err(Self::status_error(resp, "listing indices failed").await);
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| request_error("failed to parse _cat/indices", e))?;
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
            .map_err(|e| request_error("index delete failed", e))?;
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND {
            debug!(index, "dropped superseded generation");
            Ok(())
        } else {
            Err(Self::status_error(resp, "index delete failed").await)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
