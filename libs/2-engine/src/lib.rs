//! The `flusso` sync engine.
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
//! Before live capture, the engine runs an optional **backfill** phase. It asks
//! the [`DocumentBuilder`] which indexes exist and the sink whether each is
//! already seeded; for those that aren't, it asks the source to
//! [`snapshot`](ChangeCapture::snapshot) their root tables and drives that
//! finite stream through the same queue → resolve → build → sink path (scoped to
//! just the unseeded indexes), then records each as seeded. So "is a backfill
//! needed?" is the destination's call, not the source's.
//!
//! The queue, source, sink, and document builder are all trait objects, so the
//! backend choices (WAL vs polling, stdout vs OpenSearch, channel vs a durable
//! broker) are swappable without touching this loop.

mod error;

pub use error::*;

use std::collections::HashSet;
use std::sync::Arc;

use futures::StreamExt;
use futures::stream::BoxStream;
use queue_channel::{ChannelConsumer, channel};
use queue_core::{Consumer, Producer};
use schema_core::{GenericValue, IndexName, TableName};
use sinks_core::Sink;
use sources_core::cdc::{Change, ChangeCapture, ChangeEvent};
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_core::{RowKey, SnapshotTable};

/// Pending changes buffered between capture and the worker.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Drives changes from a source through to a sink.
#[derive(Debug)]
pub struct Engine {
    source: Box<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    queue_capacity: usize,
    skip_backfill: bool,
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
            skip_backfill: false,
        }
    }

    /// Set how many changes may buffer between capture and the worker.
    pub fn with_queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity.max(1);
        self
    }

    /// Force-skip the backfill phase entirely, regardless of what the sink
    /// reports. An escape hatch for sinks that can't persist seeded-state (so
    /// they would otherwise re-seed every run) or to resume without re-checking.
    pub fn skip_backfill(mut self, skip: bool) -> Self {
        self.skip_backfill = skip;
        self
    }

    /// Run until the live change stream ends or an error stops the pipeline.
    ///
    /// First seeds any unseeded index (unless [`skip_backfill`](Self::skip_backfill)
    /// is set), then follows live changes.
    pub async fn run(self) -> Result<()> {
        let Engine {
            source,
            documents,
            sink,
            queue_capacity,
            skip_backfill,
        } = self;

        if !skip_backfill {
            backfill(
                source.as_ref(),
                documents.as_ref(),
                sink.as_ref(),
                queue_capacity,
            )
            .await?;
        }

        let stream = source.live().await?;
        pump(stream, documents.as_ref(), sink.as_ref(), queue_capacity, None).await
    }
}

/// Seed every index the sink reports as unseeded, then mark them seeded.
///
/// The decision "does this index need a backfill?" is the **sink**'s — the
/// destination is what knows whether it already holds the data. For the indexes
/// that do, the source snapshots their root tables and the snapshot is applied
/// scoped to just those indexes, so an already-seeded index sharing a table is
/// never rewritten.
async fn backfill(
    source: &dyn ChangeCapture,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    queue_capacity: usize,
) -> Result<()> {
    let mut seeding: HashSet<IndexName> = HashSet::new();
    let mut tables: Vec<SnapshotTable> = Vec::new();
    for scope in documents.backfill_scopes() {
        if sink.is_seeded(&scope.index).await? {
            continue;
        }
        if !tables.contains(&scope.root) {
            tables.push(scope.root);
        }
        seeding.insert(scope.index);
    }
    if seeding.is_empty() {
        return Ok(());
    }

    tracing::info!(indexes = seeding.len(), tables = tables.len(), "seeding indexes");
    let stream = source.snapshot(&tables).await?;
    pump(stream, documents, sink, queue_capacity, Some(&seeding)).await?;

    // The snapshot is fully applied and flushed once `pump` returns; record each
    // index as seeded so a later run skips it.
    for index in &seeding {
        sink.mark_seeded(index).await?;
    }
    Ok(())
}

/// Drain one change stream through the queue to the sink: spawn a capture task,
/// run the worker, then fold the outcomes (a worker failure takes priority).
///
/// `filter`, when set, restricts which indexes a change may write to — used by
/// the backfill so a snapshot only seeds the indexes being backfilled.
async fn pump(
    stream: BoxStream<'static, sources_core::Result<Change>>,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    queue_capacity: usize,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    let (producer, mut consumer) = channel::<Change>(queue_capacity);

    // Capture runs concurrently with the worker; the worker borrows the shared
    // builder and sink.
    let capture = tokio::spawn(capture(stream, producer));
    let worker = work(&mut consumer, documents, sink, filter).await;

    capture.abort();
    let captured = capture.await;
    worker?;
    match captured {
        Ok(result) => result,
        Err(join) if join.is_cancelled() => Ok(()),
        Err(join) => Err(EngineError::Task(join.to_string())),
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
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    while let Some(delivery) = consumer.recv().await? {
        let (change, ack) = delivery.into_parts();
        apply(change, documents, sink, filter).await?;
        ack.ack().await?;
    }
    Ok(())
}

/// Apply one change: rebuild every document it affects, flush, then confirm the
/// source ack so the slot can advance.
async fn apply(
    change: Change,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    match &change.event {
        ChangeEvent::Upsert { table, key } | ChangeEvent::Delete { table, key } => {
            apply_row(table, key, documents, sink, filter).await?;
        }
    }
    // Flush per change keeps it simple and correct; a batching sink would flush
    // less often and only confirm acks up to the flushed point.
    sink.flush().await?;
    change.ack.confirm();
    Ok(())
}

/// Rebuild and write every document affected by a change to one row. When
/// `filter` is set, documents in indexes outside it are skipped (the backfill
/// seeds only the indexes it is responsible for).
async fn apply_row(
    table: &TableName,
    key: &RowKey,
    documents: &dyn DocumentBuilder,
    sink: &dyn Sink,
    filter: Option<&HashSet<IndexName>>,
) -> Result<()> {
    for id in documents.resolve(table, key).await? {
        if filter.is_some_and(|filter| !filter.contains(&id.index)) {
            continue;
        }
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
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
        async fn live(
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

        fn backfill_scopes(&self) -> Vec<sources_core::document::IndexScope> {
            vec![sources_core::document::IndexScope {
                index: IndexName::try_new("users").unwrap(),
                root: SnapshotTable {
                    db_schema: schema_core::DatabaseSchema::try_new("public").unwrap(),
                    table: TableName::try_new("users").unwrap(),
                },
            }]
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

        let changes = vec![upsert_change(1, 0, &acks), delete_change(2, 1, &acks)];

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
        // Every change is confirmed.
        assert_eq!(acks.load(Ordering::SeqCst), 2);
    }

    /// A source whose `live` stream is empty (so `run` returns) and whose
    /// `snapshot` records that it was called, with what tables, and replays a
    /// fixed set of rows.
    #[derive(Debug)]
    struct SeedSource {
        rows: Mutex<Option<Vec<Change>>>,
        called: Arc<AtomicBool>,
        tables: Arc<Mutex<Vec<SnapshotTable>>>,
    }

    impl SeedSource {
        fn new(rows: Vec<Change>) -> Self {
            Self {
                rows: Mutex::new(Some(rows)),
                called: Arc::new(AtomicBool::new(false)),
                tables: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl ChangeCapture for SeedSource {
        async fn live(
            &self,
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            Ok(Box::pin(stream::empty()))
        }

        async fn snapshot(
            &self,
            tables: &[SnapshotTable],
        ) -> sources_core::Result<BoxStream<'static, sources_core::Result<Change>>> {
            self.called.store(true, Ordering::SeqCst);
            *self.tables.lock().unwrap() = tables.to_vec();
            let rows = self.rows.lock().unwrap().take().unwrap_or_default();
            Ok(Box::pin(stream::iter(
                rows.into_iter().map(Ok::<Change, sources_core::SourceError>),
            )))
        }
    }

    /// A sink that reports a fixed seeded-state, records `mark_seeded` calls, and
    /// records the upserts it receives.
    #[derive(Debug)]
    struct SeedSink {
        seeded: bool,
        marked: Arc<Mutex<Vec<String>>>,
        ops: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Sink for SeedSink {
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

        async fn is_seeded(&self, _index: &IndexName) -> sinks_core::Result<bool> {
            Ok(self.seeded)
        }

        async fn mark_seeded(&self, index: &IndexName) -> sinks_core::Result<()> {
            self.marked.lock().unwrap().push(index.as_ref().to_owned());
            Ok(())
        }
    }

    #[tokio::test]
    async fn seeds_an_unseeded_index_then_marks_it() {
        let acks = Arc::new(AtomicU64::new(0));
        let source = SeedSource::new(vec![upsert_change(1, 0, &acks), upsert_change(3, 1, &acks)]);
        let called = Arc::clone(&source.called);
        let tables = Arc::clone(&source.tables);
        let ops = Arc::new(Mutex::new(Vec::new()));
        let marked = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::new(SeedSink {
            seeded: false,
            marked: Arc::clone(&marked),
            ops: Arc::clone(&ops),
        });

        Engine::new(Box::new(source), Arc::new(MockDocuments), sink)
            .run()
            .await
            .unwrap();

        assert!(called.load(Ordering::SeqCst), "snapshot should be requested");
        // The index's root table is what gets snapshotted.
        let tables = tables.lock().unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables.first().unwrap().table.as_ref(), "users");
        // Snapshot rows are applied, and the index is marked seeded afterwards.
        assert_eq!(
            *ops.lock().unwrap(),
            vec!["upsert users 1".to_owned(), "upsert users 3".to_owned()]
        );
        assert_eq!(*marked.lock().unwrap(), vec!["users".to_owned()]);
    }

    #[tokio::test]
    async fn skips_backfill_when_the_sink_reports_seeded() {
        let acks = Arc::new(AtomicU64::new(0));
        let source = SeedSource::new(vec![upsert_change(1, 0, &acks)]);
        let called = Arc::clone(&source.called);
        let ops = Arc::new(Mutex::new(Vec::new()));
        let marked = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::new(SeedSink {
            seeded: true,
            marked: Arc::clone(&marked),
            ops: Arc::clone(&ops),
        });

        Engine::new(Box::new(source), Arc::new(MockDocuments), sink)
            .run()
            .await
            .unwrap();

        assert!(!called.load(Ordering::SeqCst), "a seeded index is not snapshotted");
        assert!(ops.lock().unwrap().is_empty());
        assert!(marked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn skip_backfill_flag_overrides_an_unseeded_index() {
        let acks = Arc::new(AtomicU64::new(0));
        let source = SeedSource::new(vec![upsert_change(1, 0, &acks)]);
        let called = Arc::clone(&source.called);
        let ops = Arc::new(Mutex::new(Vec::new()));
        let marked = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::new(SeedSink {
            seeded: false,
            marked: Arc::clone(&marked),
            ops: Arc::clone(&ops),
        });

        Engine::new(Box::new(source), Arc::new(MockDocuments), sink)
            .skip_backfill(true)
            .run()
            .await
            .unwrap();

        assert!(!called.load(Ordering::SeqCst), "skip_backfill suppresses the snapshot");
        assert!(ops.lock().unwrap().is_empty());
        assert!(marked.lock().unwrap().is_empty());
    }
}
