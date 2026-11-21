//! A builder per index (ADR 0012 stage 8a, slice 6b-1c) against the
//! harness of the engine's own tests: one index is 6b-1b's path, a freed
//! permit reaches a builder waiting in a line (G9′), and a held index
//! rewriting one document folds (F1). Every wait is bounded.

use super::*;

const LIMIT: Duration = Duration::from_secs(60);

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

/// One index is 6b-1b's path: its builder asks the gate with G9's pass
/// and never joins a line, whatever holds its permits (a backfill, builds
/// at once, a burst).
#[test]
fn one_index_never_waits_in_a_gate_line() {
    crate::tests::paused_runtime().block_on(async {
        crate::ingest::gate::JOINS.with(|joins| joins.set(0));
        let db = Database::new(32, (0..64).map(|n| Change::upsert(n % 16)).collect());
        let builder = Builder::new(
            &db,
            Behaviour {
                delay: Some(Duration::from_millis(3)),
                ..Behaviour::default()
            },
        );
        let mut deployment = Deployment::new(Feed::new(&db), builder, Recorder::seeded(false))
            .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(2));
        deployment.run_to_end(0).await;
        assert_eq!(deployment.recorder.state(), db.expected());
        assert_eq!(
            crate::ingest::gate::JOINS.with(std::cell::Cell::get),
            0,
            "one builder never joins a line"
        );
    });
}

/// G9′: two indexes and one permit. `users` 0 builds (held) while `orders`
/// 1 waits for the permit in its line; once `users` 0 lands, the freed
/// permit wakes `orders` at once, not at the next tick. Without a waiter in
/// the line nobody is woken and `orders` 1 is never built.
#[tokio::test(start_paused = true)]
async fn a_freed_permit_wakes_a_waiting_builder() {
    let db = Database::new(0, [0, 1].map(Change::upsert).to_vec());
    let builder = Builder::two(&db, held([0]));
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
    let recorder = Arc::clone(&deployment.recorder);
    let driver = {
        let (builder, db) = (Arc::clone(&builder), Arc::clone(&db));
        async move {
            until("both changes are read", || {
                db.visible.load(Ordering::SeqCst) == 2
            })
            .await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(
                !recorder.state().contains_key(&1),
                "orders 1 waits for the permit"
            );
            let released = tokio::time::Instant::now();
            builder.release(0);
            until("orders 1 lands", || recorder.state().contains_key(&1)).await;
            assert!(
                released.elapsed() < Duration::from_millis(500),
                "orders 1 landed {:?} after the permit was freed",
                released.elapsed()
            );
        }
    };
    let mut driver = tokio::spawn(driver);
    let run = tokio::time::timeout(LIMIT, deployment.run_until_landed(&db, &stop, 0));
    tokio::pin!(run);
    tokio::select! {
        ended = &mut driver => {
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

/// F1, the repeat-heavy case of slice 4's scenario: `orders` held at its
/// share (its lane refuses every document) rewrites one document once a
/// second for ten minutes beside `users`' bursts. Folded, its changes are
/// queued nowhere and count toward no bound (one document waits), so the
/// reading never stops; then `orders` is let through and everything
/// converges.
#[tokio::test(start_paused = true)]
async fn a_held_index_rewriting_one_document_holds_no_bound_and_no_memory_per_change() {
    let log: Vec<Change> = (0..9600)
        .map(|n| {
            if n % 16 == 0 {
                Change::upsert(1001)
            } else {
                Change::upsert(2 * (n % 20))
            }
        })
        .collect();
    let total = log.len() as u64;
    let db = Database::new(400, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: 8 * KIB,
            ..Behaviour::default()
        },
        2,
    );
    let recorder = Recorder::seeded(false);
    recorder
        .seeded_indexes
        .lock()
        .unwrap()
        .insert("users".to_owned());
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_bursts(16, Duration::from_secs(1)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| ingest.with_batch(batch(16)).with_builds(4))
    .with_budget(512 * KIB, usize::MAX);
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut seen = Vec::new();
            for _ in 0..60 {
                tokio::time::sleep(Duration::from_secs(10)).await;
                seen.push(progress.changes_read());
            }
            recorder.refused_indexes.lock().unwrap().clear();
            seen
        })
    };
    deployment.run_to_end(0).await;
    let seen = watcher.await.unwrap();
    assert_eq!(
        seen.last().copied(),
        Some(total),
        "the reading never stopped: {seen:?}"
    );
    // `users` is seeded: its rows past the log's are never written.
    let state = deployment.recorder.state();
    for (id, version) in db
        .expected()
        .iter()
        .filter(|(id, _)| **id < 40 || *id % 2 == 1)
    {
        assert_eq!(
            state.get(id),
            Some(version),
            "{id} once orders is let through"
        );
    }
}

/// §3.9 §6: a builder blocked on a full lane holds no place in a line and
/// no permit. `orders`' lane is parked (two items, every document refused
/// under `stop`) and its builder blocks publishing; with two permits and a
/// 10 ms build, `users`' changes are built and land at once. Holding a
/// permit for a build that finished, or a waiter it can't ask with, the
/// blocked builder would leave `users` waiting for as long as `orders` is
/// parked.
#[tokio::test(start_paused = true)]
async fn a_builder_awaiting_outside_its_loop_holds_no_place_in_a_line() {
    let log: Vec<Change> = (0..12)
        .map(|n| Change::upsert(2 * n + 1))
        .chain((0..4).map(|n| Change::upsert(2 * n)))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(10)),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_delay(Duration::from_millis(1)).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        2,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            // The waiting caps would stop the reading behind the parked
            // index (6b-2 lifts that): out of the way, so only the gate
            // can hold `users` back.
            .with_max_waiting(crate::MaxWaiting {
                per_index: 100,
                total: 100,
            })
            .with_stop(stop.clone())
    });
    let driver = {
        let recorder = Arc::clone(&recorder);
        async move {
            until("users' changes land beside the parked orders", || {
                let state = recorder.state();
                [0, 2, 4, 6].iter().all(|id| state.contains_key(id))
            })
            .await;
            recorder.refused_indexes.lock().unwrap().clear();
        }
    };
    let mut driver = tokio::spawn(tokio::time::timeout(Duration::from_secs(10), driver));
    let run = tokio::time::timeout(LIMIT, deployment.run_until_landed(&db, &stop, 0));
    tokio::pin!(run);
    tokio::select! {
        ended = &mut driver => {
            ended.unwrap().expect("users' changes landed within ten seconds");
            run.await.expect("the run ends");
        }
        ran = &mut run => {
            ran.expect("the run ends");
            driver.await.unwrap().expect("users' changes landed within ten seconds");
        }
    }
}

/// §3.9 §5: a publish blocked on a full lane holds its builder like the
/// budget does, so the builder folds. `orders` is parked at its lane's
/// backstop (two items, every document refused) with no budget hold,
/// rewriting one document beside `users`' bursts: folded, it queues no
/// change and counts toward no bound, so the reading never stops (without
/// the fold its queue holds the reader `max_waiting` changes in). Then
/// `orders` is let through and everything converges.
#[tokio::test(start_paused = true)]
async fn an_index_parked_at_its_lane_backstop_folds() {
    let log: Vec<Change> = (0..960)
        .map(|n| {
            if n % 16 == 0 {
                Change::upsert(1001)
            } else {
                Change::upsert(2 * (n % 20))
            }
        })
        .collect();
    let total = log.len() as u64;
    let db = Database::new(0, log);
    let builder = Builder::two(&db, Behaviour::default());
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_bursts(16, Duration::from_secs(1)),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        2,
    )
    .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(2));
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(70)).await;
            let read = progress.changes_read();
            recorder.refused_indexes.lock().unwrap().clear();
            read
        })
    };
    deployment.run_to_end(0).await;
    assert_eq!(
        watcher.await.unwrap(),
        total,
        "the reading stopped behind the parked orders"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// Finding 4 of the 6b-1c review: a folding builder's backlog take wakes
/// the reader. `orders` folds behind its parked lane (two items, one in
/// flight) with two documents waiting: held at its cap it is exempt (6b-2),
/// and two documents are `max_pending_documents` here, so the reader waits.
/// Once the lane drains, `orders` takes the oldest of them into a build
/// that stays held: one document waits, the cap has room, and the reader
/// reads `users`' changes at once, without waiting for the backlog to
/// drain.
#[tokio::test(start_paused = true)]
async fn a_backlog_take_wakes_the_reader() {
    let log: Vec<Change> = [1, 3, 5, 7, 9, 0, 2].map(Change::upsert).to_vec();
    let db = Database::new(0, log);
    let builder = Builder::two(&db, held([7]));
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let stop = Stop::new();
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
            .with_max_waiting(crate::MaxWaiting {
                per_index: 2,
                total: 100,
            })
            // Held at its cap, orders is exempt (6b-2): its two documents
            // stop the reading at the pending cap instead.
            .with_max_pending_documents(2)
            .with_stop(stop.clone())
    });
    let progress = deployment.ingest.progress();
    let driver = {
        let (builder, recorder) = (Arc::clone(&builder), Arc::clone(&recorder));
        async move {
            until("orders' five changes are read", || {
                progress.changes_read() == 5
            })
            .await;
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert_eq!(
                progress.changes_read(),
                5,
                "the reader waits at orders' cap"
            );
            recorder.refused_indexes.lock().unwrap().clear();
            tokio::time::timeout(Duration::from_secs(5), async {
                while progress.changes_read() < 7 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("the backlog take made room, and the reader read on");
            builder.release(7);
        }
    };
    let mut driver = tokio::spawn(driver);
    let run = tokio::time::timeout(LIMIT, deployment.run_until_landed(&db, &stop, 0));
    tokio::pin!(run);
    tokio::select! {
        ended = &mut driver => {
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

/// `orders` (odd ids) changing `orders` distinct documents and `users` (even
/// ids) changing `users`, interleaved, on two indexes whose lanes hold two
/// items: `orders` refused (its lane parks) from the start.
fn stuck_orders(orders: i64, users: i64) -> (Arc<Database>, Arc<Builder>, Arc<Recorder>) {
    let log: Vec<Change> = (0..orders.max(users))
        .flat_map(|n| {
            let mut both = Vec::new();
            if n < orders {
                both.push(Change::upsert(2 * n + 1));
            }
            if n < users {
                both.push(Change::upsert(2 * n));
            }
            both
        })
        .map(|change| change)
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(&db, Behaviour::default());
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    (db, builder, recorder)
}

fn orders_written(recorder: &Recorder) -> usize {
    recorder.state().keys().filter(|id| *id % 2 == 1).count()
}

/// I12 (6b-2): the stream's end builds an exempt backlog. `orders` is held
/// at its cap and exempt, so `users` is read and written to the end; the
/// run doesn't end while `orders`' documents wait, and once `orders` is let
/// through at 30 s every one of them lands and the run ends.
#[tokio::test(start_paused = true)]
async fn the_stream_end_builds_an_exempt_backlog() {
    let (db, builder, recorder) = stuck_orders(20, 20);
    let mut deployment = Deployment::with_lanes(Feed::new(&db), builder, Arc::clone(&recorder), 2)
        .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(2));
    let started = tokio::time::Instant::now();
    let release = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let users = recorder.state().keys().filter(|id| *id % 2 == 0).count();
            recorder.refused_indexes.lock().unwrap().clear();
            users
        })
    };
    tokio::time::timeout(LIMIT * 2, deployment.run_to_end(0))
        .await
        .expect("the run ends once orders is let through");
    assert!(
        started.elapsed() >= Duration::from_secs(30),
        "the run waited for orders' backlog"
    );
    assert_eq!(release.await.unwrap(), 20, "users was written to the end");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// O3 (6b-2): a stop builds and lands everything read, an exempt backlog
/// included. `orders` is exempt with its documents waiting when the stop
/// comes; let through a second later, every `orders` document read lands
/// and the stop ends cleanly.
#[tokio::test(start_paused = true)]
async fn a_stop_builds_and_lands_an_exempt_backlog_under_the_deadline() {
    let (db, builder, recorder) = stuck_orders(20, 20);
    let stop = Stop::new();
    let mut deployment =
        Deployment::with_lanes(Feed::new(&db).open(), builder, Arc::clone(&recorder), 2).ingest(
            |ingest| {
                ingest
                    .with_batch(batch(1))
                    .with_builds(2)
                    .with_stop(stop.clone())
            },
        );
    let progress = deployment.ingest.progress();
    let sink = deployment.start_sink().await;
    let driver = {
        let (recorder, progress, stop) =
            (Arc::clone(&recorder), Arc::clone(&progress), stop.clone());
        tokio::spawn(async move {
            until("every change is read", || progress.changes_read() == 40).await;
            let waiting = progress.waiting();
            stop.request();
            tokio::time::sleep(Duration::from_secs(1)).await;
            recorder.refused_indexes.lock().unwrap().clear();
            waiting
        })
    };
    tokio::time::timeout(LIMIT, deployment.ingest.run())
        .await
        .expect("the stop ends once orders is let through")
        .expect("a clean stop");
    assert!(
        driver.await.unwrap() > 2,
        "orders' backlog waited at the stop"
    );
    deployment.drained().await;
    assert_eq!(
        orders_written(&recorder),
        20,
        "every orders change read was built and landed"
    );
    assert_eq!(deployment.ingest.progress().waiting(), 0);
    sink.abort();
}

/// O3, the other outcome: a stop with the exempt index still stuck can't
/// build its backlog, so it doesn't end (the CLI's deadline exits 1, naming
/// the index from `waiting_by_index`); it never ends `Ok` with read changes
/// unbuilt.
#[tokio::test(start_paused = true)]
async fn a_stop_with_an_exempt_index_still_stuck_does_not_end() {
    let (db, builder, recorder) = stuck_orders(20, 20);
    let stop = Stop::new();
    let mut deployment =
        Deployment::with_lanes(Feed::new(&db).open(), builder, Arc::clone(&recorder), 2).ingest(
            |ingest| {
                ingest
                    .with_batch(batch(1))
                    .with_builds(2)
                    .with_stop(stop.clone())
            },
        );
    let progress = deployment.ingest.progress();
    let sink = deployment.start_sink().await;
    {
        let (progress, stop) = (Arc::clone(&progress), stop.clone());
        tokio::spawn(async move {
            until("every change is read", || progress.changes_read() == 40).await;
            stop.request();
        });
    }
    let mut run = std::pin::pin!(deployment.ingest.run());
    assert!(
        tokio::time::timeout(Duration::from_secs(30), &mut run)
            .await
            .is_err(),
        "the stop waits for orders' backlog"
    );
    let left = progress.waiting_by_index();
    assert!(
        left.iter()
            .any(|(index, count)| index.as_ref() == "orders" && *count > 0),
        "what the deadline's log names: {left:?}"
    );
    recorder.refused_indexes.lock().unwrap().clear();
    tokio::time::timeout(LIMIT, run)
        .await
        .expect("let through, the stop ends")
        .expect("a clean stop");
    sink.abort();
}
