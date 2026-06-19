//! The OpenSearch bulk wire format: the buffered [`BulkAction`], its NDJSON
//! encoding, the byte/count-aware request chunking, and item-level rejection
//! parsing — the low-level pieces `flush` and
//! `send_bulk_chunk` are built from.

use serde_json::{Value, json};
use sinks_core::{RejectedDocument, Result, SinkError};

/// A buffered write destined for OpenSearch.
#[derive(Debug)]
pub(crate) enum BulkAction {
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

impl BulkAction {
    pub(crate) fn index(&self) -> &str {
        match self {
            BulkAction::Index { index, .. } | BulkAction::Delete { index, .. } => index,
        }
    }

    pub(crate) fn id(&self) -> &str {
        match self {
            BulkAction::Index { id, .. } | BulkAction::Delete { id, .. } => id,
        }
    }
}

/// Build the `/_bulk` URL with optional pipeline and refresh parameters.
pub(crate) fn build_bulk_url(base_url: &str, pipeline: Option<&str>, refresh: bool) -> String {
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
/// code builds bodies fragment-by-fragment in `flush`
/// (to honor the byte cap), so this whole-slice form is now a test convenience.
#[cfg(test)]
pub(crate) fn build_bulk_body(actions: &[BulkAction]) -> Result<String> {
    let mut body = String::new();
    for action in actions {
        body.push_str(&bulk_action_fragment(action)?);
    }
    Ok(body)
}

/// Serialize one [`BulkAction`] into its NDJSON fragment — the metadata line
/// and, for an index op, the source line, each newline-terminated. This is the
/// single place the bulk wire format is produced; the crate-private `build_bulk_body`
/// and the byte-aware chunking in `flush` both go through it.
pub(crate) fn bulk_action_fragment(action: &BulkAction) -> Result<String> {
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
pub(crate) fn plan_chunks(sizes: &[usize], batch_size: usize, max_bytes: usize) -> Vec<usize> {
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

/// Extract the item-level rejections from a 2xx bulk response.
///
/// A bulk response is `errors: true` when *any* item failed, but its accepted
/// items are still applied. Each entry in `items` reports a per-document status;
/// those `>= 400` are rejections. They are matched **by position** to `actions`
/// (the bulk API preserves order), so the rejection carries the originating
/// document's index and id; if the arrays ever disagree, the response's own
/// `_index`/`_id` are used as a fallback. Returns empty when nothing failed.
pub(crate) fn bulk_rejected(response: &Value, actions: &[BulkAction]) -> Vec<RejectedDocument> {
    let has_errors = response
        .get("errors")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !has_errors {
        return Vec::new();
    }

    let Some(items) = response.get("items").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut rejected = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let op = item
            .get("index")
            .or_else(|| item.get("create"))
            .or_else(|| item.get("delete"))
            .or_else(|| item.get("update"));
        let Some(op) = op else { continue };

        let status = op.get("status").and_then(Value::as_u64).unwrap_or(0);
        if status < 400 {
            continue;
        }

        let reason = op
            .get("error")
            .and_then(|e| e.get("reason"))
            .and_then(|r| r.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("rejected with status {status}"));

        let (index, id) = match actions.get(i) {
            Some(action) => (action.index().to_owned(), action.id().to_owned()),
            None => (
                op.get("_index")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                op.get("_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned(),
            ),
        };
        rejected.push(RejectedDocument { index, id, reason });
    }
    rejected
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
