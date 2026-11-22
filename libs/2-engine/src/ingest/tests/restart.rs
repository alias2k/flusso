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
    /// Each failure's kind, in order (slice 8-2).
    kinds: Mutex<Vec<crate::Work>>,
    /// How many walks started again after a failure.
    retried: Mutex<usize>,
    /// The builder's restarts (kind `live`).
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

    fn kinds(&self) -> Vec<crate::Work> {
        self.kinds.lock().unwrap().clone()
    }

    fn retried(&self) -> usize {
        *self.retried.lock().unwrap()
    }

    fn waits(&self) -> Vec<Duration> {
        self.failures()
            .iter()
            .map(|(_, _, wait, _)| *wait)
            .collect()
    }
}

impl Observer for Restarts {
    fn on_index_failed(
        &self,
        index: &IndexName,
        kind: crate::Work,
        error: &str,
        retry_in: Duration,
    ) {
        self.kinds.lock().unwrap().push(kind);
        self.failed.lock().unwrap().push((
            index.to_string(),
            error.to_owned(),
            retry_in,
            tokio::time::Instant::now(),
        ));
    }

    fn on_index_restarted(&self, index: &IndexName, kind: crate::Work) {
        if kind == crate::Work::Backfill {
            *self.retried.lock().unwrap() += 1;
            return;
        }
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
    assert_eq!(
        restarts.kinds(),
        [crate::Work::Live],
        "a live failure, reported as one (slice 8-2)"
    );
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
    assert_eq!(
        restarts.kinds(),
        [crate::Work::Backfill],
        "a walk's failure, reported as one (slice 8-2)"
    );
    assert_eq!(restarts.retried(), 1, "its walk started again once");
    assert!(
        restarts.restarts().is_empty(),
        "the builder never restarted"
    );
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

// Adopted from the review of #456 (fix round 1).

/// One index stuck (`orders`: every document refused, its lane of two
/// parks: held) beside `users`, quiet, whose walk fails once at the start
/// when `walk_fails`. Returns the changes read at 9 s.
async fn stuck_beside_a_quiet_index(walk_fails: bool) -> u64 {
    let changes: i64 = 300;
    let log: Vec<Change> = (0..changes)
        .map(|n| Change::upsert(2 * (n % 50) + 1))
        .collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(20 * n))
        .collect();
    let db = Database::new(1, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            keys_fail_once: Mutex::new(if walk_fails {
                HashSet::from(["users".to_owned()])
            } else {
                HashSet::new()
            }),
            padding: 8 * KIB,
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(false);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let restarts = Arc::new(IndexRestarts::default());
    let restarts_observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        64,
    )
    .with_budget(128 * KIB, usize::MAX)
    .ingest(|ingest| {
        ingest
            .with_observer(restarts_observer)
            .with_batch(batch(1))
            .with_builds(2)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
    });
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(9)).await;
            let read = progress.changes_read();
            recorder.refused_indexes.lock().unwrap().clear();
            read
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let read = watcher.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());
    read
}

/// A quiet index whose walk failed once and restarted keeps its lead hold
/// (`down` sets it; only a live grant clears it, and a quiet index makes
/// none): it is never free again, so a stuck neighbour freezes at its cap
/// instead of reading on, exempt, as it does beside the same quiet index
/// that never failed.
#[tokio::test(start_paused = true)]
async fn a_restarted_quiet_index_is_free_again() {
    let control = stuck_beside_a_quiet_index(false).await;
    let failing = stuck_beside_a_quiet_index(true).await;
    assert_eq!(
        failing, control,
        "beside a quiet index the stuck one reads on ({control} read); after the quiet one \
         restarted once, only {failing}"
    );
}

/// The twin of `a_hold_across_an_engine_restart_is_one_wait`, as that test
/// was before slice 8 (review of #456, item 3):
/// `orders` 9's build fails once, which now restarts `orders` alone while
/// `users` is held by the budget. One hold, so one refusal counted and one
/// wait reported; the failed builder's dropped `Builds` must not end
/// `users`' wait.
#[tokio::test(start_paused = true)]
async fn a_hold_across_an_index_restart_is_one_wait() {
    let mut log: Vec<Change> = (0..10).map(|n| Change::upsert(2 * n)).collect();
    log.push(Change::upsert(0));
    log.extend([1, 3, 5, 7, 9].map(Change::upsert));
    let db = Database::new(0, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            fail_once: Mutex::new(HashSet::from([9])),
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let waits = Arc::new(BudgetWaits::default());
    let observer = Arc::clone(&waits);
    let restarts = Arc::new(IndexRestarts::default());
    let restarts_observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(100)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(16))
            .with_builds(4)
            .with_observer(Arc::new(crate::FanOut::new(vec![
                observer as Arc<dyn Observer>,
                restarts_observer,
            ])))
    })
    .with_budget(128 * KIB, usize::MAX);
    let budget = deployment.budget.clone().unwrap();
    let resumer = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3_000)).await;
            recorder.resume();
        })
    };
    assert_eq!(deployment.run_to_end(0).await, 0, "no run failed");
    resumer.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());
    assert_eq!(
        restarts.live.load(Ordering::SeqCst),
        1,
        "orders restarted alone"
    );
    let live: Vec<(crate::Wake, Duration)> = waits
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(work, _, _)| *work == crate::Work::Live)
        .map(|(_, wake, waited)| (*wake, *waited))
        .collect();
    assert_eq!(
        budget.refused(crate::Work::Live),
        1,
        "one hold, counted once: {live:?}"
    );
    assert_eq!(live.len(), 1, "one wait: {live:?}");
}

/// While `orders` is down with change 1 given back, the sink's committed
/// count stays before change 1 (read 2nd), `orders`' lane at both levels
/// stays before position 1, and the slot too; at the end every change
/// counts once.
#[tokio::test(start_paused = true)]
async fn nothing_counts_or_fences_past_a_given_back_change() {
    let db = Database::new(0, (0..40).map(Change::upsert).collect());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(1, 2)])),
            delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
    );
    let restarts = Arc::new(IndexRestarts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true)).ingest(
            |ingest| {
                ingest
                    .with_batch(batch(1))
                    .with_builds(2)
                    .with_observer(observer)
            },
        );
    let progress = deployment.ingest.progress();
    let watcher = {
        let progress = Arc::clone(&progress);
        let builder = Arc::clone(&builder);
        let restarts = Arc::clone(&restarts);
        tokio::spawn(async move {
            let mut samples = 0;
            let mut bad = Vec::new();
            for _ in 0..4_000 {
                tokio::time::sleep(Duration::from_millis(1)).await;
                let built_1 = builder.behaviour.fail_times.lock().unwrap().get(&1) == Some(&0)
                    && restarts.live.load(Ordering::SeqCst) >= 2;
                if built_1 {
                    break;
                }
                samples += 1;
                let committed = progress.committed(&sink_name());
                let orders = progress.lane(
                    &stream::LaneKey::new(IndexName::try_new("orders").unwrap(), sink_name()),
                    crate::Level::Searchable,
                );
                let landed = progress.landed();
                let past = |answer: &source::cdc::Landed| {
                    !matches!(answer, source::cdc::Landed::Before(p) if p.seq <= 1)
                        && !matches!(answer, source::cdc::Landed::Idle)
                };
                if committed > 1 || orders.as_ref().is_some_and(past) || past(&landed) {
                    bad.push((committed, orders, landed));
                }
            }
            (samples, bad)
        })
    };
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0);
    let (samples, bad) = watcher.await.unwrap();
    assert!(samples > 100, "sampled while orders was down: {samples}");
    assert!(
        bad.is_empty(),
        "past the given-back change while down: {:?}",
        &bad[..bad.len().min(5)]
    );
    assert_eq!(progress.committed(&sink_name()), 40, "every change once");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// A thousand failures of `orders` (each with builds of its own in flight,
/// under a budget that refuses): every permit, reservation and charge comes
/// back each time (a leaked permit would hang the run, a leaked byte fails
/// `run_to_end`'s release check), and the run ends with every document.
#[tokio::test(start_paused = true)]
async fn a_thousand_failures_leak_nothing() {
    let log: Vec<Change> = (0..400).map(|n| Change::upsert(n % 40)).collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(1, 1000)])),
            delay: Some(Duration::from_millis(1)),
            padding: 4 * KIB,
            ..Behaviour::default()
        },
    );
    let restarts = Arc::new(IndexRestarts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(1)),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(4)
            .with_observer(observer)
            .with_backoff(
                crate::Backoff::new(Duration::from_millis(1), Duration::from_millis(1)).unwrap(),
            )
    })
    .with_budget(64 * KIB, usize::MAX);
    let base = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();
    let failed = tokio::time::timeout(Duration::from_secs(3_600), deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0);
    assert_eq!(restarts.live.load(Ordering::SeqCst), 1000);
    assert_eq!(deployment.recorder.state(), db.expected());
    let left = deployment.budget.as_ref().unwrap().held();
    assert_eq!(left, crate::Held::default());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let alive = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();
    assert!(alive <= base + 1, "tasks left: {alive} past {base}");
}

/// Both indexes down for good: neither is free, so neither grows exempt and
/// the reader stops at the caps; a stop has each try once more, both fail,
/// and the run ends with an error (never hangs, never `Ok`).
#[tokio::test(start_paused = true)]
async fn every_index_down_stops_the_reading_and_a_stop_ends_with_an_error() {
    let log: Vec<Change> = (0..200).map(|n| Change::upsert(n % 40)).collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(0, usize::MAX), (1, usize::MAX)])),
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let restarts = Arc::new(IndexRestarts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(
        Feed::new(&db).open().with_delay(Duration::from_millis(10)),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_stop(stop.clone())
            .with_observer(observer)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
    });
    let progress = deployment.ingest.progress();
    let sink = deployment.start_sink().await;
    let ingest = deployment.ingest;
    let run = tokio::spawn(async move { ingest.run().await });
    tokio::time::sleep(Duration::from_secs(30)).await;
    let read = progress.changes_read();
    let waiting = progress.waiting_by_index();
    stop.request();
    let ended = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the stop ends the run")
        .unwrap();
    sink.abort();
    assert!(ended.is_err(), "both retries failed: {ended:?}");
    assert!(
        read < 200,
        "the reading stopped with both down: {read} read, {waiting:?}"
    );
}

#[derive(Debug, Default)]
struct FailureWaits(Mutex<Vec<Duration>>);

impl Observer for FailureWaits {
    fn on_index_failed(&self, _: &IndexName, _: crate::Work, _: &str, retry_in: Duration) {
        self.0.lock().unwrap().push(retry_in);
    }
}

/// `orders`' backfill row 41 fails every time it is built, while `orders`
/// keeps getting live changes. Each restart walks the table again and its
/// live backlog publishes, which starts the backoff over: the waits never
/// grow, so the whole walk is redone about once a second for ever.
#[tokio::test(start_paused = true)]
async fn a_poison_backfill_row_beside_live_traffic_backs_off() {
    let changes = 600u64;
    let log: Vec<Change> = (0..changes as i64)
        .map(|n| Change::upsert(2 * (n % 10) + 1))
        .collect();
    let arrivals: Vec<Duration> = (0..changes)
        .map(|n| Duration::from_millis(100 * n))
        .collect();
    let db = Database::new(60, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(41, usize::MAX)])),
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let waits = Arc::new(FailureWaits::default());
    let observer: Arc<dyn Observer> = Arc::clone(&waits) as Arc<dyn Observer>;
    let recorder = Recorder::seeded(false);
    recorder
        .seeded_indexes
        .lock()
        .unwrap()
        .insert("users".to_owned());
    let mut deployment = Deployment::new(
        Feed::new(&db).open().with_arrivals(arrivals),
        Arc::clone(&builder),
        recorder,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(2)
            .with_stop(stop.clone())
            .with_observer(observer)
    });
    let sink = deployment.start_sink().await;
    let ingest = deployment.ingest;
    let run = tokio::spawn(async move { ingest.run().await });
    tokio::time::sleep(Duration::from_secs(55)).await;
    let seen = waits.0.lock().unwrap().clone();
    let row_1_built = builder
        .grants()
        .iter()
        .filter(|ids| ids.contains(&1) && !ids.contains(&3))
        .count();
    run.abort();
    sink.abort();
    let longest = seen.iter().max().copied().unwrap_or_default();
    assert!(
        longest >= Duration::from_secs(16),
        "{} failures in 55 s, the longest wait {longest:?}, row 1 built {row_1_built} times: {seen:?}",
        seen.len()
    );
}

/// Range A: `pacing_reader` is true exactly while the reader waits at that
/// index's bound. `users` 0's build hangs (holding one of the two permits),
/// 2 and 4 wait behind it; `orders` builds slowly and reaches its cap (4):
/// the reader waits on `orders` (4 + 2 = 6 is the total too). Then `users`
/// 0's build fails and is given back: `users` waits on 3. One `orders` build
/// later `orders` has 3 waiting, but 3 + 3 is still `total` (6): the reader
/// waits on the total now, and `orders` still shows as pacing it.
#[tokio::test(start_paused = true)]
async fn pacing_follows_the_bound_the_reader_waits_at() {
    let mut log: Vec<Change> = [0, 2, 4].into_iter().map(Change::upsert).collect();
    log.extend((0..30).map(|n| Change::upsert(2 * n + 1)));
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([0])),
            fail_once: Mutex::new(HashSet::from([0])),
            delay: Some(Duration::from_millis(100)),
            ..Behaviour::default()
        },
    );
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true)).ingest(
            |ingest| {
                ingest
                    .with_batch(batch(1))
                    .with_builds(2)
                    .with_max_waiting(MaxWaiting {
                        per_index: 4,
                        total: 6,
                    })
            },
        );
    let waits = Arc::new(super::states::ReaderWaits::default());
    let observer: Arc<dyn Observer> = Arc::clone(&waits) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    let states = deployment.ingest.index_states();
    let progress = deployment.ingest.progress();
    let orders = IndexName::try_new("orders").unwrap();
    let watcher = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            let mut wrong = Vec::new();
            let mut released = false;
            for _ in 0..3_000 {
                tokio::time::sleep(Duration::from_millis(1)).await;
                if !released && states.view(&orders).unwrap().pacing_reader {
                    builder.release(0);
                    released = true;
                }
                for name in ["orders", "users"] {
                    let index = IndexName::try_new(name).unwrap();
                    let waiting = progress.waiting_on(&index);
                    if states.view(&index).unwrap().pacing_reader && waiting < 4 {
                        wrong.push((name, waiting, progress.waiting()));
                    }
                }
            }
            builder.release(0);
            (released, wrong)
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let (released, wrong) = watcher.await.unwrap();
    assert!(released, "the reader never waited on orders");
    let waits = waits.0.lock().unwrap().clone();
    assert!(
        waits
            .iter()
            .any(|(bound, index, _)| *bound == crate::ReaderBound::PerIndex
                && index.as_deref() == Some("orders"))
            && waits
                .iter()
                .any(|(bound, index, _)| *bound == crate::ReaderBound::Total && index.is_none()),
        "the wait is reported under each bound it was at: {waits:?}"
    );
    assert!(
        wrong.is_empty(),
        "shown pacing the reader below its cap: {:?} ({} samples)",
        &wrong[..wrong.len().min(5)],
        wrong.len()
    );
}

/// Item 7 of the review of #456: `orders` 1's build fails at 0.5 s while
/// its build of 3 is slow (held until 4 s). The builder waits for that build before it
/// restarts, and is held and folding from the failure on, not only once the
/// build returned: past its cap (4) it is exempt and the reader reads on,
/// `users` landing every change meanwhile.
#[tokio::test(start_paused = true)]
async fn a_failed_index_is_held_while_its_slow_builds_finish() {
    let mut log: Vec<Change> = [1, 3].into_iter().map(Change::upsert).collect();
    log.extend((0..100).map(|n| Change::upsert(if n % 2 == 0 { n } else { n + 4 })));
    let arrivals: Vec<Duration> = (0..log.len() as u64)
        .map(|n| Duration::from_millis(if n < 2 { 0 } else { 100 + 10 * n }))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([1, 3])),
            fail_once: Mutex::new(HashSet::from([1])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(4)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
    });
    let progress = deployment.ingest.progress();
    let watcher = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            builder.release(1);
            tokio::time::sleep(Duration::from_millis(3_500)).await;
            let read = progress.changes_read();
            builder.release(3);
            read
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let read = watcher.await.unwrap();
    assert_eq!(
        read, 102,
        "the reader read every change while orders waited on its slow build"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// Review of #456, item 2: a restarted walk resumes each lane's request
/// after the last batch it published there, not from the request's own
/// point. `orders`' backfill row 41 fails twice: the batches before it are
/// published once, so row 1 is built once, and every row lands.
#[tokio::test(start_paused = true)]
async fn a_restarted_walk_resumes_where_its_lanes_are() {
    let db = Database::new(60, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(41, 2)])),
            resumable: true,
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(2));
    let restarts = Restarts::watching(&deployment);
    let observer: Arc<dyn Observer> = Arc::clone(&restarts) as Arc<dyn Observer>;
    deployment = deployment.ingest(|ingest| ingest.with_observer(observer));
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(restarts.failures().len(), 2, "the walk failed twice");
    assert!(restarts.restarts().is_empty(), "and failed alone");
    let built_1 = builder
        .grants()
        .iter()
        .filter(|ids| ids.contains(&1))
        .count();
    assert_eq!(built_1, 1, "row 1 built once: {:?}", builder.grants());
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R14 in the engine (review of #456, item 9): `orders`' lane fails a
/// publish. A lane is the stream's, shared: the run ends, nothing restarts
/// alone, and the replay builds everything.
#[tokio::test(start_paused = true)]
async fn a_lane_failure_ends_the_run_and_restarts_no_index_alone() {
    let db = Database::new(0, (0..20).map(Change::upsert).collect());
    let (mut deployment, _, restarts) = two_indexes(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        2,
    );
    deployment
        .failing
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let failed = tokio::time::timeout(LIMIT, deployment.run_to_end(1))
        .await
        .expect("the run ends");
    assert_eq!(failed, 1, "the lane's failure ended the run");
    assert!(restarts.failures().is_empty(), "{:?}", restarts.failures());
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R6 in the engine (review of #456, item 9): `orders` 3's build is held,
/// then fails; behind it 1 (change 1), 5 (change 2) and 1 again (change 3)
/// built and wait for their turn. All are given back: 1 waits at its older
/// change, 1, so the restart takes 3, 1, 5, oldest first (at the newer
/// change it would take 3, 5, 1).
#[tokio::test(start_paused = true)]
async fn a_given_back_document_keeps_its_older_change() {
    let db = Database::new(0, [3, 1, 5, 1, 0].into_iter().map(Change::upsert).collect());
    let (mut deployment, builder, restarts) = two_indexes(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([3])),
            fail_once: Mutex::new(HashSet::from([3])),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        4,
    );
    let releaser = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            while builder.grants().iter().filter(|ids| **ids == [1]).count() < 2 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            builder.release(3);
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    releaser.await.unwrap();
    assert_eq!(restarts.restarts().len(), 1);
    let grants = builder.grants();
    let second_1 = grants
        .iter()
        .enumerate()
        .filter(|(_, ids)| **ids == [1])
        .nth(1)
        .map(|(at, _)| at)
        .unwrap();
    let after: Vec<i64> = grants
        .iter()
        .skip(second_1 + 1)
        .flatten()
        .copied()
        .filter(|id| id % 2 == 1)
        .collect();
    assert_eq!(after, [3, 1, 5], "{grants:?}");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// B1 (review of #457): a live failure is reported when it happens, before
/// the builder waits for its running builds: `orders` 1's build fails at
/// 0.5 s while its build of 3 is held until 4 s; `/status` must show it down
/// from 0.5 s, so the event comes then.
#[tokio::test(start_paused = true)]
async fn a_live_failure_is_reported_before_its_slow_builds_return() {
    let db = Database::new(0, [1, 3].into_iter().map(Change::upsert).collect());
    let (mut deployment, builder, restarts) = two_indexes(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([1, 3])),
            fail_once: Mutex::new(HashSet::from([1])),
            ..Behaviour::default()
        },
        Recorder::seeded(true),
        4,
    );
    let opened = tokio::time::Instant::now();
    let releaser = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            builder.release(1);
            tokio::time::sleep(Duration::from_millis(3_500)).await;
            builder.release(3);
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    releaser.await.unwrap();
    let failures = restarts.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    let reported = failures[0].3 - opened;
    assert!(
        reported < Duration::from_secs(1),
        "reported at the failure, not once the slow build returned: {reported:?}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}
