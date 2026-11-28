//! The sink's two fence levels, per lane, kept in key order while writes
//! finish in any order.
//!
//! Every live lane item is entered under its lane and its [`ItemKey`] when it
//! is received, with its position, and marked when its write returns. Items
//! are known by key, not position: two items of one lane may end on the same
//! change. *Applied* is the last item before the oldest one still being
//! written; *searchable* trails it, each item due by what its own write
//! reported ([`Visibility`]). Each is reported by key to the progress and by
//! the item's position to the observers. A caught-up write that returns
//! [`Visibility::Immediate`] makes every write that returned before it was
//! granted searchable, which the port promises; each returned write gets a
//! sequence number so that "before" is exact.
//!
//! ```text
//! keys        1 (writing)   2 (done, due 10:00:05)   3 (done, due now)
//! applied     none: 1 is still being written
//! 1 returns   applied 3; searchable 1 at once, then 2 and 3 at 10:00:05
//! ```
//!
//! Coverage by a caught-up write is the sink's (the `Sink::write` contract):
//! the returned-writes counter is shared by every lane.
//!
//! The state outlives a restart of the engine's run: a released item comes
//! back under the same key and takes its old place.

use std::collections::BTreeMap;
use std::ops::Bound;

use kernel::Position;
use sink::Visibility;
use stream::{ItemKey, LaneKey};
use tokio::time::Instant;

/// The fence levels of one sink, per lane, over the live items it has
/// received, in key order.
#[derive(Debug, Default)]
pub(super) struct Fences {
    lanes: BTreeMap<LaneKey, LaneFences>,
    /// Writes returned so far; the next one's sequence number.
    returned: u64,
    /// Per lane, its writes that returned before this sequence number were
    /// covered by a caught-up write of the lane that returned: an idle
    /// refresh of the lane would add nothing.
    covered_below: BTreeMap<LaneKey, u64>,
    /// The latest time any written item was due at. It never moves back,
    /// so once it has passed no entry waits on a time.
    latest_due: Option<Instant>,
    /// Written items waiting on a caught-up write (never fewer than there
    /// are). With none, and `latest_due` passed, no entry waits at all and the
    /// scans below are skipped: what one write at a time cost per item.
    deferred: usize,
}

/// One lane's items, in lane (key) order, each with its position.
#[derive(Debug, Default)]
struct LaneFences {
    entries: BTreeMap<ItemKey, (Position, Entry)>,
    /// The last item reported applied.
    applied: Option<ItemKey>,
}

#[derive(Debug, Clone, Copy)]
enum Entry {
    Writing,
    Written { seq: u64, due: Due },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Due {
    At(Instant),
    /// When a caught-up write granted after this one returned reports
    /// [`Visibility::Immediate`].
    WithACaughtUpWrite,
}

impl Fences {
    /// A live item keyed `key`, at `position`, arrived on `lane`: it is being
    /// written until [`write_returned`](Self::write_returned). A redelivered
    /// item keeps its place.
    pub(super) fn received(&mut self, lane: &LaneKey, key: ItemKey, position: Position) {
        let fences = self.lanes.entry(lane.clone()).or_default();
        if fences.applied.is_some_and(|applied| key <= applied) {
            return;
        }
        fences
            .entries
            .entry(key)
            .or_insert((position, Entry::Writing));
    }

    /// Forget what `lane` was writing (ADR 0012 stage 10, R6): a detached
    /// lane's items are acknowledged unwritten. Its last applied key stays,
    /// so an item from before the detach, delivered again, is never entered.
    pub(super) fn forget_lane(&mut self, lane: &LaneKey) {
        if let Some(fences) = self.lanes.get_mut(lane) {
            let deferred = fences
                .entries
                .values()
                .filter(|(_, entry)| {
                    matches!(
                        entry,
                        Entry::Written {
                            due: Due::WithACaughtUpWrite,
                            ..
                        }
                    )
                })
                .count();
            self.deferred = self.deferred.saturating_sub(deferred);
            fences.entries.clear();
        }
        self.covered_below.remove(lane);
    }

    /// How many writes have returned: what a write granted now covers if it
    /// is caught up.
    pub(super) fn returned(&self) -> u64 {
        self.returned
    }

    /// A write returned, holding the items at `written` (each lane's item
    /// key; one write may hold several lanes' items, merged). Each is
    /// written, due by the write's `visibility`. A caught-up write reporting
    /// `Immediate` makes every write that returned before `covers_below`
    /// searchable now, on the lanes it was caught up for (the `Sink::write`
    /// contract, scoped by `CaughtUp::Indexes`).
    pub(super) fn write_returned(
        &mut self,
        written: &[(LaneKey, ItemKey)],
        visibility: Visibility,
        caught_up: Option<(u64, &[LaneKey])>,
        now: Instant,
    ) {
        let seq = self.returned;
        self.returned += 1;
        for (lane, key) in written {
            let Some((_, entry)) = self
                .lanes
                .get_mut(lane)
                .and_then(|fences| fences.entries.get_mut(key))
            else {
                continue;
            };
            let due = match visibility {
                Visibility::Immediate => Due::At(now),
                Visibility::Within(delay) => Due::At(now + delay),
                Visibility::Deferred => Due::WithACaughtUpWrite,
            };
            match due {
                Due::At(at) => self.latest_due = self.latest_due.max(Some(at)),
                Due::WithACaughtUpWrite => self.deferred += 1,
            }
            *entry = Entry::Written { seq, due };
        }
        if let Some((covers_below, lanes)) = caught_up {
            for lane in lanes {
                let covered = self.covered_below.entry(lane.clone()).or_default();
                *covered = (*covered).max(covers_below);
            }
            if visibility == Visibility::Immediate && self.some_may_wait(now) {
                for fences in self
                    .lanes
                    .iter_mut()
                    .filter(|(lane, _)| lanes.contains(lane))
                    .map(|(_, fences)| fences)
                {
                    for (_, entry) in fences.entries.values_mut() {
                        if let Entry::Written { seq, due } = entry
                            && *seq < covers_below
                            && match due {
                                Due::At(at) => *at > now,
                                Due::WithACaughtUpWrite => true,
                            }
                        {
                            if *due == Due::WithACaughtUpWrite {
                                self.deferred = self.deferred.saturating_sub(1);
                            }
                            *due = Due::At(now);
                        }
                    }
                }
            }
        }
    }

    /// Whether some written position may still wait to be searchable: false
    /// only when none can.
    fn some_may_wait(&self, now: Instant) -> bool {
        self.deferred > 0 || self.latest_due.is_some_and(|at| at > now)
    }

    /// `lane`'s new applied item (its key and position), if its written run
    /// from the front moved.
    pub(super) fn advance_applied(&mut self, lane: &LaneKey) -> Option<(ItemKey, Position)> {
        let fences = self.lanes.get_mut(lane)?;
        let from = fences.applied.map_or(Bound::Unbounded, Bound::Excluded);
        let mut reached = None;
        for (key, (position, entry)) in fences.entries.range((from, Bound::Unbounded)) {
            match entry {
                Entry::Written { .. } => reached = Some((*key, *position)),
                Entry::Writing => break,
            }
        }
        if let Some((key, _)) = reached {
            fences.applied = Some(key);
        }
        reached
    }

    /// For each lane whose searchable run moved, its last item now searchable
    /// (its key and position), dropping it and everything before it.
    pub(super) fn advance_searchable(&mut self, now: Instant) -> Vec<(LaneKey, ItemKey, Position)> {
        let mut moved = Vec::new();
        for (lane, fences) in &mut self.lanes {
            let mut reached = None;
            while let Some((&key, &(position, entry))) = fences.entries.first_key_value() {
                match entry {
                    Entry::Written {
                        due: Due::At(at), ..
                    } if at <= now => {
                        reached = Some((key, position));
                        fences.entries.pop_first();
                    }
                    _ => break,
                }
            }
            if let Some((key, position)) = reached {
                moved.push((lane.clone(), key, position));
            }
        }
        moved
    }

    /// When the oldest position of some lane becomes searchable, if that is
    /// a matter of time.
    pub(super) fn next_due(&self) -> Option<Instant> {
        self.lanes
            .values()
            .filter_map(|fences| match fences.entries.first_key_value() {
                Some((
                    _,
                    (
                        _,
                        Entry::Written {
                            due: Due::At(at), ..
                        },
                    ),
                )) => Some(*at),
                _ => None,
            })
            .min()
    }

    /// Whether a written position of `lane` isn't searchable yet and no
    /// caught-up write of the lane that returned after it covered it: a lane
    /// going quiet sends an empty caught-up write for its index.
    pub(super) fn wants_a_caught_up_write(&self, lane: &LaneKey, now: Instant) -> bool {
        if !self.some_may_wait(now) {
            return false;
        }
        let covered = self.covered_below.get(lane).copied().unwrap_or(0);
        self.lanes.get(lane).is_some_and(|fences| {
            fences.entries.values().any(|(_, entry)| match entry {
                Entry::Written { seq, due } => {
                    *seq >= covered
                        && match due {
                            Due::At(at) => *at > now,
                            Due::WithACaughtUpWrite => true,
                        }
                }
                Entry::Writing => false,
            })
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use kernel::{IndexName, SinkName};

    use super::*;

    fn lane(index: &str) -> LaneKey {
        LaneKey::new(
            IndexName::try_new(index).unwrap(),
            SinkName::try_new("primary").unwrap(),
        )
    }

    /// Two items of one lane sharing a position are two entries: the
    /// second, still being written, holds the lane's applied mark back.
    #[test]
    fn two_items_sharing_a_position_are_applied_one_at_a_time() {
        let now = Instant::now();
        let users = lane("users");
        let mut fences = Fences::default();
        fences.received(&users, ItemKey::new(0), Position::new(5));
        fences.received(&users, ItemKey::new(1), Position::new(5));
        fences.write_returned(
            &[(users.clone(), ItemKey::new(0))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(
            fences.advance_applied(&users),
            Some((ItemKey::new(0), Position::new(5))),
            "only the first item: the second is still being written"
        );
        fences.write_returned(
            &[(users.clone(), ItemKey::new(1))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(
            fences.advance_applied(&users),
            Some((ItemKey::new(1), Position::new(5)))
        );
    }

    #[test]
    fn applied_waits_for_the_oldest_write_and_searchable_follows_it() {
        let now = Instant::now();
        let users = lane("users");
        let mut fences = Fences::default();
        for position in 1..=3 {
            fences.received(&users, ItemKey::new(position), Position::new(position));
        }
        fences.write_returned(
            &[(users.clone(), ItemKey::new(3))],
            Visibility::Immediate,
            None,
            now,
        );
        fences.write_returned(
            &[(users.clone(), ItemKey::new(2))],
            Visibility::Within(Duration::from_secs(5)),
            None,
            now,
        );
        assert_eq!(fences.advance_applied(&users), None);
        assert!(fences.advance_searchable(now).is_empty());

        fences.write_returned(
            &[(users.clone(), ItemKey::new(1))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(
            fences.advance_applied(&users),
            Some((ItemKey::new(3), Position::new(3)))
        );
        assert_eq!(
            fences.advance_searchable(now),
            [(users.clone(), ItemKey::new(1), Position::new(1))]
        );
        assert_eq!(fences.next_due(), Some(now + Duration::from_secs(5)));
        let later = now + Duration::from_secs(5);
        assert_eq!(
            fences.advance_searchable(later),
            [(users, ItemKey::new(3), Position::new(3))]
        );
    }

    #[test]
    fn a_caught_up_write_covers_only_the_writes_that_returned_before_its_grant() {
        let now = Instant::now();
        let users = lane("users");
        let mut fences = Fences::default();
        fences.received(&users, ItemKey::new(1), Position::new(1));
        fences.received(&users, ItemKey::new(2), Position::new(2));
        fences.write_returned(
            &[(users.clone(), ItemKey::new(1))],
            Visibility::Deferred,
            None,
            now,
        );
        let covers_below = fences.returned();
        fences.write_returned(
            &[(users.clone(), ItemKey::new(2))],
            Visibility::Deferred,
            None,
            now,
        );
        assert!(fences.wants_a_caught_up_write(&users, now));

        fences.write_returned(
            &[],
            Visibility::Immediate,
            Some((covers_below, std::slice::from_ref(&users))),
            now,
        );
        assert_eq!(
            fences.advance_applied(&users),
            Some((ItemKey::new(2), Position::new(2)))
        );
        assert_eq!(
            fences.advance_searchable(now),
            [(users.clone(), ItemKey::new(1), Position::new(1))]
        );
        assert!(
            fences.wants_a_caught_up_write(&users, now),
            "2 returned after the caught-up write was granted"
        );
    }

    /// A caught-up write covers only the lanes it was caught up for: the
    /// other lane's deferred write still waits for one of its own.
    #[test]
    fn a_caught_up_write_covers_only_its_own_lanes() {
        let now = Instant::now();
        let (users, orders) = (lane("users"), lane("orders"));
        let mut fences = Fences::default();
        fences.received(&users, ItemKey::new(1), Position::new(1));
        fences.received(&orders, ItemKey::new(2), Position::new(2));
        fences.write_returned(
            &[(users.clone(), ItemKey::new(1))],
            Visibility::Deferred,
            None,
            now,
        );
        fences.write_returned(
            &[(orders.clone(), ItemKey::new(2))],
            Visibility::Deferred,
            None,
            now,
        );
        let covers_below = fences.returned();
        fences.write_returned(
            &[],
            Visibility::Immediate,
            Some((covers_below, std::slice::from_ref(&users))),
            now,
        );
        assert_eq!(
            fences.advance_searchable(now),
            [(users.clone(), ItemKey::new(1), Position::new(1))]
        );
        assert!(!fences.wants_a_caught_up_write(&users, now));
        assert!(fences.wants_a_caught_up_write(&orders, now));
    }

    #[test]
    fn a_redelivered_position_keeps_its_place() {
        let now = Instant::now();
        let users = lane("users");
        let mut fences = Fences::default();
        fences.received(&users, ItemKey::new(1), Position::new(1));
        fences.received(&users, ItemKey::new(2), Position::new(2));
        fences.write_returned(
            &[(users.clone(), ItemKey::new(2))],
            Visibility::Immediate,
            None,
            now,
        );
        fences.received(&users, ItemKey::new(1), Position::new(1));
        assert_eq!(fences.advance_applied(&users), None);
        fences.write_returned(
            &[(users.clone(), ItemKey::new(1))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(
            fences.advance_applied(&users),
            Some((ItemKey::new(2), Position::new(2)))
        );
    }

    /// Lanes keep their own order: a write still in flight on one index holds
    /// back neither the other index's applied nor its searchable position.
    #[test]
    fn a_write_in_flight_on_one_lane_holds_back_no_other_lane() {
        let now = Instant::now();
        let (users, orders) = (lane("users"), lane("orders"));
        let mut fences = Fences::default();
        fences.received(&users, ItemKey::new(1), Position::new(1));
        fences.received(&orders, ItemKey::new(2), Position::new(2));
        fences.write_returned(
            &[(orders.clone(), ItemKey::new(2))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(fences.advance_applied(&users), None);
        assert_eq!(
            fences.advance_applied(&orders),
            Some((ItemKey::new(2), Position::new(2)))
        );
        assert_eq!(
            fences.advance_searchable(now),
            [(orders, ItemKey::new(2), Position::new(2))]
        );
    }
}
