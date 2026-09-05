#![doc = include_str!("../README.md")]

mod config;

pub use config::StdoutConfig;

use std::io::Write;

use async_trait::async_trait;
use kernel::{Envelope, EnvelopeMeta, Op, SinkName};
use serde_json::{Map, Value};
use sink::{FlushReport, Result, Sink, SinkError, to_json};

/// Writes each document operation to stdout as a JSON envelope, stamped with
/// this sink's configured name.
#[derive(Debug, Clone)]
pub struct StdoutSink {
    name: SinkName,
    pretty: bool,
}

impl StdoutSink {
    /// Create a sink emitting as `name`. `pretty` selects pretty-printed JSON
    /// over compact NDJSON.
    pub fn new(name: SinkName, pretty: bool) -> Self {
        Self { name, pretty }
    }

    pub fn from_config(name: &SinkName, config: &StdoutConfig) -> Self {
        Self::new(name.clone(), config.pretty)
    }

    fn render(&self, envelope: &Envelope<Value>) -> Result<String> {
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

    /// The kernel envelope as this sink writes it: the same shape, stamped with
    /// this sink's name, the document translated to JSON, and the `meta`
    /// summary filled in.
    fn wire_envelope(&self, envelope: &Envelope) -> Envelope<Value> {
        let mut wire = envelope.clone().map_document(|document| to_json(&document));
        wire.sink = Some(self.name.clone());
        if wire.op == Op::Upsert
            && let Some(document) = &wire.document
        {
            wire.meta = Some(document_meta(document));
        }
        wire
    }
}

#[async_trait]
impl Sink for StdoutSink {
    async fn apply(&self, envelope: &Envelope) -> Result<()> {
        let line = self.render(&self.wire_envelope(envelope))?;
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

/// At-a-glance facts about a serialized document: how many top-level fields it
/// has (`None` when it isn't an object) and its compact byte size.
fn document_meta(document: &Value) -> EnvelopeMeta {
    EnvelopeMeta {
        fields: document.as_object().map(Map::len),
        bytes: document.to_string().len(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
