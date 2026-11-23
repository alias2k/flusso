//! The OpenSearch bulk wire format: the buffered [`BulkAction`], its NDJSON
//! encoding, the byte/count-aware request chunking, and item-level rejection
//! parsing — the low-level pieces `write` and
//! `send_bulk_chunk` are built from.
//!
//! An index action addressed to a hash alias carries `require_alias: true`
//! per item, so a write to an alias that no longer exists fails instead of
//! letting OpenSearch auto-create a dynamic-mapped index under that name. The
//! flag is per item, not `?require_alias=true` on the URL, because one bulk
//! request mixes alias targets (seeded indexes) and concrete generations
//! (an unseeded rebuild), and the URL flag would fail the latter.
//!
//! ```text
//! {"index":{"_index":"users_1a2b","_id":"42","require_alias":true}}
//! {"email":"ada@x.io"}
//! ```
//!
//! Two refusals are about an **index**, not a document, and are reported per
//! index rather than per document (see [`read_bulk_response`]):
//!
//! - **missing**: `index_not_found_exception`, the target is gone (deleted
//!   while running); the engine stages that index again;
//! - **blocked**: `cluster_block_exception` (403: `index.blocks.write`,
//!   `read_only`, `read_only_allow_delete`), an item-level 403
//!   `security_exception` (a role without write permission on that index), or
//!   `index_closed_exception` (a closed index, `POST /<index>/_close`); the
//!   documents are fine, so the engine parks the index's lane and writes them
//!   again once the block is lifted or the index opened, never quarantining
//!   them (#404, stage 9 slice 3).
//!
//! ```text
//! {"index":{"_index":"users_1a2b_1","_id":"42","status":403,
//!   "error":{"type":"cluster_block_exception","reason":"index [users_1a2b_1] blocked by: [FORBIDDEN/8/index write (api)];"}}}
//! ```
//!
//! A response item refused with 429 or 503 is **busy**, not rejected: a full
//! write queue, a tripped circuit breaker, a primary shard not yet active. The
//! document is fine, so `send_bulk_chunk` resends those items alone, with
//! backoff, and they never reach `on_error`:
//!
//! ```text
//! {"index":{"_index":"users_1a2b_1","_id":"42","status":429,
//!   "error":{"type":"rejected_execution_exception","reason":"rejected execution of … on …[name = node/write, …]"}}}
//! ```
//!
//! Some ids OpenSearch refuses before it runs the request at all, and one of
//! them fails the **whole** request with HTTP 400: an index action whose `_id`
//! is empty or longer than [`MAX_ID_BYTES`] UTF-8 bytes, and a delete whose
//! `_id` is empty. [`refused_id`] finds those, so `write` never sends them and
//! sends the rest: an index action is rejected alone, a delete is dropped,
//! since no document can have an empty id. A delete with an over-long id is
//! sent: OpenSearch accepts it and answers `not_found`.
//!
//! ```text
//! Validation Failed: 1: id [aaa…] is too long, must be no longer than 512 bytes but was: 513;
//! ```

use kernel::GenericValue;
use serde_json::Value;
use sink::{Json, RejectedDocument, Result, SinkError};

/// One document's bulk action, destined for OpenSearch.
#[derive(Debug)]
pub(crate) enum BulkAction {
    Index {
        index: String,
        id: String,
        /// The `_source` line: the document as [`render_document`] wrote it.
        doc: String,
        /// `index` is an alias that must exist; never auto-create it.
        require_alias: bool,
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

/// A document as the JSON `_source` line of a bulk index action: every
/// number as a JSON number, a decimal with its exact digits ([`sink::Json`]).
pub(crate) fn render_document(document: &GenericValue) -> Result<String> {
    serde_json::to_string(&Json(document)).map_err(|e| SinkError::Serialize(e.to_string()))
}

/// The longest `_id` OpenSearch accepts, in UTF-8 bytes.
pub(crate) const MAX_ID_BYTES: usize = 512;

/// Why OpenSearch would refuse `action`'s id before running the bulk request,
/// failing every other action in it too; `None` when it would accept it.
pub(crate) fn refused_id(action: &BulkAction) -> Option<String> {
    let id = action.id();
    if id.is_empty() {
        return Some("document id is empty; OpenSearch needs a non-empty _id".to_owned());
    }
    match action {
        BulkAction::Index { .. } if id.len() > MAX_ID_BYTES => Some(format!(
            "document id is {} bytes; OpenSearch accepts at most {MAX_ID_BYTES} bytes in an _id",
            id.len()
        )),
        _ => None,
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
/// code builds bodies fragment-by-fragment in `write`
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
/// and the byte-aware chunking in `write` both go through it.
pub(crate) fn bulk_action_fragment(action: &BulkAction) -> Result<String> {
    match action {
        BulkAction::Index {
            index,
            id,
            doc,
            require_alias,
        } => {
            let mut fragment = action_line("index", index, id, *require_alias, doc.len() + 1)?;
            fragment.push_str(doc);
            fragment.push('\n');
            Ok(fragment)
        }
        BulkAction::Delete { index, id } => action_line("delete", index, id, false, 0),
    }
}

/// The action's metadata line, `{"<op>":{"_index":…,"_id":…}}` and a newline,
/// with room for `extra` more bytes. Written directly: no value tree.
fn action_line(
    op: &str,
    index: &str,
    id: &str,
    require_alias: bool,
    extra: usize,
) -> Result<String> {
    let quoted =
        |text: &str| serde_json::to_string(text).map_err(|e| SinkError::Serialize(e.to_string()));
    let (index, id) = (quoted(index)?, quoted(id)?);
    let mut line = String::with_capacity(48 + op.len() + index.len() + id.len() + extra);
    line.push_str("{\"");
    line.push_str(op);
    line.push_str("\":{\"_index\":");
    line.push_str(&index);
    line.push_str(",\"_id\":");
    line.push_str(&id);
    if require_alias {
        line.push_str(",\"require_alias\":true");
    }
    line.push_str("}}\n");
    Ok(line)
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

/// What a 2xx bulk response says about the actions it answered.
#[derive(Debug, Default)]
pub(crate) struct BulkResponse {
    /// Documents refused for a reason resending won't change.
    pub(crate) rejected: Vec<RejectedDocument>,
    /// Positions, in the request, of the actions refused as busy.
    pub(crate) busy: Vec<usize>,
    /// How the last busy item was refused: status, error type and reason.
    pub(crate) busy_reason: Option<String>,
    /// Positions, in the request, of the actions whose index refuses writes
    /// for now, with why.
    pub(crate) blocked: Vec<(usize, String)>,
    /// Positions, in the request, of the actions whose target is gone, with
    /// why.
    pub(crate) missing: Vec<(usize, String)>,
}

/// Whether an item refused with `status` is the cluster being busy, so the
/// same document can be written later: 429 (a full write queue, a tripped
/// circuit breaker, a flood-stage disk block) and 503 (a shard unavailable or
/// timed out). Every other status is about the document.
pub(crate) fn is_busy(status: u64) -> bool {
    matches!(status, 429 | 503)
}

/// Whether an item refused with error `kind` means its index is gone: every document bound for it fails the same way.
pub(crate) fn is_missing(kind: Option<&str>) -> bool {
    kind == Some("index_not_found_exception")
}

/// Whether an item refused with `status` and error `kind` means its index
/// refuses writes for now, whatever the document holds: a write block
/// (`cluster_block_exception`; the flood-stage one answers 429 and is busy,
/// asked first) or a role without write permission on the index (an
/// item-level 403 `security_exception`; a request-level 403 refuses the whole
/// `_bulk` and fails the write), or a closed index (`index_closed_exception`,
/// whatever the status).
pub(crate) fn is_blocked(status: u64, kind: Option<&str>) -> bool {
    matches!(
        kind,
        Some("cluster_block_exception" | "index_closed_exception")
    ) || (status == 403 && kind == Some("security_exception"))
}

/// The reason a blocked item is reported with: OpenSearch's, except for a
/// closed index, whose reason (`closed`) names nothing: then the index, from
/// the error, else from the action.
fn blocked_reason(
    error: Option<&Value>,
    kind: Option<&str>,
    reason: String,
    action: Option<&BulkAction>,
) -> String {
    if kind != Some("index_closed_exception") {
        return reason;
    }
    let index = error
        .and_then(|e| e.get("index"))
        .and_then(Value::as_str)
        .or_else(|| action.map(BulkAction::index))
        .unwrap_or("?");
    format!("index [{index}] is closed")
}

/// Read the item-level outcome of a 2xx bulk response to `actions`.
///
/// A bulk response is `errors: true` when *any* item failed, but its accepted
/// items are still applied. Each entry in `items` reports a per-document
/// status, matched **by position** to `actions` (the bulk API preserves
/// order). Of those `>= 400`:
///
/// - **busy** ([`is_busy`]): listed by position, for the caller to resend;
/// - anything else is a **rejection** carrying the originating document's
///   index and id; if the arrays ever disagree, the response's own
///   `_index`/`_id` are used as a fallback.
///
/// - **missing** ([`is_missing`]): the target index or alias is gone, so
///   every document bound for it fails the same way; listed by position;
/// - **blocked** ([`is_blocked`]): the index refuses writes for now; listed
///   by position.
///
/// A busy, missing or blocked item past the end of `actions` can't be told
/// apart from the rest: that returns a write-wide `Err`, and the write is
/// redelivered whole.
pub(crate) fn read_bulk_response(
    response: &Value,
    actions: &[&BulkAction],
) -> Result<BulkResponse> {
    let mut read = BulkResponse::default();
    let has_errors = response
        .get("errors")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !has_errors {
        return Ok(read);
    }

    let Some(items) = response.get("items").and_then(|v| v.as_array()) else {
        return Ok(read);
    };

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

        let error = op.get("error");
        let kind = error.and_then(|e| e.get("type")).and_then(Value::as_str);
        let reason = error
            .and_then(|e| e.get("reason"))
            .and_then(|r| r.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("rejected with status {status}"));

        let action = actions.get(i);
        if is_busy(status) {
            if action.is_none() {
                return Err(SinkError::Write(format!(
                    "bulk response item {i} was refused as busy (HTTP {status}) but the request \
                     had only {} actions; failing the write so the batch is redelivered",
                    actions.len()
                )));
            }
            read.busy.push(i);
            read.busy_reason = Some(format!(
                "HTTP {status} {}: {reason}",
                kind.unwrap_or("error")
            ));
            continue;
        }

        let missing = is_missing(kind);
        if missing || is_blocked(status, kind) {
            if action.is_none() {
                return Err(SinkError::Write(format!(
                    "bulk response item {i} refused its index (HTTP {status} {}) but the request \
                     had only {} actions; failing the write so the batch is redelivered",
                    kind.unwrap_or("error"),
                    actions.len()
                )));
            }
            if missing {
                read.missing.push((i, reason));
            } else {
                read.blocked
                    .push((i, blocked_reason(error, kind, reason, action.copied())));
            }
            continue;
        }

        let (index, id) = match action {
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
        read.rejected.push(RejectedDocument { index, id, reason });
    }
    Ok(read)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
