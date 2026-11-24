//! Reading what the sink keeps about detaching an index (ADR 0012 stage 10,
//! slice 1): staging's first request for an index, before anything else
//! touches it.
//!
//! Three documents of the meta index are read in one `_mget`: the seed
//! marker (`{hash_alias}`), the detach record (`detach:{hash_alias}`) and the
//! stuck clock (`stuck:{hash_alias}`). Nothing here writes a record; a later
//! slice does. Two rules carry the weight:
//!
//! - **Only "nothing kept" reads as empty**: the meta index missing (a `404`
//!   or a per-document `index_not_found_exception`) or a document not found.
//!   Any other answer is an error, so a timed-out or refused read never
//!   passes for "no record" and the staging step is retried.
//! - **A marker a detach left half moved is put back.** A detach moves the
//!   seed marker to a new generation (keeping where it came from in
//!   `detaching_from`), then writes its record. A marker carrying
//!   `detaching_from` with no record beside it means the detach died between
//!   the two: the marker is put back on the generation it came from, as it
//!   was, conditionally on the marker read, so a write meanwhile wins. A
//!   `detaching_from` this version can't read is reported and put back
//!   nowhere: the index is staged as before, never failed for it (O24).
//! - **Every document answers for the id asked**, found or not found; an
//!   entry that says neither, or names another id, is an error.
//!
//! ```text
//! POST {meta}/_mget  {"ids": [alias, "detach:" + alias, "stuck:" + alias]}
//!   marker has detaching_from, no record  →  PUT {meta}/_doc/{alias}?if_seq_no=…
//! ```

use kernel::IndexMapping;
use serde_json::{Value, json};
use sink::{DetachRecord, Detachment, Result, SinkError, StuckClock};

use crate::OpensearchSink;
use crate::transport::request_error;

/// One document of the `_mget`, when found.
struct Found {
    source: Value,
    seq_no: Option<u64>,
    primary_term: Option<u64>,
}

impl OpensearchSink {
    /// What the meta index keeps about detaching `mapping`'s index, with a
    /// half-moved seed marker put back first.
    pub(crate) async fn read_detachment(&self, mapping: &IndexMapping) -> Result<Detachment> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let ids = [
            hash_alias.clone(),
            format!("detach:{hash_alias}"),
            format!("stuck:{hash_alias}"),
        ];
        let url = format!("{}/{}/_mget", self.base_url, self.meta_index());
        let context = "reading the detach records failed";
        let response = self
            .maybe_auth(self.client.post(&url).json(&json!({ "ids": ids })))
            .send()
            .await
            .map_err(|e| request_error(context, e))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| request_error(context, e))?;
        if status == reqwest::StatusCode::NOT_FOUND && is_index_not_found(&text) {
            return Ok(Detachment::default());
        }
        if !status.is_success() {
            return Err(SinkError::Write(format!(
                "{context}: HTTP {status}: {text}"
            )));
        }
        let body: Value = serde_json::from_str(&text)
            .map_err(|e| SinkError::Write(format!("{context}: {e}: {text}")))?;
        let docs = body
            .get("docs")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let [marker_id, record_id, stuck_id] = &ids;
        let [marker, record, stuck] = docs else {
            return Err(SinkError::Write(format!(
                "{context}: expected 3 documents, got {}: {text}",
                docs.len()
            )));
        };
        let (marker, record, stuck) = (
            found(marker, marker_id, context)?,
            found(record, record_id, context)?,
            found(stuck, stuck_id, context)?,
        );

        let mut kept = Detachment::default();
        kept.record = record.map(|record| read_record(&record.source));
        kept.stuck = stuck.map(|stuck| {
            let text = |key: &str| {
                stuck
                    .source
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            };
            StuckClock::new(text("since"), text("seen_at"))
        });
        if kept.record.is_none()
            && let Some(marker) = marker
            && let Some(from) = marker.source.get("detaching_from").filter(|v| !v.is_null())
        {
            match generation_of(from) {
                Ok(generation) => {
                    kept.restored = Some(
                        self.put_back(&hash_alias, &marker, from, generation)
                            .await?,
                    );
                }
                Err(why) => kept.unreadable_marker = Some(why),
            }
        }
        Ok(kept)
    }

    /// Put the seed marker back on `generation`, as `from` says it was,
    /// conditionally on `marker` as read: the generation put back.
    async fn put_back(
        &self,
        hash_alias: &str,
        marker: &Found,
        from: &Value,
        generation: u64,
    ) -> Result<u64> {
        let (Some(seq_no), Some(primary_term)) = (marker.seq_no, marker.primary_term) else {
            return Err(SinkError::Write(format!(
                "the seed marker of {hash_alias} was read without its sequence number"
            )));
        };
        let mut doc = json!({
            "active_generation": generation,
            "seeded": from.get("seeded").and_then(Value::as_bool).unwrap_or(false),
        });
        if let (Some(uuid), Some(fields)) = (
            from.get("uuid").and_then(Value::as_str),
            doc.as_object_mut(),
        ) {
            fields.insert("uuid".to_owned(), Value::from(uuid));
        }
        let url = format!(
            "{}/{}/_doc/{hash_alias}?if_seq_no={seq_no}&if_primary_term={primary_term}",
            self.base_url,
            self.meta_index()
        );
        self.send_ok(
            self.client.put(&url).json(&doc),
            "putting the seed marker back failed",
        )
        .await?;
        tracing::warn!(
            hash_alias,
            generation,
            "a detach left the seed marker moved with no record beside it; put back on the generation it came from"
        );
        Ok(generation)
    }
}

/// The generation a marker's `detaching_from` names, or why this version
/// can't read it.
fn generation_of(from: &Value) -> std::result::Result<u64, String> {
    if !from.is_object() {
        return Err(format!(
            "the seed marker's detaching_from is not an object: {from}"
        ));
    }
    from.get("generation")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("the seed marker's detaching_from names no generation: {from}"))
}

/// Whether `text` is OpenSearch's answer for a missing index.
fn is_index_not_found(text: &str) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|body| is_missing_index(&body))
}

/// Whether `answer` (a response, or one `_mget` document) is OpenSearch's
/// error for a missing index.
fn is_missing_index(answer: &Value) -> bool {
    answer
        .get("error")
        .and_then(|error| error.get("type"))
        .and_then(Value::as_str)
        == Some("index_not_found_exception")
}

/// One `_mget` document, which must answer for `id`: found, not found
/// (`None`), or an error, which only a missing meta index turns into "not
/// found". Anything else is an error.
fn found(doc: &Value, id: &str, context: &str) -> Result<Option<Found>> {
    let found = if let Some(error) = doc.get("error") {
        if !is_missing_index(doc) {
            return Err(SinkError::Write(format!("{context}: {error}")));
        }
        None
    } else {
        match doc.get("found").and_then(Value::as_bool) {
            Some(false) => None,
            Some(true) => Some(Found {
                source: doc.get("_source").cloned().unwrap_or(Value::Null),
                seq_no: doc.get("_seq_no").and_then(Value::as_u64),
                primary_term: doc.get("_primary_term").and_then(Value::as_u64),
            }),
            None => {
                return Err(SinkError::Write(format!(
                    "{context}: a document answered neither found nor an error: {doc}"
                )));
            }
        }
    };
    let answered = doc.get("_id").and_then(Value::as_str);
    if answered != Some(id) {
        return Err(SinkError::Write(format!(
            "{context}: asked for {id}, answered for {}: {doc}",
            answered.unwrap_or("no id")
        )));
    }
    Ok(found)
}

/// A detach record as this version reads it.
fn read_record(source: &Value) -> DetachRecord {
    let unreadable = |reason: String| DetachRecord::Unreadable { reason };
    if !source.is_object() {
        return unreadable(format!("the record is not an object: {source}"));
    }
    let live_only = match source.get("live_only") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(live_only)) => *live_only,
        Some(other) => return unreadable(format!("`live_only` is not a boolean: {other}")),
    };
    match source.get("state").and_then(Value::as_str) {
        Some("detached") => DetachRecord::Detached { live_only },
        Some("rebuilding") => DetachRecord::Rebuilding { live_only },
        Some(other) => unreadable(format!("its state `{other}` is not one this version knows")),
        None => unreadable(format!("it names no state: {source}")),
    }
}

#[cfg(test)]
mod tests;
