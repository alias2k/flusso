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
        sent.first().map(String::as_str),
        Some("attached 1"),
        "nothing reaches the lane before its boundary: {sent:?}"
    );
    assert!(
        sent.iter().any(|kind| kind == "item"),
        "the change is published after it: {sent:?}"
    );
    assert!(deployment.stream.requests().unwrap().consumer.is_empty());
}

/// Review of step 3, S1: after a run died between the attach and
/// `Attached`, the next run publishes `Attached` before anything else of the
/// lane, whenever the request it left comes back: here only after changes
/// were read and built. So no item precedes the boundary, none is discarded
/// by the sink engine waiting for it, and none is left registered unwritten
/// on the lane.
#[tokio::test(start_paused = true)]
async fn a_run_dying_between_attach_and_attached_publishes_nothing_before_it() {
    let db = Database::new(
        10,
        vec![Change::upsert(1), Change::upsert(2), Change::upsert(3)],
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_secs(1)),
        instant(&db),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)));
    let progress = deployment.ingest.progress();
    progress.detach(&lane());
    assert_eq!(progress.attach(&lane(), 1), Attach::Attached);
    let requests = deployment.stream.requests().unwrap();
    let kinds_log = Arc::clone(&deployment.kinds);
    let late_request = async {
        eventually_items(&kinds_log).await;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        requests.producer.publish(attach(1)).await.unwrap();
    };
    let (ran, ()) = tokio::join!(
        tokio::time::timeout(LIMIT, deployment.run_to_end(0)),
        late_request
    );
    ran.expect("the run ends");
    let sent = kinds(&deployment);
    assert_eq!(
        sent.first().map(String::as_str),
        Some("attached 1"),
        "{sent:?}"
    );
    assert_eq!(
        sent.iter().filter(|kind| *kind == "item").count(),
        3,
        "every change after the boundary: {sent:?}"
    );
    assert!(
        !matches!(
            progress.lane(&lane(), crate::Level::Applied),
            Some(source::cdc::Landed::Before(_))
        ),
        "nothing left registered unwritten on the lane"
    );
    assert!(requests.consumer.is_empty());
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

/// R8's order (review of step 3, m2): `Attached` goes out only once the
/// progress counts the lane. The spy marks one forwarded while the lane is
/// still detached as `early`, which no attach test then reads as the
/// boundary.
#[tokio::test(start_paused = true)]
async fn attached_is_published_only_once_the_progress_counts_the_lane() {
    let db = Database::new(10, vec![Change::upsert(1)]);
    let mut deployment = Deployment::new(Feed::new(&db), instant(&db), Recorder::seeded(true));
    deployment.ingest.progress().detach(&lane());
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
    assert!(sent.contains(&"attached 1".to_owned()), "{sent:?}");
    assert!(!sent.iter().any(|kind| kind.ends_with("early")), "{sent:?}");
}

fn secondary() -> SinkName {
    SinkName::try_new("secondary").unwrap()
}

/// An ingest engine over `users` on two sinks, each lane logged by kind, its
/// builds taking a second each: no sink engine reads the lanes.
fn two_sinks(
    db: &Arc<Database>,
) -> (
    Arc<ChannelStream>,
    Kinds,
    Arc<crate::Progress>,
    IngestEngine,
    Arc<Builder>,
) {
    let sinks = [sink_name(), secondary()];
    let stream = Arc::new(ChannelStream::over(100_000, [users()], sinks.clone()));
    let progress = Arc::new(crate::Progress::of([users()], sinks.clone()));
    let kinds: Kinds = Arc::default();
    let spy = Arc::new(Spy {
        inner: Arc::clone(&stream),
        kinds: Arc::clone(&kinds),
        progress: Some(Arc::clone(&progress)),
        published: Arc::default(),
        failing: Arc::default(),
        items: Arc::default(),
        substituted: Arc::default(),
    });
    let builder = Builder::new(
        db,
        Behaviour {
            delay: Some(Duration::from_secs(1)),
            ..Behaviour::default()
        },
    );
    let ingest = IngestEngine::new(
        Arc::new(Feed::new(db)) as Arc<dyn ChangeCapture>,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        spy as Arc<dyn Stream>,
        sinks.to_vec(),
        Arc::clone(&progress),
    )
    .with_batch(batch(10));
    (stream, kinds, progress, ingest, builder)
}

fn backfill_of(sink: SinkName) -> Request {
    Request::Backfill {
        index: users(),
        sink,
        after: None,
        ticket: Ticket::new(1),
    }
}

/// R7 inside a walk another sink shares (review of step 3, m1): detached
/// mid-walk, a lane gets no further page and no completion; the other sink's
/// walk goes on to its completion, and both requests are acknowledged.
#[tokio::test(start_paused = true)]
async fn a_walk_another_sink_shares_sends_a_detached_lane_no_page_and_no_completion() {
    let db = Database::new(200, Vec::new());
    let (stream, kinds_log, progress, ingest, _builder) = two_sinks(&db);
    let requests = stream.requests().unwrap();
    requests
        .producer
        .publish(backfill_of(sink_name()))
        .await
        .unwrap();
    requests
        .producer
        .publish(backfill_of(secondary()))
        .await
        .unwrap();
    let primary = lane();
    let detach = async {
        for _ in 0..600 {
            if kinds_log
                .lock()
                .unwrap()
                .get(&primary)
                .is_some_and(|sent| !sent.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        progress.detach(&primary);
        kinds_log.lock().unwrap().get(&primary).map_or(0, Vec::len)
    };
    let (ran, sent_before) = tokio::join!(tokio::time::timeout(LIMIT, ingest.run()), detach);
    ran.expect("the run ends").expect("the run succeeds");
    let kinds = kinds_log.lock().unwrap().clone();
    let first = kinds.get(&primary).cloned().unwrap_or_default();
    assert!(sent_before > 0);
    assert_eq!(
        first.len(),
        sent_before,
        "nothing after the detach: {first:?}"
    );
    assert!(!first.contains(&"complete".to_owned()));
    let other = kinds
        .get(&LaneKey::new(users(), secondary()))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        other.last().map(String::as_str),
        Some("complete"),
        "{other:?}"
    );
    assert!(
        other.iter().filter(|kind| *kind == "item").count() > first.len(),
        "the other sink's walk went on"
    );
    assert!(requests.consumer.is_empty(), "both requests acknowledged");
}

/// Counts the snapshots started.
#[derive(Debug, Default)]
struct Snapshots(AtomicUsize);

impl crate::Observer for Snapshots {
    fn on_snapshot_started(&self, _: &[IndexName]) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// R7 (review of step 3, m1): a backfill asked for a detached lane alone is
/// acknowledged with no walk started: no snapshot, no page built.
#[tokio::test(start_paused = true)]
async fn a_backfill_asked_for_a_detached_lane_starts_no_walk() {
    let db = Database::new(200, Vec::new());
    let (stream, kinds_log, progress, ingest, builder) = two_sinks(&db);
    let snapshots = Arc::new(Snapshots::default());
    let ingest = ingest.with_observer(Arc::clone(&snapshots) as Arc<dyn crate::Observer>);
    progress.detach(&lane());
    let requests = stream.requests().unwrap();
    requests
        .producer
        .publish(backfill_of(sink_name()))
        .await
        .unwrap();
    tokio::time::timeout(LIMIT, ingest.run())
        .await
        .expect("the run ends")
        .expect("the run succeeds");
    assert_eq!(snapshots.0.load(Ordering::SeqCst), 0, "no snapshot started");
    assert!(builder.grants.lock().unwrap().is_empty(), "no page built");
    assert!(kinds_log.lock().unwrap().is_empty());
    assert!(requests.consumer.is_empty());
}

/// The quiesce beside another builder, under a budget tight enough to hold
/// batches back (review of step 3, not proven): an attach on `users` waits
/// for its held build while `orders` keeps building and publishing (the
/// builds' permits are several: one is the engine's whole parallelism),
/// nothing deadlocks, and `users` resumes after the attach.
#[tokio::test(start_paused = true)]
async fn an_attach_beside_another_builder_under_a_budget_pauses_only_its_index() {
    // Even ids are `users`, odd ones `orders`.
    let mut log = vec![Change::upsert(2), Change::upsert(3)];
    log.extend((0..20).flat_map(|n| [Change::upsert(5 + 2 * n), Change::upsert(4 + 2 * n)]));
    let db = Database::new(60, log);
    let behaviour = Behaviour {
        padding: 20 * KIB,
        ..Behaviour::default()
    };
    behaviour.held.lock().unwrap().extend([2, 3]);
    let builder = Builder::two(&db, behaviour);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(200)),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(4))
    .with_budget(64 * KIB, usize::MAX);
    let progress = deployment.ingest.progress();
    progress.detach(&lane());
    let orders = LaneKey::new(IndexName::try_new("orders").unwrap(), sink_name());
    let requests = deployment.stream.requests().unwrap();
    let kinds_log = Arc::clone(&deployment.kinds);
    let watch = async {
        for _ in 0..600 {
            if builder.grants.lock().unwrap().len() >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        requests.producer.publish(attach(1)).await.unwrap();
        builder.release(3);
        tokio::time::sleep(Duration::from_secs(5)).await;
        let orders_sent = kinds_log.lock().unwrap().get(&orders).map_or(0, Vec::len);
        assert!(
            orders_sent > 1,
            "orders builds and publishes beside the attach: {orders_sent}"
        );
        assert!(
            progress.is_detached(&lane()),
            "users' build still holds the attach"
        );
        builder.release(2);
    };
    let (ran, ()) = tokio::join!(tokio::time::timeout(LIMIT, deployment.run_to_end(0)), watch);
    ran.expect("the run ends: no deadlock");
    let sent = kinds(&deployment);
    assert_eq!(
        sent.first().map(String::as_str),
        Some("attached 1"),
        "{sent:?}"
    );
    assert!(
        sent.iter().filter(|kind| *kind == "item").count() >= 1,
        "users resumed: {sent:?}"
    );
    let state = deployment.recorder.state();
    assert_eq!(state.get(&2), None, "built while detached");
    let expected = db.expected();
    for change in db.log.iter().filter(|change| change.id % 2 == 1) {
        assert_eq!(
            state.get(&change.id),
            expected.get(&change.id),
            "orders {}",
            change.id
        );
    }
    assert!(!progress.is_detached(&lane()));
}

/// O27: a rebuild's backfill asked while another sink's walk of the index
/// runs waits for that walk (the inbox is read only with no walk running),
/// then gets its own: its first page comes after the other walk completed.
#[tokio::test(start_paused = true)]
async fn a_rebuilds_backfill_waits_behind_another_sinks_walk_of_the_index() {
    let db = Database::new(200, Vec::new());
    let (stream, kinds_log, _progress, ingest, _builder) = two_sinks(&db);
    let requests = stream.requests().unwrap();
    requests
        .producer
        .publish(backfill_of(secondary()))
        .await
        .unwrap();
    let other = LaneKey::new(users(), secondary());
    let late = async {
        for _ in 0..600 {
            if kinds_log
                .lock()
                .unwrap()
                .get(&other)
                .is_some_and(|sent| !sent.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        requests
            .producer
            .publish(backfill_of(sink_name()))
            .await
            .unwrap();
        for _ in 0..6000 {
            if kinds_log
                .lock()
                .unwrap()
                .get(&lane())
                .is_some_and(|sent| !sent.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let other_sent = kinds_log
            .lock()
            .unwrap()
            .get(&other)
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            other_sent.last().map(String::as_str),
            Some("complete"),
            "the first page of the waiting backfill comes after the other walk's end"
        );
    };
    let (ran, ()) = tokio::join!(tokio::time::timeout(LIMIT, ingest.run()), late);
    ran.expect("the run ends").expect("the run succeeds");
    let first = kinds_log
        .lock()
        .unwrap()
        .get(&lane())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        first.last().map(String::as_str),
        Some("complete"),
        "{first:?}"
    );
}
