//! A [`Sink`] that writes documents to standard output as JSON.
//!
//! Each operation is emitted as a JSON envelope — one line of NDJSON by
//! default, or pretty-printed when configured — which makes the pipeline's
//! output easy to watch or pipe into `jq` during development.
//!
//! Alongside the operation itself, every envelope carries provenance and
//! bookkeeping so a stream is self-describing: which sink and version produced
//! it (`sink`, `version`), when (`ts`), in what order (`seq`), and a quick
//! `meta` summary of the document (top-level field count and serialized byte
//! size).
//!
//! ```text
//! {"document":{"email":"ada@x.io"},"id":"42","index":"users","meta":{"bytes":20,"fields":1},"op":"upsert","seq":1,"sink":"stdout","ts":"2026-06-03T10:20:30.123Z","version":"0.1.0"}
//! {"id":"7","index":"users","op":"delete","seq":2,"sink":"stdout","ts":"2026-06-03T10:20:30.124Z","version":"0.1.0"}
//! ```

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use schema_core::{GenericValue, IndexName};
use serde_json::{Value, json};
use sinks_core::{Result, Sink, SinkError, to_json};

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

    /// Build a sink from the schema's stdout sink configuration.
    pub fn from_config(config: &schema_core::StdoutSink) -> Self {
        Self::new(config.pretty)
    }

    /// Claim the next sequence number for an emitted envelope, starting at 1.
    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// Serialize one envelope to a single output line.
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

    async fn flush(&self) -> Result<()> {
        std::io::stdout()
            .lock()
            .flush()
            .map_err(|e| SinkError::Write(e.to_string()))
    }
}

/// The current instant as an RFC 3339 / ISO 8601 UTC timestamp with millisecond
/// precision (e.g. `2026-06-03T10:20:30.123Z`).
fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Provenance fields common to every envelope: which sink and version produced
/// it, when, and in what order.
fn header(seq: u64, ts: &str, index: &IndexName, op: &str, id: &str) -> serde_json::Map<String, Value> {
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

fn upsert_envelope(seq: u64, ts: &str, index: &IndexName, id: &str, document: &GenericValue) -> Value {
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
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn index() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    fn document() -> GenericValue {
        GenericValue::Map(BTreeMap::from([(
            "email".to_owned(),
            GenericValue::String("ada@x.io".to_owned()),
        )]))
    }

    const TS: &str = "2026-06-03T10:20:30.123Z";

    #[test]
    fn upsert_is_compact_ndjson_with_provenance_and_meta() {
        let line = StdoutSink::new(false)
            .render(&upsert_envelope(1, TS, &index(), "42", &document()))
            .unwrap();
        // Compact NDJSON: a single line.
        assert!(!line.contains('\n'));

        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["sink"], "stdout");
        assert_eq!(value["version"], VERSION);
        assert_eq!(value["ts"], TS);
        assert_eq!(value["seq"], 1);
        assert_eq!(value["index"], "users");
        assert_eq!(value["op"], "upsert");
        assert_eq!(value["id"], "42");
        assert_eq!(value["document"]["email"], "ada@x.io");
        // `{"email":"ada@x.io"}` is one field, 20 bytes compact.
        assert_eq!(value["meta"]["fields"], 1);
        assert_eq!(value["meta"]["bytes"], 20);
    }

    #[test]
    fn delete_carries_provenance_but_no_document_or_meta() {
        let line = StdoutSink::new(false)
            .render(&delete_envelope(7, TS, &index(), "7"))
            .unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["op"], "delete");
        assert_eq!(value["id"], "7");
        assert_eq!(value["seq"], 7);
        assert_eq!(value["sink"], "stdout");
        assert!(value.get("document").is_none());
        assert!(value.get("meta").is_none());
    }

    #[test]
    fn seq_increments_per_emit_and_is_shared_across_clones() {
        let sink = StdoutSink::new(false);
        assert_eq!(sink.next_seq(), 1);
        let clone = sink.clone();
        // The clone shares the counter, so it continues the sequence.
        assert_eq!(clone.next_seq(), 2);
        assert_eq!(sink.next_seq(), 3);
    }

    #[test]
    fn document_meta_reports_null_fields_for_non_objects() {
        let meta = document_meta(&json!("scalar"));
        assert!(meta["fields"].is_null());
        // `"scalar"` is 8 bytes once serialized (with quotes).
        assert_eq!(meta["bytes"], 8);
    }

    #[test]
    fn pretty_is_multiline() {
        let line = StdoutSink::new(true)
            .render(&delete_envelope(1, TS, &index(), "7"))
            .unwrap();
        assert!(line.contains('\n'));
        assert!(line.contains("\"op\": \"delete\""));
    }

    #[test]
    fn flush_runs_via_an_executor() {
        // Exercises the async `Sink` surface end-to-end (without writing output).
        futures::executor::block_on(StdoutSink::new(false).flush()).unwrap();
    }
}
