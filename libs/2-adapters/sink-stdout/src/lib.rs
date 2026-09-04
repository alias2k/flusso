#![doc = include_str!("../README.md")]

mod config;

pub use config::StdoutConfig;

use std::io::Write;

use async_trait::async_trait;
use chrono::SecondsFormat;
use kernel::{Envelope, Op};
use serde_json::{Map, Value, json};
use sink::{FlushReport, Result, Sink, SinkError, to_json};

/// Identifies this sink in every envelope's `sink` field.
const SINK_NAME: &str = "stdout";

/// This crate's version, stamped into every envelope's `version` field.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Writes each document operation to stdout as a JSON envelope.
#[derive(Debug, Clone)]
pub struct StdoutSink {
    pretty: bool,
}

impl StdoutSink {
    /// Create a sink. `pretty` selects pretty-printed JSON over compact NDJSON.
    pub fn new(pretty: bool) -> Self {
        Self { pretty }
    }

    pub fn from_config(config: &StdoutConfig) -> Self {
        Self::new(config.pretty)
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
    async fn apply(&self, envelope: &Envelope) -> Result<()> {
        let line = self.render(&wire_envelope(envelope))?;
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

/// The kernel envelope as this sink writes it: its fields as-is, plus the
/// emitting sink's name, the flusso version, the position rendered as the
/// opaque `seq` string, and a `meta` summary of the document.
fn wire_envelope(envelope: &Envelope) -> Value {
    let mut out = Map::new();
    out.insert("sink".to_owned(), json!(SINK_NAME));
    out.insert("version".to_owned(), json!(VERSION));
    out.insert(
        "ts".to_owned(),
        json!(envelope.ts.to_rfc3339_opts(SecondsFormat::Millis, true)),
    );
    if let Some(position) = envelope.position {
        out.insert("seq".to_owned(), json!(position.to_string()));
    }
    out.insert("index".to_owned(), json!(envelope.index.as_ref()));
    out.insert("op".to_owned(), json!(envelope.op.to_string()));
    out.insert("id".to_owned(), json!(envelope.id));
    if envelope.op == Op::Upsert
        && let Some(document) = &envelope.document
    {
        let document = to_json(document);
        out.insert("meta".to_owned(), document_meta(&document));
        out.insert("document".to_owned(), document);
    }
    Value::Object(out)
}

/// At-a-glance facts about a serialized document: how many top-level fields it
/// has (`null` when it isn't an object) and its compact byte size.
fn document_meta(document: &Value) -> Value {
    json!({
        "fields": document.as_object().map(serde_json::Map::len),
        "bytes": document.to_string().len(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
