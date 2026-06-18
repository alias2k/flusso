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
mod tests {
    use super::*;

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
    fn bulk_rejected_is_empty_when_no_errors_flag() {
        let resp = json!({ "errors": false, "items": [] });
        assert!(bulk_rejected(&resp, &[]).is_empty());
    }

    #[test]
    fn bulk_rejected_reports_the_item_with_a_4xx_status_and_its_reason() {
        let resp = json!({
            "errors": true,
            "items": [{ "index": {
                "_index": "users_ab12", "_id": "1", "status": 400,
                "error": { "type": "mapper_parsing_exception", "reason": "failed to parse field" }
            } }]
        });
        let rejected = bulk_rejected(&resp, &[]);
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].index, "users_ab12");
        assert_eq!(rejected[0].id, "1");
        assert_eq!(rejected[0].reason, "failed to parse field");
    }

    #[test]
    fn bulk_rejected_maps_position_to_the_originating_action() {
        // Two actions; the second is rejected. The rejection carries the
        // action's index/id (by position), not just the response's echo.
        let actions = [
            BulkAction::Delete {
                index: "users_ab12".to_owned(),
                id: "1".to_owned(),
            },
            BulkAction::Index {
                index: "users_ab12".to_owned(),
                id: "2".to_owned(),
                doc: json!({}),
            },
        ];
        let resp = json!({
            "errors": true,
            "items": [
                { "delete": { "_index": "users_ab12", "_id": "1", "status": 200 } },
                { "index": { "_index": "users_ab12", "_id": "2", "status": 400,
                             "error": { "reason": "boom" } } }
            ]
        });
        let rejected = bulk_rejected(&resp, &actions);
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].id, "2");
        assert_eq!(rejected[0].reason, "boom");
    }

    #[test]
    fn bulk_rejected_is_empty_when_all_items_succeed() {
        let resp = json!({
            "errors": true,
            "items": [{ "index": { "_index": "x", "_id": "1", "status": 200 } }]
        });
        assert!(bulk_rejected(&resp, &[]).is_empty());
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
