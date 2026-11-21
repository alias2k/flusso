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

fn unlimited() -> Arc<Budget> {
    Arc::new(Budget::unlimited())
}

/// A permit taken through the lines as the builder takes one (G9): join,
/// ask once, leave.
fn take(gate: &Arc<Gate>, work: Work) -> Option<Permit> {
    let mut waiter = gate.join(work, Vec::new());
    match waiter.try_grant() {
        Asked::Granted(permit, _) => Some(permit),
        _ => None,
    }
}

/// A permit taken with both kinds ready, the builder in both lines at once
/// (G4, G9): the kind granted.
fn take_either(gate: &Arc<Gate>) -> Option<(Work, Permit)> {
    let mut live = gate.join(Work::Live, Vec::new());
    let mut backfill = gate.join(Work::Backfill, Vec::new());
    if let Asked::Granted(permit, _) = live.try_grant() {
        return Some((Work::Live, permit));
    }
    match backfill.try_grant() {
        Asked::Granted(permit, _) => Some((Work::Backfill, permit)),
        _ => None,
    }
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
        let (permit, _) = gate.acquire(work, &[]).await;
        granted.lock().unwrap().push(name);
        tokio::time::sleep(hold).await;
        drop(permit);
    })
}

#[test]
fn both_kinds_ready_take_live_first_but_every_fourth_contested_grant_is_backfill() {
    let gate = Gate::new(1, unlimited());
    let mut kinds = Vec::new();
    for _ in 0..8 {
        let (work, permit) = take_either(&gate).unwrap();
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
        take(&gate, Work::Backfill).map(|_| Work::Backfill),
        Some(Work::Backfill),
        "uncontested: no count moves"
    );
}

#[test]
fn no_permit_beyond_the_count_and_a_dropped_one_comes_back() {
    let gate = Gate::new(2, unlimited());
    let a = take(&gate, Work::Live).unwrap();
    let _b = take(&gate, Work::Live).unwrap();
    assert!(take(&gate, Work::Live).is_none());
    drop(a);
    assert!(take(&gate, Work::Live).is_some());
}

/// G2, G7: waiters of one kind are granted oldest first, each woken by the
/// permit the one before gives back.
#[tokio::test(start_paused = true)]
async fn a_freed_permit_goes_to_the_oldest_live_waiter() {
    let gate = Gate::new(1, unlimited());
    let held = take(&gate, Work::Live).unwrap();
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
    let gate = Gate::new(1, unlimited());
    let held = take(&gate, Work::Live).unwrap();
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
    let gate = Gate::new(1, Arc::clone(&budget));
    let granted = Arc::new(Mutex::new(Vec::new()));
    let reserving = |name: &'static str, kib: usize| {
        let gate = Arc::clone(&gate);
        let granted = Arc::clone(&granted);
        tokio::spawn(async move {
            let (permit, parts) = gate.acquire(Work::Live, &[(index(name), kib * KIB)]).await;
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
    let gate = Gate::new(2, Arc::clone(&budget));
    let granted = Arc::new(Mutex::new(Vec::new()));
    let reserving = |name: &'static str, kib: usize| {
        let gate = Arc::clone(&gate);
        let granted = Arc::clone(&granted);
        tokio::spawn(async move {
            let (_permit, _parts) = gate.acquire(Work::Live, &[(index(name), kib * KIB)]).await;
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
    let gate = Gate::new(2, Arc::clone(&budget));
    let granted = Arc::new(Mutex::new(Vec::new()));
    let reserving = |name: &'static str, kib: usize| {
        let gate = Arc::clone(&gate);
        let granted = Arc::clone(&granted);
        tokio::spawn(async move {
            let (_permit, _parts) = gate.acquire(Work::Live, &[(index(name), kib * KIB)]).await;
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
    tokio::time::timeout(Duration::from_secs(5), healthy)
        .await
        .unwrap()
        .unwrap();
}

/// G8: a waiter dropped before its grant leaves its line, so the next one is
/// granted.
#[tokio::test(start_paused = true)]
async fn a_waiter_dropped_before_its_grant_gives_its_place_to_the_next() {
    let gate = Gate::new(1, unlimited());
    let held = take(&gate, Work::Live).unwrap();
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

/// G2 and G9: the builder, joining a line for its grant pass, gets no permit
/// ahead of an eligible waiter that joined first, even with one free.
#[tokio::test(start_paused = true)]
async fn a_grant_pass_gets_no_permit_ahead_of_an_eligible_waiter() {
    let gate = Gate::new(1, unlimited());
    let held = take(&gate, Work::Live).unwrap();
    let granted = Arc::new(Mutex::new(Vec::new()));
    let waiting = waiter(&gate, Work::Live, "waiter", &granted, Duration::ZERO);
    settle().await;
    drop(held);
    assert!(take_either(&gate).is_none(), "the waiter was there first");
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*granted.lock().unwrap(), ["waiter"]);
    assert!(take(&gate, Work::Live).is_some(), "no one waits now");
}

/// G1, G7, G8: many waiters of both kinds, holding their permits for random
/// times, some dropped while they wait: never more permits out than the
/// gate has, and every waiter not dropped is granted.
#[tokio::test(start_paused = true)]
async fn many_waiters_never_hold_more_than_the_permits_and_every_one_is_granted() {
    let gate = Gate::new(3, unlimited());
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
            let (permit, _) = gate.acquire(work, &[]).await;
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

/// A reservation a waiter's grant makes is its, once: kept with the
/// permit, given back with it.
#[tokio::test(start_paused = true)]
async fn the_reservation_a_waiter_makes_is_handed_back_with_its_permit() {
    let budget = Arc::new(Budget::new(64 * KIB));
    let gate = Gate::new(1, Arc::clone(&budget));
    let (permit, parts): (Permit, Vec<Reservation>) = tokio::time::timeout(
        Duration::from_secs(5),
        gate.acquire(Work::Live, &[(index("users"), 8 * KIB)]),
    )
    .await
    .expect("users fits");
    assert_eq!(budget.held().total, 8 * KIB);
    drop(parts);
    drop(permit);
    assert_eq!(budget.held().total, 0);
    assert!(take(&gate, Work::Live).is_some());
}

/// A waiter reserving `kib` of live documents of `name` from the gate's
/// budget, recording its name when granted.
fn budgeted(
    gate: &Arc<Gate>,
    name: &'static str,
    kib: usize,
    granted: &Arc<Mutex<Vec<&'static str>>>,
) -> tokio::task::JoinHandle<()> {
    let (gate, granted) = (Arc::clone(gate), Arc::clone(granted));
    tokio::spawn(async move {
        let (_permit, _parts) = gate.acquire(Work::Live, &[(index(name), kib * KIB)]).await;
        granted.lock().unwrap().push(name);
    })
}

/// G6 and G8 together: a waiter refused only for being behind an older one
/// is eligible again once that one leaves its line, though no budget release
/// came.
#[tokio::test(start_paused = true)]
async fn a_waiter_refused_for_being_behind_is_eligible_again_when_the_one_ahead_leaves() {
    let budget = Arc::new(Budget::new(64 * KIB));
    let _users = held(&budget, "users", 40);
    let _orders = held(&budget, "orders", 8);
    let gate = Gate::new(2, Arc::clone(&budget));
    let granted = Arc::new(Mutex::new(Vec::new()));
    let large = budgeted(&gate, "users", 24, &granted);
    settle().await;
    let small = budgeted(&gate, "orders", 8, &granted);
    settle().await;
    assert!(
        granted.lock().unwrap().is_empty(),
        "orders waits behind users"
    );
    large.abort();
    tokio::time::timeout(Duration::from_secs(3600), small)
        .await
        .expect("orders fits (56 of 64 KiB) once users left the line")
        .unwrap();
    assert_eq!(*granted.lock().unwrap(), ["orders"]);
}

/// G4: a grant with only one kind waiting doesn't move the count, so the
/// first contested grants after it are live, live, live, then backfill.
#[tokio::test(start_paused = true)]
async fn an_uncontested_grant_does_not_move_the_count() {
    let gate = Gate::new(1, unlimited());
    let held = take(&gate, Work::Live).unwrap();
    let granted = Arc::new(Mutex::new(Vec::new()));
    let mut waiters = vec![
        waiter(&gate, Work::Live, "a1", &granted, Duration::from_millis(10)),
        waiter(&gate, Work::Live, "a2", &granted, Duration::from_millis(10)),
    ];
    settle().await;
    drop(held);
    // a1 and a2 are granted alone (no backfill waits): uncontested.
    tokio::time::sleep(Duration::from_millis(15)).await;
    assert_eq!(*granted.lock().unwrap(), ["a1", "a2"]);
    for name in ["l1", "l2", "l3", "l4"] {
        waiters.push(waiter(
            &gate,
            Work::Live,
            name,
            &granted,
            Duration::from_millis(1),
        ));
    }
    waiters.push(waiter(
        &gate,
        Work::Backfill,
        "b1",
        &granted,
        Duration::from_millis(1),
    ));
    for waiter in waiters {
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(
        *granted.lock().unwrap(),
        ["a1", "a2", "l1", "l2", "l3", "b1", "l4"]
    );
}

/// G4 and G5: the backfill waiter whose turn it is is refused by the budget;
/// the grants after it, with no eligible backfill waiter, are uncontested.
/// The refusal doesn't move the count, so once a release makes it eligible
/// it takes the next contested grant. The live waiters reserve nothing, so no
/// grant changes the budget and the refusal stands until the release (G3).
#[tokio::test(start_paused = true)]
async fn a_refusal_does_not_move_the_count() {
    let budget = Arc::new(Budget::new(64 * KIB));
    let backfill_held = budget
        .reserve(Work::Backfill, &[(index("other"), 32 * KIB)])
        .unwrap();
    let gate = Gate::new(1, Arc::clone(&budget));
    let held = take(&gate, Work::Live).unwrap();
    let granted = Arc::new(Mutex::new(Vec::new()));
    let live = |name: &'static str| {
        let (gate, granted) = (Arc::clone(&gate), Arc::clone(&granted));
        tokio::spawn(async move {
            let (permit, parts) = gate.acquire(Work::Live, &[]).await;
            granted.lock().unwrap().push(name);
            tokio::time::sleep(Duration::from_millis(1)).await;
            drop((permit, parts));
        })
    };
    let mut tasks: Vec<_> = ["l1", "l2", "l3", "l4", "l5", "l6"]
        .into_iter()
        .map(live)
        .collect();
    tasks.push({
        let (gate, granted) = (Arc::clone(&gate), Arc::clone(&granted));
        tokio::spawn(async move {
            let (_permit, _parts) = gate.acquire(Work::Backfill, &[(index("b"), 8 * KIB)]).await;
            granted.lock().unwrap().push("b1");
        })
    });
    settle().await;
    drop(held);
    // l1-l3 contested, b1 refused at its turn (backfill holds half), l4-l6
    // uncontested.
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        *granted.lock().unwrap(),
        ["l1", "l2", "l3", "l4", "l5", "l6"]
    );
    // Hold the permit while l7 and l8 join and the release makes b1 eligible
    // (an uncontested grant: only live asked).
    let again = take(&gate, Work::Live).unwrap();
    tasks.push(live("l7"));
    tasks.push(live("l8"));
    settle().await;
    drop(backfill_held);
    settle().await;
    drop(again);
    for task in tasks {
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(
        *granted.lock().unwrap(),
        ["l1", "l2", "l3", "l4", "l5", "l6", "b1", "l7", "l8"],
        "the count stood at three contested grants: the fourth is b1's"
    );
}

/// G8: the waiter a free permit is offered to is dropped before it runs;
/// the next one, which saw it ahead and went back to sleep, is woken by the
/// drop.
#[tokio::test(start_paused = true)]
async fn a_selected_waiter_dropped_before_it_runs_wakes_the_next() {
    let gate = Gate::new(1, unlimited());
    let held = take(&gate, Work::Live).unwrap();
    let mut first = Box::pin(gate.acquire(Work::Live, &[]));
    assert!(futures::poll!(first.as_mut()).is_pending());
    let granted = Arc::new(Mutex::new(Vec::new()));
    let second = waiter(&gate, Work::Live, "second", &granted, Duration::ZERO);
    settle().await;
    drop(held);
    settle().await;
    assert!(
        granted.lock().unwrap().is_empty(),
        "first is offered the permit"
    );
    drop(first);
    tokio::time::timeout(Duration::from_secs(5), second)
        .await
        .expect("the drop woke the next waiter")
        .unwrap();
}

/// G7: two permits free, the younger waiter woke first and found the older
/// one ahead; the older one's grant wakes it, so it takes the other permit.
#[tokio::test(start_paused = true)]
async fn a_grant_wakes_the_waiter_behind_for_the_permit_left() {
    let gate = Gate::new(2, unlimited());
    let one = take(&gate, Work::Live).unwrap();
    let two = take(&gate, Work::Live).unwrap();
    let mut first = Box::pin(gate.acquire(Work::Live, &[]));
    assert!(futures::poll!(first.as_mut()).is_pending());
    let granted = Arc::new(Mutex::new(Vec::new()));
    let second = waiter(
        &gate,
        Work::Live,
        "second",
        &granted,
        Duration::from_secs(60),
    );
    settle().await;
    drop((one, two));
    settle().await;
    assert!(granted.lock().unwrap().is_empty(), "second saw first ahead");
    let std::task::Poll::Ready((_permit, _)) = futures::poll!(first.as_mut()) else {
        panic!("first is granted");
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        while granted.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the grant woke second for the permit left");
    second.abort();
}

/// G5: the waiter offered the permit is refused by its share; the waiter
/// asleep behind it, which fits, is woken by the refusal.
#[tokio::test(start_paused = true)]
async fn a_refusal_wakes_the_waiter_behind() {
    let budget = Arc::new(Budget::new(64 * KIB).with_index_share(2, 0.5));
    let _users = held(&budget, "users", 30);
    let gate = Gate::new(1, Arc::clone(&budget));
    let held_permit = take(&gate, Work::Live).unwrap();
    let users_batch = [(index("users"), 8 * KIB)];
    let mut refused = Box::pin(gate.acquire(Work::Live, &users_batch));
    assert!(futures::poll!(refused.as_mut()).is_pending());
    let granted = Arc::new(Mutex::new(Vec::new()));
    let behind = budgeted(&gate, "orders", 8, &granted);
    settle().await;
    drop(held_permit);
    settle().await;
    assert!(
        granted.lock().unwrap().is_empty(),
        "users is offered the permit first"
    );
    assert!(
        futures::poll!(refused.as_mut()).is_pending(),
        "users is refused by its share"
    );
    tokio::time::timeout(Duration::from_secs(5), behind)
        .await
        .expect("the refusal woke orders")
        .unwrap();
    assert_eq!(*granted.lock().unwrap(), ["orders"]);
}

/// The budget of the stale-mark case: 64 KiB, 32 KiB per index; `users`
/// holds an unsettled 20 KiB, `x` 38 and `orders` 2.
fn stale_mark_budget() -> (Arc<Budget>, Vec<Reservation>, Vec<Charge>, Vec<Charge>) {
    let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
    let users = budget
        .reserve(Work::Live, &[(index("users"), 20 * KIB)])
        .unwrap();
    let x = held(&budget, "x", 38);
    let orders = held(&budget, "orders", 2);
    (budget, users, x, orders)
}

/// G6 after a release: the release wakes `users` (refused by the limit) and
/// `orders` (refused behind it), and `orders` runs first. It must not take
/// `users`' refusal from before the release as standing: asked again,
/// `users` is refused by its share, which holds no one back.
#[tokio::test(start_paused = true)]
async fn a_waiter_behind_reads_the_older_ones_standing_after_a_release_not_its_old_refusal() {
    let (budget, users, x, _orders) = stale_mark_budget();
    let gate = Gate::new(1, Arc::clone(&budget));
    let users_batch = [(index("users"), 8 * KIB)];
    let mut older = Box::pin(gate.acquire(Work::Live, &users_batch));
    assert!(
        futures::poll!(older.as_mut()).is_pending(),
        "users is refused by the limit (68 of 64 KiB)"
    );
    let orders_batch = [(index("orders"), 2 * KIB)];
    let mut younger = Box::pin(gate.acquire(Work::Live, &orders_batch));
    assert!(
        futures::poll!(younger.as_mut()).is_pending(),
        "orders waits behind users"
    );
    let _users: Vec<Charge> = users
        .into_iter()
        .map(|part| part.settle(30 * KIB))
        .collect();
    drop(x);
    assert!(
        futures::poll!(younger.as_mut()).is_pending(),
        "users is offered the permit first"
    );
    assert!(
        futures::poll!(older.as_mut()).is_pending(),
        "users is refused by its share (38 of 32 KiB)"
    );
    assert!(
        futures::poll!(younger.as_mut()).is_ready(),
        "orders fits (34 of 64 KiB): a share refusal holds no one back"
    );
}

/// The same case with both waiters as tasks, woken in the order the runtime
/// picks.
#[tokio::test(start_paused = true)]
async fn a_waiter_behind_is_granted_when_the_older_one_is_refused_by_its_share_after_a_release() {
    let (budget, users, x, _orders) = stale_mark_budget();
    let gate = Gate::new(1, Arc::clone(&budget));
    let granted = Arc::new(Mutex::new(Vec::new()));
    let older = budgeted(&gate, "users", 8, &granted);
    settle().await;
    let younger = {
        let (gate, granted) = (Arc::clone(&gate), Arc::clone(&granted));
        tokio::spawn(async move {
            let (_permit, _parts) = gate
                .acquire(Work::Live, &[(index("orders"), 2 * KIB)])
                .await;
            granted.lock().unwrap().push("orders");
        })
    };
    settle().await;
    assert!(
        granted.lock().unwrap().is_empty(),
        "orders waits behind users"
    );
    let _users: Vec<Charge> = users
        .into_iter()
        .map(|part| part.settle(30 * KIB))
        .collect();
    drop(x);
    tokio::time::timeout(Duration::from_secs(3600), younger)
        .await
        .expect("orders fits (34 of 64 KiB) once users is refused by its share")
        .unwrap();
    assert_eq!(*granted.lock().unwrap(), ["orders"]);
    older.abort();
}

/// G6 with no release: `users`' build settles larger than its estimate,
/// which turns `users`' refusal by the limit into one by its share. The
/// waiter behind it is asked again and fits.
#[tokio::test(start_paused = true)]
async fn a_settle_that_adds_bytes_lets_the_waiter_behind_be_asked_again() {
    let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
    let users = budget
        .reserve(Work::Live, &[(index("users"), 24 * KIB)])
        .unwrap();
    let _x = held(&budget, "x", 31);
    let _orders = held(&budget, "orders", 2);
    let gate = Gate::new(1, Arc::clone(&budget));
    let granted = Arc::new(Mutex::new(Vec::new()));
    let older = budgeted(&gate, "users", 8, &granted);
    settle().await;
    let younger = {
        let (gate, granted) = (Arc::clone(&gate), Arc::clone(&granted));
        tokio::spawn(async move {
            let (_permit, _parts) = gate
                .acquire(Work::Live, &[(index("orders"), 2 * KIB)])
                .await;
            granted.lock().unwrap().push("orders");
        })
    };
    settle().await;
    assert!(
        granted.lock().unwrap().is_empty(),
        "users is refused by the limit (65 of 64 KiB); orders waits behind it"
    );
    let _users: Vec<Charge> = users
        .into_iter()
        .map(|part| part.settle(25 * KIB))
        .collect();
    tokio::time::timeout(Duration::from_secs(3600), younger)
        .await
        .expect("users is now refused by its share (33 of 32 KiB); orders fits (60 of 64 KiB)")
        .unwrap();
    assert_eq!(*granted.lock().unwrap(), ["orders"]);
    older.abort();
}

/// G5 with a release racing the grant: `orders` is chosen while `users` is
/// refused by its share, then `users`' 20 KiB are released before `orders`
/// reserves. `orders` gives the offer back, and `users`, now fitting, is
/// granted first.
#[tokio::test(start_paused = true)]
async fn a_release_between_choosing_a_waiter_and_its_reservation_offers_the_permit_again() {
    let budget = Arc::new(Budget::new(64 * KIB).with_index_share(3, 0.5));
    let released = held(&budget, "users", 20);
    let _users = held(&budget, "users", 10);
    let _x = held(&budget, "x", 30);
    let _orders = held(&budget, "orders", 2);
    let gate = Gate::new(1, Arc::clone(&budget));
    let users_batch = [(index("users"), 8 * KIB)];
    let mut older = Box::pin(gate.acquire(Work::Live, &users_batch));
    assert!(
        futures::poll!(older.as_mut()).is_pending(),
        "users is refused by its share (38 of 32 KiB)"
    );
    gate.before_reserve(move || drop(released));
    let orders_batch = [(index("orders"), 20 * KIB)];
    let mut younger = Box::pin(gate.acquire(Work::Live, &orders_batch));
    assert!(
        futures::poll!(younger.as_mut()).is_pending(),
        "the release came after orders was chosen: the offer goes back to users"
    );
    let std::task::Poll::Ready(_users_granted) = futures::poll!(older.as_mut()) else {
        panic!("users fits now (50 of 64 KiB)");
    };
    assert!(
        futures::poll!(younger.as_mut()).is_pending(),
        "orders no longer fits (70 of 64 KiB)"
    );
}

/// With no waiter in a line (today's production), an add moves nothing: no
/// reservation, settle or untracked reservation moves the generation or
/// wakes anyone. The same after one waiter was granted and another dropped.
#[tokio::test(start_paused = true)]
async fn with_no_waiter_an_add_moves_no_generation() {
    fn adds_move_nothing(budget: &Arc<Budget>) {
        let before = budget.generation();
        let reserved = budget
            .reserve(Work::Live, &[(index("users"), 4 * KIB)])
            .unwrap();
        assert_eq!(budget.generation(), before, "a reservation");
        let settled: Vec<Charge> = reserved
            .into_iter()
            .map(|part| part.settle(8 * KIB))
            .collect();
        assert_eq!(
            budget.generation(),
            before,
            "a settle larger than its estimate"
        );
        let untracked = budget.try_reserve(Work::Live, 4 * KIB).unwrap();
        assert_eq!(budget.generation(), before, "an untracked reservation");
        drop((settled, untracked));
    }
    let budget = Arc::new(Budget::new(64 * KIB));
    adds_move_nothing(&budget);
    let gate = Gate::new(1, Arc::clone(&budget));
    let (permit, parts) = tokio::time::timeout(
        Duration::from_secs(5),
        gate.acquire(Work::Live, &[(index("orders"), KIB)]),
    )
    .await
    .expect("orders fits");
    drop((permit, parts));
    let held_permit = take(&gate, Work::Live).unwrap();
    let mut dropped = Box::pin(gate.acquire(Work::Live, &[]));
    assert!(futures::poll!(dropped.as_mut()).is_pending());
    drop(dropped);
    drop(held_permit);
    adds_move_nothing(&budget);
}

/// The kind a pass with both kinds ready and nothing to reserve is granted.
fn pass_both(gate: &Arc<Gate>) -> Option<(Work, Permit)> {
    gate.pass(Some(Vec::new()), Some(Vec::new()))
        .granted
        .map(|(work, permit, _)| (work, permit))
}

/// G4 and G9 with no one in the lines: a pass is decided as joining at the
/// front of empty lines would decide it: live first, every fourth contested
/// grant to backfill, an uncontested grant moving no count.
#[test]
fn a_pass_alone_takes_live_first_but_every_fourth_contested_grant_is_backfill() {
    let gate = Gate::new(1, unlimited());
    let kinds: Vec<Work> = (0..8)
        .map(|_| pass_both(&gate).map(|(work, _)| work).unwrap())
        .collect();
    assert_eq!(
        kinds,
        [Work::Live, Work::Live, Work::Live, Work::Backfill].repeat(2)
    );
    let alone = gate.pass(None, Some(Vec::new()));
    assert!(matches!(alone.granted, Some((Work::Backfill, _, _))));
    drop(alone);
    assert_eq!(
        pass_both(&gate).map(|(work, _)| work),
        Some(Work::Live),
        "the uncontested grant moved no count"
    );
}

/// G5 and G9 with no one in the lines: the budget refuses the live batch
/// whose turn it is, so the permit goes to the backfill batch in the same
/// pass, and the count doesn't move (the live batch was not eligible).
#[test]
fn a_pass_alone_hands_the_permit_to_the_other_kind_when_its_turn_is_refused() {
    let budget = Arc::new(Budget::new(64 * KIB));
    let users = held(&budget, "users", 60);
    let gate = Gate::new(1, Arc::clone(&budget));
    let passed = gate.pass(
        Some(vec![(index("users"), 16 * KIB)]),
        Some(vec![(index("orders"), 8 * KIB)]),
    );
    assert_eq!(passed.refused.len(), 1);
    assert_eq!(passed.refused[0].0, Work::Live);
    let Some((Work::Backfill, permit, reserved)) = passed.granted else {
        panic!("the backfill batch takes the permit");
    };
    drop((permit, reserved, users));
    let kinds: Vec<Work> = (0..4)
        .map(|_| pass_both(&gate).map(|(work, _)| work).unwrap())
        .collect();
    assert_eq!(
        kinds,
        [Work::Live, Work::Live, Work::Live, Work::Backfill],
        "the refusal moved no count"
    );
}

/// G9′, round 2: a waiter whose batch grows keeps its refusal (a refusal is
/// monotone in size: the larger batch is refused too); one whose batch
/// shrinks has it cleared and is asked again, so a stale refusal by the
/// limit holds no one back (G6).
#[test]
fn a_waiter_that_grows_keeps_its_refusal_and_one_that_shrinks_drops_it() {
    let budget = Arc::new(Budget::new(64 * KIB));
    // `users` holds live bytes already: no headroom past the limit for it.
    let _users = held(&budget, "users", 60);
    let gate = Gate::new(1, Arc::clone(&budget));
    let mut waiter = gate.join(Work::Live, vec![(index("users"), 8 * KIB)]);
    assert!(
        matches!(waiter.try_grant(), Asked::Refused(_)),
        "8 KiB past the limit"
    );
    waiter.set_parts(vec![(index("users"), 16 * KIB)]);
    assert!(
        matches!(waiter.try_grant(), Asked::NotNow(Some(_))),
        "grown, it keeps its refusal and is not offered the permit"
    );
    waiter.set_parts(vec![(index("users"), 2 * KIB)]);
    assert!(
        matches!(waiter.try_grant(), Asked::Granted(..)),
        "shrunk below the room left, its refusal is gone and it is granted"
    );
}
