//! Items ready at once go out as one write (ADR 0012 stage 7, #397's
//! revisit): across lanes, and several small items of one lane, up to the
//! merge limit, never waiting for more.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ColumnName, Envelope, GenericValue, IndexMapping, IndexName, Position, TableName};
use sink::{RejectedDocument, Sink, WriteReport};
use source::RowKey;
use source::cdc::{ChangeCapture, ChangeEvent};
use source::document::{Document, DocumentBuilder, DocumentId};
use stream::{Batch, LaneItem, LaneKey, Stream};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;

use super::{MockSource, sink_name, wait_for};
use crate::sink_engine::Blame;
use crate::{
    BatchPolicy, FailurePolicies, FailurePolicy, IngestEngine, Progress, SinkControl, SinkEngine,
};

const TABLES: [&str; 3] = ["orders", "products", "users"];

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

fn mapping(name: &str) -> IndexMapping {
    IndexMapping {
        index: index(name),
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

fn mappings() -> Vec<IndexMapping> {
    TABLES.iter().map(|name| mapping(name)).collect()
}

fn row(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

fn change(table: &str, id: i64) -> ChangeEvent {
    ChangeEvent::Upsert {
        table: TableName::try_new(table).unwrap(),
        key: row(id),
        before: None,
        after: None,
    }
}

/// Three indexes, each rooted at the table of its name.
#[derive(Debug)]
struct ThreeIndexes;

#[async_trait]
impl DocumentBuilder for ThreeIndexes {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        _: Option<&source::RowImage>,
        _: Option<&source::RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: index(table.as_ref()),
            key: key.clone(),
        }])
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: GenericValue::Null,
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        TABLES.iter().map(|name| index(name)).collect()
    }

    async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
        Ok(None)
    }

    async fn keys(
        &self,
        _: &IndexName,
        _: Option<&RowKey>,
        _: &RowKey,
        _: usize,
    ) -> source::Result<Vec<RowKey>> {
        Ok(Vec::new())
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        Ok(mappings())
    }
}

/// One write as the sink got it: each part's index and ids.
type Write = Vec<(String, Vec<String>)>;

/// Records every write, as parts; takes `delay` per write; rejects every
/// document of `rejects`.
#[derive(Debug, Default)]
struct Parts {
    writes: Mutex<Vec<Write>>,
    delay: Duration,
    rejects: Option<IndexName>,
}

impl Parts {
    fn writes(&self) -> Vec<Write> {
        self.writes.lock().unwrap().clone()
    }

    fn documents_per_write(&self) -> Vec<usize> {
        self.writes()
            .iter()
            .map(|write| write.iter().map(|(_, ids)| ids.len()).sum())
            .collect()
    }
}

#[async_trait]
impl Sink for Parts {
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        let caught_up = if caught_up {
            sink::CaughtUp::All
        } else {
            sink::CaughtUp::No
        };
        self.write_parts(&[envelopes], caught_up).await
    }

    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        _: sink::CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        if parts.iter().all(|part| part.is_empty()) {
            return Ok(WriteReport::clean());
        }
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        let mut rejected = Vec::new();
        let mut write = Vec::new();
        for part in parts {
            let Some(first) = part.first() else { continue };
            assert!(
                part.iter().all(|e| e.index == first.index),
                "a part is one lane item: one index"
            );
            if self.rejects.as_ref() == Some(&first.index) {
                rejected.extend(part.iter().map(|e| RejectedDocument {
                    index: e.index.to_string(),
                    id: e.id.clone(),
                    reason: "refused".to_owned(),
                }));
            }
            write.push((
                first.index.to_string(),
                part.iter().map(|e| e.id.clone()).collect(),
            ));
        }
        self.writes.lock().unwrap().push(write);
        Ok(WriteReport::clean().rejecting(rejected))
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

fn item(index_name: &str, ids: std::ops::Range<i64>, position: Option<u64>) -> LaneItem {
    LaneItem::Batch(Batch {
        position: position.map(Position::new),
        changes: 1,
        envelopes: ids
            .map(|id| {
                Envelope::upsert(
                    index(index_name),
                    id.to_string(),
                    GenericValue::Map(BTreeMap::new()),
                    position.map(Position::new),
                    chrono::Utc::now(),
                )
            })
            .collect::<Vec<_>>()
            .into(),
        hold: Default::default(),
        walked: None,
    })
}

struct Rig {
    stream: Arc<ChannelStream>,
    progress: Arc<Progress>,
}

impl Rig {
    fn new() -> Self {
        let primary = sink_name("primary");
        let indexes: Vec<IndexName> = TABLES.iter().map(|name| index(name)).collect();
        Self {
            stream: Arc::new(ChannelStream::over(
                1024,
                indexes.clone(),
                [primary.clone()],
            )),
            progress: Arc::new(Progress::of(indexes, [primary])),
        }
    }

    async fn publish(&self, index_name: &str, item: LaneItem) {
        self.stream
            .lane(&LaneKey::new(index(index_name), sink_name("primary")))
            .unwrap()
            .producer
            .publish(item)
            .await
            .unwrap();
    }

    /// An engine past its first landing: a fresh one writes each lane's
    /// first item alone (a previous process may have stopped on it).
    fn engine(&self, sink: Arc<Parts>) -> SinkEngine {
        let engine = SinkEngine::new(
            sink_name("primary"),
            sink as Arc<dyn Sink>,
            Arc::clone(&self.stream) as Arc<dyn Stream>,
            mappings(),
            Arc::clone(&self.progress),
        );
        engine.set_blame(Blame::Nobody);
        engine
    }
}

fn spawn(engine: SinkEngine) -> tokio::task::JoinHandle<crate::Result<()>> {
    tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    })
}

/// A burst over three indexes is written in writes of about a batch, as one
/// lane per sink wrote it, not one per index: `main` wrote ~129 documents per
/// write on `reference`; one item per index (stage 7 before merging) ~35.
#[tokio::test]
async fn a_burst_over_three_indexes_is_written_in_writes_of_about_a_batch() {
    for writes in [1, 4] {
        let sink = Arc::new(Parts {
            delay: Duration::from_millis(2),
            ..Parts::default()
        });
        let rig = Rig::new();
        let primary = sink_name("primary");
        let engine = rig.engine(Arc::clone(&sink)).with_writes(writes);
        engine
            .stage(source::cdc::Continuity::Resumed)
            .await
            .unwrap();
        let task = spawn(engine);
        let changes: Vec<ChangeEvent> = (0..1536)
            .map(|id| change(TABLES[(id % 3) as usize], id))
            .collect();
        IngestEngine::new(
            Arc::new(MockSource::new(changes)) as Arc<dyn ChangeCapture>,
            Arc::new(ThreeIndexes) as Arc<dyn DocumentBuilder>,
            Arc::clone(&rig.stream) as Arc<dyn Stream>,
            vec![primary],
            Arc::clone(&rig.progress),
        )
        .with_batch(BatchPolicy {
            max_changes: 256,
            max_delay: Duration::from_secs(5),
        })
        .run()
        .await
        .unwrap();
        wait_for(|| rig.stream.is_idle()).await;
        task.abort();

        let sizes = sink.documents_per_write();
        assert_eq!(sizes.iter().sum::<usize>(), 1536, "writes = {writes}");
        assert!(
            sizes.iter().all(|size| *size <= 256),
            "a merged write holds at most max_changes documents: {sizes:?}"
        );
        let mean = sizes.iter().sum::<usize>() / sizes.len();
        assert!(
            mean >= 150,
            "writes = {writes}: {} writes, {mean} documents each on average: {sizes:?}",
            sizes.len()
        );
    }
}

/// Small items of one lane, ready at once, go out together too.
#[tokio::test]
async fn small_items_of_one_lane_are_merged() {
    let rig = Rig::new();
    for n in 0..8 {
        rig.publish("users", item("users", n * 4..n * 4 + 4, Some(n as u64)))
            .await;
    }
    let sink = Arc::new(Parts::default());
    let task = spawn(rig.engine(Arc::clone(&sink)).with_writes(1));
    wait_for(|| rig.stream.is_idle()).await;
    task.abort();
    assert_eq!(sink.documents_per_write(), [32], "{:?}", sink.writes());
}

/// Merging takes what is ready; it never waits for more. A lone item is
/// written at once, alone, with no timer involved.
#[tokio::test(start_paused = true)]
async fn a_lone_item_is_written_at_once() {
    let rig = Rig::new();
    let sink = Arc::new(Parts::default());
    let task = spawn(rig.engine(Arc::clone(&sink)));
    rig.publish("users", item("users", 0..1, Some(0))).await;
    // Yielding never lets the paused clock move: a write that waited on a
    // timer would never come.
    for _ in 0..10_000 {
        if !sink.writes().is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(sink.documents_per_write(), [1]);
    task.abort();
}

/// Items past the limit together are not merged, and an item larger than the
/// limit on its own is written alone, whole.
#[tokio::test]
async fn a_merged_write_stays_within_the_limit() {
    let rig = Rig::new();
    rig.publish("orders", item("orders", 0..200, Some(0))).await;
    rig.publish("products", item("products", 0..200, Some(1)))
        .await;
    rig.publish("users", item("users", 0..300, Some(2))).await;
    rig.publish("users", item("users", 300..356, Some(3))).await;
    let sink = Arc::new(Parts::default());
    let task = spawn(rig.engine(Arc::clone(&sink)).with_writes(1));
    wait_for(|| rig.stream.is_idle()).await;
    task.abort();
    let mut sizes = sink.documents_per_write();
    sizes.sort_unstable();
    assert_eq!(sizes.iter().sum::<usize>(), 756);
    assert!(
        sizes
            .iter()
            .filter(|size| **size > 256)
            .all(|size| *size == 300),
        "only the 300-document item exceeds the limit, alone: {:?}",
        sink.writes()
    );
}

/// A document written twice is never in one write twice, nor in two writes at
/// once: the later item of its lane waits for the earlier, merged or not.
#[tokio::test]
async fn a_document_is_never_merged_with_its_own_later_build() {
    let rig = Rig::new();
    rig.publish("users", item("users", 0..4, Some(0))).await;
    rig.publish("users", item("users", 2..6, Some(1))).await;
    rig.publish("orders", item("orders", 0..4, Some(2))).await;
    let sink = Arc::new(Parts::default());
    let task = spawn(rig.engine(Arc::clone(&sink)).with_writes(4));
    wait_for(|| rig.stream.is_idle()).await;
    task.abort();
    let writes = sink.writes();
    for write in &writes {
        let users: Vec<&String> = write
            .iter()
            .filter(|(index, _)| index == "users")
            .flat_map(|(_, ids)| ids)
            .collect();
        let mut distinct = users.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(users.len(), distinct.len(), "{writes:?}");
    }
    let first_users = writes
        .iter()
        .position(|write| write.iter().any(|(_, ids)| ids.contains(&"0".to_owned())))
        .unwrap();
    let second_users = writes
        .iter()
        .position(|write| write.iter().any(|(_, ids)| ids.contains(&"5".to_owned())))
        .unwrap();
    assert!(first_users < second_users, "{writes:?}");
}

/// A `stop` rejection in a merged write is its own index's: only that lane's
/// item stays unacknowledged and only that lane is blamed; the other parts
/// landed and are acknowledged.
#[tokio::test]
async fn a_stop_rejection_in_a_merged_write_bars_only_its_own_lane() {
    let rig = Rig::new();
    rig.publish("users", item("users", 0..4, Some(0))).await;
    rig.publish("orders", item("orders", 0..4, Some(1))).await;
    let sink = Arc::new(Parts {
        rejects: Some(index("orders")),
        ..Parts::default()
    });
    let engine = rig
        .engine(Arc::clone(&sink))
        .with_writes(1)
        .with_failure_policies(FailurePolicies::new(FailurePolicy::Stop));
    let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
    let result = engine.run(&mut control).await;
    assert!(result.is_err(), "the orders rejection stops the run");
    assert_eq!(sink.documents_per_write(), [8], "one merged write");
    assert_eq!(
        engine.blame(),
        Blame::Lanes([LaneKey::new(index("orders"), sink_name("primary"))].into()),
    );

    // The next run gets back only what wasn't acknowledged: the orders item.
    let again = Arc::new(Parts::default());
    let task = spawn(rig.engine(Arc::clone(&again)).with_writes(1));
    wait_for(|| rig.stream.is_idle()).await;
    task.abort();
    assert_eq!(
        again.writes(),
        [vec![(
            "orders".to_owned(),
            (0..4).map(|id| id.to_string()).collect::<Vec<_>>()
        )]],
        "the users part landed and was acknowledged"
    );
}

/// Under `skip` the rejected part's documents are quarantined and every part
/// is acknowledged.
#[tokio::test]
async fn a_skip_rejection_in_a_merged_write_acknowledges_every_part() {
    let rig = Rig::new();
    rig.publish("users", item("users", 0..4, Some(0))).await;
    rig.publish("orders", item("orders", 0..4, Some(1))).await;
    let sink = Arc::new(Parts {
        rejects: Some(index("orders")),
        ..Parts::default()
    });
    let engine = rig
        .engine(Arc::clone(&sink))
        .with_writes(1)
        .with_failure_policies(
            FailurePolicies::new(FailurePolicy::Stop).with_override("orders", FailurePolicy::Skip),
        );
    let task = spawn(engine);
    wait_for(|| rig.stream.is_idle()).await;
    task.abort();
    assert_eq!(sink.documents_per_write(), [8]);
}

/// `item` carrying a memory charge of `bytes`, as the ingest engine publishes it.
fn charged(item: LaneItem, budget: &Arc<crate::Budget>, bytes: usize) -> LaneItem {
    let LaneItem::Batch(mut batch) = item else {
        unreachable!("item builds a batch")
    };
    let charge = budget
        .try_reserve(crate::Work::Live, bytes)
        .unwrap()
        .settle(bytes);
    batch.hold = stream::Hold::new(Arc::new(charge));
    LaneItem::Batch(batch)
}

/// Each part of a merged write gives its memory back at its own
/// acknowledgement, once: a part a `stop` rejection holds back keeps its bytes
/// while the part that landed has returned its, and they come back when the
/// held part lands on the next run.
#[tokio::test]
async fn each_part_of_a_merged_write_gives_its_bytes_back_at_its_own_ack() {
    const KIB: usize = 1024;
    let budget = Arc::new(crate::Budget::new(1 << 20));
    let rig = Rig::new();
    rig.publish(
        "users",
        charged(item("users", 0..4, Some(0)), &budget, 4 * KIB),
    )
    .await;
    rig.publish(
        "orders",
        charged(item("orders", 0..4, Some(1)), &budget, 8 * KIB),
    )
    .await;
    assert_eq!(budget.held().total, 12 * KIB);

    let sink = Arc::new(Parts {
        rejects: Some(index("orders")),
        ..Parts::default()
    });
    let engine = rig
        .engine(Arc::clone(&sink))
        .with_writes(1)
        .with_failure_policies(FailurePolicies::new(FailurePolicy::Stop));
    let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
    assert!(engine.run(&mut control).await.is_err());
    assert_eq!(sink.documents_per_write(), [8], "one merged write");
    wait_for(|| budget.held().total == 8 * KIB).await;

    let again = Arc::new(Parts::default());
    let task = spawn(rig.engine(Arc::clone(&again)).with_writes(1));
    wait_for(|| rig.stream.is_idle()).await;
    task.abort();
    let _ = task.await;
    wait_for(|| budget.held() == crate::Held::default()).await;
}

/// Refuses every document of `orders` and reports what it wrote searchable
/// only later; the empty caught-up write that follows fails.
#[derive(Debug, Default)]
struct RefusesThenFailsToRefresh;

#[async_trait]
impl Sink for RefusesThenFailsToRefresh {
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        let scope = if caught_up {
            sink::CaughtUp::All
        } else {
            sink::CaughtUp::No
        };
        self.write_parts(&[envelopes], scope).await
    }

    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        _: sink::CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        if parts.iter().all(|part| part.is_empty()) {
            return Err(sink::SinkError::Write("the refresh failed".to_owned()));
        }
        let rejected = parts
            .iter()
            .flat_map(|part| part.iter())
            .filter(|e| e.index == index("orders"))
            .map(|e| RejectedDocument {
                index: e.index.to_string(),
                id: e.id.clone(),
                reason: "refused".to_owned(),
            })
            .collect::<Vec<_>>();
        Ok(WriteReport::clean()
            .rejecting(rejected)
            .visible(sink::Visibility::Deferred))
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// A `stop` rejection in a merged write is blamed on its own lane even when
/// acknowledging the other part fails (here, its caught-up write): the run
/// doesn't fall back to blaming every lane.
#[tokio::test]
async fn a_stop_rejection_keeps_its_blame_when_an_acknowledgement_fails() {
    let rig = Rig::new();
    rig.publish("users", item("users", 0..4, Some(0))).await;
    rig.publish("orders", item("orders", 0..4, Some(1))).await;
    let engine = SinkEngine::new(
        sink_name("primary"),
        Arc::new(RefusesThenFailsToRefresh) as Arc<dyn Sink>,
        Arc::clone(&rig.stream) as Arc<dyn Stream>,
        mappings(),
        Arc::clone(&rig.progress),
    )
    .with_writes(1)
    .with_failure_policies(FailurePolicies::new(FailurePolicy::Stop));
    engine.set_blame(Blame::Nobody);
    let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
    assert!(engine.run(&mut control).await.is_err());
    assert_eq!(
        engine.blame(),
        Blame::Lanes([LaneKey::new(index("orders"), sink_name("primary"))].into()),
    );
}
