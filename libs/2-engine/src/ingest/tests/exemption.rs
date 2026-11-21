//! The exemption's limits (ADR 0012 stage 8a, 6b-2, review of #452): an
//! exempt index grows only while it is held and another index isn't. Once
//! its hold ends, or every index is held (a whole sink down), it may drain
//! but not grow. Adopted from the reviewer's tests. Every wait is bounded.

use super::*;
use crate::policy::MaxWaiting;

const LIMIT: Duration = Duration::from_secs(120);

fn waiting_of(progress: &crate::Progress, name: &str) -> usize {
    progress
        .waiting_by_index()
        .iter()
        .find(|(index, _)| index.as_ref() == name)
        .map_or(0, |(_, count)| *count)
}

/// A whole sink down: every index's lane stalls (600 changes, `per_index`
/// 4, `total` 8, `max_pending_documents` 100). Returns the changes read at
/// 5 s and the documents waiting then, over every index.
async fn whole_sink_down(indexes: usize, capacity: usize, budget: bool) -> (u64, usize) {
    let log: Vec<Change> = (0..600).map(Change::upsert).collect();
    let db = Database::new(0, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: if budget { 8 * KIB } else { 0 },
            ..Behaviour::default()
        },
        indexes,
    );
    let recorder = Recorder::seeded(true);
    recorder.stall();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        capacity,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
            .with_max_pending_documents(100)
    });
    if budget {
        deployment = deployment.with_budget(64 * KIB, usize::MAX);
    }
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let seen = (progress.changes_read(), progress.waiting());
            recorder.resume();
            seen
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends once the sink resumes");
    assert_eq!(deployment.recorder.state(), db.expected());
    watcher.await.unwrap()
}

/// Two indexes, every lane full: nobody is exempt, since no other index
/// would benefit; the reading stops near `max_waiting`.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_two_indexes_behind_full_lanes_stop_reading() {
    let (read, waiting) = whole_sink_down(2, 2, false).await;
    assert!(
        read <= 20 && waiting <= 12,
        "every index held: the reading stops at the non-exempt bounds, as before 6b-2 (read \
         {read}, {waiting} waiting)"
    );
}

/// Two indexes held by the budget behind a stalled sink: the same.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_two_indexes_held_by_the_budget_stop_reading() {
    let (read, waiting) = whole_sink_down(2, 1 << 20, true).await;
    assert!(
        read <= 20 && waiting <= 12,
        "every index held: the reading stops at the non-exempt bounds, as before 6b-2 (read \
         {read}, {waiting} waiting)"
    );
}

/// Three indexes held by the budget: the same.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_three_indexes_held_by_the_budget_stop_reading() {
    let (read, waiting) = whole_sink_down(3, 1 << 20, true).await;
    assert!(
        read <= 20 && waiting <= 12,
        "every index held: the reading stops at the non-exempt bounds, as before 6b-2 (read \
         {read}, {waiting} waiting)"
    );
}

/// One index behind a full lane: as before (no exemption with one index).
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_one_index_behind_a_full_lane_stops_reading() {
    let (read, waiting) = whole_sink_down(1, 2, false).await;
    assert!(
        read <= 12 && waiting <= 4,
        "one index: unchanged by the exemption (read {read}, {waiting} waiting)"
    );
}

/// One index held by the budget: as before.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_one_index_held_by_the_budget_stops_reading() {
    let (read, waiting) = whole_sink_down(1, 1 << 20, true).await;
    assert!(
        read <= 12 && waiting <= 4,
        "one index: unchanged by the exemption (read {read}, {waiting} waiting)"
    );
}

/// The latch outlives the hold: `orders` is held by its full lane and
/// exempt with a backlog; the lane recovers, but its next builds are slow
/// (two hang). Not held any more, still at its cap, it should pace the
/// reading ("an index that is merely slow, not held, still paces the
/// reading at its cap"); the latch keeps it exempt and the reader reads
/// every later `orders` change.
#[tokio::test(start_paused = true)]
async fn a_slow_index_once_held_paces_the_reading_again() {
    // orders 1, 3, .., 39 at once; orders 41, .., 139 from 10 s.
    let log: Vec<Change> = (0..20)
        .map(|n| Change::upsert(2 * n + 1))
        .chain((20..70).map(|n| Change::upsert(2 * n + 1)))
        .collect();
    let arrivals: Vec<Duration> = (0..20)
        .map(Duration::from_millis)
        .chain((0..50).map(|n| Duration::from_secs(10) + Duration::from_millis(10 * n)))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            held: Mutex::new([7, 9].into_iter().collect()),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        2,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 100,
            })
    });
    let progress = deployment.ingest.progress();
    let watcher = {
        let (recorder, builder) = (Arc::clone(&recorder), Arc::clone(&builder));
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let at_hold = waiting_of(&progress, "orders");
            recorder.refused_indexes.lock().unwrap().clear();
            tokio::time::sleep(Duration::from_secs(20)).await;
            let later = (progress.changes_read(), waiting_of(&progress, "orders"));
            builder.release(7);
            builder.release(9);
            (at_hold, later)
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let (at_hold, (read, later)) = watcher.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());
    assert!(
        later <= at_hold + 1,
        "orders, no longer held, read past its cap: {at_hold} waiting at the hold, {later} at 21 s \
         ({read} changes read)"
    );
}

/// M7: a live grant ends the hold. `orders` is held by the budget (its lane
/// refuses, so its share stays full) and exempt; let through, it drains to
/// nothing. Then two of its builds hang: not held, it must pace the reading
/// at its cap. A hold left set by the earlier budget refusal would latch it
/// exempt again at the cap and let the reader read on.
#[tokio::test(start_paused = true)]
async fn a_live_grant_ends_the_hold() {
    let log: Vec<Change> = (0..20)
        .map(|n| Change::upsert(2 * n + 1))
        .chain((50..100).map(|n| Change::upsert(2 * n + 1)))
        .collect();
    let arrivals: Vec<Duration> = (0..20)
        .map(|n| Duration::from_millis(10 * n))
        .chain((0..50).map(|n| Duration::from_secs(20) + Duration::from_millis(10 * n)))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            held: Mutex::new([101, 103].into_iter().collect()),
            padding: 8 * KIB,
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 100,
            })
    })
    .with_budget(64 * KIB, usize::MAX);
    let progress = deployment.ingest.progress();
    let watcher = {
        let (recorder, builder) = (Arc::clone(&recorder), Arc::clone(&builder));
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let at_hold = (progress.changes_read(), waiting_of(&progress, "orders"));
            recorder.refused_indexes.lock().unwrap().clear();
            tokio::time::sleep(Duration::from_secs(15)).await;
            let drained = waiting_of(&progress, "orders");
            tokio::time::sleep(Duration::from_secs(13)).await;
            let later = (progress.changes_read(), waiting_of(&progress, "orders"));
            builder.release(101);
            builder.release(103);
            (at_hold, drained, later)
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let (at_hold, drained, later) = watcher.await.unwrap();
    assert_eq!(deployment.recorder.state(), db.expected());
    assert!(
        at_hold.1 >= 4,
        "orders was exempt past its cap: {at_hold:?}"
    );
    assert_eq!(drained, 0, "orders drained once let through");
    assert!(
        later.1 <= 5,
        "orders, slow and not held, paces the reading at its cap: {later:?}"
    );
}

/// The exemption with three indexes (review of #452): `orders` and `items`
/// stuck behind refused lanes, `users` healthy. While `users` flows, the two
/// stuck ones are exempt and the reading goes on, `users` written as it is
/// read. Then `users` is held too: no index is let go, so the reading stops
/// and the backlogs stay as they are. Once every lane is let through the
/// reading goes on and everything lands, nothing lost.
#[tokio::test(start_paused = true)]
async fn the_exemption_lasts_while_some_index_flows_and_stops_when_none_does() {
    let changes: u64 = 600;
    let log: Vec<Change> = (0..changes as i64).map(Change::upsert).collect();
    let arrivals: Vec<Duration> = (0..changes)
        .map(|n| Duration::from_millis(20 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::over(&db, Behaviour::default(), 3);
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .extend(["orders".to_owned(), "items".to_owned()]);
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        2,
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
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let flowing = progress.changes_read();
            let users_written = recorder.state().keys().filter(|id| *id % 3 == 0).count();
            recorder
                .refused_indexes
                .lock()
                .unwrap()
                .insert("users".to_owned());
            tokio::time::sleep(Duration::from_secs(2)).await;
            let stopped = progress.changes_read();
            tokio::time::sleep(Duration::from_secs(3)).await;
            let still = progress.changes_read();
            recorder.refused_indexes.lock().unwrap().clear();
            (flowing, users_written, stopped, still)
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends once every lane is let through");
    let (flowing, users_written, stopped, still) = watcher.await.unwrap();
    assert!(
        flowing >= 190,
        "while users flows the stuck indexes are exempt: {flowing} of 200 read at 4 s"
    );
    assert!(
        users_written >= 60,
        "users is written as it is read: {users_written} of its documents at 4 s"
    );
    assert_eq!(
        stopped, still,
        "every index held: the reading stopped ({stopped} at 6 s, {still} at 9 s)"
    );
    assert_eq!(
        deployment.recorder.state(),
        db.expected(),
        "let through, everything lands"
    );
}
