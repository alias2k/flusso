use super::*;

#[test]
fn in_order_confirmations_advance_the_resume_point() {
    let p = Positions::new(0);
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
    let p = Positions::new(0);
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
    let p = Positions::new(100);
    let a = p.register(50);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 100);
}

#[test]
fn register_confirmed_advances_in_place_when_nothing_is_in_flight() {
    let p = Positions::new(0);
    p.register_confirmed(100);
    assert_eq!(p.confirmed_lsn(), 100);
    p.register_confirmed(200);
    assert_eq!(p.confirmed_lsn(), 200);
}

#[test]
fn register_confirmed_waits_for_in_flight_changes() {
    let p = Positions::new(0);
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
    let p = Positions::new(100);
    p.register_confirmed(50);
    assert_eq!(p.confirmed_lsn(), 100);
}

#[test]
fn a_position_past_a_confirmed_one_still_gates_its_own_lsn() {
    let p = Positions::new(0);
    p.register_confirmed(100);
    let a = p.register(200);
    assert_eq!(p.confirmed_lsn(), 100);
    p.confirm(a);
    assert_eq!(p.confirmed_lsn(), 200);
}
