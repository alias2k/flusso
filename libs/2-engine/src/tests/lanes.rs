//! One lane per `(index, sink)`: each lane's progress is its own, a batch is
//! split by index at publish, and a sink engine reads all its lanes, keeping
//! each lane's order while lanes take turns and never wait on each other.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ColumnName, Envelope, GenericValue, IndexMapping, IndexName, Position, TableName};
use sink::{Sink, WriteReport};
use source::RowKey;
use source::cdc::ChangeEvent;
use source::cdc::{ChangeCapture, Landed};
use source::document::{Document, DocumentBuilder, DocumentId};
use stream::{Batch, LaneItem, LaneKey, Stream};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;

use super::{
    MockDocuments, MockSource, RecordingSink, mapping, sink_name, upsert, users, wait_for,
};
use crate::{BatchPolicy, IngestEngine, Level, SinkControl, SinkEngine};

/// A sink whose writes never return.
#[derive(Debug)]
struct Stalled;

#[async_trait]
impl Sink for Stalled {
    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        std::future::pending().await
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// The seq just past what `landed` says is landed.
fn bound(landed: Landed) -> u64 {
    match landed {
        Landed::Before(oldest) => oldest.seq,
        Landed::Through(last) => last.seq + 1,
        Landed::Idle => 0,
    }
}

/// Lanes hold one item, `stalled` never writes: the ingest engine blocks
/// publishing to it while `healthy` goes on writing what it already took.
/// Everything `healthy` wrote must be what its lane reaches.
#[tokio::test]
async fn a_healthy_lane_reaches_what_it_landed_while_a_sibling_lane_is_full() {
    let (healthy, stalled) = (sink_name("healthy"), sink_name("stalled"));
    let stream = Arc::new(ChannelStream::over(
        1,
        [users()],
        [healthy.clone(), stalled.clone()],
    ));
    let source = Arc::new(MockSource::new((0..60).map(upsert).collect()));
    let ingest = IngestEngine::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::new(MockDocuments::default()) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![healthy.clone(), stalled.clone()],
        Arc::new(crate::Progress::of(
            [users()],
            [healthy.clone(), stalled.clone()],
        )),
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(1),
    });
    let progress = ingest.progress();
    let recording = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&recording.ops);
    let mut tasks = Vec::new();
    for (name, sink) in [
        (healthy.clone(), recording as Arc<dyn Sink>),
        (stalled.clone(), Arc::new(Stalled) as Arc<dyn Sink>),
    ] {
        let engine = SinkEngine::new(
            name,
            sink,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![mapping()],
            Arc::clone(&progress),
        );
        engine
            .stage(source::cdc::Continuity::Resumed)
            .await
            .unwrap();
        tasks.push(tokio::spawn(async move {
            let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
            engine.run(&mut control).await
        }));
    }
    let run = tokio::spawn(async move { ingest.run().await });

    wait_for(|| ops.lock().unwrap().len() >= 3).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let written = ops.lock().unwrap().len() as u64;
    let reached = progress
        .lane(&LaneKey::new(users(), healthy.clone()), Level::Applied)
        .unwrap();
    assert!(
        bound(reached) >= written,
        "the healthy sink wrote positions 0..{written}, its lane says {reached:?}"
    );
    assert_eq!(
        progress.lane(&LaneKey::new(users(), stalled.clone()), Level::Applied),
        Some(Landed::Before(Position::new(0))),
        "the stalled lane still pins at its first item"
    );
    assert_eq!(progress.landed(), Landed::Before(Position::new(0)));

    run.abort();
    for task in tasks {
        task.abort();
    }
}

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

fn row(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

/// A change to `table` (`users` or `orders`), whose document is the row in
/// the index of the same name.
fn change(table: &str, id: i64) -> ChangeEvent {
    ChangeEvent::Upsert {
        table: TableName::try_new(table).unwrap(),
        key: row(id),
        before: None,
        after: None,
    }
}

fn index_mapping(index: IndexName) -> IndexMapping {
    IndexMapping {
        index,
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// Two indexes, each rooted at the table of its name.
#[derive(Debug)]
struct TwoIndexes;

#[async_trait]
impl DocumentBuilder for TwoIndexes {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        _: Option<&source::RowImage>,
        _: Option<&source::RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok(vec![DocumentId {
            index: IndexName::try_new(table.as_ref()).unwrap(),
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
        vec![orders(), users()]
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
        Ok(vec![index_mapping(orders()), index_mapping(users())])
    }
}

/// Records each write as the index of its envelopes and their ids, and
/// holds every write of `held` until released.
#[derive(Debug, Default)]
struct ByIndex {
    writes: Mutex<Vec<(String, Vec<String>)>>,
    held: Option<IndexName>,
    seeded: Mutex<Vec<String>>,
}

impl ByIndex {
    fn holding(index: IndexName) -> Self {
        Self {
            held: Some(index),
            ..Self::default()
        }
    }

    fn writes(&self) -> Vec<(String, Vec<String>)> {
        self.writes.lock().unwrap().clone()
    }
}

#[async_trait]
impl Sink for ByIndex {
    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        let Some(first) = envelopes.first() else {
            return Ok(WriteReport::clean());
        };
        assert!(
            envelopes.iter().all(|e| e.index == first.index),
            "an item holds one index's documents"
        );
        if self.held.as_ref() == Some(&first.index) {
            std::future::pending::<()>().await;
        }
        self.writes.lock().unwrap().push((
            first.index.to_string(),
            envelopes.iter().map(|e| e.id.clone()).collect(),
        ));
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.seeded.lock().unwrap().push(index.to_string());
        Ok(())
    }
}

/// The ingest engine over two indexes and one sink, run until its live
/// stream ends.
async fn run_two_indexes(
    changes: Vec<ChangeEvent>,
    max_changes: usize,
    sink: Arc<ByIndex>,
) -> Arc<crate::Progress> {
    let primary = sink_name("primary");
    let indexes = [orders(), users()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(crate::Progress::of(indexes, [primary.clone()]));
    let engine = SinkEngine::new(
        primary.clone(),
        sink as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![index_mapping(orders()), index_mapping(users())],
        Arc::clone(&progress),
    );
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let task = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    IngestEngine::new(
        Arc::new(MockSource::new(changes)) as Arc<dyn ChangeCapture>,
        Arc::new(TwoIndexes) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary],
        Arc::clone(&progress),
    )
    .with_batch(BatchPolicy {
        max_changes,
        max_delay: Duration::from_secs(5),
    })
    .run()
    .await
    .unwrap();
    wait_for(|| stream.is_idle()).await;
    task.abort();
    progress
}

/// A batch touching two indexes reaches each index's lane with that index's
/// documents alone; a batch touching one leaves the other's lane skipped, so
/// that lane pins nothing.
#[tokio::test]
async fn a_batch_is_split_by_index_and_a_quiet_index_pins_nothing() {
    let sink = Arc::new(ByIndex::default());
    let progress = run_two_indexes(
        vec![change("users", 1), change("orders", 2), change("users", 3)],
        2,
        Arc::clone(&sink),
    )
    .await;

    let mut writes = sink.writes();
    writes.sort();
    assert_eq!(
        writes,
        [
            ("orders".to_owned(), vec!["2".to_owned()]),
            ("users".to_owned(), vec!["1".to_owned()]),
            ("users".to_owned(), vec!["3".to_owned()]),
        ],
        "one item per index of each batch"
    );
    let lane = |index| LaneKey::new(index, sink_name("primary"));
    assert_eq!(
        progress.lane(&lane(orders()), Level::Applied),
        Some(Landed::Through(Position::new(2))),
        "the last batch had nothing for orders: skipped, its lane is through it"
    );
    assert_eq!(progress.landed(), Landed::Through(Position::new(2)));
}

/// One index's writes never return; the other index's lane goes on writing,
/// its progress moves, and only confirmation waits on the stalled lane.
#[tokio::test]
async fn a_stalled_index_holds_back_neither_its_siblings_writes_nor_its_progress() {
    let sink = Arc::new(ByIndex::holding(orders()));
    let primary = sink_name("primary");
    let indexes = [orders(), users()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(crate::Progress::of(indexes, [primary.clone()]));
    let engine = SinkEngine::new(
        primary.clone(),
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![index_mapping(orders()), index_mapping(users())],
        Arc::clone(&progress),
    )
    .with_writes(4);
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let task = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    let ingest = IngestEngine::new(
        Arc::new(MockSource::new(
            (1..=6)
                .map(|id| change(if id % 2 == 0 { "orders" } else { "users" }, id))
                .collect(),
        )) as Arc<dyn ChangeCapture>,
        Arc::new(TwoIndexes) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary.clone()],
        Arc::clone(&progress),
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(1),
    });
    let run = tokio::spawn(async move { ingest.run().await });

    wait_for(|| sink.writes().len() == 3).await;
    assert_eq!(
        sink.writes()
            .iter()
            .map(|(_, ids)| ids.join(","))
            .collect::<Vec<_>>(),
        ["1", "3", "5"],
        "every users item is written, orders 2 is still in flight"
    );
    let users_lane = LaneKey::new(users(), primary.clone());
    wait_for(|| {
        progress.lane(&users_lane, Level::Applied) == Some(Landed::Through(Position::new(5)))
    })
    .await;
    assert_eq!(
        progress.lane(&LaneKey::new(orders(), primary), Level::Applied),
        Some(Landed::Before(Position::new(1))),
        "orders 2 is the oldest item it hasn't landed"
    );
    assert_eq!(progress.landed(), Landed::Before(Position::new(1)));
    run.abort();
    task.abort();
}

/// Lanes take turns: with one write at a time and a backlog on one index,
/// the other index's item is not left behind the backlog.
#[tokio::test]
async fn lanes_take_turns_so_a_backlog_on_one_index_does_not_starve_another() {
    let sink = Arc::new(ByIndex::default());
    let primary = sink_name("primary");
    let indexes = [orders(), users()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(crate::Progress::of(indexes, [primary.clone()]));
    let item = |index: IndexName, position: u64| {
        LaneItem::Batch(Batch {
            position: Some(Position::new(position)),
            changes: 1,
            envelopes: vec![Envelope::upsert(
                index,
                position.to_string(),
                GenericValue::Map(BTreeMap::new()),
                Some(Position::new(position)),
                chrono::Utc::now(),
            )]
            .into(),
        })
    };
    let orders_lane = stream
        .lane(&LaneKey::new(orders(), primary.clone()))
        .unwrap()
        .producer;
    let users_lane = stream
        .lane(&LaneKey::new(users(), primary.clone()))
        .unwrap()
        .producer;
    for position in 0..10 {
        orders_lane.publish(item(orders(), position)).await.unwrap();
    }
    users_lane.publish(item(users(), 10)).await.unwrap();

    let engine = SinkEngine::new(
        primary,
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![index_mapping(orders()), index_mapping(users())],
        progress,
    );
    let task = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    wait_for(|| sink.writes().len() == 11).await;
    task.abort();
    let at = sink
        .writes()
        .iter()
        .position(|(index, _)| index == "users")
        .unwrap();
    assert!(
        at <= 2,
        "the users item waited behind {at} orders items: {:?}",
        sink.writes()
    );
    let orders_ids: Vec<String> = sink
        .writes()
        .into_iter()
        .filter(|(index, _)| index == "orders")
        .flat_map(|(_, ids)| ids)
        .collect();
    assert_eq!(
        orders_ids,
        (0..10).map(|p| p.to_string()).collect::<Vec<_>>(),
        "a lane keeps its order"
    );
}

/// A `SnapshotComplete` is a barrier on its own lane: it waits for the
/// backfill before it on that lane, and another lane's items and snapshot go
/// on meanwhile.
#[tokio::test]
async fn a_snapshot_complete_waits_only_for_its_own_lane() {
    let sink = Arc::new(ByIndex::holding(orders()));
    let primary = sink_name("primary");
    let indexes = [orders(), users()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(crate::Progress::of(indexes, [primary.clone()]));
    let backfill = |index: IndexName, id: &str| {
        LaneItem::Batch(Batch {
            position: None,
            changes: 0,
            envelopes: vec![Envelope::upsert(
                index,
                id,
                GenericValue::Map(BTreeMap::new()),
                None,
                chrono::Utc::now(),
            )]
            .into(),
        })
    };
    let lane = |index: IndexName| {
        stream
            .lane(&LaneKey::new(index, primary.clone()))
            .unwrap()
            .producer
    };
    let (orders_lane, users_lane) = (lane(orders()), lane(users()));
    orders_lane.publish(backfill(orders(), "o")).await.unwrap();
    orders_lane
        .publish(LaneItem::SnapshotComplete { index: orders() })
        .await
        .unwrap();
    users_lane.publish(backfill(users(), "u")).await.unwrap();
    users_lane
        .publish(LaneItem::SnapshotComplete { index: users() })
        .await
        .unwrap();

    let engine = SinkEngine::new(
        primary,
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![index_mapping(orders()), index_mapping(users())],
        progress,
    )
    .with_writes(4);
    let task = tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    });
    wait_for(|| sink.seeded.lock().unwrap().len() == 1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(*sink.seeded.lock().unwrap(), ["users"]);
    assert_eq!(sink.writes(), [("users".to_owned(), vec!["u".to_owned()])]);
    task.abort();
}
