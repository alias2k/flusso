//! The gate's rules, one test each (G1–G9 of the stage note), on paused
//! time: waiters are tasks of one current-thread runtime, so a waiter runs
//! only when the test yields.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use kernel::IndexName;

use super::*;
use crate::budget::{Charge, Reservation};

const KIB: usize = 1024;

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

/// `kib` of live documents of `name`, settled.
fn held(budget: &Arc<Budget>, name: &str, kib: usize) -> Vec<Charge> {
    budget
        .reserve(Work::Live, &[(index(name), kib * KIB)])
        .unwrap()
        .into_iter()
        .map(|part| part.settle(kib * KIB))
        .collect()
}

/// Let every runnable task run.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

/// A waiter of `work` that records its name when granted, then holds the
/// permit until `hold` passes.
fn waiter(
    gate: &Arc<Gate>,
    work: Work,
    name: &'static str,
    granted: &Arc<Mutex<Vec<&'static str>>>,
    hold: Duration,
) -> tokio::task::JoinHandle<()> {
    let gate = Arc::clone(gate);
    let granted = Arc::clone(granted);
    tokio::spawn(async move {
        let (permit, ()) = gate.acquire(work, None, |_| Ok(())).await;
        granted.lock().unwrap().push(name);
        tokio::time::sleep(hold).await;
        drop(permit);
    })
}

#[test]
fn choose_takes_live_first_but_every_fourth_contested_grant_is_backfill() {
    let gate = Gate::new(1);
    let mut kinds = Vec::new();
    for _ in 0..8 {
        let (work, permit) = gate.choose(true, true).unwrap();
        kinds.push(work);
        drop(permit);
    }
    assert_eq!(
        kinds,
        [
            Work::Live,
            Work::Live,
            Work::Live,
            Work::Backfill,
            Work::Live,
            Work::Live,
            Work::Live,
            Work::Backfill
        ]
    );
    assert_eq!(
        gate.choose(false, true).map(|(work, _)| work),
        Some(Work::Backfill),
        "uncontested: no count moves"
    );
}

#[test]
fn no_permit_beyond_the_count_and_a_dropped_one_comes_back() {
    let gate = Gate::new(2);
    let a = gate.choose(true, false).unwrap();
    let _b = gate.choose(true, false).unwrap();
    assert!(gate.choose(true, false).is_none());
    drop(a);
    assert!(gate.choose(true, false).is_some());
}

/// G2, G7: waiters of one kind are granted oldest first, each woken by the
/// permit the one before gives back.
#[tokio::test(start_paused = true)]
async fn a_freed_permit_goes_to_the_oldest_live_waiter() {
    let gate = Gate::new(1);
    let (_, held) = gate.choose(true, false).unwrap();
    let granted = Arc::new(Mutex::new(Vec::new()));
    let mut waiters = Vec::new();
    for name in ["first", "second", "third"] {
        waiters.push(waiter(
            &gate,
            Work::Live,
            name,
            &granted,
            Duration::from_millis(10),
        ));
        settle().await;
    }
    drop(held);
    for waiter in waiters {
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .expect("every waiter is granted")
            .unwrap();
    }
    assert_eq!(*granted.lock().unwrap(), ["first", "second", "third"]);
}

/// G4 with lines: while both lines have a waiter, every fourth grant goes to
/// the oldest backfill waiter.
#[tokio::test(start_paused = true)]
async fn every_fourth_contested_grant_goes_to_the_oldest_backfill_waiter() {
    let gate = Gate::new(1);
    let (_, held) = gate.choose(true, false).unwrap();
    let granted = Arc::new(Mutex::new(Vec::new()));
    let mut waiters = Vec::new();
    for name in ["l1", "l2", "l3", "l4", "l5", "l6"] {
        waiters.push(waiter(
            &gate,
            Work::Live,
            name,
            &granted,
            Duration::from_millis(1),
        ));
    }
    for name in ["b1", "b2"] {
        waiters.push(waiter(
            &gate,
            Work::Backfill,
            name,
            &granted,
            Duration::from_millis(1),
        ));
    }
    settle().await;
    drop(held);
    for waiter in waiters {
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(
        *granted.lock().unwrap(),
        ["l1", "l2", "l3", "b1", "l4", "l5", "l6", "b2"]
    );
}

/// G3, G5: the budget is asked at grant; a waiter it refuses keeps its place
/// and the permit goes on to the next that fits; after a release it is
/// granted before a waiter that joined later.
#[tokio::test(start_paused = true)]
async fn a_waiter_refused_by_the_budget_keeps_its_place_and_the_permit_goes_on() {
    let budget = Arc::new(Budget::new(64 * KIB));
    let users = held(&budget, "users", 60);
    let gate = Gate::new(1);
    let granted = Arc::new(Mutex::new(Vec::new()));
    let reserving = |name: &'static str, kib: usize| {
        let gate = Arc::clone(&gate);
        let budget = Arc::clone(&budget);
        let granted = Arc::clone(&granted);
        tokio::spawn(async move {
            let (permit, parts) = gate
                .acquire(Work::Live, Some(&budget), |behind| {
                    budget.reserve_behind(Work::Live, &[(index(name), kib * KIB)], behind)
                })
                .await;
            granted.lock().unwrap().push(name);
            tokio::time::sleep(Duration::from_millis(10)).await;
            drop(parts);
            drop(permit);
        })
    };
    let refused = reserving("users", 16);
    settle().await;
    let next = reserving("orders", 8);
    settle().await;
    assert_eq!(
        *granted.lock().unwrap(),
        ["orders"],
        "users is refused by the limit and keeps its place; orders, holding no live bytes, gets \
         the permit"
    );
    let later = reserving("items", 4);
    settle().await;
    drop(users);
    for task in [refused, next, later] {
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(*granted.lock().unwrap(), ["orders", "users", "items"]);
}

/// G6: behind an older waiter the limit refused, a younger one takes no room
/// by the limit, so it waits though it would fit; the older is granted first
/// once a release lets it.
#[tokio::test(start_paused = true)]
async fn a_younger_waiter_behind_a_limit_refusal_takes_no_room_by_the_limit() {
    let budget = Arc::new(Budget::new(64 * KIB));
    let users = held(&budget, "users", 40);
    let _orders = held(&budget, "orders", 8);
    let gate = Gate::new(2);
    let granted = Arc::new(Mutex::new(Vec::new()));
    let reserving = |name: &'static str, kib: usize| {
        let gate = Arc::clone(&gate);
        let budget = Arc::clone(&budget);
        let granted = Arc::clone(&granted);
        tokio::spawn(async move {
            let (_permit, _parts) = gate
                .acquire(Work::Live, Some(&budget), |behind| {
                    budget.reserve_behind(Work::Live, &[(index(name), kib * KIB)], behind)
                })
                .await;
            granted.lock().unwrap().push(name);
        })
    };
    let large = reserving("users", 24);
    settle().await;
    let small = reserving("orders", 8);
    settle().await;
    assert!(
        granted.lock().unwrap().is_empty(),
        "orders would fit by the limit (56 of 64 KiB) but waits behind users"
    );
    drop(users);
    for task in [large, small] {
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(*granted.lock().unwrap(), ["users", "orders"]);
}

/// G6: a refusal by an index's share holds no one back: a stuck index never
/// blocks another's line.
#[tokio::test(start_paused = true)]
async fn a_waiter_refused_by_its_share_holds_no_one_back() {
    let budget = Arc::new(Budget::new(64 * KIB).with_index_share(2, 0.5));
    let _users = held(&budget, "users", 30);
    let _orders = held(&budget, "orders", 8);
    let gate = Gate::new(2);
    let granted = Arc::new(Mutex::new(Vec::new()));
    let reserving = |name: &'static str, kib: usize| {
        let gate = Arc::clone(&gate);
        let budget = Arc::clone(&budget);
        let granted = Arc::clone(&granted);
        tokio::spawn(async move {
            let (_permit, _parts) = gate
                .acquire(Work::Live, Some(&budget), |behind| {
                    budget.reserve_behind(Work::Live, &[(index(name), kib * KIB)], behind)
                })
                .await;
            granted.lock().unwrap().push(name);
        })
    };
    let stuck = reserving("users", 8);
    settle().await;
    let healthy = reserving("orders", 8);
    settle().await;
    assert_eq!(
        *granted.lock().unwrap(),
        ["orders"],
        "users is past its share; orders fits by the limit and isn't held back"
    );
    stuck.abort();
    healthy.await.unwrap();
}

/// G8: a waiter dropped before its grant leaves its line, so the next one is
/// granted.
#[tokio::test(start_paused = true)]
async fn a_waiter_dropped_before_its_grant_gives_its_place_to_the_next() {
    let gate = Gate::new(1);
    let (_, held) = gate.choose(true, false).unwrap();
    let granted = Arc::new(Mutex::new(Vec::new()));
    let first = waiter(&gate, Work::Live, "first", &granted, Duration::ZERO);
    settle().await;
    let second = waiter(&gate, Work::Live, "second", &granted, Duration::ZERO);
    settle().await;
    first.abort();
    settle().await;
    drop(held);
    tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .expect("the dropped waiter's place went to the next")
        .unwrap();
    assert_eq!(*granted.lock().unwrap(), ["second"]);
}

/// G9: the one task asking synchronously gets no permit while an eligible
/// waiter is in a line, even with one free.
#[tokio::test(start_paused = true)]
async fn the_one_task_gets_no_permit_ahead_of_an_eligible_waiter() {
    let gate = Gate::new(1);
    let (_, held) = gate.choose(true, false).unwrap();
    let granted = Arc::new(Mutex::new(Vec::new()));
    let waiting = waiter(&gate, Work::Live, "waiter", &granted, Duration::ZERO);
    settle().await;
    drop(held);
    assert!(
        gate.choose(true, true).is_none(),
        "the waiter was there first"
    );
    waiting.await.unwrap();
    assert_eq!(*granted.lock().unwrap(), ["waiter"]);
    assert!(gate.choose(true, false).is_some(), "no one waits now");
}

/// G1, G7, G8: many waiters of both kinds, holding their permits for random
/// times, some dropped while they wait: never more permits out than the
/// gate has, and every waiter not dropped is granted.
#[tokio::test(start_paused = true)]
async fn many_waiters_never_hold_more_than_the_permits_and_every_one_is_granted() {
    let gate = Gate::new(3);
    let out = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    let mut dropped = Vec::new();
    for n in 0..60_u64 {
        let gate = Arc::clone(&gate);
        let (out, most) = (Arc::clone(&out), Arc::clone(&most));
        let work = if n % 3 == 0 {
            Work::Backfill
        } else {
            Work::Live
        };
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(n % 7)).await;
            let (permit, ()) = gate.acquire(work, None, |_| Ok(())).await;
            let now = out.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(1 + (n * 13) % 5)).await;
            out.fetch_sub(1, Ordering::SeqCst);
            drop(permit);
        });
        if n % 11 == 5 {
            dropped.push(task);
        } else {
            tasks.push(task);
        }
    }
    tokio::time::sleep(Duration::from_millis(3)).await;
    for task in &dropped {
        task.abort();
    }
    for task in tasks {
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("every waiter is granted")
            .unwrap();
    }
    assert!(most.load(Ordering::SeqCst) <= 3);
    assert_eq!(most.load(Ordering::SeqCst), 3, "the permits are all used");
}

/// A reservation the waiter's `reserve` makes is its, once: kept with the
/// permit, given back with it.
#[tokio::test(start_paused = true)]
async fn the_reservation_a_waiter_makes_is_handed_back_with_its_permit() {
    let budget = Arc::new(Budget::new(64 * KIB));
    let gate = Gate::new(1);
    let (permit, parts): (Permit, Vec<Reservation>) = gate
        .acquire(Work::Live, Some(&budget), |behind| {
            budget.reserve_behind(Work::Live, &[(index("users"), 8 * KIB)], behind)
        })
        .await;
    assert_eq!(budget.held().total, 8 * KIB);
    drop(parts);
    drop(permit);
    assert_eq!(budget.held().total, 0);
    assert!(gate.choose(true, false).is_some());
}
