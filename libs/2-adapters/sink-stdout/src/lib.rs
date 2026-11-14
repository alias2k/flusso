#![doc = include_str!("../README.md")]

mod config;

pub use config::StdoutConfig;

use std::io::Write;

use async_trait::async_trait;
use kernel::{Envelope, EnvelopeMeta, GenericValue, Op, SinkName};
use sink::{FlushReport, Json, Result, Sink, SinkError};

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

    fn render(&self, envelope: &Envelope<Json<'_>>) -> Result<String> {
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
    /// this sink's name, the document rendered as [`Json`], and the `meta`
    /// summary filled in.
    fn wire_envelope<'a>(&self, envelope: &'a Envelope) -> Result<Envelope<Json<'a>>> {
        let meta = match (&envelope.document, envelope.op) {
            (Some(document), Op::Upsert) => Some(document_meta(document)?),
            _ => envelope.meta,
        };
        Ok(Envelope {
            sink: Some(self.name.clone()),
            version: envelope.version,
            ts: envelope.ts,
            position: envelope.position,
            index: envelope.index.clone(),
            op: envelope.op,
            id: envelope.id.clone(),
            meta,
            document: envelope.document.as_ref().map(Json),
        })
    }
}

#[async_trait]
impl Sink for StdoutSink {
    async fn apply(&self, envelope: &Envelope) -> Result<()> {
        let line = self.render(&self.wire_envelope(envelope)?)?;
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
fn document_meta(document: &GenericValue) -> Result<EnvelopeMeta> {
    let mut size = ByteCount(0);
    serde_json::to_writer(&mut size, &Json(document))
        .map_err(|e| SinkError::Serialize(e.to_string()))?;
    Ok(EnvelopeMeta {
        fields: match document {
            GenericValue::Map(fields) => Some(fields.len()),
            _ => None,
        },
        bytes: size.0,
    })
}

/// A writer that only counts the bytes written to it.
struct ByteCount(usize);

impl Write for ByteCount {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
