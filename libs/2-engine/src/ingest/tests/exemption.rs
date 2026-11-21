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
        read <= 15 && waiting <= 8,
        "every index held: the reading stops at the non-exempt bounds, as before 6b-2, measured \
         plus one (read {read}, {waiting} waiting)"
    );
}

/// Two indexes held by the budget behind a stalled sink: the same.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_two_indexes_held_by_the_budget_stop_reading() {
    let (read, waiting) = whole_sink_down(2, 1 << 20, true).await;
    assert!(
        read <= 15 && waiting <= 9,
        "every index held: the reading stops at the non-exempt bounds, as before 6b-2, measured \
         plus one (read {read}, {waiting} waiting)"
    );
}

/// Three indexes held by the budget: the same.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_three_indexes_held_by_the_budget_stop_reading() {
    let (read, waiting) = whole_sink_down(3, 1 << 20, true).await;
    assert!(
        read <= 17 && waiting <= 11,
        "every index held: the reading stops at the non-exempt bounds, as before 6b-2, measured \
         plus one (read {read}, {waiting} waiting)"
    );
}

/// One index behind a full lane: as before (no exemption with one index).
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_one_index_behind_a_full_lane_stops_reading() {
    let (read, waiting) = whole_sink_down(1, 2, false).await;
    assert!(
        read <= 9 && waiting <= 5,
        "one index: unchanged by the exemption (read {read}, {waiting} waiting)"
    );
}

/// One index held by the budget: as before.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_one_index_held_by_the_budget_stops_reading() {
    let (read, waiting) = whole_sink_down(1, 1 << 20, true).await;
    assert!(
        read <= 11 && waiting <= 5,
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

/// A whole sink down over three indexes, `users` and `orders` changing one
/// document each every `every` from 0 s, `items` as `items` says; every
/// write stalls. Returns the changes read and the documents waiting at each
/// of `at`, then resumes the sink and checks everything lands.
async fn sink_down_beside(
    items: Items,
    every: Duration,
    pending: usize,
    budget: Option<(usize, f64)>,
    at: &[u64],
) -> Vec<(u64, usize)> {
    let changes: i64 = 600;
    let mut log: Vec<Change> = Vec::new();
    let mut arrivals: Vec<Duration> = Vec::new();
    for n in 0..changes {
        // users (n % 3 == 0) and orders (n % 3 == 1) only.
        let id = 3 * (n / 2) + n % 2;
        log.push(Change::upsert(id));
        arrivals.push(every * u32::try_from(n).unwrap());
    }
    match items {
        Items::Silent | Items::Backfilling => {}
        Items::OneChange => {
            log.insert(0, Change::upsert(2));
            arrivals.insert(0, Duration::ZERO);
        }
        Items::EverySecond => {
            for n in 0..40u32 {
                let at = Duration::from_secs(u64::from(n));
                let slot = arrivals.partition_point(|arrival| *arrival <= at);
                log.insert(slot, Change::upsert(3 * i64::from(n) + 2));
                arrivals.insert(slot, at);
            }
        }
    }
    let rows = if items == Items::Backfilling { 600 } else { 0 };
    let db = Database::new(rows, log);
    let builder = Builder::over(
        &db,
        Behaviour {
            padding: if budget.is_some() { 8 * KIB } else { 0 },
            ..Behaviour::default()
        },
        3,
    );
    let recorder = Recorder::seeded(items != Items::Backfilling);
    if items == Items::Backfilling {
        recorder
            .seeded_indexes
            .lock()
            .unwrap()
            .extend(["users".to_owned(), "orders".to_owned()]);
    }
    recorder.stall();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        if budget.is_some() { 1 << 20 } else { 2 },
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
            .with_max_pending_documents(pending)
    });
    if let Some((bytes, share)) = budget {
        deployment = deployment.with_budget_shared_at(bytes, share);
    }
    let progress = deployment.ingest.progress();
    let at = at.to_vec();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            let mut seen = Vec::new();
            let start = tokio::time::Instant::now();
            for second in at {
                tokio::time::sleep_until(start + Duration::from_secs(second)).await;
                seen.push((progress.changes_read(), progress.waiting()));
            }
            recorder.resume();
            seen
        })
    };
    tokio::time::timeout(LIMIT * 4, deployment.run_to_end(0))
        .await
        .expect("the run ends once the sink resumes");
    assert_eq!(deployment.recorder.state(), db.expected());
    watcher.await.unwrap()
}

/// What `items` does while the sink is down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Items {
    /// No change at all.
    Silent,
    /// One change, at the start.
    OneChange,
    /// One change a second.
    EverySecond,
    /// No change; its rows are backfilled.
    Backfilling,
}

/// (a) A whole sink down beside a silent index: `items` has no change and
/// nothing outstanding, so it is free, and the held `users` and `orders` read
/// on, exempt, to `max_pending_documents`: what one index stuck beside quiet
/// healthy ones does, which this cannot be told apart from. Bounded by the
/// cap.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_a_silent_index_lets_the_others_read_on_to_the_pending_cap() {
    let seen = sink_down_beside(Items::Silent, Duration::from_millis(10), 100, None, &[10]).await;
    let (read, waiting) = seen[0];
    assert!(
        read > 50 && waiting <= 100 + 2,
        "reads on to the cap and no further: {read} read, {waiting} waiting"
    );
}

/// (b) The same with one `items` change the sink never acknowledges: after
/// five seconds `items` is not free either, nobody is, and the reading
/// stops, having grown by about five seconds of changes at most.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_one_unacknowledged_change_stops_the_reading_after_five_seconds() {
    let seen = sink_down_beside(
        Items::OneChange,
        Duration::from_millis(50),
        1000,
        None,
        &[7, 20],
    )
    .await;
    let ((read, waiting), (later, _)) = (seen[0], seen[1]);
    assert_eq!(read, later, "the reading stopped: {seen:?}");
    assert!(
        read <= 5 * 20 + 12 && waiting <= 5 * 20 + 12,
        "at most five seconds of changes, twenty a second, past the bounds: {seen:?}"
    );
}

/// (c) A light index publishing one change a second to a dead sink is not
/// free once its first item has waited five seconds, long before the budget
/// would hold it (a quarter of 1 MiB each: its share takes about 28 seconds
/// to fill, while `users` and `orders` fill theirs at once): the reading
/// stops.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_a_light_index_is_not_free_once_its_item_waits_five_seconds() {
    let seen = sink_down_beside(
        Items::EverySecond,
        Duration::from_millis(20),
        100_000,
        Some((1024 * KIB, 0.25)),
        &[8, 20],
    )
    .await;
    assert_eq!(seen[0].0, seen[1].0, "the reading stopped by 8 s: {seen:?}");
}

/// (d) An index only backfilling to a dead sink is not free once its backfill
/// items have waited five seconds: the reading stops.
#[tokio::test(start_paused = true)]
async fn with_a_whole_sink_down_a_backfilling_index_is_not_free_once_its_items_wait_five_seconds() {
    let seen = sink_down_beside(
        Items::Backfilling,
        Duration::from_millis(20),
        100_000,
        Some((64 * KIB, 0.5)),
        &[8, 20],
    )
    .await;
    assert_eq!(seen[0].0, seen[1].0, "the reading stopped by 8 s: {seen:?}");
}

/// (e) One stuck index beside a quiet healthy one and a flowing one:
/// `orders` is exempt, `items` (quiet, nothing outstanding) and `users`
/// (acknowledged) are free, and `users`' changes are read and land.
#[tokio::test(start_paused = true)]
async fn one_stuck_index_beside_a_quiet_and_a_flowing_one_is_exempt() {
    let changes: i64 = 400;
    let log: Vec<Change> = (0..changes)
        .map(|n| Change::upsert(3 * (n / 2) + n % 2))
        .collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(20 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::over(&db, Behaviour::default(), 3);
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
                total: 8,
            })
    });
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(9)).await;
            let seen = (
                progress.changes_read(),
                recorder.state().keys().filter(|id| *id % 3 == 0).count(),
            );
            recorder.refused_indexes.lock().unwrap().clear();
            seen
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let (read, users) = watcher.await.unwrap();
    assert_eq!(
        read, changes as u64,
        "every change read beside the stuck orders"
    );
    assert_eq!(users, 200, "every users change landed");
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// (f) Self-correction: `orders` is stuck (its lane refuses, its budget
/// share fills: held); `users`, one change a second, is healthy but each
/// write takes eight seconds, so for three seconds of each it has an item
/// waiting longer than five: not free, `orders` freezes and the reading
/// stops. When `users`'
/// item is acknowledged it is free again, and the reader, parked, is woken
/// by the dispatcher's tick: the reading goes on, write after write.
#[tokio::test(start_paused = true)]
async fn a_slow_healthy_index_lets_the_reading_go_on_after_each_acknowledgement() {
    // orders (odd ids) every 40 ms, users (even) once a second, for 80 s.
    let mut timed: Vec<(Duration, i64)> = (0..2000i64)
        .map(|n| (Duration::from_millis(40 * n as u64), 2 * n + 1))
        .chain((0..80i64).map(|n| (Duration::from_secs(n as u64), 2 * n)))
        .collect();
    timed.sort();
    let log: Vec<Change> = timed.iter().map(|(_, id)| Change::upsert(*id)).collect();
    let arrivals: Vec<Duration> = timed.iter().map(|(at, _)| *at).collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            padding: 8 * KIB,
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    *recorder.write_delay.lock().unwrap() = Some(Duration::from_secs(8));
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
            .with_batch(batch(4))
            .with_builds(2)
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
    })
    .with_budget_shared_at(16 * 1024 * KIB, 0.05);
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            let mut seen = Vec::new();
            for _ in 0..12 {
                tokio::time::sleep(Duration::from_secs(5)).await;
                seen.push(progress.changes_read());
            }
            recorder.refused_indexes.lock().unwrap().clear();
            *recorder.write_delay.lock().unwrap() = None;
            seen
        })
    };
    tokio::time::timeout(LIMIT * 8, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let seen = watcher.await.unwrap();
    let grew = seen.windows(2).filter(|pair| pair[1] > pair[0]).count();
    let stood = seen.windows(2).filter(|pair| pair[1] == pair[0]).count();
    assert!(
        grew >= 3 && stood >= 1,
        "the reading stopped while users waited and went on after its acknowledgements: {seen:?}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// A hold ending wakes the reader (review round 2, item 5): `orders` stuck
/// and exempt, `users` stuck too, so the reading stops; `users` is let
/// through, and once its lane takes its items again (its first write after
/// the release) the reading goes on within half a second, before the
/// dispatcher's tick could have carried it.
#[tokio::test(start_paused = true)]
async fn the_reading_goes_on_within_half_a_second_of_a_hold_ending() {
    let changes: i64 = 600;
    let log: Vec<Change> = (0..changes).map(Change::upsert).collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(20 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(&db, Behaviour::default());
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .extend(["orders".to_owned(), "users".to_owned()]);
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
            tokio::time::sleep(Duration::from_secs(8)).await;
            let stopped = progress.changes_read();
            let written = recorder.landings.lock().unwrap().len();
            recorder.refused_indexes.lock().unwrap().remove("users");
            while recorder.landings.lock().unwrap().len() <= written {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let landed = tokio::time::Instant::now();
            while progress.changes_read() <= stopped {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let after = landed.elapsed();
            recorder.refused_indexes.lock().unwrap().clear();
            (stopped, after)
        })
    };
    tokio::time::timeout(LIMIT, deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let (stopped, after) = watcher.await.unwrap();
    assert!(
        stopped < changes as u64,
        "the reading had stopped: {stopped}"
    );
    assert!(
        after < Duration::from_millis(500),
        "the reading went on {after:?} after users' lane took its items again"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// Every line logged, to read the last of a kind (review round 3 of #452).
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn last_of(&self, needles: &[&str]) -> Option<String> {
        String::from_utf8_lossy(&self.0.lock().unwrap())
            .lines()
            .filter(|line| needles.iter().any(|needle| line.contains(needle)))
            .next_back()
            .map(str::to_owned)
    }
}

/// `orders` (odd ids) is refused for 4 s and becomes exempt, then reaches
/// `max_pending_documents` (6); both warnings log. It is let through at 4 s
/// and drains within the minute, so the opposite lines are throttled. The
/// stream then stays open and idle. Three minutes later the last line of each
/// pair must name the current state (§5): the tick checks the lines whether
/// or not anything is read.
#[tokio::test(start_paused = true)]
async fn on_an_idle_stream_the_last_lines_name_the_state_within_a_minute() {
    let lines = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let lines = lines.clone();
            move || lines.clone()
        })
        .finish();
    let _logging = tracing::subscriber::set_default(subscriber);
    let changes: i64 = 200;
    let log: Vec<Change> = (0..changes).map(Change::upsert).collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(10 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(&db, Behaviour::default());
    let recorder = Recorder::seeded(true);
    recorder
        .refused_indexes
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals).open(),
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
            .with_max_pending_documents(6)
            .with_stop(stop.clone())
    });
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        let lines = lines.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let before = (
                lines.last_of(&["exempt, so reading goes on", "no longer exempt"]),
                lines.last_of(&["reading stopped for every index", "reading on"]),
            );
            recorder.refused_indexes.lock().unwrap().clear();
            tokio::time::sleep(Duration::from_secs(180)).await;
            let after = (
                lines.last_of(&["exempt, so reading goes on", "no longer exempt"]),
                lines.last_of(&["reading stopped for every index", "reading on"]),
                progress.changes_read(),
                waiting_of(&progress, "orders"),
            );
            stop.request();
            (before, after)
        })
    };
    tokio::time::timeout(LIMIT * 4, deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    let (before, (exempt_line, cap_line, read, waiting)) = watcher.await.unwrap();
    assert!(
        before
            .0
            .as_deref()
            .is_some_and(|l| l.contains("exempt, so reading goes on"))
            && before
                .1
                .as_deref()
                .is_some_and(|l| l.contains("reading stopped for every index")),
        "setup: both warnings logged by 4 s: {before:?}"
    );
    assert_eq!(
        (read, waiting),
        (changes as u64, 0),
        "everything read, nothing waits"
    );
    let exempt_ok = exempt_line
        .as_deref()
        .is_some_and(|l| l.contains("no longer exempt"));
    let cap_ok = cap_line
        .as_deref()
        .is_some_and(|l| l.contains("reading on"));
    assert!(
        exempt_ok && cap_ok,
        "three minutes after orders drained and everything was read, the last lines name the \
         state before: exempt line ok {exempt_ok}: {exempt_line:?}; cap line ok {cap_ok}: {cap_line:?}"
    );
}

/// `orders` (odd ids) is refused for 3 s: exempt, logged. It recovers and
/// drains within the minute (the "no longer exempt" line throttled). At
/// 20 s a `users` build fails once: the ingest run fails and restarts. The
/// log state is the engine's, so the throttled line is still logged. Changes keep arriving every 100 ms to the end, so lead
/// checks keep coming (not the idle case). Two minutes in, the last line
/// should name the current state (not exempt).
#[tokio::test(start_paused = true)]
async fn across_an_ingest_restart_the_last_line_names_the_state_within_a_minute() {
    let lines = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let lines = lines.clone();
            move || lines.clone()
        })
        .finish();
    let _logging = tracing::subscriber::set_default(subscriber);
    let changes: i64 = 1400;
    let log: Vec<Change> = (0..changes).map(Change::upsert).collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(100 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([200])),
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
        Feed::new(&db).with_arrivals(arrivals).open(),
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
            .with_stop(stop.clone())
    });
    let progress = deployment.ingest.progress();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        let lines = lines.clone();
        let stop = stop.clone();
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let before = lines.last_of(&["exempt, so reading goes on", "no longer exempt"]);
            recorder.refused_indexes.lock().unwrap().clear();
            tokio::time::sleep(Duration::from_secs(117)).await;
            let after = lines.last_of(&["exempt, so reading goes on", "no longer exempt"]);
            let waiting = waiting_of(&progress, "orders");
            while recorder.state() != db.expected() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            stop.request();
            (before, after, waiting)
        })
    };
    let failed = tokio::time::timeout(LIMIT * 4, deployment.run_to_end(1))
        .await
        .expect("the run ends at the stop");
    let (before, after, waiting) = watcher.await.unwrap();
    assert!(
        before
            .as_deref()
            .is_some_and(|l| l.contains("exempt, so reading goes on")),
        "setup: exempt logged by 3 s: {before:?}"
    );
    assert_eq!(failed, 1, "setup: the ingest run failed once");
    assert!(
        waiting < 4,
        "setup: orders is below its cap at 2 min ({waiting} waiting)"
    );
    assert!(
        after
            .as_deref()
            .is_some_and(|l| l.contains("no longer exempt")),
        "two minutes in, under steady traffic, after an ingest restart, the last line still \
         names exempt: {after:?}"
    );
}

/// Review round 3 of #452: an item is outstanding until **every** lane lets
/// go of it, so a healthy sink acknowledging an index's items does not make
/// it free while another sink holds them. `audit` is dead; `orders` (every
/// 10 ms) fills its `audit` lane at once and is held; `users` (once a
/// second) is acknowledged by the healthy sink every second, its `audit`
/// lane far from full. After five seconds `users` is not free, nobody is,
/// and the reading stops.
#[tokio::test(start_paused = true)]
async fn an_index_one_dead_sink_holds_is_not_free_although_a_healthy_sink_acknowledges_it() {
    let audit = SinkName::try_new("audit").unwrap();
    let sinks = [sink_name(), audit.clone()];
    let indexes = [users(), orders()];
    let stream = Arc::new(ChannelStream::over(64, indexes.clone(), sinks.clone()));
    let mut log: Vec<Change> = Vec::new();
    let mut arrivals: Vec<Duration> = Vec::new();
    for n in 0..3000u32 {
        let at = Duration::from_millis(10 * u64::from(n));
        if n % 100 == 0 {
            log.push(Change::upsert(2 * i64::from(n / 100)));
            arrivals.push(at);
        }
        log.push(Change::upsert(2 * i64::from(n) + 1));
        arrivals.push(at);
    }
    let db = Database::new(0, log);
    let feed = Arc::new(Feed::new(&db).with_arrivals(arrivals));
    let progress = Arc::new(crate::Progress::of(indexes.clone(), sinks.clone()));
    let builder = Builder::two(&db, Behaviour::default());
    let ingest = IngestEngine::new(
        Arc::clone(&feed) as Arc<dyn ChangeCapture>,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        sinks.to_vec(),
        Arc::clone(&progress),
    )
    .with_batch(batch(1))
    .with_builds(2)
    .with_max_waiting(MaxWaiting {
        per_index: 4,
        total: 8,
    })
    .with_max_pending_documents(100_000);
    let healthy = Recorder::seeded(true);
    let dead = Recorder::seeded(true);
    dead.stall();
    let mut tasks = Vec::new();
    for (name, recorder) in [(sink_name(), &healthy), (audit, &dead)] {
        let mappings = indexes
            .iter()
            .map(|index| IndexMapping {
                index: index.clone(),
                ..mapping()
            })
            .collect();
        let engine = SinkEngine::new(
            name,
            Arc::clone(recorder) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            mappings,
            Arc::clone(&progress),
        );
        engine.stage(Continuity::Resumed).await.unwrap();
        tasks.push(tokio::spawn(async move {
            let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
            engine.run(&mut rx).await
        }));
    }
    let run = tokio::spawn(async move { ingest.run().await });
    tokio::time::sleep(Duration::from_secs(8)).await;
    let at_8 = progress.changes_read();
    let users_landed = healthy
        .written_ids()
        .into_iter()
        .flatten()
        .filter(|id| id % 2 == 0)
        .count();
    tokio::time::sleep(Duration::from_secs(12)).await;
    let at_20 = progress.changes_read();
    assert!(
        users_landed >= 5,
        "setup: the healthy sink acknowledged users every second ({users_landed} by 8 s)"
    );
    assert_eq!(
        at_8, at_20,
        "users waits on the dead sink: not free, so orders stops growing and the reading stops"
    );
    dead.resume();
    tokio::time::timeout(LIMIT * 4, run)
        .await
        .expect("the run ends once the dead sink resumes")
        .unwrap()
        .unwrap();
    let deadline = tokio::time::Instant::now() + LIMIT;
    while healthy.state() != db.expected() || dead.state() != db.expected() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "everything lands on both sinks"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for task in tasks {
        task.abort();
    }
}
