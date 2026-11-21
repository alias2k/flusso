//! The reader and the builder together (ADR 0012 stage 8a, slice 6a, the
//! rules R1–R10 of §3.8): batches as `main` cut them, the lead, the stop,
//! errors, and the stage timings. Every wait is bounded, so a broken rule
//! fails instead of hanging.

use super::*;
use crate::{MaxWaiting, Observer, Stage, StageTiming};

const LIMIT: Duration = Duration::from_secs(60);

/// Wait until `done`, failing past [`LIMIT`].
async fn until(what: &str, done: impl Fn() -> bool) {
    tokio::time::timeout(LIMIT, async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what}"));
}

fn held(ids: impl IntoIterator<Item = i64>) -> Behaviour {
    Behaviour {
        held: Mutex::new(ids.into_iter().collect()),
        ..Behaviour::default()
    }
}

/// Run `deployment`'s engine until every change of `db` landed, while
/// `driver` runs beside it.
async fn run_beside(
    deployment: &mut Deployment,
    db: &Arc<Database>,
    stop: &Stop,
    driver: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let driver = tokio::spawn(driver);
    tokio::time::timeout(LIMIT, deployment.run_until_landed(db, stop, 0))
        .await
        .expect("the run ends");
    driver.await.unwrap();
}

/// R2 and R3: behind a held build the reader reads up to its lead (two
/// documents waiting at `max_changes = 1`) and no further; once the build
/// lands it reads on, and each run is built in a batch of its own, as
/// `main` cut them, never a later run's document in an earlier batch (R4).
#[tokio::test(start_paused = true)]
async fn the_reader_waits_at_the_lead_and_resumes() {
    let db = Database::new(0, [0, 2, 4, 6, 8].map(Change::upsert).to_vec());
    let builder = Builder::new(&db, held([0]));
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(1)).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(1)
            .with_stop(stop.clone())
    });
    let driver = {
        let builder = Arc::clone(&builder);
        let db = Arc::clone(&db);
        async move {
            until("the reader reads to its lead", || {
                db.visible.load(Ordering::SeqCst) == 3
            })
            .await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                db.visible.load(Ordering::SeqCst),
                3,
                "the build of 0 holds, 2 and 4 wait: the reader waits at its lead"
            );
            builder.release(0);
        }
    };
    run_beside(&mut deployment, &db, &stop, driver).await;
    assert_eq!(builder.grants(), [[0], [2], [4], [6], [8]]);
}

/// R3: a trickle read while a live build runs isn't built beside it (it
/// isn't a full batch) and is one batch after it, as `main`'s open run
/// gathered it.
#[tokio::test(start_paused = true)]
async fn a_trickle_read_while_a_live_build_runs_is_one_batch_after_it() {
    let db = Database::new(0, [0, 2, 4, 6].map(Change::upsert).to_vec());
    let builder = Builder::new(&db, held([0]));
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(1)).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(BatchPolicy {
                max_changes: 16,
                max_delay: Duration::from_secs(5),
            })
            .with_builds(4)
            .with_stop(stop.clone())
    });
    let driver = {
        let builder = Arc::clone(&builder);
        let db = Arc::clone(&db);
        async move {
            until("every change is read", || {
                db.visible.load(Ordering::SeqCst) == 4
            })
            .await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                builder.grants(),
                [[0]],
                "no second live build beside a batch that isn't full"
            );
            builder.release(0);
        }
    };
    run_beside(&mut deployment, &db, &stop, driver).await;
    assert_eq!(builder.grants(), [vec![0], vec![2, 4, 6]]);
}

/// R4: a change read ahead is never counted committed before its build
/// lands. `users` 0 builds (held) while the reader reads `orders` 1 and
/// `users` 2; once 0 lands, `orders` 1 is the next batch, and with it landed
/// and `users` 2 held, the sink has committed two changes, not three: the
/// batch's step stops at its own run.
#[tokio::test(start_paused = true)]
async fn a_change_read_ahead_is_not_counted_committed_before_its_build_lands() {
    let db = Database::new(0, [0, 1, 2].map(Change::upsert).to_vec());
    let builder = Builder::two(&db, held([0, 2]));
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(1)).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(1)
            .with_stop(stop.clone())
    });
    let progress = deployment.ingest.progress();
    let recorder = Arc::clone(&deployment.recorder);
    let driver = {
        let builder = Arc::clone(&builder);
        let db = Arc::clone(&db);
        async move {
            until("every change is read", || {
                db.visible.load(Ordering::SeqCst) == 3
            })
            .await;
            builder.release(0);
            until("orders 1 lands", || recorder.state().contains_key(&1)).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                progress.committed(&sink_name()),
                2,
                "users 2 is read and marked, not built: its change isn't committed"
            );
            builder.release(2);
        }
    };
    run_beside(&mut deployment, &db, &stop, driver).await;
    assert_eq!(builder.grants(), [[0], [1], [2]]);
}

/// R6: a document the reader marked holds confirmation before its change
/// until its build lands.
#[tokio::test(start_paused = true)]
async fn a_document_the_reader_marked_pins_confirmation_until_its_build_lands() {
    let db = Database::new(0, [0, 2].map(Change::upsert).to_vec());
    let builder = Builder::new(&db, held([0]));
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(1)).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(1)
            .with_stop(stop.clone())
    });
    let progress = deployment.ingest.progress();
    let feed = Arc::clone(&deployment.feed);
    let driver = {
        let builder = Arc::clone(&builder);
        let db = Arc::clone(&db);
        async move {
            until("both changes are read", || {
                db.visible.load(Ordering::SeqCst) == 2
            })
            .await;
            tokio::time::sleep(Duration::from_millis(1500)).await;
            assert_eq!(
                progress.landed(),
                source::cdc::Landed::Before(Position::new(0)),
                "0 builds, 2 waits: nothing is confirmed"
            );
            assert!(feed.confirmed.lock().unwrap().is_empty());
            builder.release(0);
        }
    };
    run_beside(&mut deployment, &db, &stop, driver).await;
    assert_eq!(
        deployment.feed.confirmed.lock().unwrap().last(),
        Some(&Position::new(1))
    );
}

/// R7: a stop in the middle of a run cuts it there: what the reader read is
/// built, published and confirmed before the run ends, and nothing more is
/// read.
#[tokio::test(start_paused = true)]
async fn a_stop_mid_run_builds_what_the_reader_marked_last() {
    let db = Database::new(0, [0, 2, 4, 6, 8].map(Change::upsert).to_vec());
    let stop = Stop::new();
    let builder = Builder::new(
        &db,
        Behaviour {
            stop_on_resolve: Mutex::new(Some((4, stop.clone()))),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(BatchPolicy {
                max_changes: 16,
                max_delay: Duration::from_secs(5),
            })
            .with_builds(4)
            .with_stop(stop.clone())
    });
    let sink = deployment.start_sink().await;
    let result = tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .expect("the stop ends the run");
    deployment.drained().await;
    deployment.assert_released().await;
    sink.abort();

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(builder.grants(), [[0, 2, 4]], "the run cut at the stop");
    assert_eq!(db.visible.load(Ordering::SeqCst), 3, "nothing read after");
    assert_eq!(
        deployment.feed.confirmed.lock().unwrap().last(),
        Some(&Position::new(2)),
        "confirmed through the run's last change before the run ends"
    );
}

/// R7 with the reader far ahead: 500 changes read while the first build is
/// held, then a stop. Everything read is built in `main`'s batches,
/// published and confirmed, well within a stop's deadline.
#[tokio::test(start_paused = true)]
async fn a_stop_with_many_pending_ids_finishes_within_the_deadline() {
    let db = Database::new(0, (0..500).map(|n| Change::upsert(2 * n)).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([0])),
            delay: Some(Duration::from_millis(10)),
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
            .with_batch(batch(16))
            .with_builds(4)
            .with_max_waiting(MaxWaiting {
                per_index: 1000,
                total: 1000,
            })
            .with_stop(stop.clone())
    });
    let sink = deployment.start_sink().await;
    let driver = {
        let builder = Arc::clone(&builder);
        let db = Arc::clone(&db);
        let stop = stop.clone();
        tokio::spawn(async move {
            until("the reader reads every change", || {
                db.visible.load(Ordering::SeqCst) == 500
            })
            .await;
            stop.request();
            builder.release(0);
        })
    };
    let started = tokio::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(30), deployment.ingest.run())
        .await
        .expect("the stop's deadline: what was read is built within it");
    driver.await.unwrap();
    deployment.drained().await;
    deployment.assert_released().await;
    sink.abort();

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(deployment.recorder.state(), db.expected());
    assert!(
        builder.grants().iter().skip(1).all(|ids| ids.len() <= 16),
        "built in batches of `max_changes`"
    );
    assert_eq!(
        deployment.feed.confirmed.lock().unwrap().last(),
        Some(&Position::new(499))
    );
    assert!(started.elapsed() < Duration::from_secs(30));
}

/// R9: a resolve error ends the run with it.
#[tokio::test(start_paused = true)]
async fn a_reader_error_ends_the_run_with_its_error() {
    let db = Database::new(0, [0, 2, 4].map(Change::upsert).to_vec());
    let builder = Builder::new(
        &db,
        Behaviour {
            resolve_fails: Mutex::new(HashSet::from([2])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(Feed::new(&db).open(), builder, Recorder::seeded(true))
        .ingest(|ingest| ingest.with_batch(batch(1)));
    let sink = deployment.start_sink().await;
    let result = tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .expect("the reader's error ends the run");
    sink.abort();
    assert!(
        matches!(&result, Err(crate::EngineError::Source(SourceError::Connection(why))) if why.contains("resolving 2")),
        "{result:?}"
    );
}

/// R9: a builder error ends the run, and the reader with it: it reads
/// nothing more once the run returned.
#[tokio::test(start_paused = true)]
async fn a_build_error_ends_the_run_and_the_reader_with_it() {
    let db = Database::new(0, (0..100).map(|n| Change::upsert(2 * n)).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([0])),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(1)).open(),
        builder,
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest.with_batch(batch(1)).with_max_waiting(MaxWaiting {
            per_index: 1000,
            total: 1000,
        })
    });
    let sink = deployment.start_sink().await;
    let result = tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .expect("the build's error ends the run");
    assert!(result.is_err());
    let read = db.visible.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        db.visible.load(Ordering::SeqCst),
        read,
        "the reader was aborted with its run"
    );
    sink.abort();
}

/// The stage timings of an idle stretch then one change.
#[derive(Debug, Default)]
struct Stages(Mutex<Vec<(Stage, Duration)>>);

impl Observer for Stages {
    fn on_stage(&self, timing: StageTiming<'_>) {
        self.0.lock().unwrap().push((timing.stage, timing.elapsed));
    }
}

impl Stages {
    fn total(&self, stage: Stage) -> Duration {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(of, _)| *of == stage)
            .map(|(_, took)| *took)
            .sum()
    }
}

/// Observability: the builder's idle wait ends where the reader began the
/// run that woke it, so `Idle` and the reader's `Resolve` never overlap:
/// ten idle seconds, then a change resolved in 5 ms, are ten seconds of
/// `Idle` and 5 ms of `Resolve`.
#[tokio::test(start_paused = true)]
async fn the_builders_idle_ends_where_the_reader_began_the_run() {
    let db = Database::new(0, vec![Change::upsert(0)]);
    let builder = Builder::new(
        &db,
        Behaviour {
            resolve_delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
    );
    let stages = Arc::new(Stages::default());
    let observer = Arc::clone(&stages);
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_secs(10)),
        builder,
        Recorder::seeded(true),
    )
    .ingest(|ingest| ingest.with_batch(batch(16)).with_observer(observer));
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the stream ends");
    assert_eq!(stages.total(Stage::Idle), Duration::from_secs(10));
    assert_eq!(stages.total(Stage::Resolve), Duration::from_millis(5));
}

/// R2 behind a parked lane: the sink stalls, the lane fills and the builder
/// waits to publish; the reader goes on only to its lead, so the documents
/// waiting stay within it plus one run, and the reading stops, until the
/// sink resumes and everything lands.
#[tokio::test(start_paused = true)]
async fn a_parked_lane_bounds_pending_memory() {
    let db = Database::new(0, (0..2000).map(|n| Change::upsert(2 * n)).collect());
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let max = MaxWaiting {
        per_index: 40,
        total: 40,
    };
    let mut deployment = Deployment::new(
        Feed::new(&db).with_delay(Duration::from_millis(1)),
        Builder::new(&db, Behaviour::default()),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(2)
            .with_max_waiting(max)
    });
    let progress = deployment.ingest.progress();
    let watcher = {
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            let mut most = 0;
            for _ in 0..2000 {
                tokio::time::sleep(Duration::from_millis(5)).await;
                most = most.max(progress.waiting());
            }
            let read = db.visible.load(Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(1)).await;
            let still = db.visible.load(Ordering::SeqCst);
            recorder.resume();
            (most, read, still)
        })
    };
    tokio::time::timeout(LIMIT * 10, deployment.run_to_end(0))
        .await
        .expect("everything lands once the sink resumes");
    let (most, read, still) = watcher.await.unwrap();
    assert!(
        most <= max.per_index + 4,
        "{most} documents waited at once: past the lead plus one run"
    );
    assert!(
        read < 2000,
        "the reading stopped behind the parked lane: {read} read"
    );
    assert_eq!(read, still, "and stayed stopped");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// R7's barrier: the builder is blocked publishing to a full lane (the sink
/// stalled) while the reader reads on, and the stop, requested as the reader
/// resolves change 60, finds runs still in the channel. The builder waits
/// for the reader's end, takes every run it handed over, and lands them:
/// everything read is built and confirmed, nothing more is read.
#[tokio::test(start_paused = true)]
async fn a_stop_while_the_builder_waits_on_a_full_lane_builds_every_run_handed_over() {
    let db = Database::new(0, (0..100).map(|n| Change::upsert(2 * n)).collect());
    let stop = Stop::new();
    let builder = Builder::new(
        &db,
        Behaviour {
            stop_on_resolve: Mutex::new(Some((120, stop.clone()))),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_delay(Duration::from_millis(1)).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(1)
            .with_max_waiting(MaxWaiting {
                per_index: 1000,
                total: 1000,
            })
            .with_stop(stop.clone())
    });
    let sink = deployment.start_sink().await;
    let driver = {
        let db = Arc::clone(&db);
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            until("the reader reads to the stop", || {
                db.visible.load(Ordering::SeqCst) == 61
            })
            .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            recorder.resume();
        })
    };
    let result = tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .expect("the stop ends the run once the sink resumes");
    driver.await.unwrap();
    deployment.drained().await;
    deployment.assert_released().await;
    sink.abort();

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(db.visible.load(Ordering::SeqCst), 61, "nothing read after");
    let expected: BTreeMap<i64, u64> = (0..61).map(|n| (2 * n, n as u64 + 1)).collect();
    assert_eq!(
        deployment.recorder.state(),
        expected,
        "every change read landed"
    );
    assert_eq!(
        deployment.feed.confirmed.lock().unwrap().last(),
        Some(&Position::new(60))
    );
}
