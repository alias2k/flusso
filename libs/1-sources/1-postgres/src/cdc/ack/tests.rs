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
