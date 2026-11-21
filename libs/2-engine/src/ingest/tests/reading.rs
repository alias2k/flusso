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
    let mut driver = tokio::spawn(driver);
    let run = tokio::time::timeout(LIMIT, deployment.run_until_landed(db, stop, 0));
    tokio::pin!(run);
    tokio::select! {
        ended = &mut driver => {
            // A driver that failed holds builds the run waits on: its panic
            // is the failure, not the run's timeout.
            if let Err(failed) = ended {
                std::panic::resume_unwind(failed.into_panic());
            }
            run.await.expect("the run ends");
        }
        ran = &mut run => {
            ran.expect("the run ends");
            driver.await.unwrap();
        }
    }
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

/// R3 beside a running live build, where a hand-over that can't start a
/// build walks nothing (`Runs::full_candidate`): changes 1 and 2 at 10 ms,
/// 3 and 4 at 70 ms, past the batch's 60 ms deadline, so four are queued but
/// the batch is cut at two and nothing starts beside the held build of 0.
/// Once 0 lands, `[1, 2]` is granted (held too); 5 at 110 ms leaves `[3, 4,
/// 5]` short of four and 6 at 120 ms fills it, so it starts beside `[1, 2]`.
/// Kept "cut by its deadline" past the grant, or counting a granted change
/// as still queued, misses the full batch until `[1, 2]` lands.
#[tokio::test(start_paused = true)]
async fn a_full_batch_starts_beside_a_running_build_after_one_its_deadline_cut() {
    let db = Database::new(0, (0..7).map(Change::upsert).collect());
    let builder = Builder::new(&db, held([0, 1]));
    let stop = Stop::new();
    let arrivals = [0, 10, 10, 70, 70, 110, 120]
        .map(Duration::from_millis)
        .to_vec();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_arrivals(arrivals).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(2)
            .with_stop(stop.clone())
    });
    let driver = {
        let builder = Arc::clone(&builder);
        let db = Arc::clone(&db);
        async move {
            until("3 and 4 are read", || {
                db.visible.load(Ordering::SeqCst) == 5
            })
            .await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert_eq!(
                builder.grants(),
                [vec![0]],
                "four queued, but the deadline cut the batch at two"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
            builder.release(0);
            until("6 is read", || db.visible.load(Ordering::SeqCst) == 7).await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert_eq!(
                builder.grants(),
                [vec![0], vec![1, 2], vec![3, 4, 5, 6]],
                "the full batch starts beside the held build of 1 and 2"
            );
            builder.release(1);
        }
    };
    run_beside(&mut deployment, &db, &stop, driver).await;
    assert_eq!(builder.grants(), [vec![0], vec![1, 2], vec![3, 4, 5, 6]]);
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
    let stages = Arc::new(Stages::default());
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
            .with_observer(Arc::clone(&stages) as Arc<dyn Observer>)
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
    let resolved = stages
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(stage, _)| *stage == Stage::Resolve)
        .count();
    assert_eq!(
        resolved, 61,
        "each run's resolve is reported, the ones the barrier took from the channel too"
    );
}

/// The batches `main`'s one task cut for live changes read at `arrivals`
/// (sorted, each a document of its own), with builds of `build`, at most
/// `permits` at once, `max_changes` and `max_delay` (ADR 0012 stage 8a, R3).
/// A model of `main`'s loop, kept here so the engine is compared with the
/// rule itself:
///
/// - a batch's first change is read at its arrival, or at the last live
///   grant when it arrived before (reading stopped while a batch waited);
///   the batch closes `max_delay` after that read, or at `max_changes`;
/// - every change that arrived by then is read at once (the stream is
///   drained), later ones as they arrive, until the batch closes;
/// - it is granted the moment a permit is free and either no live build
///   runs or it is full; otherwise it waits for a build to land.
fn main_batches(
    arrivals: &[Duration],
    build: Duration,
    max_changes: usize,
    max_delay: Duration,
    permits: usize,
) -> Vec<usize> {
    let mut batches = Vec::new();
    let mut next = 0;
    let mut running: Vec<Duration> = Vec::new();
    let mut last_grant: Option<Duration> = None;
    while next < arrivals.len() {
        let opened = last_grant.map_or(arrivals[next], |grant| grant.max(arrivals[next]));
        let closes = opened + max_delay;
        let readable = |by: Duration| {
            arrivals[next..]
                .iter()
                .take_while(|arrival| **arrival <= by && **arrival < closes)
                .count()
                .min(max_changes)
        };
        let mut now = opened;
        loop {
            running.retain(|end| *end > now);
            let read = readable(now);
            if read > 0 && running.len() < permits && (running.is_empty() || read == max_changes) {
                batches.push(read);
                running.push(now + build);
                last_grant = Some(now);
                next += read;
                break;
            }
            let arrival = arrivals
                .get(next + read)
                .filter(|arrival| read < max_changes && **arrival < closes)
                .copied();
            let landing = running.iter().copied().min();
            now = match (arrival, landing) {
                (Some(arrival), Some(landing)) => arrival.min(landing),
                (Some(at), None) | (None, Some(at)) => at,
                (None, None) => break,
            };
        }
    }
    batches
}

/// Run `arrivals` (each a document of its own) through the engine with
/// builds of `build`, and return each live build's size.
async fn batches_of(
    arrivals: Vec<Duration>,
    build: Duration,
    max_changes: usize,
    builds: usize,
) -> Vec<usize> {
    resolved_batches_of(arrivals, None, build, max_changes, builds).await
}

/// [`batches_of`], each change taking `resolve` to resolve.
async fn resolved_batches_of(
    arrivals: Vec<Duration>,
    resolve: Option<Duration>,
    build: Duration,
    max_changes: usize,
    builds: usize,
) -> Vec<usize> {
    let count = arrivals.len() as i64;
    let db = Database::new(0, (0..count).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            delay: Some(build),
            resolve_delay: resolve,
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
        1 << 20,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(max_changes))
            .with_builds(builds)
            .with_max_waiting(MaxWaiting {
                per_index: 100_000,
                total: 100_000,
            })
            .with_stop(stop.clone())
    });
    tokio::time::timeout(
        Duration::from_secs(3600),
        deployment.run_until_landed(&db, &stop, 0),
    )
    .await
    .expect("everything lands");
    builder.grants().iter().map(Vec::len).collect()
}

/// R3, a steady trickle behind builds slower than `max_delay`: one change
/// every 10 ms, builds of 205 ms (off the 10 ms grid, so no grant and no
/// arrival share an instant), `max_changes = 256`, four builds. `main` cut
/// one change, then the five read within 50 ms of the second, then about
/// twenty per build: everything read while a build ran. The builder cuts
/// the same batches.
#[tokio::test(start_paused = true)]
async fn a_paced_trickle_behind_slow_builds_is_batched_as_main_batched_it() {
    let arrivals: Vec<Duration> = (1..=600).map(|n| Duration::from_millis(10 * n)).collect();
    let build = Duration::from_millis(205);
    let expected = main_batches(&arrivals, build, 256, Duration::from_millis(50), 4);
    assert_eq!(&expected[..4], [1, 5, 20, 20], "the model of main's rule");
    assert_eq!(expected.iter().sum::<usize>(), 600);
    assert_eq!(batches_of(arrivals, build, 256, 4).await, expected);
}

/// R3 under bursts: three bursts of 300 changes, a second apart, builds of
/// 205 ms, `max_changes = 64`: full batches build four at once, the rest of a
/// burst waits for the first to land, as `main` cut it.
#[tokio::test(start_paused = true)]
async fn bursts_behind_slow_builds_are_batched_as_main_batched_them() {
    let arrivals: Vec<Duration> = (0..900)
        .map(|n| Duration::from_millis(100 + 1000 * (n / 300)))
        .collect();
    let build = Duration::from_millis(205);
    let expected = main_batches(&arrivals, build, 64, Duration::from_millis(50), 4);
    assert_eq!(
        expected,
        [64, 64, 64, 64, 44].repeat(3),
        "the model of main's rule"
    );
    assert_eq!(batches_of(arrivals, build, 64, 4).await, expected);
}

/// R7 with a full lead behind slow builds: a change every 10 ms, builds of
/// 500 ms, one at a time, so the reader fills its lead (32 documents per
/// index at `max_changes = 16`) while the first build runs; then the stop.
/// What was read is built in batches of `max_changes`, not cut by
/// `max_delay` (which would cut five changes a batch here), so a stop's work
/// takes as few builds as it can.
#[tokio::test(start_paused = true)]
async fn a_stop_with_a_full_lead_behind_slow_builds_builds_batches_of_max_changes() {
    let db = Database::new(0, (0..2000).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(500)),
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db)
            .with_arrivals((1..=2000).map(|n| Duration::from_millis(10 * n)).collect())
            .open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
        1 << 20,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(16))
            .with_builds(1)
            .with_stop(stop.clone())
    });
    let progress = deployment.ingest.progress();
    let sink = deployment.start_sink().await;
    let granted_before = {
        let builder = Arc::clone(&builder);
        let stop = stop.clone();
        tokio::spawn(async move {
            until("the reader fills its lead", || progress.waiting() >= 32).await;
            let granted = builder.grants().len();
            stop.request();
            granted
        })
    };
    let result = tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .expect("the stop ends the run");
    let granted_before = granted_before.await.unwrap();
    deployment.drained().await;
    sink.abort();
    assert!(result.is_ok(), "{result:?}");
    let after: Vec<usize> = builder.grants()[granted_before..]
        .iter()
        .map(Vec::len)
        .collect();
    let (last, full) = after.split_last().expect("the stop builds what was read");
    assert!(
        full.iter().all(|size| *size == 16) && *last <= 16,
        "the stop's batches: {after:?}"
    );
}

#[derive(Debug, Default)]
struct Captured(std::sync::atomic::AtomicU64);

impl Observer for Captured {
    fn on_change_captured(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// R9 and `changes_in_flight`: a build error ends the run while the reader
/// is in the middle of a run; the reader is aborted and waited for before
/// the run returns, and what it read and never handed over is counted read,
/// so the changes captured and the changes read are equal the moment the
/// run returns (and the next run's `forget_all` meets no old reader).
#[tokio::test(start_paused = true)]
async fn a_build_error_leaves_every_change_captured_counted_read_when_the_run_returns() {
    let db = Database::new(0, (0..100).map(|n| Change::upsert(2 * n)).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([0])),
            resolve_delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
    );
    let captured = Arc::new(Captured::default());
    let observer: Arc<dyn Observer> = Arc::clone(&captured) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(Feed::new(&db).open(), builder, Recorder::seeded(true))
        .ingest(|ingest| ingest.with_batch(batch(16)).with_observer(observer));
    let progress = deployment.ingest.progress();
    let sink = deployment.start_sink().await;
    let result = tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .unwrap();
    assert!(result.is_err());
    assert_eq!(
        captured.0.load(Ordering::SeqCst),
        progress.changes_read(),
        "every change captured is counted read before the run returns"
    );
    sink.abort();
}

/// R3 cutting inside a run: bursts of 100 changes every 10 ms behind builds
/// of 205 ms, `max_changes = 256`. The reader's runs are the bursts, `main`'s
/// batches are 256 changes: a batch ends inside a run, and its rest starts
/// the next batch.
#[tokio::test(start_paused = true)]
async fn a_batch_ends_inside_a_run_where_main_closed_it() {
    let arrivals: Vec<Duration> = (0..1000)
        .map(|n| Duration::from_millis(10 * (1 + n / 100)))
        .collect();
    let build = Duration::from_millis(205);
    let expected = main_batches(&arrivals, build, 256, Duration::from_millis(50), 4);
    assert!(
        expected.iter().any(|size| size % 100 != 0),
        "the model cuts inside a burst: {expected:?}"
    );
    assert_eq!(batches_of(arrivals, build, 256, 4).await, expected);
}

/// R4 when a batch ends inside a run: nine changes in three runs of three
/// (at 10, 20 and 30 ms), `max_changes = 4`, one build at a time, the builds
/// of 0 and of 7 held. The second batch takes run two and the first change of
/// run three; its step closes at that change, so once 6 lands the sink has
/// committed seven changes, not the whole third run's nine.
#[tokio::test(start_paused = true)]
async fn a_batch_ending_inside_a_run_closes_its_step_at_its_own_last_change() {
    let db = Database::new(0, (0..9).map(Change::upsert).collect());
    let builder = Builder::new(&db, held([0, 7]));
    let stop = Stop::new();
    let arrivals = (0..9)
        .map(|n| Duration::from_millis(10 * (1 + n / 3)))
        .collect();
    let mut deployment = Deployment::new(
        Feed::new(&db).with_arrivals(arrivals).open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
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
                db.visible.load(Ordering::SeqCst) == 9
            })
            .await;
            builder.release(0);
            until("6 lands", || recorder.state().contains_key(&6)).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(
                progress.committed(&sink_name()),
                7,
                "7 and 8 are read, not built: their changes aren't committed"
            );
            builder.release(7);
        }
    };
    run_beside(&mut deployment, &db, &stop, driver).await;
    assert_eq!(
        builder.grants(),
        [vec![0, 1, 2], vec![3, 4, 5, 6], vec![7, 8]]
    );
}

/// R3 when a build lands with a run already handed over: resolving the
/// last change releases the held build of 0, so the builder wakes with both
/// the landing and the third run ready, and `select!` takes either first.
/// The grant after the landing sees every change read before it, so it takes
/// run two and the first change of run three whichever arm ran first. Each
/// round leaves that choice to `select!` again. This is the scenario end to
/// end; that no batch is formed before the hand-over is drained is pinned in
/// `live::tests::a_batch_is_formed_only_after_every_run_handed_over_is_queued`.
#[tokio::test(start_paused = true)]
async fn a_grant_takes_the_runs_handed_over_beside_the_landing_that_wakes_it() {
    for round in 0..16 {
        let db = Database::new(0, (0..9).map(Change::upsert).collect());
        let builder = Builder::new(
            &db,
            Behaviour {
                release_on_resolve: Some((8, 0)),
                ..held([0])
            },
        );
        let stop = Stop::new();
        let arrivals = (0..9)
            .map(|n| Duration::from_millis(10 * (1 + n / 3)))
            .collect();
        let mut deployment = Deployment::new(
            Feed::new(&db).with_arrivals(arrivals).open(),
            Arc::clone(&builder),
            Recorder::seeded(true),
        )
        .ingest(|ingest| {
            ingest
                .with_batch(batch(4))
                .with_builds(1)
                .with_stop(stop.clone())
        });
        run_beside(&mut deployment, &db, &stop, async {}).await;
        assert_eq!(
            builder.grants(),
            [vec![0, 1, 2], vec![3, 4, 5, 6], vec![7, 8]],
            "round {round}"
        );
    }
}

/// R9 with one task: a panic in the reader unwinds the engine run's task,
/// as a panic of `main`'s one task did, and the run reports nothing else.
#[tokio::test(start_paused = true)]
async fn a_reader_panic_unwinds_the_engine_run() {
    let db = Database::new(0, (0..3).map(Change::upsert).collect());
    let builder = Builder::new(
        &db,
        Behaviour {
            resolve_panics: Some(1),
            ..Behaviour::default()
        },
    );
    let mut deployment = Deployment::new(Feed::new(&db).open(), builder, Recorder::seeded(true));
    let sink = deployment.start_sink().await;
    let ingest = deployment.ingest;
    let ended = tokio::time::timeout(LIMIT, tokio::spawn(async move { ingest.run().await }))
        .await
        .expect("the run ends");
    sink.abort();
    let panicked = ended.expect_err("the run panicked");
    assert!(panicked.is_panic());
    let message = panicked.into_panic();
    let text = message
        .downcast_ref::<String>()
        .map(String::as_str)
        .unwrap_or_default();
    assert_eq!(text, "resolving 1 panicked");
}

/// Tokio's cooperative budget (review round 4). The reader and the builder
/// share one task, so one poll spends one budget between them. A burst read
/// through a tokio channel (one unit per change) exhausts it again and again:
/// the reader's `now_or_never` then sees the ready stream as pending and cuts
/// its run short, and its next read polls the stream with its own waker,
/// which the spent budget wakes (once the poll returns on a worker, at once
/// under a multi-thread `block_on`), so the reader is never parked with
/// changes ready. A cut run looks like the stream blocking, so the builder
/// may grant the batch then, short of `max_changes`, as `main` committed at
/// its own `now_or_never` cut (measured: 40 runs, 21 batches, the first 128
/// changes, the rest about 253: the budget a poll leaves after the builder's
/// turn). Every change lands, no batch is past
/// `max_changes`, and runs outnumber batches, so the budget did cut them.
#[tokio::test(start_paused = true)]
async fn a_burst_past_the_cooperative_budget_neither_stalls_nor_breaks_a_batch() {
    const CHANGES: i64 = 5_000;
    let db = Database::new(0, (0..CHANGES).map(Change::upsert).collect());
    let builder = instant(&db);
    let stop = Stop::new();
    let stages = Arc::new(Stages::default());
    let observer = Arc::clone(&stages);
    let mut deployment = Deployment::new(
        Feed::new(&db).through_a_tokio_channel().open(),
        Arc::clone(&builder),
        Recorder::seeded(true),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(256))
            .with_builds(4)
            .with_stop(stop.clone())
            .with_observer(observer)
    });
    run_beside(&mut deployment, &db, &stop, async {}).await;
    let grants = builder.grants();
    let built: usize = grants.iter().map(Vec::len).sum();
    assert_eq!(built, CHANGES as usize, "every change built once");
    assert!(grants.iter().all(|grant| grant.len() <= 256));
    let runs = stages
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(stage, _)| *stage == Stage::Resolve)
        .count();
    assert!(
        runs > CHANGES as usize / 256 + 1,
        "the budget cut runs short: {runs} runs"
    );
}

/// R3 with resolving that takes time, which `main_batches` doesn't model: a
/// backlog of 600 changes ready at once, each taking 1 ms to resolve, builds
/// of 20 ms, `max_changes = 256`. `main` read one change a millisecond into
/// its open batch until the batch's 50 ms deadline, then granted it, so its
/// batches were 50 changes, twelve of them; the reader reads at the same
/// pace and the builder cuts the same batches, none later.
#[tokio::test(start_paused = true)]
async fn a_backlog_resolved_a_change_a_millisecond_is_batched_as_main_batched_it() {
    let arrivals = vec![Duration::from_millis(1); 600];
    let batches = resolved_batches_of(
        arrivals,
        Some(Duration::from_millis(1)),
        Duration::from_millis(20),
        256,
        4,
    )
    .await;
    assert_eq!(batches, [50; 12]);
}
