use super::*;

#[test]
fn in_order_confirmations_advance_the_resume_point() {
    let p = Positions::new(0, 0);
    let a = p.register(10);
    let b = p.register(20);
    assert_eq!(p.confirmed_lsn(), 0);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 10);
    p.confirm(b);
    assert_eq!(p.confirmed_lsn(), 20);
}

#[test]
fn confirmation_is_cumulative() {
    let p = Positions::new(0, 0);
    p.register(10);
    p.register(20);
    let c = p.register(30);
    p.confirm(c);
    assert_eq!(p.confirmed_lsn(), 30);
    p.confirm(c);
    assert_eq!(p.confirmed_lsn(), 30, "re-confirming is harmless");
}

#[test]
fn never_regresses_below_start_lsn() {
    let p = Positions::new(100, 0);
    let a = p.register(50);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 100);
}

#[test]
fn register_confirmed_advances_in_place_when_nothing_is_in_flight() {
    let p = Positions::new(0, 0);
    p.register_confirmed(100);
    assert_eq!(p.confirmed_lsn(), 100);
    p.register_confirmed(200);
    assert_eq!(p.confirmed_lsn(), 200);
}

#[test]
fn register_confirmed_waits_for_in_flight_changes() {
    let p = Positions::new(0, 0);
    let a = p.register(10);
    p.register_confirmed(100);
    assert_eq!(p.confirmed_lsn(), 0, "must not pass the unflushed change");
    p.confirm(a);
    assert_eq!(
        p.confirmed_lsn(),
        10,
        "the keepalive sits behind the change"
    );
    let later = p.register(200);
    p.confirm(later);
    assert_eq!(p.confirmed_lsn(), 200);
}

#[test]
fn register_confirmed_never_regresses() {
    let p = Positions::new(100, 0);
    p.register_confirmed(50);
    assert_eq!(p.confirmed_lsn(), 100);
}

#[test]
fn a_position_past_a_confirmed_one_still_gates_its_own_lsn() {
    let p = Positions::new(0, 0);
    p.register_confirmed(100);
    let a = p.register(200);
    assert_eq!(p.confirmed_lsn(), 100);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 200);
}

/// A reopened stream continues the numbering, so a watermark the lanes still
/// hold from the previous stream never covers a change of the new one.
#[test]
fn a_reopened_stream_continues_the_numbering() {
    let first = Positions::new(0, 0);
    assert_eq!(first.register(10), 0);
    assert_eq!(first.register(20), 1);

    let second = Positions::new(0, first.next_seq());
    assert_eq!(second.register(30), 2);
    second.confirm(1);
    assert_eq!(
        second.confirmed_lsn(),
        0,
        "an old-stream position confirms nothing new"
    );
    second.confirm(2);
    assert_eq!(second.confirmed_lsn(), 30);
}
