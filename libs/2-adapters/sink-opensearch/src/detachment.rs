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
//!   was, gaps included, conditionally on the marker read, so a write
//!   meanwhile wins. A
//!   `detaching_from` this version can't read is reported and put back
//!   nowhere: the index is staged as before, never failed for it (O24).
//! - **Every document answers for the id asked**, found or not found; an
//!   entry that says neither, or names another id, is an error.
//! - **Who wrote each document is returned, never judged.** A record or a
//!   marker carries the [`Stamp`] of its last write (`stamp`),
//!   which is read back as is; deciding on it is the engine's. A record
//!   forgotten is a tombstone (`state: "forgotten"`, with its stamp): read
//!   as no record, its stamp and version kept.
//!
//! ```text
//! POST {meta}/_mget  {"ids": [alias, "detach:" + alias, "stuck:" + alias]}
//!   marker has detaching_from, no record  →  PUT {meta}/_doc/{alias}?if_seq_no=…
//! ```

use kernel::IndexMapping;
use serde_json::{Value, json};
use sink::{
    DetachRecord, Detachment, MarkerAt, RecordVersion, Result, SinkError, Stamp, StuckClock,
};

use crate::OpensearchSink;
use crate::transport::request_error;

/// One document of the `_mget`, when found.
struct Found {
    source: Value,
    seq_no: Option<u64>,
    primary_term: Option<u64>,
}

/// The three documents of one index's `_mget`, each when found.
struct Kept {
    marker: Option<Found>,
    record: Option<Found>,
    stuck: Option<Found>,
}

impl Found {
    /// The `(seq_no, primary_term)` a write conditional on this read names.
    fn version(&self) -> Option<(u64, u64)> {
        self.seq_no.zip(self.primary_term)
    }

    /// Whether this is a record's tombstone, left where a record was
    /// forgotten.
    fn is_tombstone(&self) -> bool {
        self.source.get("state").and_then(Value::as_str) == Some(FORGOTTEN)
    }
}

/// The `state` of a record's tombstone.
pub(crate) const FORGOTTEN: &str = "forgotten";

/// A stamp as the documents keep it: `null` with none.
pub(crate) fn stamp_value(stamp: Option<&Stamp>) -> Value {
    stamp.map_or(
        Value::Null,
        |stamp| json!({ "writer": stamp.writer, "seq": stamp.seq }),
    )
}

/// Gaps as a marker keeps them: a list of `{from, to}`.
pub(crate) fn gaps_value(gaps: &[sink::Gap]) -> Value {
    gaps.iter()
        .map(|gap| json!({ "from": gap.from, "to": gap.to }))
        .collect()
}

/// The gaps a moved marker's `detaching_from` carries, to restore when the
/// marker comes back on the serving generation (the review of round 11,
/// S-1). `None` for none.
pub(crate) fn carried_gaps(from: Option<&Value>) -> Option<Value> {
    let gaps = read_gaps(from?);
    (!gaps.is_empty()).then(|| gaps_value(&gaps))
}

/// The stamp a document keeps, when it keeps one this version reads.
fn read_stamp(source: &Value) -> Option<Stamp> {
    let stamp = source.get("stamp")?;
    Some(Stamp::new(
        stamp.get("writer").and_then(Value::as_str)?,
        stamp.get("seq").and_then(Value::as_u64)?,
    ))
}

impl OpensearchSink {
    /// What the meta index keeps about detaching `mapping`'s index, with a
    /// half-moved seed marker put back first, stamped `stamp`.
    pub(crate) async fn read_detachment(
        &self,
        mapping: &IndexMapping,
        stamp: &Stamp,
    ) -> Result<Detachment> {
        let hash_alias = self.hash_alias(mapping.index.as_ref(), &mapping.hash.to_string());
        let Kept {
            marker,
            record,
            stuck,
        } = self.read_kept(&hash_alias).await?;

        let mut kept = Detachment::default();
        if let Some(found) = &record {
            kept.record_stamp = read_stamp(&found.source);
            kept.version = found
                .version()
                .map(|(seq_no, term)| RecordVersion::new(format!("{seq_no}:{term}")));
        }
        let record = record.filter(|found| !found.is_tombstone());
        kept.marker_stamp = marker.as_ref().and_then(|found| read_stamp(&found.source));
        if let Some(record) = &record {
            let serving = record.source.get("serving").and_then(Value::as_u64);
            kept.marker = marker
                .as_ref()
                .and_then(|marker| marker_at(&marker.source, serving));
            kept.attempts = record.source.get("attempts").and_then(Value::as_u64);
            kept.through = record
                .source
                .get("through")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
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
        kept.gaps = marker
            .as_ref()
            .map(|marker| read_gaps(&marker.source))
            .unwrap_or_default();
        if kept.record.is_none()
            && let Some(marker) = marker
            && let Some(from) = marker.source.get("detaching_from").filter(|v| !v.is_null())
        {
            match generation_of(from) {
                Ok(generation) => {
                    kept.restored = Some(
                        self.put_back(&hash_alias, &marker, from, generation, stamp)
                            .await?,
                    );
                    kept.marker_stamp = Some(stamp.clone());
                    kept.gaps = read_gaps(from);
                }
                Err(why) => kept.unreadable_marker = Some(why),
            }
        }
        Ok(kept)
    }

    /// The seed marker, the detach record and the stuck clock of
    /// `hash_alias`, in one `_mget`, each with its version.
    async fn read_kept(&self, hash_alias: &str) -> Result<Kept> {
        let ids = [
            hash_alias.to_owned(),
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
            return Ok(Kept {
                marker: None,
                record: None,
                stuck: None,
            });
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
        Ok(Kept {
            marker: found(marker, marker_id, context)?,
            record: found(record, record_id, context)?,
            stuck: found(stuck, stuck_id, context)?,
        })
    }

    /// Put the seed marker back on `generation`, as `from` says it was,
    /// stamped `stamp`, conditionally on `marker` as read: the generation
    /// put back.
    async fn put_back(
        &self,
        hash_alias: &str,
        marker: &Found,
        from: &Value,
        generation: u64,
        stamp: &Stamp,
    ) -> Result<u64> {
        let mut doc = json!({
            "active_generation": generation,
            "seeded": from.get("seeded").and_then(Value::as_bool).unwrap_or(false),
        });
        if let Some(fields) = doc.as_object_mut() {
            if let Some(uuid) = from.get("uuid").and_then(Value::as_str) {
                fields.insert("uuid".to_owned(), Value::from(uuid));
            }
            if let Some(walked) = from.get("walked").filter(|walked| !walked.is_null()) {
                fields.insert("walked".to_owned(), walked.clone());
            }
            if let Some(gaps) = carried_gaps(Some(from)) {
                fields.insert("gaps".to_owned(), gaps);
            }
        }
        if let record::Written::Conflict = self
            .put_if(hash_alias, &doc, Some(marker), Some(stamp))
            .await?
        {
            return Err(SinkError::Write(format!(
                "putting the seed marker of {hash_alias} back failed: it changed since it was read"
            )));
        }
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

/// Where `marker` stands against a record naming `serving`: `None` for a
/// marker naming no generation.
fn marker_at(marker: &Value, serving: Option<u64>) -> Option<MarkerAt> {
    let generation = marker.get("active_generation").and_then(Value::as_u64)?;
    let seeded = marker
        .get("seeded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(match serving {
        Some(serving) if serving == generation => MarkerAt::Serving { seeded },
        Some(serving) if serving < generation => MarkerAt::Newer { seeded },
        _ => MarkerAt::Elsewhere { seeded },
    })
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

mod record;

#[cfg(test)]
mod tests;

/// The gaps a seed marker keeps: `gaps`, or a single `gap` as an earlier
/// step 4 wrote it, read as a list of one. A gap that can't be read is
/// never "no gap" (review of the step-4 fixes, m-c): it reads as one with
/// both ends unknown, which refuses every position until a read while the
/// slot is held settles it, and is logged loudly.
fn read_gaps(marker: &Value) -> Vec<sink::Gap> {
    let one = |gap: &Value| {
        if !gap.is_object() {
            tracing::error!(%gap, "a gap the seed marker keeps can't be read: taken as a gap with both ends unknown, which refuses every position until it is settled. flusso settles it by itself at its next read while it holds the slot (to where the lane stands, refusing only the past from there); a detached lane keeps it until it is attached. Meanwhile a reindex on a sink that can rebuild clears it");
        }
        let text = |key: &str| gap.get(key).and_then(Value::as_str).map(str::to_owned);
        sink::Gap {
            from: text("from"),
            to: text("to"),
        }
    };
    match (
        marker.get("gaps").filter(|gaps| !gaps.is_null()),
        marker.get("gap").filter(|gap| !gap.is_null()),
    ) {
        (Some(Value::Array(gaps)), _) => gaps.iter().map(one).collect(),
        (Some(other), _) | (None, Some(other)) => vec![one(other)],
        (None, None) => Vec::new(),
    }
}
