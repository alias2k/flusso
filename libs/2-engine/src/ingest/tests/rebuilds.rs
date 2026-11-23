//! Fresh builds of a parked lane's item (ADR 0012 stage 9, second half,
//! `docs/adr/0012-stage-notes/stage-9.md` §14.1): the ingest side. A
//! `Request::Rebuild` reaches its index's builder, which builds those
//! documents once, publishes them as a substitute on the asking sink's
//! substitute lane only, and acknowledges the request. Every wait is bounded.

use std::sync::atomic::AtomicUsize;

use super::*;
use crate::observer::Observer;
use stream::{LaneKey, Request, Substitute, Ticket};

const LIMIT: Duration = Duration::from_secs(120);

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

fn rebuild(ids: &[i64], ticket: u64) -> Request {
    Request::Rebuild {
        index: orders(),
        sink: sink_name(),
        keys: ids.iter().map(|id| key(*id)).collect(),
        ticket: Ticket::new(ticket),
    }
}

/// Every substitute waiting on `sink`'s substitute lane, acknowledged.
async fn substitutes(stream: &ChannelStream) -> Vec<Substitute> {
    let mut lane = stream.substitutes(&sink_name()).unwrap();
    let mut seen = Vec::new();
    while !lane.consumer.is_empty() {
        let delivery = tokio::time::timeout(Duration::from_secs(5), lane.consumer.recv())
            .await
            .expect("a substitute waiting is delivered")
            .unwrap()
            .unwrap();
        let (substitute, handle) = delivery.into_parts();
        handle.ack().await.unwrap();
        seen.push(substitute);
    }
    seen
}

fn ids(substitute: &Substitute) -> Vec<String> {
    substitute
        .batch
        .envelopes
        .iter()
        .map(|envelope| envelope.id.clone())
        .collect()
}

/// Counts the index failures and restarts reported.
#[derive(Debug, Default)]
struct Failures {
    failed: AtomicUsize,
    restarted: AtomicUsize,
}

impl Observer for Failures {
    fn on_index_failed(&self, _: &IndexName, _: crate::Work, _: &str, _: Duration) {
        self.failed.fetch_add(1, Ordering::SeqCst);
    }

    fn on_index_restarted(&self, _: &IndexName, work: crate::Work) {
        if work == crate::Work::Live {
            self.restarted.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// F3, F4: a rebuild is built once, as one build of its ids, and published
/// to the asking sink's substitute lane only, with its keys and a build
/// number; the request is acknowledged. Nothing reaches a lane, and nothing
/// is registered: the run's lanes publish what the feed gave them alone.
/// A row that is gone is built as a delete (F7's ingest half).
#[tokio::test(start_paused = true)]
async fn a_rebuild_is_built_once_and_sent_to_its_sink_only() {
    let db = Database::new(10, Vec::new());
    let builder = Builder::two(&db, Behaviour::default());
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true));
    let requests = deployment.stream.requests().unwrap();
    requests
        .producer
        .publish(rebuild(&[5, 25], 7))
        .await
        .unwrap();
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert!(requests.consumer.is_empty(), "the request is acknowledged");
    let sent = substitutes(&deployment.stream).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    let substitute = &sent[0];
    assert_eq!(substitute.lane, LaneKey::new(orders(), sink_name()));
    assert_eq!(substitute.ticket, Ticket::new(7));
    assert_eq!(ids(substitute), ["5", "25"]);
    assert_eq!(
        substitute
            .batch
            .envelopes
            .iter()
            .map(|envelope| envelope.op)
            .collect::<Vec<_>>(),
        [kernel::Op::Upsert, kernel::Op::Delete],
        "row 25 is gone: a delete"
    );
    assert_eq!(substitute.batch.keys.to_vec(), [key(5), key(25)]);
    assert!(substitute.batch.build.is_some());
    assert_eq!(
        (substitute.batch.key, substitute.batch.position),
        (None, None)
    );
    assert_eq!(
        builder
            .grants()
            .iter()
            .filter(|ids| ids.contains(&5))
            .count(),
        1,
        "built once"
    );
    assert!(
        deployment
            .published
            .lock()
            .unwrap()
            .values()
            .all(Vec::is_empty),
        "nothing reached a lane"
    );
    assert!(
        deployment.recorder.state().is_empty(),
        "the sink wrote nothing: a substitute goes to no lane"
    );
}

/// Live and snapshot items carry their documents' keys, in envelope order,
/// and build numbers that grow in publication order.
#[tokio::test(start_paused = true)]
async fn every_item_carries_its_keys_and_a_growing_build_number() {
    let db = Database::new(6, (0..6).map(Change::upsert).collect());
    let builder = Builder::new(&db, Behaviour::default());
    let mut deployment = Deployment::new(
        Feed::new(&db),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| ingest.with_batch(batch(2)));
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let items = deployment.items.lock().unwrap().clone();
    assert!(items.len() >= 2, "{items:?}");
    let mut last = 0;
    for (keys, ids, build) in &items {
        let rendered: Vec<i64> = ids.iter().map(|id| id.parse().unwrap()).collect();
        assert_eq!(keys, &rendered, "a key per envelope, in envelope order");
        let build = build.expect("a build number");
        assert!(build > last, "{build} after {last}");
        last = build;
    }
}

/// F10, F11: a rebuild whose build fails is its index's failure: the
/// builder restarts alone, and the request, unacknowledged, is served after
/// the restart.
#[tokio::test(start_paused = true)]
async fn a_failing_rebuild_restarts_its_index_and_is_served_after() {
    let db = Database::new(10, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([5])),
            ..Behaviour::default()
        },
    );
    let failures = Arc::new(Failures::default());
    let observer: Arc<dyn Observer> = Arc::clone(&failures) as Arc<dyn Observer>;
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true))
            .ingest(|ingest| ingest.with_observer(observer));
    let requests = deployment.stream.requests().unwrap();
    requests.producer.publish(rebuild(&[5], 3)).await.unwrap();
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0, "the index restarted alone, not the run");
    assert_eq!(failures.failed.load(Ordering::SeqCst), 1);
    assert_eq!(failures.restarted.load(Ordering::SeqCst), 1);
    let sent = substitutes(&deployment.stream).await;
    assert_eq!(sent.len(), 1, "served after the restart: {sent:?}");
    assert_eq!(ids(&sent[0]), ["5"]);
}

/// F14: with the budget held full by the index's own live bytes, a
/// rebuild is still built and sent, and the budget counts none of it.
#[tokio::test(start_paused = true)]
async fn a_rebuild_is_served_with_the_budget_held_full() {
    let db = Database::new(10, Vec::new());
    let builder = Builder::two(&db, Behaviour::default());
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true))
            .with_budget_shared_at(1_000, 0.5);
    let budget = Arc::clone(deployment.budget.as_ref().unwrap());
    let full = budget
        .reserve(crate::Work::Live, &[(orders(), 1_000)])
        .expect("an index holding nothing gets one batch");
    let before = budget.held();
    assert!(
        budget.reserve(crate::Work::Live, &[(orders(), 1)]).is_err(),
        "the budget refuses the index any more live bytes"
    );
    let requests = deployment.stream.requests().unwrap();
    requests.producer.publish(rebuild(&[5], 9)).await.unwrap();
    // Held full until the substitute is out, then let go so the run's own
    // end-of-run check finds the budget empty.
    let lane = deployment.stream.substitutes(&sink_name()).unwrap();
    let watched = Arc::clone(&budget);
    let holder = tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + LIMIT;
        while lane.consumer.is_empty() {
            assert!(tokio::time::Instant::now() < deadline, "no substitute");
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let at_publish = watched.held();
        drop(full);
        at_publish
    });
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the rebuild is served though the budget is full");
    let at_publish = holder.await.unwrap();
    assert_eq!(at_publish, before, "the fresh build was never counted");
    let sent = substitutes(&deployment.stream).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(ids(&sent[0]), ["5"]);
}
