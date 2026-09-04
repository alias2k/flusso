use super::*;

#[test]
fn in_order_confirmations_advance_watermark() {
    let s = AckShared::new(0);
    let a = s.register(10);
    let b = s.register(20);
    assert_eq!(s.confirmed_lsn(), 0);
    s.confirm(a);
    assert_eq!(s.confirmed_lsn(), 10);
    s.confirm(b);
    assert_eq!(s.confirmed_lsn(), 20);
}

#[test]
fn out_of_order_confirmation_holds_until_gap_fills() {
    let s = AckShared::new(0);
    let a = s.register(10);
    let b = s.register(20);
    let c = s.register(30);

    s.confirm(c); // gap: a and b still open
    assert_eq!(s.confirmed_lsn(), 0);
    s.confirm(b); // still gated on a
    assert_eq!(s.confirmed_lsn(), 0);
    s.confirm(a); // fills the gap → jumps across b and c
    assert_eq!(s.confirmed_lsn(), 30);
}

#[test]
fn never_regresses_below_start_lsn() {
    let s = AckShared::new(100);
    let a = s.register(50); // a commit at a lower LSN than the start point
    s.confirm(a);
    assert_eq!(s.confirmed_lsn(), 100);
}

#[test]
fn register_confirmed_advances_in_place_when_nothing_is_in_flight() {
    let s = AckShared::new(0);
    s.register_confirmed(100);
    assert_eq!(s.confirmed_lsn(), 100);
    s.register_confirmed(200);
    assert_eq!(s.confirmed_lsn(), 200);
}

#[test]
fn register_confirmed_waits_for_in_flight_changes() {
    let s = AckShared::new(0);
    let a = s.register(10);
    s.register_confirmed(100); // keepalive past an unconfirmed change
    assert_eq!(s.confirmed_lsn(), 0, "must not pass the unflushed change");
    s.confirm(a); // the gap fills → jumps across the keepalive too
    assert_eq!(s.confirmed_lsn(), 100);
}

#[test]
fn register_confirmed_never_regresses() {
    let s = AckShared::new(100);
    s.register_confirmed(50); // a stale/low position
    assert_eq!(s.confirmed_lsn(), 100);
}

#[test]
fn changes_after_a_pre_confirmed_position_still_gate_their_own_lsn() {
    let s = AckShared::new(0);
    s.register_confirmed(100);
    let a = s.register(200); // emitted after the keepalive
    assert_eq!(s.confirmed_lsn(), 100);
    s.confirm(a);
    assert_eq!(s.confirmed_lsn(), 200);
}
