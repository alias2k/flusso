//! Item keys (ADR 0012 stage 8a, slice 1): every live item is numbered per
//! index, the same on each lane of the index, and the numbering outlives a
//! run of the ingest engine, so a lane never holds two items under one key.

use stream::{ItemKey, LaneItem, LaneKey};

use super::*;

/// A source whose live stream replays the same changes from position 0 on
/// every open, as a stream reopened before anything was confirmed would.
#[derive(Debug)]
struct Replaying(Vec<ChangeEvent>);

#[async_trait]
impl ChangeCapture for Replaying {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        let changes = self.0.clone();
        Ok(Box::pin(futures::stream::iter(
            changes
                .into_iter()
                .enumerate()
                .map(|(i, event)| Ok((Position::new(i as u64), event))),
        )))
    }

    fn confirm(&self, _: source::cdc::Landed) {}
}

fn ingest(
    source: Arc<dyn ChangeCapture>,
    stream: &Arc<ChannelStream>,
    sinks: Vec<SinkName>,
    progress: &Arc<Progress>,
) -> IngestEngine {
    IngestEngine::new(
        source,
        Arc::new(MockDocuments::default()),
        Arc::clone(stream) as Arc<dyn Stream>,
        sinks,
        Arc::clone(progress),
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(10),
    })
}

/// The `(key, position seq)` of the next `count` items on `lane`, left
/// unacknowledged.
async fn items(stream: &ChannelStream, lane: &LaneKey, count: usize) -> Vec<(ItemKey, u64)> {
    let mut consumer = stream.lane(lane).unwrap().consumer;
    let mut items = Vec::new();
    let mut held = Vec::new();
    for _ in 0..count {
        let delivery = consumer.recv().await.unwrap().unwrap();
        let (item, handle) = delivery.into_parts();
        let LaneItem::Batch(batch) = item else {
            panic!("only live batches were published");
        };
        items.push((batch.key.unwrap(), batch.position.unwrap().seq));
        held.push(handle);
    }
    assert!(consumer.is_empty(), "nothing more was published");
    items
}

/// A second run replays changes under positions the first run's items still
/// hold, unacknowledged on the lane: its items get the next keys, so landing
/// the first run's items covers none of the second's.
#[tokio::test]
async fn a_key_survives_an_ingest_restart() {
    let primary = sink_name("primary");
    let lane = LaneKey::new(users(), primary.clone());
    let stream = Arc::new(ChannelStream::over(64, [users()], [primary.clone()]));
    let progress = Arc::new(Progress::of([users()], [primary.clone()]));
    let source: Arc<dyn ChangeCapture> = Arc::new(Replaying(vec![upsert(1), upsert(2)]));
    let engine = ingest(source, &stream, vec![primary], &progress);
    engine.run().await.unwrap();
    engine.run().await.unwrap();

    let items = items(&stream, &lane, 4).await;
    assert_eq!(
        items,
        [
            (ItemKey::new(0), 0),
            (ItemKey::new(1), 1),
            (ItemKey::new(2), 0),
            (ItemKey::new(3), 1)
        ],
        "positions repeat across the runs; keys go on"
    );

    progress.applied(&lane, ItemKey::new(1));
    assert_eq!(
        progress.landed(),
        source::cdc::Landed::Before(Position::new(0)),
        "the second run's items are still outstanding"
    );
    progress.applied(&lane, ItemKey::new(3));
    assert_eq!(
        progress.landed(),
        source::cdc::Landed::Through(Position::new(1))
    );
}

/// One batch puts one item on each lane of its index, all under the same key,
/// and the next batch takes the next one.
#[tokio::test]
async fn a_batch_takes_one_key_per_index_for_every_lane() {
    let (primary, audit) = (sink_name("primary"), sink_name("audit"));
    let sinks = vec![primary.clone(), audit.clone()];
    let stream = Arc::new(ChannelStream::over(64, [users()], sinks.clone()));
    let progress = Arc::new(Progress::of([users()], sinks.clone()));
    let source: Arc<dyn ChangeCapture> =
        Arc::new(MockSource::new(vec![upsert(1), upsert(2), upsert(3)]));
    ingest(source, &stream, sinks, &progress)
        .run()
        .await
        .unwrap();

    let expected = [
        (ItemKey::new(0), 0),
        (ItemKey::new(1), 1),
        (ItemKey::new(2), 2),
    ];
    assert_eq!(
        items(&stream, &LaneKey::new(users(), primary), 3).await,
        expected
    );
    assert_eq!(
        items(&stream, &LaneKey::new(users(), audit), 3).await,
        expected
    );
}

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

/// Resolves change `1` to its `orders` document alone and every other change
/// to both its `users` and its `orders` document.
#[derive(Debug)]
struct UsersAndOrders;

#[async_trait]
impl DocumentBuilder for UsersAndOrders {
    async fn resolve(
        &self,
        _table: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        let indexes = if *key == super::key(1) {
            vec![orders()]
        } else {
            vec![users(), orders()]
        };
        Ok(indexes
            .into_iter()
            .map(|index| DocumentId {
                index,
                key: key.clone(),
            })
            .collect())
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Upsert {
            id: id.clone(),
            body: kernel::GenericValue::Map(BTreeMap::new()),
        })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        Vec::new()
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
}

/// Two indexes into two sinks: each index numbers its own items, and both of
/// its lanes get the same key for one batch. `orders` gets an item from the
/// first batch, `users` doesn't, so their keys part from the start.
#[tokio::test]
async fn each_index_keys_its_own_items_and_its_lanes_share_them() {
    let (primary, audit) = (sink_name("primary"), sink_name("audit"));
    let sinks = vec![primary.clone(), audit.clone()];
    let indexes = [users(), orders()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), sinks.clone()));
    let progress = Arc::new(Progress::of(indexes, sinks.clone()));
    IngestEngine::new(
        Arc::new(MockSource::new(vec![upsert(1), upsert(2), upsert(3)])),
        Arc::new(UsersAndOrders),
        Arc::clone(&stream) as Arc<dyn Stream>,
        sinks,
        Arc::clone(&progress),
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(10),
    })
    .run()
    .await
    .unwrap();

    let orders_items = [
        (ItemKey::new(0), 0),
        (ItemKey::new(1), 1),
        (ItemKey::new(2), 2),
    ];
    let users_items = [(ItemKey::new(0), 1), (ItemKey::new(1), 2)];
    for sink in [primary, audit] {
        assert_eq!(
            items(&stream, &LaneKey::new(orders(), sink.clone()), 3).await,
            orders_items,
            "orders into {sink}"
        );
        assert_eq!(
            items(&stream, &LaneKey::new(users(), sink.clone()), 2).await,
            users_items,
            "users into {sink}"
        );
    }
}

/// A live item that carries no key could be neither marked landed nor
/// fenced, so a later item's mark would cover it: the sink engine refuses
/// it and fails its run, leaving it unacknowledged.
#[tokio::test]
async fn a_live_item_without_a_key_fails_the_sink_engine() {
    let primary = sink_name("primary");
    let lane = LaneKey::new(users(), primary.clone());
    let stream = Arc::new(ChannelStream::over(64, [users()], [primary.clone()]));
    let progress = Arc::new(Progress::of([users()], [primary.clone()]));
    stream
        .lane(&lane)
        .unwrap()
        .producer
        .publish(LaneItem::Batch(stream::Batch {
            key: None,
            position: Some(Position::new(3)),
            envelopes: Vec::new().into(),
            hold: Default::default(),
            walked: None,
            ticket: None,
        }))
        .await
        .unwrap();
    let engine = SinkEngine::new(
        primary,
        Arc::new(RecordingSink::seeded(true)),
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
        Arc::clone(&progress),
    );
    let (_keep, mut control) = tokio::sync::mpsc::channel::<SinkControl>(1);
    let outcome = tokio::time::timeout(Duration::from_secs(5), engine.run(&mut control))
        .await
        .expect("the run ends");
    assert!(
        matches!(outcome, Err(EngineError::UnkeyedLiveItem(ref named)) if *named == lane.to_string()),
        "{outcome:?}"
    );
    assert!(!stream.is_idle(), "the item is left unacknowledged");
    assert_eq!(progress.landed(), source::cdc::Landed::Idle);
}
