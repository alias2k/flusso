//! The `pg_sync_rs` sync engine.
//!
//! Wires the pluggable edges together and runs the pipeline:
//!
//! ```text
//! ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
//! ```
//!
//! A **capture** task drains the source's change stream into a bounded
//! in-process [`queue`](queue_channel) (back-pressure: capture blocks when the
//! queue is full). A **worker** pulls each change and, for the row it names,
//! resolves the affected document ids, assembles each one, and writes it to the
//! [`Sink`]. Once the change's documents are flushed, the source ack is
//! confirmed so the replication slot can advance — making delivery
//! at-least-once: a change is only forgotten after it has landed in the sink.
//!
//! Stopping on any error is therefore safe: unconfirmed changes are redelivered
//! when the run restarts.
//!
//! The queue, source, sink, and document builder are all trait objects, so the
//! backend choices (WAL vs polling, stdout vs OpenSearch, channel vs a durable
//! broker) are swappable without touching this loop.

mod error;

pub use error::*;

use std::sync::Arc;

use futures::StreamExt;
use futures::stream::BoxStream;
use queue_channel::{ChannelConsumer, channel};
use queue_core::{Consumer, Producer};
use schema_core::{GenericValue, TableName};
use sinks_core::Sink;
use sources_core::RowKey;
use sources_core::cdc::{Change, ChangeCapture, ChangeEvent};
use sources_core::document::{Document, DocumentBuilder, DocumentId};

/// Pending changes buffered between capture and the worker.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Drives changes from a source through to a sink.
#[derive(Debug)]
pub struct Engine {
    source: Box<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    queue_capacity: usize,
}

impl Engine {
    /// Assemble an engine from its pluggable parts.
    pub fn new(
        source: Box<dyn ChangeCapture>,
        documents: Arc<dyn DocumentBuilder>,
        sink: Arc<dyn Sink>,
    ) -> Self {
        Self {
            source,
            documents,
            sink,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
        }
    }

    /// Set how many changes may buffer between capture and the worker.
    pub fn with_queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity.max(1);
        self
    }

    /// Run until the change stream ends or an error stops the pipeline.
    pub async fn run(self) -> Result<()> {
        let Engine {
            source,
            documents,
            sink,
            queue_capacity,
        } = self;

        let stream = source.start().await?;
        let (producer, mut consumer) = channel::<Change>(queue_capacity);

        // Capture runs concurrently with the worker; the worker borrows the
        // shared builder and sink.
        let capture = tokio::spawn(capture(stream, producer));
        let worker = work(&mut consumer, documents.as_ref(), sink.as_ref()).await;

        // Stop capture if it is still running, then fold the outcomes — a worker
        // failure takes priority, otherwise surface a capture failure.
        capture.abort();
        let captured = capture.await;
        worker?;
        match captured {
            Ok(result) => result,
            Err(join) if join.is_cancelled() => Ok(()),
            Err(join) => Err(EngineError::Task(join.to_string())),
        }
    }
}

/// Drain the change stream into the queue. Ends (closing the queue) when the
/// stream is exhausted.
async fn capture(
    mut stream: BoxStream<'static, sources_core::Result<Change>>,
    producer: queue_channel::ChannelProducer<Change>,
) -> Result<()> {
    while let Some(change) = stream.next().await {
        producer.publish(change?).await?;
    }
    Ok(())
}

/// Pull changes and apply each one, confirming its ack when done.
async fn work(
    consumer: &mut ChannelConsumer<Change>,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
) -> Result<()> {
    while let Some(delivery) = consumer.recv().await? {
        let (change, ack) = delivery.into_parts();
        apply(change, documents, sink).await?;
        ack.ack().await?;
    }
    Ok(())
}

/// Apply one change: rebuild every document it affects, flush, then confirm the
/// source ack so the slot can advance.
async fn apply(change: Change, documents: &dyn DocumentBuilder, sink: &dyn Sink) -> Result<()> {
    match &change.event {
        ChangeEvent::SnapshotComplete => {
            tracing::info!("backfill complete; following live changes");
        }
        ChangeEvent::Snapshot { table, key }
        | ChangeEvent::Upsert { table, key }
        | ChangeEvent::Delete { table, key } => {
            apply_row(table, key, documents, sink).await?;
        }
    }
    // Flush per change keeps it simple and correct; a batching sink would flush
    // less often and only confirm acks up to the flushed point.
    sink.flush().await?;
    change.ack.confirm();
    Ok(())
}

/// Rebuild and write every document affected by a change to one row.
async fn apply_row(
    table: &TableName,
    key: &RowKey,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
) -> Result<()> {
    for id in documents.resolve(table, key).await? {
        match documents.build(&id).await? {
            Document::Upsert { id, body } => {
                sink.upsert(&id.index, &document_id(&id), &body).await?;
            }
            Document::Delete { id } => {
                sink.delete(&id.index, &document_id(&id)).await?;
            }
        }
    }
    Ok(())
}

/// The sink's document `_id`, derived from the document key (the root primary
/// key); composite keys join their parts with `:`.
fn document_id(id: &DocumentId) -> String {
    id.key
        .0
        .iter()
        .map(|(_, value)| value_to_string(value))
        .collect::<Vec<_>>()
        .join(":")
}

fn value_to_string(value: &GenericValue) -> String {
    match value {
        GenericValue::Bool(b) => b.to_string(),
        GenericValue::Int(i) => i.to_string(),
        GenericValue::Decimal(d) => d.to_string(),
        GenericValue::String(s) => s.clone(),
        GenericValue::Null => "null".to_owned(),
        GenericValue::Array(_) | GenericValue::Map(_) => String::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use async_trait::async_trait;
    use futures::stream;
    use schema_core::{ColumnName, IndexName};
    use sources_core::cdc::{Ack, AckSink};

    /// A source that replays a fixed list of changes once.
    #[derive(Debug)]
    struct MockSource {
        changes: Mutex<Option<Vec<Change>>>,
    }

    #[async_trait]
    impl ChangeCapture for MockSource {
        async fn start(
            &self,
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            let changes = self.changes.lock().unwrap().take().unwrap_or_default();
            Ok(Box::pin(stream::iter(
                changes.into_iter().map(Ok::<Change, sources_core::SourceError>),
            )))
        }
    }

    /// Counts how many changes were confirmed.
    #[derive(Debug)]
    struct CountingAck(Arc<AtomicU64>);

    impl AckSink for CountingAck {
        fn confirm(&self, _seq: u64) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Resolves each change to one document; key value `2` builds a tombstone.
    #[derive(Debug)]
    struct MockDocuments;

    #[async_trait]
    impl DocumentBuilder for MockDocuments {
        async fn resolve(
            &self,
            _table: &TableName,
            key: &RowKey,
        ) -> sources_core::Result<Vec<DocumentId>> {
            Ok(vec![DocumentId {
                index: IndexName::try_new("users").unwrap(),
                key: key.clone(),
            }])
        }

        async fn build(&self, id: &DocumentId) -> sources_core::Result<Document> {
            let deleted = matches!(id.key.0.first(), Some((_, GenericValue::Int(2))));
            Ok(if deleted {
                Document::Delete { id: id.clone() }
            } else {
                Document::Upsert {
                    id: id.clone(),
                    body: GenericValue::Map(Default::default()),
                }
            })
        }
    }

    /// Records the sink operations it receives.
    #[derive(Debug, Default)]
    struct RecordingSink {
        ops: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Sink for RecordingSink {
        async fn upsert(
            &self,
            index: &IndexName,
            id: &str,
            _document: &GenericValue,
        ) -> sinks_core::Result<()> {
            self.ops.lock().unwrap().push(format!("upsert {} {id}", index.as_ref()));
            Ok(())
        }

        async fn delete(&self, index: &IndexName, id: &str) -> sinks_core::Result<()> {
            self.ops.lock().unwrap().push(format!("delete {} {id}", index.as_ref()));
            Ok(())
        }

        async fn flush(&self) -> sinks_core::Result<()> {
            Ok(())
        }
    }

    fn upsert_change(id: i64, seq: u64, acks: &Arc<AtomicU64>) -> Change {
        row_change(false, id, seq, acks)
    }

    fn delete_change(id: i64, seq: u64, acks: &Arc<AtomicU64>) -> Change {
        row_change(true, id, seq, acks)
    }

    fn row_change(delete: bool, id: i64, seq: u64, acks: &Arc<AtomicU64>) -> Change {
        let table = TableName::try_new("users").unwrap();
        let key = RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::Int(id),
        )]);
        let event = if delete {
            ChangeEvent::Delete { table, key }
        } else {
            ChangeEvent::Upsert { table, key }
        };
        Change {
            event,
            ack: Ack::new(seq, Arc::new(CountingAck(Arc::clone(acks)))),
        }
    }

    #[tokio::test]
    async fn drives_changes_to_the_sink_and_acks_each() {
        let acks = Arc::new(AtomicU64::new(0));
        let ops = Arc::new(Mutex::new(Vec::new()));

        let changes = vec![
            upsert_change(1, 0, &acks),
            delete_change(2, 1, &acks),
            Change {
                event: ChangeEvent::SnapshotComplete,
                ack: Ack::new(2, Arc::new(CountingAck(Arc::clone(&acks)))),
            },
        ];

        let engine = Engine::new(
            Box::new(MockSource {
                changes: Mutex::new(Some(changes)),
            }),
            Arc::new(MockDocuments),
            Arc::new(RecordingSink {
                ops: Arc::clone(&ops),
            }),
        );
        engine.run().await.unwrap();

        assert_eq!(
            *ops.lock().unwrap(),
            vec!["upsert users 1".to_owned(), "delete users 2".to_owned()]
        );
        // Every change — including SnapshotComplete — is confirmed.
        assert_eq!(acks.load(Ordering::SeqCst), 3);
    }
}
