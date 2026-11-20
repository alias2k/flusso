//! Pending documents (ADR 0012 stage 8a, slice 2): a document the ingest
//! engine has taken to build is pending on its index in the progress, so
//! confirmation and fences hold before it while it builds, and a lane's own
//! answers stay its own.

use source::cdc::Landed;
use stream::{ItemKey, LaneItem, LaneKey};
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
