#![doc = include_str!("../README.md")]

mod config;

pub use config::StdoutConfig;

use std::io::Write;

use async_trait::async_trait;
use kernel::{Envelope, EnvelopeMeta, GenericValue, Op, SinkName};
use sink::{Json, Result, Sink, SinkError, WriteReport};

/// Writes each document operation to stdout as a JSON envelope, stamped with
/// this sink's configured name.
#[derive(Debug, Clone)]
pub struct StdoutSink {
    name: SinkName,
    pretty: bool,
    out: Output,
}

/// Where the lines go: stdout, or a buffer a test reads back.
#[derive(Debug, Clone)]
enum Output {
    Stdout,
    #[cfg(test)]
    Buffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>),
}

impl StdoutSink {
    /// Create a sink emitting as `name`. `pretty` selects pretty-printed JSON
    /// over compact NDJSON.
    pub fn new(name: SinkName, pretty: bool) -> Self {
        Self {
            name,
            pretty,
            out: Output::Stdout,
        }
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
    /// Render every envelope, then write them as one run of lines under the
    /// stdout lock, so the lines of two concurrent writes never interleave.
    /// Uses the stdout handle directly (not the `print!` family) so it stays a
    /// real data sink, not logging. It never rejects a document: a write either
    /// lands whole or errors.
    async fn write(&self, envelopes: &[Envelope], _caught_up: bool) -> Result<WriteReport> {
        let mut text = String::new();
        for envelope in envelopes {
            text.push_str(&self.render(&self.wire_envelope(envelope)?)?);
            text.push('\n');
        }
        let written = match &self.out {
            Output::Stdout => {
                let stdout = std::io::stdout();
                let mut handle = stdout.lock();
                handle
                    .write_all(text.as_bytes())
                    .and_then(|()| handle.flush())
            }
            #[cfg(test)]
            Output::Buffer(buffer) => {
                buffer
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .extend_from_slice(text.as_bytes());
                Ok(())
            }
        };
        written.map_err(|e| SinkError::Write(e.to_string()))?;
        Ok(WriteReport::clean())
    }

    /// Its output is a stream read in order: lines come in lane order, one
    /// write at a time, whatever `[concurrency] writes` says.
    fn ordered(&self) -> bool {
        true
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
