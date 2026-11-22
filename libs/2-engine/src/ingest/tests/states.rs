//! What the engine shows of each index (ADR 0012 stage 8a, slice 7): the
//! exemption from the tick, a hold by the budget or by a full lane, and a
//! walk, each written at its edge and cleared at the other. Every wait is
//! bounded.

use std::collections::BTreeSet;

use super::*;
use crate::{ExemptionState, HeldBy, IndexView, policy::MaxWaiting};

/// Waits, up to a minute, until `index`'s view satisfies `done`.
async fn until_view(
    states: &crate::IndexStates,
    index: &str,
    what: &str,
    done: impl Fn(&IndexView) -> bool,
) -> IndexView {
    let index = IndexName::try_new(index).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let view = states.view(&index).unwrap();
        if done(&view) {
            return view;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} never happened: {view:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// `orders` (odd ids) refused by the sink, its lane of two items parked:
/// the publish after them pends, so it is held by that lane and doesn't
/// work; held at its cap beside `users`, the tick shows it exempt. Let
/// through, nothing holds it, it works again, and it is no longer exempt.
#[tokio::test(start_paused = true)]
async fn a_lane_hold_and_the_exemption_are_shown_and_cleared() {
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
            .with_stop(stop.clone())
    });
    let states = deployment.ingest.index_states();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        let db = Arc::clone(&db);
        let stop = stop.clone();
        tokio::spawn(async move {
            let held =
                until_view(&states, "orders", "a lane hold", |view| view.held.is_some()).await;
            let exempt = until_view(&states, "orders", "the exemption", |view| {
                view.exemption != ExemptionState::None
            })
            .await;
            let users = states.view(&IndexName::try_new("users").unwrap()).unwrap();
            recorder.refused_indexes.lock().unwrap().clear();
            let cleared = until_view(&states, "orders", "the hold's end", |view| {
                view.held.is_none() && view.exemption == ExemptionState::None
            })
            .await;
            while recorder.state() != db.expected() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            stop.request();
            (held, exempt, users, cleared)
        })
    };
    tokio::time::timeout(Duration::from_secs(600), deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    let (held, exempt, users, cleared) = watcher.await.unwrap();
    assert_eq!(
        held.held,
        Some(HeldBy::Lanes(BTreeSet::from([sink_name()]))),
        "held by its full lane into the sink"
    );
    assert!(held.held_since.is_some());
    assert!(exempt.exemption_since.is_some());
    assert_eq!(users.exemption, ExemptionState::None, "users is not exempt");
    assert!(
        users.held.is_none() && users.stuck != Some(true),
        "users is neither held nor stuck: {users:?}"
    );
    assert_eq!(cleared.exemption_since, None);
    assert_eq!(cleared.held_since, None);
}

/// `orders` refused, its lane as large as a deployment's: its unacknowledged
/// batches fill its share of a small budget, so its live batch is refused:
/// held by the budget.
#[tokio::test(start_paused = true)]
async fn a_budget_hold_is_shown_and_cleared() {
    let changes: i64 = 200;
    let log: Vec<Change> = (0..changes).map(Change::upsert).collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(10 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
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
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_builds(2)
            .with_stop(stop.clone())
    })
    .with_budget_shared_at(256 * KIB, 0.25);
    let states = deployment.ingest.index_states();
    let watcher = {
        let recorder = Arc::clone(&recorder);
        let db = Arc::clone(&db);
        let stop = stop.clone();
        tokio::spawn(async move {
            let held = until_view(&states, "orders", "a budget hold", |view| {
                view.held == Some(HeldBy::Budget)
            })
            .await;
            recorder.refused_indexes.lock().unwrap().clear();
            let cleared = until_view(&states, "orders", "the hold's end", |view| {
                view.held.is_none()
            })
            .await;
            while recorder.state() != db.expected() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            stop.request();
            (held, cleared)
        })
    };
    tokio::time::timeout(Duration::from_secs(600), deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    let (held, cleared) = watcher.await.unwrap();
    assert!(held.held_since.is_some());
    assert_eq!(cleared.held_since, None);
}

/// A walk is shown with the sink it serves while it runs (each key page
/// takes a second), and no longer once it completed.
#[tokio::test(start_paused = true)]
async fn a_walk_is_shown_while_it_runs() {
    let db = Database::new(64, Vec::new());
    let builder = Builder::new(
        &db,
        Behaviour {
            keys_delay: Some(Duration::from_secs(1)),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(false);
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Arc::clone(&recorder))
            .ingest(|ingest| ingest.with_batch(batch(4)));
    let states = deployment.ingest.index_states();
    let watcher = tokio::spawn(async move {
        until_view(&states, "users", "the walk", |view| view.backfill.is_some()).await
    });
    tokio::time::timeout(Duration::from_secs(120), deployment.run_to_end(0))
        .await
        .expect("the run ends with the stream");
    let walking = watcher.await.unwrap();
    assert_eq!(walking.backfill, Some(BTreeSet::from([sink_name()])));
    let after = deployment
        .ingest
        .index_states()
        .view(&IndexName::try_new("users").unwrap())
        .unwrap();
    assert_eq!(after.backfill, None, "the walk completed");
}

/// A stop mid-walk drops the snapshot: its walk is no longer shown.
#[tokio::test(start_paused = true)]
async fn a_walk_stopped_is_no_longer_shown() {
    let db = Database::new(64, Vec::new());
    let builder = Builder::new(
        &db,
        Behaviour {
            keys_delay: Some(Duration::from_secs(1)),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(false);
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| ingest.with_batch(batch(4)).with_stop(stop.clone()));
    let states = deployment.ingest.index_states();
    let watcher = {
        let stop = stop.clone();
        tokio::spawn(async move {
            until_view(&states, "users", "the walk", |view| view.backfill.is_some()).await;
            stop.request();
        })
    };
    tokio::time::timeout(Duration::from_secs(120), deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    watcher.await.unwrap();
    let after = deployment
        .ingest
        .index_states()
        .view(&IndexName::try_new("users").unwrap())
        .unwrap();
    assert_eq!(after.backfill, None, "a stop drops the walk");
}

/// Records every wait of the reader at its lead.
#[derive(Debug, Default)]
pub(super) struct ReaderWaits(
    pub(super) Mutex<Vec<(crate::ReaderBound, Option<String>, Duration)>>,
);

impl crate::Observer for ReaderWaits {
    fn on_reader_wait(
        &self,
        bound: crate::ReaderBound,
        index: Option<&IndexName>,
        waited: Duration,
    ) {
        self.0
            .lock()
            .unwrap()
            .push((bound, index.map(|index| index.to_string()), waited));
    }
}

/// `flusso.reader.waits{bound}` (slice 7): builds of 100 ms behind twenty
/// changes read at once, two documents a cap: the reader waits at the
/// per-index bound, and each wait is reported with how long it lasted.
#[tokio::test(start_paused = true)]
async fn each_wait_of_the_reader_is_reported_with_its_bound() {
    let log: Vec<Change> = (0..20).map(Change::upsert).collect();
    let db = Database::new(0, log);
    let builder = Builder::new(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(100)),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    let waits = Arc::new(ReaderWaits::default());
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Arc::clone(&recorder)).ingest(
            |ingest| {
                ingest
                    .with_batch(batch(1))
                    .with_max_waiting(MaxWaiting {
                        per_index: 2,
                        total: 100,
                    })
                    .with_observer(Arc::clone(&waits) as Arc<dyn crate::Observer>)
            },
        );
    tokio::time::timeout(Duration::from_secs(120), deployment.run_to_end(0))
        .await
        .expect("the run ends with the stream");
    assert_eq!(recorder.state(), db.expected());
    let waits = waits.0.lock().unwrap().clone();
    assert!(!waits.is_empty(), "the reader waited");
    assert!(
        waits.iter().all(|(bound, index, _)| {
            *bound == crate::ReaderBound::PerIndex && index.as_deref() == Some("users")
        }),
        "each wait at users' cap, naming it: {waits:?}"
    );
    assert!(
        waits
            .iter()
            .any(|(_, _, waited)| *waited >= Duration::from_millis(50)),
        "a wait lasts until a build takes: {waits:?}"
    );
}

/// An engine run that fails while `orders` is exempt: once the run has
/// ended (the supervisor now waits its backoff, up to a minute, or the
/// source stays down), nothing is read, nothing waits on a cap, yet the
/// index is still shown exempt (and `flusso_index_exempt` stays 1, so
/// `FlussoIndexBacklog` keeps counting).
#[tokio::test(start_paused = true)]
async fn a_failed_run_leaves_no_exemption_shown() {
    let changes: i64 = 400;
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
    });
    let states = deployment.ingest.index_states();
    let sink = deployment.start_sink().await;
    let failer = {
        let states = Arc::clone(&states);
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            until_view(&states, "orders", "the exemption", |view| {
                view.exemption != ExemptionState::None
            })
            .await;
            // The source fails from the next change on: the run ends.
            builder
                .behaviour
                .resolve_fails
                .lock()
                .unwrap()
                .extend(0..changes);
        })
    };
    let ended = tokio::time::timeout(Duration::from_secs(600), deployment.ingest.run())
        .await
        .expect("the run ends");
    failer.await.unwrap();
    assert!(ended.is_err(), "the run failed");
    // The supervisor's backoff: nothing runs.
    tokio::time::sleep(Duration::from_secs(30)).await;
    let orders = states.view(&IndexName::try_new("orders").unwrap()).unwrap();
    sink.abort();
    assert_eq!(
        orders.exemption,
        ExemptionState::None,
        "no run, yet still shown: {orders:?}"
    );
}

/// Review of #454, item 1: a sustained live load a healthy sink lands
/// steadily (every write within 50 ms) in a small memory budget, so the
/// builder is held by the budget most of the time. Held is information; the
/// published "working" is about progress (6b-2's rule: nothing waited 5 s
/// for an acknowledgement with none acknowledged), so the saturated index is
/// never stuck. `users` (even ids) carries the load, `orders` is quiet, so the
/// engine keeps its record (two indexes).
#[tokio::test(start_paused = true)]
async fn a_healthy_sink_bound_burst_is_never_stuck() {
    let changes: i64 = 6000;
    let log: Vec<Change> = (0..changes)
        .map(|n| Change::upsert(2 * (n % 250)))
        .collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(2 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            padding: 8 * KIB,
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    *recorder.write_delay.lock().unwrap() = Some(Duration::from_millis(50));
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(16))
            .with_builds(2)
            .with_stop(stop.clone())
    })
    .with_shared_budget(256 * KIB, usize::MAX, None);
    let states = deployment.ingest.index_states();
    let sampler = {
        let recorder = Arc::clone(&recorder);
        let db = Arc::clone(&db);
        let stop = stop.clone();
        tokio::spawn(async move {
            let users = IndexName::try_new("users").unwrap();
            let mut samples = Vec::new();
            let mut landed = 0usize;
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let view = states.view(&users).unwrap();
                let now = recorder.landings.lock().unwrap().len();
                samples.push((view.stuck, view.held.clone(), now - landed));
                landed = now;
            }
            while recorder.state() != db.expected() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            stop.request();
            samples
        })
    };
    tokio::time::timeout(Duration::from_secs(600), deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    let samples = sampler.await.unwrap();
    assert!(
        samples.iter().all(|(_, _, landed)| *landed > 0),
        "the sink lands documents every second: {samples:?}"
    );
    assert!(
        samples.iter().any(|(_, held, _)| held.is_some()),
        "setup: the budget holds the builder at some samples: {samples:?}"
    );
    assert!(
        samples.iter().all(|(stuck, _, _)| *stuck == Some(false)),
        "a saturated index whose sink lands steadily is never stuck: {samples:?}"
    );
}

/// Review of #454, item 3, the budget's half: `orders` held by the budget
/// when the run fails; once it has ended nothing holds anything, so nothing
/// is shown held through the supervisor's backoff.
#[tokio::test(start_paused = true)]
async fn a_failed_run_leaves_no_budget_hold_shown() {
    let changes: i64 = 400;
    let log: Vec<Change> = (0..changes).map(Change::upsert).collect();
    let arrivals: Vec<Duration> = (0..changes as u64)
        .map(|n| Duration::from_millis(10 * n))
        .collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
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
        Feed::new(&db).with_arrivals(arrivals).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1 << 20,
    )
    .ingest(|ingest| ingest.with_batch(batch(1)).with_builds(2))
    .with_budget_shared_at(256 * KIB, 0.25);
    let states = deployment.ingest.index_states();
    let sink = deployment.start_sink().await;
    let failer = {
        let states = Arc::clone(&states);
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            until_view(&states, "orders", "a budget hold", |view| {
                view.held == Some(HeldBy::Budget)
            })
            .await;
            builder
                .behaviour
                .resolve_fails
                .lock()
                .unwrap()
                .extend(0..changes);
        })
    };
    let ended = tokio::time::timeout(Duration::from_secs(600), deployment.ingest.run())
        .await
        .expect("the run ends");
    failer.await.unwrap();
    assert!(ended.is_err(), "the run failed");
    tokio::time::sleep(Duration::from_secs(30)).await;
    let orders = states.view(&IndexName::try_new("orders").unwrap()).unwrap();
    sink.abort();
    assert_eq!(
        (orders.held.clone(), orders.held_since),
        (None, None),
        "no run, nothing held: {orders:?}"
    );
}

/// A run that ends cleanly at a stop clears what only a run decides: an
/// index shown exempt (one index: no tick writes it back) is shown not
/// exempt once the run has ended.
#[tokio::test(start_paused = true)]
async fn a_stopped_run_leaves_no_exemption_shown() {
    let db = Database::new(0, Vec::new());
    let builder = Builder::new(&db, Behaviour::default());
    let recorder = Recorder::seeded(true);
    let stop = Stop::new();
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| ingest.with_stop(stop.clone()));
    let states = deployment.ingest.index_states();
    let users = IndexName::try_new("users").unwrap();
    let stopper = {
        let states = Arc::clone(&states);
        let users = users.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            states.set_exemption(&users, ExemptionState::Exempt);
            stop.request();
        })
    };
    tokio::time::timeout(Duration::from_secs(60), deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    stopper.await.unwrap();
    assert_eq!(states.view(&users).unwrap().exemption, ExemptionState::None);
}

/// Review of #454, item 7: a snapshot's end blocked on a full lane shows the
/// lane's hold too, with one index. One backfill batch fills the stalled
/// sink's lane of one item; the `SnapshotComplete` after it waits.
#[tokio::test(start_paused = true)]
async fn a_snapshot_end_blocked_on_a_full_lane_shows_the_lane_hold() {
    let db = Database::new(2, Vec::new());
    let builder = Builder::new(&db, Behaviour::default());
    let recorder = Recorder::seeded(false);
    recorder.stall();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1,
    )
    .ingest(|ingest| ingest.with_batch(batch(4)));
    let states = deployment.ingest.index_states();
    let watcher = {
        let states = Arc::clone(&states);
        let recorder = Arc::clone(&recorder);
        tokio::spawn(async move {
            let held = until_view(&states, "users", "the snapshot end's lane hold", |view| {
                view.held.is_some()
            })
            .await;
            recorder.resume();
            held
        })
    };
    tokio::time::timeout(Duration::from_secs(120), deployment.run_to_end(0))
        .await
        .expect("the run ends with the stream");
    let held = watcher.await.unwrap();
    assert_eq!(
        held.held,
        Some(HeldBy::Lanes(BTreeSet::from([sink_name()])))
    );
    assert_eq!(recorder.state(), db.expected());
}

/// Review round 2 of #454: what the backlog age does NOT show. One index
/// whose healthy sink lands fewer documents a second than change: flusso
/// falls further behind the source every second, but the reader stops at
/// `max_waiting_per_index` for an index that isn't exempt, so its oldest
/// unbuilt change is never older than about the cap's worth of builds (4
/// documents at one a 400 ms write: under two seconds). The backlog age
/// (measured as `daemon::Stalls` does: from the first 1 s sample whose read
/// count covers the oldest unbuilt read number) stays that small; how far
/// behind the source an index is shows as its lanes' WAL behind and the
/// slot's lag, not here. Kept true so nobody reads it as a source lag.
#[tokio::test(start_paused = true)]
async fn an_index_falling_behind_its_source_keeps_a_backlog_within_its_read_ahead() {
    let changes: i64 = 3000;
    let log: Vec<Change> = (0..changes).map(Change::upsert).collect();
    let every = Duration::from_millis(50);
    let arrivals: Vec<Duration> = (0..changes as u64).map(|n| every * n as u32).collect();
    let db = Database::new(0, log);
    let builder = Builder::new(&db, Behaviour::default());
    let recorder = Recorder::seeded(true);
    *recorder.write_delay.lock().unwrap() = Some(Duration::from_millis(400));
    let stop = Stop::new();
    let mut deployment = Deployment::with_lanes(
        Feed::new(&db).with_arrivals(arrivals).open(),
        Arc::clone(&builder),
        Arc::clone(&recorder),
        1,
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(1))
            .with_max_waiting(MaxWaiting {
                per_index: 4,
                total: 8,
            })
            .with_stop(stop.clone())
    });
    let progress = deployment.ingest.progress();
    let users = IndexName::try_new("users").unwrap();
    let sampler = {
        let stop = stop.clone();
        tokio::spawn(async move {
            let opened = tokio::time::Instant::now();
            let mut reads: Vec<(u64, tokio::time::Instant)> = Vec::new();
            let mut seen = Vec::new();
            for _ in 0..150 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let now = tokio::time::Instant::now();
                let read = progress.changes_read();
                reads.push((read, now));
                let age = progress.oldest_unbuilt(&users).map_or(0, |oldest| {
                    let at = reads.partition_point(|(read, _)| *read < oldest);
                    reads.get(at).map_or(0, |(_, when)| (now - *when).as_secs())
                });
                // The newest change read arrived `read - 1` steps after the
                // stream opened: how far behind the source flusso is.
                let behind = (now - opened)
                    .saturating_sub(every * read.saturating_sub(1) as u32)
                    .as_secs();
                seen.push((behind, age));
            }
            stop.request();
            seen
        })
    };
    tokio::time::timeout(Duration::from_secs(900), deployment.run_to_end(0))
        .await
        .expect("the run ends at the stop");
    let seen = sampler.await.unwrap();
    let (behind, _) = *seen.last().unwrap();
    assert!(
        behind > 60,
        "setup: flusso falls behind its source: {seen:?}"
    );
    let most = seen.iter().map(|(_, age)| *age).max().unwrap_or(0);
    assert!(
        most <= 3,
        "the backlog stays within the read-ahead (4 documents at 400 ms each), {behind} s \
         behind the source: {most} s at most"
    );
}

/// Review round 2 of #454, item 1: which index stops the reader. Two indexes,
/// every build 100 ms, one at a time, two documents a cap: the reader waits
/// at one index's cap, each per-index wait names its index, and while it
/// waits `pacing_reader` is true for that index alone; once the stream is
/// read and built, neither paces it.
#[tokio::test(start_paused = true)]
async fn the_index_that_stops_the_reader_is_named_and_shown_while_it_does() {
    let log: Vec<Change> = (0..40).map(Change::upsert).collect();
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            delay: Some(Duration::from_millis(100)),
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(true);
    let waits = Arc::new(ReaderWaits::default());
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Arc::clone(&recorder)).ingest(
            |ingest| {
                ingest
                    .with_batch(batch(1))
                    .with_builds(1)
                    .with_max_waiting(MaxWaiting {
                        per_index: 2,
                        total: 100,
                    })
                    .with_observer(Arc::clone(&waits) as Arc<dyn crate::Observer>)
            },
        );
    let states = deployment.ingest.index_states();
    let watcher = {
        let states = Arc::clone(&states);
        tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
            loop {
                let shown: Vec<(String, bool)> = ["orders", "users"]
                    .into_iter()
                    .map(|name| {
                        let view = states.view(&IndexName::try_new(name).unwrap()).unwrap();
                        (name.to_owned(), view.pacing_reader)
                    })
                    .collect();
                if shown.iter().any(|(_, pacing)| *pacing) {
                    return shown;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the reader never waited"
                );
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
    };
    tokio::time::timeout(Duration::from_secs(120), deployment.run_to_end(0))
        .await
        .expect("the run ends with the stream");
    let shown = watcher.await.unwrap();
    assert_eq!(recorder.state(), db.expected());
    let pacing: Vec<&str> = shown
        .iter()
        .filter(|(_, pacing)| *pacing)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        pacing.len(),
        1,
        "one index paces the reader at a time: {shown:?}"
    );
    let waits = waits.0.lock().unwrap().clone();
    assert!(!waits.is_empty(), "the reader waited");
    assert!(
        waits
            .iter()
            .all(|(bound, index, _)| *bound != crate::ReaderBound::PerIndex || index.is_some()),
        "a per-index wait always names its index: {waits:?}"
    );
    for name in ["orders", "users"] {
        assert!(
            !states
                .view(&IndexName::try_new(name).unwrap())
                .unwrap()
                .pacing_reader,
            "{name}: nothing paces the reader once it ended"
        );
    }
}
