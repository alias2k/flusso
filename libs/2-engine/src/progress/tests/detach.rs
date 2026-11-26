//! A detached lane (ADR 0012 stage 10, R5): out of the minimum, its own
//! items and its index's backlog alike; registrations on it ignored; kept
//! across a run's end; and with every lane detached the source is free.

use super::*;

fn lane(index: &str, sink: &str) -> LaneKey {
    LaneKey::new(
        kernel::IndexName::try_new(index).unwrap(),
        kernel::SinkName::try_new(sink).unwrap(),
    )
}

/// A detached lane holds nothing back: not its unlanded items, not its
/// index's documents waiting, not a build sent to it. Its sibling on another
/// sink still holds what it holds, and decides.
#[test]
fn a_detached_lane_leaves_the_minimum_and_its_backlog() {
    let (stuck, healthy) = (lane("users", "a"), lane("users", "b"));
    let progress = Progress::new([stuck.clone(), healthy.clone()]);
    let key = progress.next_key(&stuck.index).unwrap();
    progress.published(&stuck, key, p(1), p(1));
    progress.published(&healthy, key, p(1), p(1));
    progress.applied(&healthy, key);
    progress.mark([(id("users", 1), p(5))]);
    assert_eq!(progress.landed(), Landed::Before(p(1)));
    assert_eq!(progress.pinned_by(), Some(Pin::Lane(stuck.clone())));

    progress.detach(&stuck);
    assert!(progress.is_detached(&stuck));
    assert_eq!(
        progress.landed(),
        Landed::Before(p(5)),
        "the sibling still waits for the marked document"
    );
    assert_eq!(
        progress.pinned_by(),
        Some(Pin::Backlog(healthy.index.clone()))
    );
    assert_eq!(progress.own_oldest(&stuck), None);
    assert_eq!(progress.outstanding(&stuck), 0);
    assert!(progress.is_clear(&stuck, Level::Applied));
    assert_eq!(
        progress.over([&stuck, &healthy], Level::Applied),
        Some(Landed::Before(p(5)))
    );
}

/// With every lane detached nothing needs any change: the source is free to
/// confirm through its own horizon, also after a run ends (A5). With no lane
/// at all it is idle, as before.
#[test]
fn with_every_lane_detached_the_source_is_free_even_after_forget_all() {
    let (users, orders) = (lane("users", "a"), lane("orders", "a"));
    let progress = Progress::new([users.clone(), orders.clone()]);
    let key = progress.next_key(&users.index).unwrap();
    progress.published(&users, key, p(1), p(3));
    progress.detach(&users);
    assert_eq!(progress.landed(), Landed::Idle, "orders routed nothing yet");

    progress.detach(&orders);
    assert_eq!(progress.landed(), Landed::Free);
    assert_eq!(progress.pinned_by(), None);
    assert_eq!(progress.lane(&users, Level::Applied), Some(Landed::Free));
    assert_eq!(progress.lane(&users, Level::Searchable), Some(Landed::Free));

    progress.mark([(id("users", 1), p(4))]);
    progress.forget_all();
    assert_eq!(progress.landed(), Landed::Free);
    assert_eq!(Progress::new([]).landed(), Landed::Idle);
}

/// Every registration on a detached lane is ignored: an item, a skip, a
/// build's item, a step; the lane's marks move nothing either.
#[test]
fn a_registration_on_a_detached_lane_is_ignored() {
    let (detached, attached) = (lane("users", "a"), lane("users", "b"));
    let progress = Progress::new([detached.clone(), attached.clone()]);
    progress.detach(&detached);
    progress.change_read();
    let step = progress.close_step();
    let key = progress.next_key(&detached.index).unwrap();
    progress.published(&detached, key, p(1), p(1));
    progress.published_step(&detached, key, p(1), p(1), step);
    progress.skipped(&detached, p(9));
    progress.skipped_step(&detached, p(9), step);
    progress.applied(&detached, key);
    assert_eq!(progress.outstanding(&detached), 0);
    assert_eq!(progress.steps_kept(&detached), Some((0, 0)));
    assert_eq!(progress.lane(&detached, Level::Applied), Some(Landed::Free));

    progress.published_step(&attached, key, p(1), p(1), step);
    assert_eq!(
        progress.landed(),
        Landed::Before(p(1)),
        "only the attached lane counts"
    );
}

/// A run's end forgets its documents and what it routed, never a detach.
#[test]
fn forget_all_keeps_the_detach() {
    let (detached, attached) = (lane("users", "a"), lane("users", "b"));
    let progress = Progress::new([detached.clone(), attached.clone()]);
    progress.detach(&detached);
    progress.forget_all();
    assert!(progress.is_detached(&detached));
    assert!(!progress.is_detached(&attached));
    let key = progress.next_key(&detached.index).unwrap();
    progress.published(&detached, key, p(2), p(2));
    assert_eq!(progress.outstanding(&detached), 0);
}

/// A sink's committed count is over its attached lanes only (O21): a
/// detached lane neither holds it back nor answers for it, and a sink whose
/// every lane is detached has every read.
#[test]
fn a_detached_lane_counts_in_no_committed() {
    let (users, orders) = (lane("users", "a"), lane("orders", "a"));
    let sink = SinkName::try_new("a").unwrap();
    let progress = Progress::new([users.clone(), orders.clone()]);
    progress.change_read();
    let step = progress.close_step();
    let key = progress.next_key(&users.index).unwrap();
    progress.published_step(&users, key, p(1), p(1), step);
    progress.skipped_step(&orders, p(1), step);
    assert_eq!(progress.committed(&sink), 0, "users holds it");

    progress.detach(&users);
    assert_eq!(progress.committed(&sink), 1);
    progress.detach(&orders);
    progress.change_read();
    assert_eq!(progress.committed(&sink), 2, "no attached lane: every read");
}

/// Detaching an unknown lane does nothing and says so.
#[test]
fn an_unknown_lane_is_not_detached() {
    let progress = Progress::new([lane("users", "a")]);
    assert!(!progress.detach(&lane("users", "b")));
    assert!(!progress.is_detached(&lane("users", "b")));
    assert!(progress.detach(&lane("users", "a")));
}

/// An attach (ADR 0012 stage 10, R8): a detached lane is attached for an
/// attempt above the one it last attached for; the same attempt on a lane
/// attached for it answers `Again` (its `Attached` is published again); an
/// older attempt, or the attempt a lane was attached for and then detached
/// again, is stale.
#[test]
fn a_detached_lane_is_attached_for_a_newer_attempt_only() {
    let (lane_a, lane_b) = (lane("users", "a"), lane("users", "b"));
    let progress = Progress::new([lane_a.clone(), lane_b.clone()]);
    assert!(!progress.any_detached());
    assert_eq!(progress.attach(&lane_a, 1), Attach::Stale, "never detached");

    progress.detach(&lane_a);
    assert!(progress.any_detached());
    assert_eq!(progress.attach(&lane_a, 1), Attach::Attached);
    assert!(!progress.is_detached(&lane_a));
    assert!(!progress.any_detached());
    assert_eq!(progress.attach(&lane_a, 1), Attach::Again);
    assert_eq!(progress.attach(&lane_a, 0), Attach::Stale);

    progress.detach(&lane_a);
    assert_eq!(
        progress.attach(&lane_a, 1),
        Attach::Stale,
        "attached for it, then detached again"
    );
    assert!(progress.is_detached(&lane_a));
    assert_eq!(progress.attach(&lane_a, 2), Attach::Attached);
}

/// An attached lane answers through the newest position routed in this
/// process, which `forget_all` keeps: a lane that registered nothing since
/// is not `Idle`, so a quiet database keeps confirming (H7). Registrations
/// count on it again.
#[test]
fn an_attached_lane_answers_through_what_was_routed_even_after_forget_all() {
    let (attached, other) = (lane("users", "a"), lane("users", "b"));
    let progress = Progress::new([attached.clone(), other.clone()]);
    progress.routed(p(5));
    progress.forget_all();
    progress.detach(&attached);
    progress.detach(&other);
    assert_eq!(progress.landed(), Landed::Free);
    assert_eq!(progress.attach(&attached, 1), Attach::Attached);
    assert_eq!(progress.landed(), Landed::Through(p(5)));
    let key = progress.next_key(&attached.index).unwrap();
    progress.published(&attached, key, p(6), p(6));
    assert_eq!(
        progress.landed(),
        Landed::Before(p(6)),
        "registrations count again"
    );
}

/// The fourth re-check, B-2 at the slot: a detached lane leaves the minimum
/// only once its records were read under the current stream. Until then the
/// source is handed `Idle`, which keeps what it confirmed: another process
/// may have begun a rebuild of the index meanwhile, and a later start would
/// resume its walk over whatever this one let the slot pass.
#[test]
fn a_detached_lane_holds_confirmation_until_its_records_are_read_again() {
    let rereads = Arc::new(crate::Rereads::default());
    let progress = Progress::new([sink("a"), sink("b")]);
    progress.hold_while_unread(Arc::clone(&rereads));
    assert!(progress.detach(&sink("a")));
    publish(&progress, &[&sink("b")], 0, 0);
    progress.applied(&sink("b"), ItemKey::new(0));
    assert_eq!(progress.landed(), Landed::Through(p(0)), "nothing opened");
    futures::executor::block_on(rereads.open(async { Ok::<(), ()>(()) })).unwrap();
    rereads.read(&sink("b"), rereads.read_begins());
    assert_eq!(progress.landed(), Landed::Idle, "a unread: held");
    rereads.read(&sink("a"), rereads.read_begins());
    assert_eq!(progress.landed(), Landed::Through(p(0)), "both read");
    assert!(progress.detach(&sink("b")));
    assert_eq!(progress.landed(), Landed::Free);
}

/// The fifth re-check (second report, S-2): with two lanes detached, one
/// read under the current stream and one not, confirmation still holds: the
/// unread one may stand for another process's rebuild.
#[test]
fn a_detached_lane_unread_holds_confirmation_while_another_detached_lane_was_read() {
    let rereads = Arc::new(crate::Rereads::default());
    let progress = Progress::new([sink("a"), sink("b"), sink("c")]);
    progress.hold_while_unread(Arc::clone(&rereads));
    assert!(progress.detach(&sink("a")));
    assert!(progress.detach(&sink("c")));
    publish(&progress, &[&sink("b")], 0, 0);
    progress.applied(&sink("b"), ItemKey::new(0));
    futures::executor::block_on(rereads.open(async { Ok::<(), ()>(()) })).unwrap();
    rereads.read(&sink("b"), rereads.read_begins());
    rereads.read(&sink("a"), rereads.read_begins());
    assert_eq!(progress.landed(), Landed::Idle, "c unread: held");
}
