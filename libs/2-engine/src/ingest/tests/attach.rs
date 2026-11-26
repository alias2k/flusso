//! The attach on the ingest side (ADR 0012 stage 10, R7, R8): the index's
//! builder quiesces, attaches the lane in the progress, then publishes
//! `Attached` on it; nothing is published to a detached lane; a walk that
//! only detached lanes asked for is cut.

use super::*;
use crate::progress::Attach;
use stream::{LaneKey, Request, Ticket};

const LIMIT: Duration = Duration::from_secs(300);

fn lane() -> LaneKey {
    LaneKey::new(users(), sink_name())
}

fn attach(attempt: u64) -> Request {
    Request::Attach {
        index: users(),
        sink: sink_name(),
        attempt,
    }
}

fn backfill(ticket: u64) -> Request {
    Request::Backfill {
        index: users(),
        sink: sink_name(),
        after: None,
        ticket: Ticket::new(ticket),
    }
}

/// What `lane` was sent, in lane order, by kind.
fn kinds(deployment: &Deployment) -> Vec<String> {
    deployment
        .kinds
        .lock()
        .unwrap()
        .get(&lane())
        .cloned()
        .unwrap_or_default()
}

/// The attach waits for the build of the index that runs when it is asked
/// (§1's quiesce): nothing of the index is granted meanwhile; once that
/// build is published (to every lane but the detached one, R7), the lane is
/// attached, `Attached` is its first item, and the changes that waited are
/// built and published after it.
#[tokio::test(start_paused = true)]
async fn an_attach_quiesces_the_builder_and_attaches_with_nothing_of_the_index_in_flight() {
    let db = Database::new(
        10,
        vec![Change::upsert(1), Change::upsert(2), Change::upsert(3)],
    );
    let behaviour = Behaviour::default();
    behaviour.held.lock().unwrap().insert(1);
    let builder = Builder::new(&db, behaviour);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_secs(1)),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)));
    let progress = deployment.ingest.progress();
    progress.detach(&lane());
    let requests = deployment.stream.requests().unwrap();
    let kinds_log = Arc::clone(&deployment.kinds);
    let watch = async {
        for _ in 0..600 {
            if !builder.grants.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // The build of change 1 runs, held: the attach is asked now.
        requests.producer.publish(attach(1)).await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert!(
            progress.is_detached(&lane()),
            "the build of change 1 still runs: no attach"
        );
        assert!(
            kinds_log
                .lock()
                .unwrap()
                .get(&lane())
                .is_none_or(Vec::is_empty),
            "nothing reaches the detached lane"
        );
        assert_eq!(
            builder.grants.lock().unwrap().len(),
            1,
            "nothing else of the index is granted while the attach waits"
        );
        builder.release(1);
    };
    let (ran, ()) = tokio::join!(tokio::time::timeout(LIMIT, deployment.run_to_end(0)), watch);
    ran.expect("the run ends");
    let sent = kinds(&deployment);
    assert_eq!(
        sent.first().map(String::as_str),
        Some("attached 1"),
        "{sent:?}"
    );
    assert!(sent.iter().skip(1).all(|kind| kind == "item"), "{sent:?}");
    let state = deployment.recorder.state();
    assert_eq!(state.get(&2), Some(&2), "change 2, built after the attach");
    assert_eq!(state.get(&3), Some(&3));
    assert_eq!(state.get(&1), None, "change 1 was built while detached");
    assert!(!progress.is_detached(&lane()));
    assert!(requests.consumer.is_empty());
}

/// A run that dies between the attach in the progress and `Attached` on the
/// lane leaves the request unacknowledged: the next run's builder finds the
/// lane attached for that attempt and publishes `Attached` again (H2).
#[tokio::test(start_paused = true)]
async fn a_run_dying_between_attach_and_attached_republishes_it() {
    let db = Database::new(10, vec![Change::upsert(1)]);
    let mut deployment = Deployment::new(Feed::new(&db), instant(&db), Recorder::seeded(true));
    let progress = deployment.ingest.progress();
    progress.detach(&lane());
    // What the dying run did: the attach, and nothing after it.
    assert_eq!(progress.attach(&lane(), 1), Attach::Attached);
    deployment
        .stream
        .requests()
        .unwrap()
        .producer
        .publish(attach(1))
        .await
        .unwrap();
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let sent = kinds(&deployment);
    assert_eq!(
        sent.iter().filter(|kind| *kind == "attached 1").count(),
        1,
        "{sent:?}"
    );
    assert!(deployment.stream.requests().unwrap().consumer.is_empty());
}

/// A request for an attempt the lane is past publishes nothing and is
/// acknowledged: one for the attempt it was attached for and then detached
/// again after, and an older one.
#[tokio::test(start_paused = true)]
async fn a_stale_attach_publishes_nothing() {
    let db = Database::new(10, vec![Change::upsert(1)]);
    let mut deployment = Deployment::new(Feed::new(&db), instant(&db), Recorder::seeded(true));
    let progress = deployment.ingest.progress();
    progress.detach(&lane());
    assert_eq!(progress.attach(&lane(), 2), Attach::Attached);
    progress.detach(&lane());
    let requests = deployment.stream.requests().unwrap();
    requests.producer.publish(attach(2)).await.unwrap();
    requests.producer.publish(attach(1)).await.unwrap();
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert!(kinds(&deployment).is_empty(), "{:?}", kinds(&deployment));
    assert!(progress.is_detached(&lane()));
    assert!(requests.consumer.is_empty());
}

/// R7: a detached lane gets no live item and no snapshot: a `Backfill`
/// asked for it is acknowledged unserved, and with every lane detached the
/// source is free.
#[tokio::test(start_paused = true)]
async fn nothing_is_published_to_a_detached_lane() {
    let db = Database::new(10, vec![Change::upsert(1), Change::upsert(2)]);
    let mut deployment = Deployment::new(Feed::new(&db), instant(&db), Recorder::seeded(true));
    let progress = deployment.ingest.progress();
    progress.detach(&lane());
    let requests = deployment.stream.requests().unwrap();
    requests.producer.publish(backfill(1)).await.unwrap();
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert!(kinds(&deployment).is_empty(), "{:?}", kinds(&deployment));
    assert!(requests.consumer.is_empty(), "the backfill is acknowledged");
    assert_eq!(progress.landed(), source::cdc::Landed::Free);
}

/// R7: a walk whose every requesting lane is detached is cut, its request
/// acknowledged, and its pages stop: no `SnapshotComplete` reaches the lane.
#[tokio::test(start_paused = true)]
async fn a_walk_only_detached_lanes_asked_for_is_cut() {
    let db = Database::new(1_000, Vec::new());
    let builder = Builder::new(
        &db,
        Behaviour {
            delay: Some(Duration::from_secs(1)),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(10)));
    let progress = deployment.ingest.progress();
    let requests = deployment.stream.requests().unwrap();
    requests.producer.publish(backfill(1)).await.unwrap();
    let kinds_log = Arc::clone(&deployment.kinds);
    let stop = Stop::new();
    deployment = deployment.ingest(|ingest| ingest.with_stop(stop.clone()));
    let watch = async {
        eventually_items(&kinds_log).await;
        progress.detach(&lane());
        tokio::time::sleep(Duration::from_secs(5)).await;
        let pages = kinds_log.lock().unwrap().get(&lane()).map_or(0, Vec::len);
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert_eq!(
            kinds_log.lock().unwrap().get(&lane()).map_or(0, Vec::len),
            pages,
            "the walk stopped"
        );
        stop.request();
    };
    let (ran, ()) = tokio::join!(tokio::time::timeout(LIMIT, deployment.run_to_end(0)), watch);
    ran.expect("the run ends");
    assert!(!kinds(&deployment).contains(&"complete".to_owned()));
    assert!(requests.consumer.is_empty(), "the request is acknowledged");
}

/// Wait until some item reached the lane.
async fn eventually_items(kinds: &Kinds) {
    for _ in 0..600 {
        if kinds
            .lock()
            .unwrap()
            .get(&lane())
            .is_some_and(|sent| !sent.is_empty())
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no item reached the lane");
}

/// m6 of the step 2 review: while every lane is detached the source is free
/// and moves as far as its horizon; once a lane is attached again, a landing
/// on it is an advance, counted (the supervisor reads it as progress).
#[tokio::test(start_paused = true)]
async fn an_advance_after_every_lane_was_free_is_counted() {
    let db = Database::new(10, Vec::new());
    let deployment = Deployment::new(Feed::new(&db), instant(&db), Recorder::seeded(true));
    let progress = deployment.ingest.progress();
    let mut last = None;
    progress.detach(&lane());
    deployment.ingest.confirm(&mut last, true);
    assert_eq!(progress.attach(&lane(), 1), Attach::Attached);
    deployment.ingest.confirm(&mut last, true);
    let before = deployment.ingest.advances();
    let key = progress.next_key(&users()).unwrap();
    progress.published(&lane(), key, Position::new(5), Position::new(5));
    progress.applied(&lane(), key);
    deployment.ingest.confirm(&mut last, true);
    assert_eq!(
        deployment.ingest.advances(),
        before + 1,
        "the landing after the attach is progress"
    );
}
