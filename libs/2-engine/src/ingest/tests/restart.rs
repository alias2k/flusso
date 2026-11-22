//! Slice 8 (ADR 0012 stage 8a, §3.10): with two or more indexes, a failure of
//! one index's own work (a build, a document under `on_error = "stop"`, its
//! walk's key read, a build that panics) restarts that index's builder alone,
//! after its own backoff, while the reader and the other index go on. Its
//! unpublished builds are given back, never forgotten: the run does not
//! restart, so nothing is read again. `users` builds the even ids, `orders`
//! the odd. Every wait is bounded.

use super::*;
use crate::observer::Observer;
use crate::policy::{Backoff, MaxWaiting};
use source::cdc::Landed;
use stream::LaneKey;

const LIMIT: Duration = Duration::from_secs(300);

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

fn lane(index: IndexName) -> LaneKey {
    LaneKey::new(index, sink_name())
}

/// What the deployment looked like when an index restarted.
#[derive(Debug, Clone)]
struct AtRestart {
    index: String,
    at: tokio::time::Instant,
    /// Every document the sink held.
    state: BTreeMap<i64, u64>,
    /// What every lane together, `users`' and `orders`' answered.
    landed: Landed,
    users: Option<Landed>,
    orders: Option<Landed>,
}

/// Records each failure (index, error, wait, when) and each restart, with
/// what the sink held and the lanes answered at that moment.
#[derive(Debug, Default)]
struct Restarts {
    failed: Mutex<Vec<(String, String, Duration, tokio::time::Instant)>>,
    restarted: Mutex<Vec<AtRestart>>,
    look: Mutex<Option<(Arc<Recorder>, Arc<crate::Progress>)>>,
}

impl Restarts {
    fn watching(deployment: &Deployment) -> Arc<Self> {
        let restarts = Arc::new(Self::default());
        *restarts.look.lock().unwrap() = Some((
            Arc::clone(&deployment.recorder),
            deployment.ingest.progress(),
        ));
        restarts
    }

    fn failures(&self) -> Vec<(String, String, Duration, tokio::time::Instant)> {
        self.failed.lock().unwrap().clone()
    }

    fn restarts(&self) -> Vec<AtRestart> {
        self.restarted.lock().unwrap().clone()
    }

    fn waits(&self) -> Vec<Duration> {
        self.failures()
            .iter()
            .map(|(_, _, wait, _)| *wait)
            .collect()
    }
}

impl Observer for Restarts {
    fn on_index_failed(&self, index: &IndexName, error: &str, retry_in: Duration) {
        self.failed.lock().unwrap().push((
            index.to_string(),
            error.to_owned(),
            retry_in,
            tokio::time::Instant::now(),
        ));
    }

    fn on_index_restarted(&self, index: &IndexName) {
        let look = self.look.lock().unwrap();
        let Some((recorder, progress)) = look.as_ref() else {
            return;
        };
        self.restarted.lock().unwrap().push(AtRestart {
            index: index.to_string(),
            at: tokio::time::Instant::now(),
            state: recorder.state(),
            landed: progress.landed(),
            users: progress.lane(&lane(users()), crate::Level::Applied),
            orders: progress.lane(&lane(orders()), crate::Level::Applied),
        });
    }
}

/// Two indexes over `db`, batches of one, `builds` at once, the observer
/// watching restarts.
fn two_indexes(
    db: &Arc<Database>,
    behaviour: Behaviour,
    recorder: Arc<Recorder>,
    builds: usize,
) -> (Deployment, Arc<Builder>, Arc<Restarts>) {
    let builder = Builder::two(db, behaviour);
    let deployment = Deployment::new(Feed::new(db), Arc::clone(&builder), recorder)
        .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(builds));
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    let deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    (deployment, builder, restarts)
}

fn even(state: &BTreeMap<i64, u64>) -> BTreeMap<i64, u64> {
    state
        .iter()
        .filter(|(id, _)| *id % 2 == 0)
        .map(|(id, version)| (*id, *version))
        .collect()
}

/// R1: `orders`' build of id 1 fails once. The run goes on (no run
/// failed); `orders` restarts once, after a second, and `users` landed
/// every change meanwhile.
#[tokio::test(start_paused = true)]
async fn a_build_failure_on_one_index_restarts_that_index_alone() {
    let db = Database::new(0, (0..20).map(Change::upsert).collect());
    let (mut deployment, _, restarts) = two_indexes(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([1])),
            delay: Some(Duration::from_millis(10)),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        2,
    );
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0, "no run failed");
    let failures = restarts.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].0, "orders");
    assert_eq!(failures[0].2, Duration::from_secs(1), "the first wait");
    let restarted = restarts.restarts();
    assert_eq!(restarted.len(), 1);
    assert_eq!(restarted[0].index, "orders");
    assert_eq!(
        even(&restarted[0].state),
        even(&db.expected()),
        "users landed every change while orders was down"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R2: a document of `orders` fails under `on_error = "stop"`: the same.
#[tokio::test(start_paused = true)]
async fn a_stop_failure_on_one_index_lets_the_others_land() {
    let db = Database::new(0, (0..20).map(Change::upsert).collect());
    let (deployment, _, restarts) = two_indexes(
        &db,
        Behaviour {
            document_fails_once: Mutex::new(HashSet::from([1])),
            delay: Some(Duration::from_millis(10)),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        2,
    );
    let mut deployment = deployment
        .ingest(|ingest| ingest.with_failure_policies(FailurePolicies::new(FailurePolicy::Stop)));
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0, "no run failed");
    let failures = restarts.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].1.contains("could not be built"), "{failures:?}");
    let restarted = restarts.restarts();
    assert_eq!(
        even(&restarted[0].state),
        even(&db.expected()),
        "users landed every change while orders was down"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R3: `orders`' walk fails reading its first key page, with live changes
/// on both indexes. `orders` restarts alone; its request comes back through
/// the request lane and is walked again, so the sink holds every row.
#[tokio::test(start_paused = true)]
async fn a_restarted_index_task_gives_back_its_builds_and_serves_its_request_again() {
    let db = Database::new(20, (20..40).map(Change::upsert).collect());
    let (mut deployment, _, restarts) = two_indexes(
        &db,
        Behaviour {
            keys_fail_once: Mutex::new(HashSet::from(["orders".to_owned()])),
            delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
        Recorder::seeded(false),
        2,
    );
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0, "no run failed");
    let failures = restarts.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].1.contains("keys failed"), "{failures:?}");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R4: `orders`' build of 1 is held, then fails, while 3, 5 and 7 built
/// behind it and wait for their turn. All four are given back and built
/// again after the restart; none is lost.
#[tokio::test(start_paused = true)]
async fn a_failed_index_rebuilds_every_document_it_had_taken() {
    let db = Database::new(
        0,
        [1, 3, 5, 7, 0, 2, 4]
            .into_iter()
            .map(Change::upsert)
            .collect(),
    );
    let (mut deployment, builder, restarts) = two_indexes(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([1])),
            fail_once: Mutex::new(HashSet::from([1])),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        4,
    );
    let releaser = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            while !builder.grants().iter().any(|ids| ids.contains(&7)) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            builder.release(1);
        })
    };
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    releaser.await.unwrap();
    assert_eq!(failed, 0);
    assert_eq!(restarts.restarts().len(), 1);
    let grants = builder.grants();
    let again: Vec<i64> = grants
        .iter()
        .skip_while(|ids| !ids.contains(&7))
        .skip(1)
        .flatten()
        .copied()
        .filter(|id| id % 2 == 1)
        .collect();
    for id in [1, 3, 5, 7] {
        assert!(
            again.contains(&id),
            "{id} is built again after the restart: {grants:?}"
        );
    }
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R5: while `orders` is down, the slot and `orders`' lane stay before its
/// oldest unbuilt change (change 1), and `users`' lane answers through what
/// it landed.
#[tokio::test(start_paused = true)]
async fn confirmation_stays_before_a_down_index_and_its_sibling_lands() {
    let db = Database::new(0, (0..20).map(Change::upsert).collect());
    let (mut deployment, _, restarts) = two_indexes(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([1])),
            delay: Some(Duration::from_millis(10)),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        2,
    );
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let restarted = restarts.restarts();
    let down = &restarted[0];
    assert_eq!(
        down.orders,
        Some(Landed::Before(Position::new(1))),
        "orders' lane holds before its oldest unbuilt change"
    );
    assert_eq!(
        down.landed,
        Landed::Before(Position::new(1)),
        "and the slot"
    );
    assert!(
        matches!(down.users, Some(Landed::Through(position)) if position.seq >= 18),
        "users' lane answers through what it landed: {:?}",
        down.users
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R7: `orders` fails three times (down for 1 + 2 + 4 s) while both
/// indexes keep changing, a change every 10 ms. Down, it is exempt: the
/// reader reads past its cap (4) and `users` keeps landing.
#[tokio::test(start_paused = true)]
async fn the_reader_reads_on_past_a_down_index_at_its_cap() {
    let db = Database::new(0, (0..200).map(Change::upsert).collect());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(1, 3)])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(10)),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
    });
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    let progress = deployment.ingest.progress();
    let recorder = Arc::clone(&deployment.recorder);
    let watcher = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let orders_waiting = progress
            .waiting_by_index()
            .iter()
            .find(|(index, _)| index.as_ref() == "orders")
            .map_or(0, |(_, count)| *count);
        let users_landed = recorder.state().keys().filter(|id| *id % 2 == 0).count();
        (progress.changes_read(), orders_waiting, users_landed)
    });
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let (read, orders_waiting, users_landed) = watcher.await.unwrap();
    assert_eq!(restarts.failures().len(), 3, "{:?}", restarts.failures());
    assert_eq!(read, 200, "every change was read while orders was down");
    assert!(
        orders_waiting > 4,
        "orders grew past its cap, exempt ({orders_waiting} waiting)"
    );
    assert_eq!(users_landed, 100, "users landed every change meanwhile");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R8: `orders` fails three times on change 1, then lands; later its
/// change 41 fails once. The waits double (1, 2, 4 s) and start over (1 s)
/// once it published.
#[tokio::test(start_paused = true)]
async fn a_failing_index_waits_longer_each_time_and_starts_over_after_it_published() {
    let log: Vec<Change> = [1, 0, 2, 3, 41].into_iter().map(Change::upsert).collect();
    let arrivals = vec![
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::from_secs(30),
    ];
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(1, 3)])),
            fail_once: Mutex::new(HashSet::from([41])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(2));
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(
        restarts.waits(),
        [1, 2, 4, 1].map(Duration::from_secs),
        "{:?}",
        restarts.failures()
    );
    let failures = restarts.failures();
    for (failure, restart) in failures.iter().zip(restarts.restarts()) {
        assert_eq!(
            restart.at - failure.3,
            failure.2,
            "restarted after its wait"
        );
    }
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// The deployment over an open stream, `orders` failing on change 1
/// `times` times, a 30 s first wait, and a stop requested 2 s after the
/// first failure. Returns the run's result, how long after the failure it
/// ended, and the failures seen.
async fn stop_while_down(
    times: usize,
) -> (
    crate::Result<()>,
    Duration,
    Arc<Restarts>,
    Arc<Recorder>,
    Arc<Database>,
) {
    let db = Database::new(0, (0..10).map(Change::upsert).collect());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(1, times)])),
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_stop(stop.clone())
            .with_backoff(Backoff::new(Duration::from_secs(30), Duration::from_secs(60)).unwrap())
    });
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    let sink = deployment.start_sink().await;
    let stopper = {
        let restarts = Arc::clone(&restarts);
        tokio::spawn(async move {
            while restarts.failures().is_empty() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let failed = tokio::time::Instant::now();
            tokio::time::sleep(Duration::from_secs(2)).await;
            stop.request();
            failed
        })
    };
    let result = tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .expect("the run ends");
    let ended = tokio::time::Instant::now();
    let failed = stopper.await.unwrap();
    deployment.drained().await;
    sink.abort();
    (
        result,
        ended - failed,
        restarts,
        Arc::clone(&deployment.recorder),
        db,
    )
}

/// R9: a stop while `orders` waits out its 30 s backoff cuts the wait: it
/// restarts at once, builds its backlog under the stop and the run ends
/// `Ok` with every document landed.
#[tokio::test(start_paused = true)]
async fn a_stop_while_an_index_is_down_builds_its_backlog() {
    let (result, took, restarts, recorder, db) = stop_while_down(1).await;
    result.expect("the stop ends the run cleanly");
    assert!(
        took < Duration::from_secs(10),
        "the stop didn't wait out the backoff ({took:?})"
    );
    assert_eq!(restarts.restarts().len(), 1);
    assert_eq!(recorder.state(), db.expected());
}

/// R10: the try at the stop fails too: the run ends on that error (a stop
/// never restarts anything), and the daemon's stop exits 1.
#[tokio::test(start_paused = true)]
async fn a_failure_while_stopping_ends_the_run() {
    let (result, took, restarts, _, _) = stop_while_down(2).await;
    let error = result.expect_err("a failure at the stop ends the run");
    assert!(error.to_string().contains("the build failed"), "{error}");
    assert!(took < Duration::from_secs(10), "{took:?}");
    assert_eq!(restarts.failures().len(), 1, "the second is the run's");
}

/// R12: `orders`' build of 1 panics on its first poll, which the builder
/// runs inline: a failed build of `orders`, restarted alone.
#[tokio::test(start_paused = true)]
async fn a_build_panicking_at_once_restarts_its_index_alone() {
    let db = Database::new(0, (0..20).map(Change::upsert).collect());
    let (mut deployment, _, restarts) = two_indexes(
        &db,
        Behaviour {
            panic_once: Mutex::new(HashSet::from([1])),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        2,
    );
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0, "no run failed");
    let failures = restarts.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].1.contains("panicked"), "{failures:?}");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R11: one index has no other index to keep going: a failure ends the run,
/// as before slice 8, and nothing restarts alone.
#[tokio::test(start_paused = true)]
async fn one_index_ends_the_run_as_before() {
    let db = Database::new(0, (0..10).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([3])),
            ..Behaviour::default()
        },
    );
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true))
            .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(2));
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(1))
        .await
        .expect("the run ends");
    assert_eq!(failed, 1, "the run failed and the replay rebuilt it");
    assert!(restarts.failures().is_empty());
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R15: on a stream that stays open, nothing but the restart asks for the
/// backlog (the reader hands a folding builder nothing, and no build of it
/// runs): the restarted builder builds it with no further change.
#[tokio::test(start_paused = true)]
async fn a_restarted_index_builds_its_backlog_with_nothing_else_happening() {
    let db = Database::new(0, (0..10).map(Change::upsert).collect());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([1])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(2));
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    let sink = deployment.start_sink().await;
    let recorder = Arc::clone(&deployment.recorder);
    let ingest = deployment.ingest;
    let run = tokio::spawn(async move { ingest.run().await });
    let started = tokio::time::Instant::now();
    // `expected` reads the changes the stream has yielded so far.
    while restarts.restarts().is_empty()
        || db.visible.load(Ordering::SeqCst) < db.log.len()
        || recorder.state() != db.expected()
    {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "orders' backlog never built after its restart: {:?} (restarts {})",
            recorder.state(),
            restarts.restarts().len()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    run.abort();
    sink.abort();
}

/// R3 on a stream that stays open: no idle report comes to pick a request
/// up from the request lane, so the restarted builder must carry its walk's
/// requests itself. `orders`' walk fails once; every row lands with no
/// further change.
#[tokio::test(start_paused = true)]
async fn a_restarted_walk_is_served_again_on_an_open_stream() {
    let db = Database::new(20, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            keys_fail_once: Mutex::new(HashSet::from(["orders".to_owned()])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(2));
    let sink = deployment.start_sink().await;
    let recorder = Arc::clone(&deployment.recorder);
    let ingest = deployment.ingest;
    let run = tokio::spawn(async move { ingest.run().await });
    let started = tokio::time::Instant::now();
    while recorder.state() != db.expected() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "orders' walk was never served again: {:?}",
            recorder.state()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    run.abort();
    sink.abort();
}

/// R13: a lane's positions never go down across a restart. Every change
/// to row 1 touches three `orders` documents, and one take holds at most
/// two (`max_changes` 2, O2). Changes 1@0 and 1@1 form one batch through
/// position 1; its take builds two documents of change 0 and leaves the
/// third waiting at 0. That one's build fails, and it is given back at 0.
/// The restart's backlog batch carries position 1, the newest its lane was
/// sent, never 0.
#[tokio::test(start_paused = true)]
async fn a_lanes_positions_never_go_down_across_a_restart() {
    let db = Database::fanned(0, [1, 1, 0].into_iter().map(Change::upsert).collect(), 3);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([1 + 2 * ALIAS])),
            ..Behaviour::default()
        },
    );
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true))
            .ingest(|ingest| ingest.with_batch(batch(2)).with_builds(1));
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0);
    assert_eq!(restarts.restarts().len(), 1, "{:?}", restarts.failures());
    let published = deployment.published.lock().unwrap().clone();
    let positions = published.get(&lane(orders())).cloned().unwrap_or_default();
    assert!(
        positions.windows(2).all(|pair| pair[0] <= pair[1]),
        "orders' positions went down across its restart: {positions:?}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}
