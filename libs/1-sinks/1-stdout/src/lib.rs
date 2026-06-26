#![doc = include_str!("../README.md")]

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use schema_core::{GenericValue, IndexName};
use serde_json::{Value, json};
use sinks_core::{FlushReport, Result, Sink, SinkError, to_json};

/// Identifies this sink in every envelope's `sink` field.
const SINK_NAME: &str = "stdout";

/// This crate's version, stamped into every envelope's `version` field.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Writes each document operation to stdout as a JSON envelope.
#[derive(Debug, Clone)]
pub struct StdoutSink {
    pretty: bool,
    /// Monotonic per-sink counter stamped into each envelope as `seq`. Shared
    /// across clones so one logical sink yields one continuous sequence.
    seq: Arc<AtomicU64>,
}

impl StdoutSink {
    /// Create a sink. `pretty` selects pretty-printed JSON over compact NDJSON.
    pub fn new(pretty: bool) -> Self {
        Self {
            pretty,
            seq: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn from_config(config: &schema_core::StdoutSink) -> Self {
        Self::new(config.pretty)
    }

    /// Claim the next sequence number for an emitted envelope, starting at 1.
    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    fn render(&self, envelope: &Value) -> Result<String> {
        let json = if self.pretty {
            serde_json::to_string_pretty(envelope)
        } else {
            serde_json::to_string(envelope)
        };
        json.map_err(|e| SinkError::Serialize(e.to_string()))
    }

    /// Write a rendered line to stdout. Uses the stdout handle directly (not the
    /// `print!` family) so it stays a real data sink, not logging.
    fn write_line(&self, line: &str) -> Result<()> {
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        handle
            .write_all(line.as_bytes())
            .and_then(|()| handle.write_all(b"\n"))
            .map_err(|e| SinkError::Write(e.to_string()))
    }
}

#[async_trait]
impl Sink for StdoutSink {
    async fn upsert(&self, index: &IndexName, id: &str, document: &GenericValue) -> Result<()> {
        let envelope = upsert_envelope(self.next_seq(), &now(), index, id, document);
        let line = self.render(&envelope)?;
        self.write_line(&line)
    }

    async fn delete(&self, index: &IndexName, id: &str) -> Result<()> {
        let envelope = delete_envelope(self.next_seq(), &now(), index, id);
        let line = self.render(&envelope)?;
        self.write_line(&line)
    }

    async fn flush(&self, _caught_up: bool) -> Result<FlushReport> {
        // stdout has no visibility/durability split, so the caught-up hint is
        // irrelevant — just flush the writer. It never rejects a document: a
        // line either writes or the whole flush errors.
        std::io::stdout()
            .lock()
            .flush()
            .map_err(|e| SinkError::Write(e.to_string()))?;
        Ok(FlushReport::clean())
    }
}

/// The current instant as an RFC 3339 / ISO 8601 UTC timestamp with millisecond
/// precision (e.g. `2026-06-03T10:20:30.123Z`).
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Provenance fields common to every envelope: which sink and version produced
/// it, when, and in what order.
fn header(
    seq: u64,
    ts: &str,
    index: &IndexName,
    op: &str,
    id: &str,
) -> serde_json::Map<String, Value> {
    json!({
        "sink": SINK_NAME,
        "version": VERSION,
        "ts": ts,
        "seq": seq,
        "index": index.as_ref(),
        "op": op,
        "id": id,
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}

/// At-a-glance facts about a serialized document: how many top-level fields it
/// has (`null` when it isn't an object) and its compact byte size.
fn document_meta(document: &Value) -> Value {
    json!({
        "fields": document.as_object().map(serde_json::Map::len),
        "bytes": document.to_string().len(),
    })
}

fn upsert_envelope(
    seq: u64,
    ts: &str,
    index: &IndexName,
    id: &str,
    document: &GenericValue,
) -> Value {
    let document = to_json(document);
    let mut envelope = header(seq, ts, index, "upsert", id);
    envelope.insert("meta".to_owned(), document_meta(&document));
    envelope.insert("document".to_owned(), document);
    Value::Object(envelope)
}

fn delete_envelope(seq: u64, ts: &str, index: &IndexName, id: &str) -> Value {
    Value::Object(header(seq, ts, index, "delete", id))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
