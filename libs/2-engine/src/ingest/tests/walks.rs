//! Walks at once (ADR 0012 stage 8a, slice 6b-3) against the harness of the
//! engine's own tests: a stuck index's walk holds no healthy index's walk,
//! and walking indexes take backfill grants in turn. `users` is the even
//! ids, `orders` the odd. Every wait is bounded.

use super::*;

const LIMIT: Duration = Duration::from_secs(120);

/// How many of `users`' rows below `rows` the sink holds.
fn users_landed(recorder: &Recorder, rows: i64) -> usize {
    recorder
        .state()
        .keys()
        .filter(|id| **id % 2 == 0 && **id < rows)
        .count()
}

/// Rule 4 per index (6b-3): `orders`' sink refuses every document, so its
/// lane parks and its backfill batches stay charged, filling backfill's half
/// of the budget (two batches) for as long as it is stuck. `users` holds no
/// backfill bytes each time its batch is acknowledged, so it gets one more
/// and walks to the end beside it; with rule 4 for every index together
/// (backfill holding nothing) it would be refused until `orders` recovers.
#[tokio::test(start_paused = true)]
async fn a_stuck_index_walking_does_not_hold_a_healthy_indexs_walk() {
    let rows: i64 = 400;
    let db = Database::new(rows, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
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
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(2)
            .with_stop(stop.clone())
    })
    .with_budget_shared_at(128 * KIB, 0.5);
    let watcher = {
        let recorder = Arc::clone(&recorder);
        let stop = stop.clone();
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
            while users_landed(&recorder, rows) < (rows / 2) as usize
                && tokio::time::Instant::now() < deadline
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let landed = users_landed(&recorder, rows);
            recorder.refused_indexes.lock().unwrap().clear();
            while recorder.state() != db.expected() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            stop.request();
            landed
        })
    };
    tokio::time::timeout(LIMIT * 4, deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    let landed = watcher.await.unwrap();
    assert_eq!(
        landed,
        (rows / 2) as usize,
        "users' walk lands beside orders stuck mid-walk"
    );
}

/// Fairness (§3.9 §7): two indexes walking at once, one build at a time.
/// Each builder keeps one backfill waiter, FIFO (G2), so while both walk
/// they take the grants in turn; neither waits for the other's whole walk.
#[tokio::test(start_paused = true)]
async fn two_backfilling_indexes_take_backfill_grants_in_turn() {
    let rows: i64 = 64;
    let db = Database::new(rows, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(10)),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(false);
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Arc::clone(&recorder))
            .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(1));
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends with both walks");
    assert_eq!(recorder.state(), db.expected());
    let order: Vec<i64> = builder
        .grants
        .lock()
        .unwrap()
        .iter()
        .filter_map(|ids| ids.first().map(|id| id % 2))
        .collect();
    // While both walk: up to the last grant of the walk that ends first.
    let ends_first = [0, 1]
        .into_iter()
        .filter_map(|kind| order.iter().rposition(|of| *of == kind))
        .min()
        .unwrap_or(0);
    let both = order.get(..=ends_first).unwrap_or_default();
    let repeats = both.windows(2).filter(|pair| pair[0] == pair[1]).count();
    assert!(
        repeats <= 1,
        "while both walk, grants alternate (one repeat at the start at most): {order:?}"
    );
}
