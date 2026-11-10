use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn positions(start_lsn: u64, first_seq: u64) -> Positions {
    Positions::new(start_lsn, first_seq, Arc::new(Listeners::default()))
}

/// Emit a single-change commit: register it and mark the commit complete.
fn emit(p: &Positions, lsn: u64) -> u64 {
    let seq = p.register(lsn);
    p.read_through(lsn);
    seq
}

#[test]
fn in_order_confirmations_advance_the_resume_point() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    let b = emit(&p, 20);
    assert_eq!(p.confirmed_lsn(), 0);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 10);
    p.confirm(b);
    assert_eq!(p.confirmed_lsn(), 20);
}

#[test]
fn confirmation_is_cumulative() {
    let p = positions(0, 0);
    emit(&p, 10);
    emit(&p, 20);
    let c = emit(&p, 30);
    p.confirm(c);
    assert_eq!(p.confirmed_lsn(), 30);
    p.confirm(c);
    assert_eq!(p.confirmed_lsn(), 30, "re-confirming is harmless");
}

#[test]
fn never_regresses_below_start_lsn() {
    let p = positions(100, 0);
    let a = emit(&p, 50);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 100);
}

#[test]
fn the_horizon_advances_the_resume_point_when_nothing_is_in_flight() {
    let p = positions(0, 0);
    p.read_through(100);
    assert_eq!(p.confirmed_lsn(), 100);
    p.read_through(200);
    assert_eq!(p.confirmed_lsn(), 200);
    p.read_through(50);
    assert_eq!(p.confirmed_lsn(), 200, "never regresses");
}

#[test]
fn the_horizon_waits_for_in_flight_changes() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    p.read_through(100);
    assert_eq!(p.confirmed_lsn(), 0, "must not pass the unflushed change");
    p.confirm(a);
    assert_eq!(
        p.confirmed_lsn(),
        100,
        "with the change confirmed, everything read is covered"
    );
}

/// The slot keeps following keepalives after one arrived mid-flight: nothing
/// is left queued behind the confirmed change.
#[test]
fn a_keepalive_during_a_flight_does_not_pin_the_resume_point() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    p.read_through(100);
    p.confirm(a);
    p.read_through(300);
    assert_eq!(p.confirmed_lsn(), 300);
}

#[test]
fn a_later_change_gates_its_own_lsn() {
    let p = positions(0, 0);
    p.read_through(100);
    let a = emit(&p, 200);
    assert_eq!(p.confirmed_lsn(), 100);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 200);
}

/// The changes of one transaction share its commit LSN. Confirming the first
/// must not confirm the commit: a restart would resume past it and lose the
/// second.
#[test]
fn a_split_transaction_is_not_confirmed_until_its_last_change() {
    let p = positions(0, 0);
    let before = emit(&p, 10);
    let first = p.register(20);
    let second = p.register(20);
    p.read_through(20);

    p.confirm(first);
    assert_eq!(p.confirmed_lsn(), 10, "only the earlier commit is covered");
    assert!(before < first);
    p.confirm(second);
    assert_eq!(p.confirmed_lsn(), 20);
}

#[test]
fn reached_is_the_horizon_once_nothing_is_outstanding() {
    let p = positions(5, 0);
    assert_eq!(p.reached(None), 5, "a stream that emitted nothing");
    let a = emit(&p, 10);
    assert_eq!(p.reached(None), 5, "the change is outstanding");
    assert_eq!(p.reached(Some(a)), 10);
    p.read_through(40);
    assert_eq!(p.reached(Some(a)), 40, "idle WAL moves it on");
}

#[test]
fn reached_stops_below_the_next_outstanding_change() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    let b = emit(&p, 20);
    let c = emit(&p, 30);
    p.read_through(90);
    assert_eq!(p.reached(Some(a)), 10);
    assert_eq!(p.reached(Some(b)), 20);
    assert_eq!(p.reached(Some(c)), 90);
}

/// One sink is ahead of the confirmed watermark: its position still resolves
/// after the entries at or before the watermark were dropped.
#[test]
fn reached_survives_confirmation_of_earlier_positions() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    let b = emit(&p, 20);
    let c = emit(&p, 30);
    p.confirm(a);
    assert_eq!(p.reached(Some(a)), 10, "falls back to the resume point");
    assert_eq!(p.reached(Some(b)), 20);
    assert_eq!(p.reached(Some(c)), 30);
}

#[test]
fn reached_holds_a_split_transaction_back() {
    let p = positions(0, 0);
    emit(&p, 10);
    let first = p.register(20);
    assert_eq!(
        p.reached(Some(first)),
        10,
        "the rest of the transaction is not emitted yet"
    );
    let second = p.register(20);
    assert_eq!(p.reached(Some(first)), 10);
    p.read_through(20);
    assert_eq!(p.reached(Some(second)), 20);
}

/// A reopened stream continues the numbering, so a watermark the lanes still
/// hold from the previous stream never covers a change of the new one.
#[test]
fn a_reopened_stream_continues_the_numbering() {
    let first = positions(0, 0);
    assert_eq!(emit(&first, 10), 0);
    assert_eq!(emit(&first, 20), 1);

    let second = positions(0, first.next_seq());
    assert_eq!(emit(&second, 30), 2);
    second.confirm(1);
    assert_eq!(
        second.confirmed_lsn(),
        0,
        "an old-stream position confirms nothing new"
    );
    assert_eq!(second.reached(Some(1)), 0);
    second.confirm(2);
    assert_eq!(second.confirmed_lsn(), 30);
}

#[test]
fn listeners_hear_the_horizon_move_and_nothing_else() {
    let listeners = Arc::new(Listeners::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    listeners.add(Arc::new(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    let p = Positions::new(0, 0, listeners);

    let a = p.register(10);
    p.confirm(a);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    p.read_through(10);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    p.read_through(10);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "no move, no call");
}
