//! A snapshot of several related indexes (issue #385): each snapshot row is
//! its own document, once per index rooted at its table. Nothing is resolved
//! and every document is built once, however the indexes read each other.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{
    ColumnName, ContentHash, DatabaseSchema, Envelope, GenericValue, IndexMapping, IndexName,
    Position, SinkName, TableName,
};
use sink::{FlushReport, Sink};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId, IndexScope};
use source::{RowImage, RowKey, SnapshotTable};
use stream::Stream;
use stream_channel::ChannelStream;

use super::sink_name;
use crate::{BatchPolicy, IngestEngine, SinkControl, SinkEngine};

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

fn row(table_name: &str, id: i64) -> ChangeEvent {
    ChangeEvent::Upsert {
        table: table(table_name),
        key: RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::BigInt(id),
        )]),
        before: None,
        after: None,
    }
}

fn id_of(key: &RowKey) -> i64 {
    match key.0.first() {
        Some((_, GenericValue::BigInt(id))) => *id,
        _ => panic!("not an integer key: {key:?}"),
    }
}

fn doc(index_name: &str, id: i64) -> DocumentId {
    DocumentId {
        index: index(index_name),
        key: RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::BigInt(id),
        )]),
    }
}

/// Three users (1..=3), each with orders `10u + 1` and `10u + 2`.
fn rows() -> Vec<ChangeEvent> {
    let mut rows = Vec::new();
    for user in 1..=3 {
        rows.push(row("users", user));
        rows.push(row("orders", 10 * user + 1));
        rows.push(row("orders", 10 * user + 2));
    }
    rows
}

/// A source whose snapshot streams the rows of the tables it is asked for.
#[derive(Debug)]
struct TableSource {
    rows: Vec<ChangeEvent>,
    snapshots: Mutex<Vec<Vec<String>>>,
}

impl TableSource {
    fn new(rows: Vec<ChangeEvent>) -> Self {
        Self {
            rows,
            snapshots: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ChangeCapture for TableSource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        Ok(Box::pin(futures::stream::pending()))
    }

    fn confirm(&self, _: Position) {}

    async fn snapshot(
        &self,
        tables: &[SnapshotTable],
    ) -> source::Result<BoxStream<'static, source::Result<ChangeEvent>>> {
        let names: Vec<String> = tables.iter().map(|t| t.table.to_string()).collect();
        self.snapshots.lock().unwrap().push(names.clone());
        let rows: Vec<_> = self
            .rows
            .iter()
            .filter(|row| names.contains(&row.table().to_string()))
            .cloned()
            .map(Ok)
            .collect();
        Ok(Box::pin(futures::stream::iter(rows)))
    }
}

/// Three indexes over two related tables:
///
/// - `users` (root `users`) embeds its orders,
/// - `accounts` (root `users`, a second index on the same table),
/// - `orders` (root `orders`) embeds its user.
///
/// `resolve` answers as a source must for a live change: a user row reaches
/// its own `users`/`accounts` documents and its orders' `orders` documents, an
/// order row its own document and its user's `users` document.
#[derive(Debug, Default)]
struct RelatedDocuments {
    resolved: Mutex<usize>,
    built: Mutex<Vec<DocumentId>>,
}

impl RelatedDocuments {
    fn build_counts(&self) -> HashMap<DocumentId, usize> {
        let mut counts = HashMap::new();
        for id in self.built.lock().unwrap().iter() {
            *counts.entry(id.clone()).or_insert(0) += 1;
        }
        counts
    }
}

#[async_trait]
impl DocumentBuilder for RelatedDocuments {
    async fn resolve(
        &self,
        table_name: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        *self.resolved.lock().unwrap() += 1;
        let id = id_of(key);
        Ok(match table_name.as_ref() {
            "users" => vec![
                doc("users", id),
                doc("accounts", id),
                doc("orders", 10 * id + 1),
                doc("orders", 10 * id + 2),
            ],
            "orders" => vec![doc("orders", id), doc("users", id / 10)],
            _ => Vec::new(),
        })
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        self.built.lock().unwrap().push(id.clone());
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Map(BTreeMap::new()),
        })
    }

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        [
            ("users", "users"),
            ("accounts", "users"),
            ("orders", "orders"),
        ]
        .into_iter()
        .map(|(name, root)| IndexScope {
            index: index(name),
            root: SnapshotTable {
                db_schema: DatabaseSchema::try_new("public").unwrap(),
                table: table(root),
            },
        })
        .collect()
    }
}

/// Records every applied envelope; seeded for exactly the indexes it is
/// given.
#[derive(Debug, Default)]
struct IndexSink {
    applied: Mutex<Vec<String>>,
    seeded: Mutex<BTreeSet<String>>,
}

impl IndexSink {
    fn seeded(indexes: &[&str]) -> Self {
        Self {
            applied: Mutex::new(Vec::new()),
            seeded: Mutex::new(indexes.iter().map(|i| (*i).to_owned()).collect()),
        }
    }
}

#[async_trait]
impl Sink for IndexSink {
    async fn apply(&self, envelope: &Envelope) -> sink::Result<()> {
        self.applied
            .lock()
            .unwrap()
            .push(format!("{} {}", envelope.index.as_ref(), envelope.id));
        Ok(())
    }

    async fn flush(&self, _: bool) -> sink::Result<FlushReport> {
        Ok(FlushReport::clean())
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.seeded
            .lock()
            .unwrap()
            .insert(index.as_ref().to_owned());
        Ok(())
    }
}

fn mappings() -> Vec<IndexMapping> {
    ["users", "accounts", "orders"]
        .into_iter()
        .map(|name| IndexMapping {
            index: index(name),
            hash: ContentHash::new(1),
            fields: Vec::new(),
        })
        .collect()
}

/// Stage every sink, run the ingest engine and the sink engines until every
/// sink has marked all three indexes seeded, then stop.
async fn backfill(
    source: &Arc<TableSource>,
    documents: &Arc<RelatedDocuments>,
    sinks: &[(SinkName, Arc<IndexSink>)],
    max_changes: usize,
) {
    let names: Vec<SinkName> = sinks.iter().map(|(name, _)| name.clone()).collect();
    let stream = Arc::new(ChannelStream::new(64, names.clone()));
    let mut tasks = Vec::new();
    let mut controls = Vec::new();
    for (name, sink) in sinks {
        let engine = SinkEngine::new(
            name.clone(),
            Arc::clone(sink) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            mappings(),
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
        controls.push(tx);
        tasks.push(tokio::spawn(async move { engine.run(&mut rx).await }));
    }
    let ingest = IngestEngine::new(
        Arc::clone(source) as Arc<dyn ChangeCapture>,
        Arc::clone(documents) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        names,
    )
    .with_batch(BatchPolicy {
        max_changes,
        max_delay: Duration::from_millis(10),
    });
    tasks.push(tokio::spawn(async move { ingest.run().await }));
    super::wait_for(|| {
        sinks
            .iter()
            .all(|(_, sink)| sink.seeded.lock().unwrap().len() == 3)
    })
    .await;
    for task in tasks {
        task.abort();
    }
    drop(controls);
}

fn sorted(mut applied: Vec<String>) -> Vec<String> {
    applied.sort();
    applied
}

/// Every document of the three indexes, as a sink records it.
fn every_document() -> Vec<String> {
    let mut all = Vec::new();
    for user in 1..=3 {
        all.push(format!("users {user}"));
        all.push(format!("accounts {user}"));
        all.push(format!("orders {}", 10 * user + 1));
        all.push(format!("orders {}", 10 * user + 2));
    }
    all.sort();
    all
}

#[tokio::test]
async fn a_backfill_of_related_indexes_builds_each_document_once() {
    for max_changes in [1, 2, 256] {
        let source = Arc::new(TableSource::new(rows()));
        let documents = Arc::new(RelatedDocuments::default());
        let sinks = [
            (sink_name("a"), Arc::new(IndexSink::default())),
            (sink_name("b"), Arc::new(IndexSink::default())),
        ];
        backfill(&source, &documents, &sinks, max_changes).await;

        let counts = documents.build_counts();
        assert_eq!(counts.len(), 12, "{counts:?}");
        assert!(
            counts.values().all(|n| *n == 1),
            "every document built once (batches of {max_changes}): {counts:?}"
        );
        for (name, sink) in &sinks {
            assert_eq!(
                sorted(sink.applied.lock().unwrap().clone()),
                every_document(),
                "lane {name} gets each document once (batches of {max_changes})"
            );
        }
        assert_eq!(
            *documents.resolved.lock().unwrap(),
            0,
            "a snapshot row is its own document; nothing is resolved (batches of {max_changes})"
        );
    }
}

#[tokio::test]
async fn a_backfill_beside_seeded_related_indexes_writes_them_nothing() {
    let source = Arc::new(TableSource::new(rows()));
    let documents = Arc::new(RelatedDocuments::default());
    let sinks = [(
        sink_name("a"),
        Arc::new(IndexSink::seeded(&["users", "accounts"])),
    )];
    backfill(&source, &documents, &sinks, 256).await;

    assert_eq!(
        *source.snapshots.lock().unwrap(),
        vec![vec!["orders".to_owned()]]
    );
    let expected: Vec<String> = every_document()
        .into_iter()
        .filter(|doc| doc.starts_with("orders "))
        .collect();
    assert_eq!(sorted(sinks[0].1.applied.lock().unwrap().clone()), expected);
    assert_eq!(*documents.resolved.lock().unwrap(), 0);
}

#[tokio::test]
async fn a_backfill_of_one_of_two_indexes_on_a_table_builds_only_that_one() {
    let source = Arc::new(TableSource::new(rows()));
    let documents = Arc::new(RelatedDocuments::default());
    let sinks = [(
        sink_name("a"),
        Arc::new(IndexSink::seeded(&["users", "orders"])),
    )];
    backfill(&source, &documents, &sinks, 256).await;

    assert_eq!(
        sorted(sinks[0].1.applied.lock().unwrap().clone()),
        vec!["accounts 1", "accounts 2", "accounts 3"]
    );
}
