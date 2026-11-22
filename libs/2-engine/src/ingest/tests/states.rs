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
                view.held.is_none() && view.working && view.exemption == ExemptionState::None
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
    assert!(held.held_since.is_some() && !held.working);
    assert!(exempt.exemption_since.is_some());
    assert_eq!(users.exemption, ExemptionState::None, "users is not exempt");
    assert!(
        users.held.is_none() && users.working,
        "users works: {users:?}"
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
    assert!(held.held_since.is_some() && !held.working);
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
struct ReaderWaits(Mutex<Vec<(crate::ReaderBound, Duration)>>);

impl crate::Observer for ReaderWaits {
    fn on_reader_wait(&self, bound: crate::ReaderBound, waited: Duration) {
        self.0.lock().unwrap().push((bound, waited));
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
        waits
            .iter()
            .all(|(bound, _)| *bound == crate::ReaderBound::PerIndex),
        "{waits:?}"
    );
    assert!(
        waits
            .iter()
            .any(|(_, waited)| *waited >= Duration::from_millis(50)),
        "a wait lasts until a build takes: {waits:?}"
    );
}
