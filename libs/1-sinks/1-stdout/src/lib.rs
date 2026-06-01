//! A [`Sink`] that writes documents to standard output as JSON.
//!
//! Each operation is emitted as a JSON envelope — one line of NDJSON by
//! default, or pretty-printed when configured — which makes the pipeline's
//! output easy to watch or pipe into `jq` during development.
//!
//! ```text
//! {"document":{"email":"ada@x.io"},"id":"42","index":"users","op":"upsert"}
//! {"id":"7","index":"users","op":"delete"}
//! ```

use std::io::Write;

use async_trait::async_trait;
use schema_core::{GenericValue, IndexName};
use serde_json::{Value, json};
use sinks_core::{Result, Sink, SinkError, to_json};

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

    /// Build a sink from the schema's stdout sink configuration.
    pub fn from_config(config: &schema_core::StdoutSink) -> Self {
        Self::new(config.pretty)
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
        let line = self.render(&upsert_envelope(index, id, document))?;
        self.write_line(&line)
    }

    async fn delete(&self, index: &IndexName, id: &str) -> Result<()> {
        let line = self.render(&delete_envelope(index, id))?;
        self.write_line(&line)
    }

    async fn flush(&self) -> Result<()> {
        std::io::stdout()
            .lock()
            .flush()
            .map_err(|e| SinkError::Write(e.to_string()))
    }
}

fn upsert_envelope(index: &IndexName, id: &str, document: &GenericValue) -> Value {
    json!({
        "index": index.as_ref(),
        "op": "upsert",
        "id": id,
        "document": to_json(document),
    })
}

fn delete_envelope(index: &IndexName, id: &str) -> Value {
    json!({
        "index": index.as_ref(),
        "op": "delete",
        "id": id,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn index() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    #[test]
    fn upsert_renders_compact_ndjson() {
        let document = GenericValue::Map(BTreeMap::from([(
            "email".to_owned(),
            GenericValue::String("ada@x.io".to_owned()),
        )]));
        let line = StdoutSink::new(false)
            .render(&upsert_envelope(&index(), "42", &document))
            .unwrap();
        assert_eq!(
            line,
            r#"{"document":{"email":"ada@x.io"},"id":"42","index":"users","op":"upsert"}"#
        );
    }

    #[test]
    fn delete_renders_without_a_document() {
        let line = StdoutSink::new(false)
            .render(&delete_envelope(&index(), "7"))
            .unwrap();
        assert_eq!(line, r#"{"id":"7","index":"users","op":"delete"}"#);
    }

    #[test]
    fn pretty_is_multiline() {
        let line = StdoutSink::new(true)
            .render(&delete_envelope(&index(), "7"))
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
