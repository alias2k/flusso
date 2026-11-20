//! Pending documents (ADR 0012 stage 8a, slice 2): a document the ingest
//! engine has taken to build is pending on its index in the progress, so
//! confirmation and fences hold before it while it builds, and a lane's own
//! answers stay its own.

use source::cdc::Landed;
use stream::{ItemKey, LaneItem, LaneKey};

use crate::Level;
use tokio::sync::Notify;

use super::*;

/// Every change is its row's `users` document, plus a `ghost` one for an
/// index no lane receives. `build_many` waits for `release` once `held`.
#[derive(Debug, Default)]
struct Gated {
    held: AtomicBool,
    started: Notify,
    release: Notify,
}

#[async_trait]
impl DocumentBuilder for Gated {
    async fn resolve(
        &self,
        _table: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        Ok([users(), IndexName::try_new("ghost").unwrap()]
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

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        if self.held.load(Ordering::SeqCst) {
            self.started.notify_one();
            self.release.notified().await;
        }
        let mut documents = Vec::with_capacity(ids.len());
        for id in ids {
            documents.push(self.build(id).await?);
        }
        Ok(documents)
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

/// While the batch of change 0 builds, nothing of it is on a lane yet: the
/// progress itself holds confirmation before change 0, so it never rests on
/// the source alone. Once published, the lane's item holds it, and once the
/// item lands everything is confirmed. The `ghost` index has no lane: its
/// document pins nothing.
#[tokio::test]
async fn a_document_being_built_holds_confirmation_in_the_progress() {
    let primary = sink_name("primary");
    let lane = LaneKey::new(users(), primary.clone());
    let stream = Arc::new(ChannelStream::over(64, [users()], [primary.clone()]));
    let progress = Arc::new(Progress::of([users()], [primary.clone()]));
    let documents = Arc::new(Gated::default());
    documents.held.store(true, Ordering::SeqCst);
    let started = documents.started.notified();
    let engine = IngestEngine::new(
        Arc::new(MockSource::new(vec![upsert(1)])),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary],
        Arc::clone(&progress),
    );
    let run = tokio::spawn(async move { engine.run().await });
    tokio::time::timeout(Duration::from_secs(5), started)
        .await
        .expect("the build starts");

    assert_eq!(
        progress.landed(),
        Landed::Before(Position::new(0)),
        "change 0 is being built: confirmation stays before it"
    );

    documents.release.notify_one();
    run.await.unwrap().unwrap();
    let mut consumer = stream.lane(&lane).unwrap().consumer;
    let delivery = consumer.recv().await.unwrap().unwrap();
    let LaneItem::Batch(batch) = delivery.item() else {
        panic!("a live batch");
    };
    assert_eq!(batch.key, Some(ItemKey::new(0)));
    assert_eq!(progress.landed(), Landed::Before(Position::new(0)));
    progress.applied(&lane, ItemKey::new(0));
    assert_eq!(
        progress.landed(),
        Landed::Through(Position::new(0)),
        "landed, and the ghost index's document pins nothing"
    );
}

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

/// Change 9 touches only a `ghost` index no lane receives; every other
/// change touches its row's `users` and `orders` documents. The first build
/// holding a `users` document waits for `release` while `hold` is set;
/// `users` documents fail while `fail_users` is set.
#[derive(Debug, Default)]
struct TwoIndexes {
    hold: AtomicBool,
    fail_users: bool,
    release: Notify,
}

#[async_trait]
impl DocumentBuilder for TwoIndexes {
    async fn resolve(
        &self,
        _table: &TableName,
        key: &RowKey,
        _before: Option<&RowImage>,
        _after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        let indexes = if *key == super::key(9) {
            vec![IndexName::try_new("ghost").unwrap()]
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
        if self.fail_users && id.index == users() {
            return Ok(Document::Failed {
                id: id.clone(),
                failure: source::document::BuildFailure {
                    field: "label".to_owned(),
                    message: "boom".to_owned(),
                    position: None,
                },
            });
        }
        Ok(Document::Upsert {
            id: id.clone(),
            body: kernel::GenericValue::Map(BTreeMap::new()),
        })
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        if ids.iter().any(|id| id.index == users()) && self.hold.swap(false, Ordering::SeqCst) {
            self.release.notified().await;
        }
        let mut documents = Vec::with_capacity(ids.len());
        for id in ids {
            documents.push(self.build(id).await?);
        }
        Ok(documents)
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

/// What a test reads off the observer: batches built and changes settled,
/// in order.
#[derive(Debug, Default)]
struct Events(Mutex<Vec<String>>);

impl Observer for Events {
    fn on_batch_built(&self, stats: crate::BuildStats) {
        self.0
            .lock()
            .unwrap()
            .push(format!("built {}", stats.documents));
    }

    fn on_changes_settled(&self, sink: &SinkName, changes: usize) {
        self.0
            .lock()
            .unwrap()
            .push(format!("settled {changes} on {sink}"));
    }
}

/// A change that builds no document is settled in slot order, as an empty
/// batch was: after the batch granted before it is published, never ahead
/// of it.
#[tokio::test(start_paused = true)]
async fn a_change_that_builds_nothing_settles_after_the_batch_before_it() {
    let primary = sink_name("primary");
    let indexes = [orders(), users()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(Progress::of(indexes, [primary.clone()]));
    let documents = Arc::new(TwoIndexes::default());
    documents.hold.store(true, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let engine = IngestEngine::new(
        Arc::new(MockSource::new(vec![upsert(1), upsert(9)])),
        Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary],
        Arc::clone(&progress),
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(10),
    })
    .with_observer(Arc::clone(&events) as Arc<dyn Observer>);
    let run = tokio::spawn(async move { engine.run().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    documents.release.notify_one();
    run.await.unwrap().unwrap();
    assert_eq!(
        *events.0.lock().unwrap(),
        ["built 2", "settled 1 on primary", "built 0"],
        "the empty batch settles after the one granted before it, as an empty batch"
    );
}

/// Every document a build made for `users` fails under `skip`, so the
/// `users` lane gets no item and is told it was skipped; the `orders` lane
/// is full, so the batch's send to it waits. The `users` lane is settled
/// with nothing to land, and doesn't wait for the full lane of another index.
#[tokio::test(start_paused = true)]
async fn a_skipped_lane_is_not_held_by_a_full_lane_of_another_index() {
    let primary = sink_name("primary");
    let indexes = [orders(), users()];
    let stream = Arc::new(ChannelStream::over(2, indexes.clone(), [primary.clone()]));
    let progress = Arc::new(Progress::of(indexes, [primary.clone()]));
    let users_lane = LaneKey::new(users(), primary.clone());
    let orders_lane = LaneKey::new(orders(), primary.clone());
    stream
        .lane(&orders_lane)
        .unwrap()
        .producer
        .publish(LaneItem::Batch(stream::Batch {
            key: None,
            position: None,
            changes: 0,
            envelopes: Vec::new().into(),
            hold: Default::default(),
            walked: None,
            ticket: None,
        }))
        .await
        .unwrap();
    let engine = IngestEngine::new(
        Arc::new(MockSource::new(vec![upsert(1)])),
        Arc::new(TwoIndexes {
            fail_users: true,
            ..TwoIndexes::default()
        }) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary],
        Arc::clone(&progress),
    )
    .with_failure_policies(crate::FailurePolicies::new(crate::FailurePolicy::Skip));
    let run = tokio::spawn(async move { engine.run().await });
    super::wait_for(|| {
        progress.lane(&users_lane, Level::Applied) == Some(Landed::Through(Position::new(0)))
    })
    .await;
    assert!(!run.is_finished(), "the orders lane's send still waits");
    assert_eq!(
        progress.lane(&orders_lane, Level::Applied),
        Some(Landed::Before(Position::new(0)))
    );
    run.abort();
}

/// A run published the batch through change 10 to `primary` only (the send
/// to `audit` failed and the run ended), and `primary` landed it. The next
/// run's stream replays change 0. Its batch carries change 0's own position:
/// once both lanes land it, `audit` answers through change 0, never through
/// change 10, whose document it never got.
#[tokio::test]
async fn a_replayed_batch_claims_no_change_another_lane_landed() {
    let (primary, audit) = (sink_name("primary"), sink_name("audit"));
    let sinks = vec![primary.clone(), audit.clone()];
    let on_primary = LaneKey::new(users(), primary);
    let on_audit = LaneKey::new(users(), audit);
    let stream = Arc::new(ChannelStream::over(64, [users()], sinks.clone()));
    let progress = Arc::new(Progress::of([users()], sinks.clone()));
    let earlier = progress.next_key(&users()).unwrap();
    progress.published(&on_primary, earlier, Position::new(10), Position::new(10));
    progress.applied(&on_primary, earlier);

    IngestEngine::new(
        Arc::new(MockSource::new(vec![upsert(1)])),
        Arc::new(MockDocuments::default()),
        Arc::clone(&stream) as Arc<dyn Stream>,
        sinks,
        Arc::clone(&progress),
    )
    .run()
    .await
    .unwrap();
    for lane in [&on_primary, &on_audit] {
        let mut consumer = stream.lane(lane).unwrap().consumer;
        let delivery = consumer.recv().await.unwrap().unwrap();
        let (item, handle) = delivery.into_parts();
        let LaneItem::Batch(batch) = item else {
            panic!("a live batch");
        };
        progress.applied(lane, batch.key.unwrap());
        handle.ack().await.unwrap();
    }

    assert_eq!(
        progress.lane(&on_audit, Level::Applied),
        Some(Landed::Through(Position::new(0))),
        "audit landed only change 0"
    );
    assert_eq!(progress.landed(), Landed::Through(Position::new(0)));
}
