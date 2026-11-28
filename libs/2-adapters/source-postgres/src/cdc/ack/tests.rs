use std::sync::atomic::{AtomicUsize, Ordering};

use source::cdc::Landed::{Before, Idle, Through};

use super::*;

fn positions(start_lsn: u64, first_seq: u64) -> Positions {
    Positions::new(start_lsn, first_seq, Arc::new(Listeners::default()))
}

/// Emit a single-change commit.
fn emit(p: &Positions, lsn: u64) -> Position {
    p.emit(lsn, true)
}

#[test]
fn in_order_confirmations_advance_the_resume_point() {
    let p = positions(0, 0);
    emit(&p, 10);
    let b = emit(&p, 20);
    assert_eq!(p.confirmed_lsn(), 0);
    p.confirm(Before(b));
    assert_eq!(p.confirmed_lsn(), 10);
    p.confirm(Through(b));
    assert_eq!(p.confirmed_lsn(), 20);
}

#[test]
fn confirmation_never_goes_back() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    emit(&p, 20);
    let c = emit(&p, 30);
    p.confirm(Through(c));
    assert_eq!(p.confirmed_lsn(), 30);
    p.confirm(Through(c));
    assert_eq!(p.confirmed_lsn(), 30, "re-confirming is harmless");
    p.confirm(Before(a));
    assert_eq!(p.confirmed_lsn(), 30, "an older answer moves nothing");
}

#[test]
fn never_regresses_below_start_lsn() {
    let p = positions(100, 0);
    let a = emit(&p, 50);
    p.confirm(Through(a));
    assert_eq!(p.confirmed_lsn(), 100);
    p.confirm(Before(a));
    assert_eq!(p.confirmed_lsn(), 100);
}

/// A capture that never emitted follows idle WAL on every confirmation, and
/// only on one: reading on confirms nothing by itself.
#[test]
fn the_horizon_advances_the_resume_point_when_nothing_was_emitted() {
    let p = positions(0, 0);
    p.read_through(100);
    assert_eq!(
        p.confirmed_lsn(),
        0,
        "the engine's next confirmation moves it"
    );
    p.confirm(Idle);
    assert_eq!(p.confirmed_lsn(), 100);
    p.read_through(200);
    p.confirm(Idle);
    assert_eq!(p.confirmed_lsn(), 200);
    p.read_through(50);
    p.confirm(Idle);
    assert_eq!(p.confirmed_lsn(), 200, "never regresses");
}

#[test]
fn the_horizon_waits_for_in_flight_changes() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    p.read_through(100);
    p.confirm(Before(a));
    assert_eq!(p.confirmed_lsn(), 0, "must not pass the unlanded change");
    p.confirm(Through(a));
    assert_eq!(
        p.confirmed_lsn(),
        100,
        "with the change landed, everything read is covered"
    );
}

/// The slot keeps following keepalives after one arrived mid-flight: nothing
/// is left queued behind the landed change.
#[test]
fn a_keepalive_during_a_flight_does_not_pin_the_resume_point() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    p.read_through(100);
    p.confirm(Through(a));
    p.read_through(300);
    p.confirm(Through(a));
    assert_eq!(p.confirmed_lsn(), 300);
}

/// ADR 0012: the routed position stays put while the horizon moves, so only a
/// confirmation that is sent anyway (the engine's tick) releases idle WAL.
#[test]
fn the_tick_releases_idle_wal_while_routed_stays_put() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    p.confirm(Through(a));
    assert_eq!(p.confirmed_lsn(), 10);
    for keepalive in [20, 30, 40] {
        p.read_through(keepalive);
        p.confirm(Through(a));
        assert_eq!(p.confirmed_lsn(), keepalive);
    }
}

#[test]
fn a_later_change_gates_its_own_lsn() {
    let p = positions(0, 0);
    p.read_through(100);
    let a = emit(&p, 200);
    p.confirm(Before(a));
    assert_eq!(p.confirmed_lsn(), 100);
    p.confirm(Through(a));
    assert_eq!(p.confirmed_lsn(), 200);
}

/// The changes of one transaction share its commit LSN. Landing the first
/// must not confirm the commit: a restart would resume past it and lose the
/// second.
#[test]
fn a_split_transaction_is_not_confirmed_until_its_last_change() {
    let p = positions(0, 0);
    let before = emit(&p, 10);
    let first = p.emit(20, false);
    let second = p.emit(20, true);
    assert!(before < first);

    p.confirm(Before(second));
    assert_eq!(p.confirmed_lsn(), 10, "only the earlier commit is covered");
    p.confirm(Through(first));
    assert_eq!(
        p.confirmed_lsn(),
        10,
        "the second change is emitted, not routed: still not the commit"
    );
    p.confirm(Through(second));
    assert_eq!(p.confirmed_lsn(), 20);
}

/// ADR 0012: a change the engine hasn't routed is in no lane. Even with the
/// horizon past it (a keepalive after the emission), neither confirmation nor
/// a fence passes it.
#[test]
fn a_change_emitted_but_not_routed_is_never_fenced_or_confirmed() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    let b = emit(&p, 20);
    p.read_through(50);
    assert!(b > a);

    p.confirm(Through(a));
    assert_eq!(p.confirmed_lsn(), 10);
    assert_eq!(p.reached(Through(a)), 10);
}

/// ADR 0012: in a fresh process change 0 can be emitted, with the horizon past
/// it, before the engine routes anything.
#[test]
fn a_fresh_process_never_confirms_its_first_unrouted_change() {
    let p = positions(5, FIRST_SEQ);
    emit(&p, 10);
    p.read_through(50);
    p.confirm(Idle);
    assert_eq!(p.confirmed_lsn(), 5);
    assert_eq!(p.reached(Idle), 5);
}

/// ADR 0012: after an ingest restart the reopened stream continues the
/// numbering, so the change routed last on the old stream is still the last
/// emitted, and idle WAL is released.
#[test]
fn an_idle_stream_after_an_ingest_restart_still_releases_wal() {
    let old = positions(0, FIRST_SEQ);
    let routed = emit(&old, 10);
    let new = positions(10, old.next_seq());
    new.read_through(60);
    new.confirm(Through(routed));
    assert_eq!(new.confirmed_lsn(), 60);
    assert_eq!(new.reached(Through(routed)), 60);
}

#[test]
fn reached_is_the_horizon_once_nothing_is_outstanding() {
    let p = positions(5, 0);
    assert_eq!(p.reached(Idle), 5, "a stream that emitted nothing");
    let a = emit(&p, 10);
    assert_eq!(p.reached(Before(a)), 9, "everything short of the commit");
    assert_eq!(p.reached(Through(a)), 10);
    p.read_through(40);
    assert_eq!(p.reached(Through(a)), 40, "idle WAL moves it on");
    assert_eq!(
        p.reached(Idle),
        5,
        "emitted, routed nothing: what is confirmed"
    );
}

#[test]
fn reached_stops_below_the_next_outstanding_change() {
    let p = positions(0, 0);
    let a = emit(&p, 10);
    let b = emit(&p, 20);
    let c = emit(&p, 30);
    p.read_through(90);
    assert_eq!(
        p.reached(Before(b)),
        19,
        "unwatched WAL before the next commit is covered"
    );
    assert_eq!(p.reached(Before(c)), 29);
    assert_eq!(p.reached(Through(c)), 90);
    assert_eq!(
        p.reached(Through(a)),
        10,
        "b is emitted, not routed: only a's own commit"
    );
}

/// A fence may claim everything short of the next commit; the slot may not.
/// The server skips a transaction whose commit record starts before the
/// confirmed LSN, so confirmation stays at the end of the last covered commit.
#[test]
fn confirmation_stops_at_a_commit_boundary_where_a_fence_goes_further() {
    let p = positions(0, 0);
    emit(&p, 10);
    let b = emit(&p, 20);
    assert_eq!(p.reached(Before(b)), 19);
    p.confirm(Before(b));
    assert_eq!(
        p.confirmed_lsn(),
        10,
        "never inside the next commit's record"
    );
}

/// A sink ahead of what every sink landed still reads its own answer: the
/// position carries it, whatever was confirmed.
#[test]
fn reached_survives_confirmation_of_earlier_positions() {
    let p = positions(0, 0);
    emit(&p, 10);
    let b = emit(&p, 20);
    let c = emit(&p, 30);
    p.confirm(Before(b));
    assert_eq!(p.reached(Before(b)), 19);
    assert_eq!(p.reached(Before(c)), 29);
    assert_eq!(p.reached(Through(c)), 30);
}

#[test]
fn reached_holds_a_split_transaction_back() {
    let p = positions(0, 0);
    emit(&p, 10);
    let first = p.emit(20, false);
    assert_eq!(
        p.reached(Through(first)),
        10,
        "the rest of the transaction is not emitted yet"
    );
    let second = p.emit(20, true);
    assert_eq!(
        p.reached(Through(first)),
        10,
        "the rest is emitted, not landed"
    );
    assert_eq!(
        p.reached(Before(second)),
        19,
        "short of the commit it is half of"
    );
    assert_eq!(p.reached(Through(second)), 20);
}

#[test]
fn a_position_is_stamped_from_its_commit_and_the_horizon_before_it() {
    let p = positions(5, 0);
    let first = p.emit(20, false);
    let second = p.emit(20, true);
    assert_eq!(first, Position::stamped(0, 19, 5, 5));
    assert_eq!(second, Position::stamped(1, 19, 5, 20));
    let next = emit(&p, 30);
    assert_eq!(next, Position::stamped(2, 29, 20, 30));
}

/// A reopened stream continues the numbering, so a position the lanes still
/// hold from the previous stream never names a change of the new one, and it
/// still confirms only what it covers.
#[test]
fn a_reopened_stream_continues_the_numbering() {
    let first = positions(0, 0);
    assert_eq!(emit(&first, 10).seq, 0);
    let old = emit(&first, 20);
    assert_eq!(old.seq, 1);

    let second = positions(0, first.next_seq());
    let redelivered = emit(&second, 30);
    assert_eq!(redelivered.seq, 2);
    second.confirm(Idle);
    assert_eq!(
        second.confirmed_lsn(),
        0,
        "Idle after an emission moves nothing"
    );
    second.confirm(Through(old));
    assert_eq!(
        second.confirmed_lsn(),
        20,
        "everything through the old position landed; not the new commit"
    );
    assert_eq!(second.reached(Through(old)), 20);
    second.confirm(Through(redelivered));
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

    let a = p.emit(10, false);
    p.confirm(Before(a));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    p.read_through(10);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    p.read_through(10);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "no move, no call");
    emit(&p, 20);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "a commit's last change");
}

/// `Landed::Idle`'s contract (the fifth re-check): a capture that emitted
/// nothing answers it at or past where its stream started, the slot's
/// confirmed position; one that emitted answers what it confirmed. Never
/// below what an earlier holder of the slot dropped.
#[test]
fn idle_is_never_answered_below_the_streams_start() {
    let p = positions(100, 0);
    assert!(p.reached(Idle) >= 100);
    emit(&p, 110);
    assert!(p.reached(Idle) >= 100);
}
