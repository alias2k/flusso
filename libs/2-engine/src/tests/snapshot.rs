//! A snapshot of several related indexes (issue #385): each key a page reads
//! is its own document of the index walked. Nothing is resolved and every
//! document is built once, however the indexes read each other. The walk
//! reads each index's keys in pages, each following the last key read
//! (issue #386 stage 3).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{
    ColumnName, ContentHash, Envelope, GenericValue, IndexMapping, IndexName, SinkName, TableName,
};
use sink::{Sink, WriteReport};
use source::cdc::{ChangeCapture, ChangeEvent, Continuity, LiveChange};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{RowImage, RowKey};
use stream::Stream;
use stream_channel::ChannelStream;

use super::sink_name;
use crate::{BatchPolicy, IngestEngine, KEY_PAGE, SinkControl, SinkEngine};

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

/// A source with a live stream that never yields.
#[derive(Debug)]
struct QuietSource;

#[async_trait]
impl ChangeCapture for QuietSource {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        Ok(Box::pin(futures::stream::pending()))
    }

    fn confirm(&self, _: source::cdc::Landed) {}
}

/// Three indexes over two related tables:
///
/// - `users` (root `users`) embeds its orders,
/// - `accounts` (root `users`, a second index on the same table),
/// - `orders` (root `orders`) embeds its user.
///
/// `resolve` answers as a source must for a live change: a user row reaches
/// its own `users`/`accounts` documents and its orders' `orders` documents, an
/// order row its own document and its user's `users` document. `keys` pages
/// the rows of each index's root table, recording every page asked for.
#[derive(Debug, Default)]
struct RelatedDocuments {
    rows: Mutex<Vec<ChangeEvent>>,
    /// Users added past the greatest key after every page read: a writer
    /// outrunning the walk.
    grow: usize,
    resolved: Mutex<usize>,
    built: Mutex<Vec<DocumentId>>,
    pages: Mutex<Vec<(String, Option<i64>)>>,
}

impl RelatedDocuments {
    /// Every key of `index_name`'s root table now, in walk order.
    fn root_keys(&self, index_name: &IndexName) -> Vec<RowKey> {
        let root = match index_name.as_ref() {
            "users" | "accounts" => "users",
            other => other,
        };
        self.rows
            .lock()
            .unwrap()
            .iter()
            .filter(|row| row.table().as_ref() == root)
            .map(|row| row.key().clone())
            .collect()
    }

    fn new(rows: Vec<ChangeEvent>) -> Self {
        Self {
            rows: Mutex::new(rows),
            ..Self::default()
        }
    }

    /// The indexes walked, in order: each first page's index.
    fn walks(&self) -> Vec<String> {
        self.pages
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, after)| after.is_none())
            .map(|(index, _)| index.clone())
            .collect()
    }

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

    fn backfill_indexes(&self) -> Vec<IndexName> {
        ["users", "accounts", "orders"]
            .into_iter()
            .map(index)
            .collect()
    }

    async fn last_key(&self, index_name: &IndexName) -> source::Result<Option<RowKey>> {
        Ok(self.root_keys(index_name).last().cloned())
    }

    async fn keys(
        &self,
        index_name: &IndexName,
        after: Option<&RowKey>,
        through: &RowKey,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        self.pages
            .lock()
            .unwrap()
            .push((index_name.to_string(), after.map(id_of)));
        let page = super::page_of(&self.root_keys(index_name), after, through, limit);
        let mut rows = self.rows.lock().unwrap();
        let next = rows
            .iter()
            .filter(|row| row.table().as_ref() == "users")
            .map(|row| id_of(row.key()))
            .max()
            .unwrap_or(0);
        for user in 1..=i64::try_from(self.grow).unwrap() {
            rows.push(row("users", next + user));
        }
        Ok(page)
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
    async fn write(&self, envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        for envelope in envelopes {
            self.applied.lock().unwrap().push(format!(
                "{} {}",
                envelope.index.as_ref(),
                envelope.id
            ));
        }
        Ok(WriteReport::clean())
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
    documents: &Arc<RelatedDocuments>,
    sinks: &[(SinkName, Arc<IndexSink>)],
    max_changes: usize,
) {
    let names: Vec<SinkName> = sinks.iter().map(|(name, _)| name.clone()).collect();
    let indexes: Vec<IndexName> = mappings().into_iter().map(|m| m.index).collect();
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), names.clone()));
    let progress = Arc::new(crate::Progress::of(indexes, names.clone()));
    let mut tasks = Vec::new();
    let mut controls = Vec::new();
    for (name, sink) in sinks {
        let engine = SinkEngine::new(
            name.clone(),
            Arc::clone(sink) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            mappings(),
            Arc::clone(&progress),
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
        controls.push(tx);
        tasks.push(tokio::spawn(async move { engine.run(&mut rx).await }));
    }
    let ingest = IngestEngine::new(
        Arc::new(QuietSource) as Arc<dyn ChangeCapture>,
        Arc::clone(documents) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        names,
        Arc::clone(&progress),
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
        let documents = Arc::new(RelatedDocuments::new(rows()));
        let sinks = [
            (sink_name("a"), Arc::new(IndexSink::default())),
            (sink_name("b"), Arc::new(IndexSink::default())),
        ];
        backfill(&documents, &sinks, max_changes).await;

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
            "a paged key is its own document; nothing is resolved (batches of {max_changes})"
        );
    }
}

#[tokio::test]
async fn a_backfill_beside_seeded_related_indexes_writes_them_nothing() {
    let documents = Arc::new(RelatedDocuments::new(rows()));
    let sinks = [(
        sink_name("a"),
        Arc::new(IndexSink::seeded(&["users", "accounts"])),
    )];
    backfill(&documents, &sinks, 256).await;

    assert_eq!(documents.walks(), ["orders"]);
    let expected: Vec<String> = every_document()
        .into_iter()
        .filter(|doc| doc.starts_with("orders "))
        .collect();
    assert_eq!(sorted(sinks[0].1.applied.lock().unwrap().clone()), expected);
    assert_eq!(*documents.resolved.lock().unwrap(), 0);
}

#[tokio::test]
async fn a_backfill_of_one_of_two_indexes_on_a_table_builds_only_that_one() {
    let documents = Arc::new(RelatedDocuments::new(rows()));
    let sinks = [(
        sink_name("a"),
        Arc::new(IndexSink::seeded(&["users", "orders"])),
    )];
    backfill(&documents, &sinks, 256).await;

    assert_eq!(
        sorted(sinks[0].1.applied.lock().unwrap().clone()),
        vec!["accounts 1", "accounts 2", "accounts 3"]
    );
}

/// Users `1..=n`, no orders.
fn users(n: i64) -> Vec<ChangeEvent> {
    (1..=n).map(|user| row("users", user)).collect()
}

#[tokio::test]
async fn each_page_follows_the_last_key_the_walk_read() {
    let total = i64::try_from(2 * KEY_PAGE + 100).unwrap();
    let page = i64::try_from(KEY_PAGE).unwrap();
    let documents = Arc::new(RelatedDocuments::new(users(total)));
    let sinks = [(
        sink_name("a"),
        Arc::new(IndexSink::seeded(&["accounts", "orders"])),
    )];
    backfill(&documents, &sinks, 256).await;

    assert_eq!(
        *documents.pages.lock().unwrap(),
        [
            ("users".to_owned(), None),
            ("users".to_owned(), Some(page)),
            ("users".to_owned(), Some(2 * page)),
        ],
        "a page shorter than the limit ends the walk"
    );
    let applied = sinks[0].1.applied.lock().unwrap().clone();
    assert_eq!(applied.len(), 2 * KEY_PAGE + 100);
    let expected: Vec<String> = (1..=total).map(|user| format!("users {user}")).collect();
    assert_eq!(sorted(applied), sorted(expected));
}

#[tokio::test]
async fn a_walk_ending_on_a_full_page_reads_one_empty_page_then_ends() {
    let page = i64::try_from(KEY_PAGE).unwrap();
    let documents = Arc::new(RelatedDocuments::new(users(page)));
    let sinks = [(
        sink_name("a"),
        Arc::new(IndexSink::seeded(&["accounts", "orders"])),
    )];
    backfill(&documents, &sinks, 256).await;

    assert_eq!(
        *documents.pages.lock().unwrap(),
        [("users".to_owned(), None), ("users".to_owned(), Some(page))]
    );
    assert_eq!(sinks[0].1.applied.lock().unwrap().len(), KEY_PAGE);
}

#[tokio::test]
async fn indexes_are_walked_one_after_another_each_from_its_start() {
    let documents = Arc::new(RelatedDocuments::new(rows()));
    let sinks = [(sink_name("a"), Arc::new(IndexSink::default()))];
    backfill(&documents, &sinks, 256).await;

    assert_eq!(
        *documents.pages.lock().unwrap(),
        [
            ("accounts".to_owned(), None),
            ("orders".to_owned(), None),
            ("users".to_owned(), None),
        ]
    );
}

#[tokio::test]
async fn a_walk_ends_at_the_greatest_key_its_index_had_when_it_started() {
    let total = i64::try_from(2 * KEY_PAGE + 100).unwrap();
    let page = i64::try_from(KEY_PAGE).unwrap();
    let documents = Arc::new(RelatedDocuments {
        grow: KEY_PAGE,
        ..RelatedDocuments::new(users(total))
    });
    let sinks = [(
        sink_name("a"),
        Arc::new(IndexSink::seeded(&["accounts", "orders"])),
    )];
    backfill(&documents, &sinks, 256).await;

    assert_eq!(
        *documents.pages.lock().unwrap(),
        [
            ("users".to_owned(), None),
            ("users".to_owned(), Some(page)),
            ("users".to_owned(), Some(2 * page)),
        ],
        "a full page of new greatest keys after every page doesn't keep the walk going"
    );
    let applied = sorted(sinks[0].1.applied.lock().unwrap().clone());
    let expected: Vec<String> = (1..=total).map(|user| format!("users {user}")).collect();
    assert_eq!(
        applied,
        sorted(expected),
        "every row there at the start, once; none added during the walk"
    );
}
