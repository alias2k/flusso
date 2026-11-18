//! The sink's two fence levels, per lane, kept in position order while writes
//! finish in any order.
//!
//! Every positioned lane item is entered under its lane when it is received
//! and marked when its write returns. *Applied* is the last position before the oldest one
//! still being written; *searchable* trails it, each position due by what its
//! own write reported ([`Visibility`]). A caught-up write that returns
//! [`Visibility::Immediate`] makes every write that returned before it was
//! granted searchable, which the port promises; each returned write gets a
//! sequence number so that "before" is exact.
//!
//! ```text
//! positions   1 (writing)   2 (done, due 10:00:05)   3 (done, due now)
//! applied     none: 1 is still being written
//! 1 returns   applied 3; searchable 1 at once, then 2 and 3 at 10:00:05
//! ```
//!
//! Coverage by a caught-up write is the sink's (the `Sink::write` contract):
//! the returned-writes counter is shared by every lane.
//!
//! The state outlives a restart of the engine's run: a released item comes
//! back under the same position and takes its old place.

use std::collections::BTreeMap;
use std::ops::Bound;

use kernel::Position;
use sink::Visibility;
use stream::LaneKey;
use tokio::time::Instant;

/// The fence levels of one sink, per lane, over the positions it has received.
#[derive(Debug, Default)]
pub(super) struct Fences {
    lanes: BTreeMap<LaneKey, LaneFences>,
    /// Writes returned so far; the next one's sequence number.
    returned: u64,
    /// Writes that returned before this sequence number were covered by a
    /// caught-up write that returned: an idle refresh would add nothing.
    covered_below: u64,
    /// The latest time any written position was due at. It never moves back,
    /// so once it has passed no entry waits on a time.
    latest_due: Option<Instant>,
    /// Written positions waiting on a caught-up write (never fewer than there
    /// are). With none, and `latest_due` passed, no entry waits at all and the
    /// scans below are skipped: what one write at a time cost per item.
    deferred: usize,
}

/// One lane's positions, in lane (position) order.
#[derive(Debug, Default)]
struct LaneFences {
    entries: BTreeMap<Position, Entry>,
    /// The last position reported applied.
    applied: Option<Position>,
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
    /// A positioned item arrived on `lane`: it is being written until
    /// [`write_returned`](Self::write_returned). A redelivered item keeps its
    /// place.
    pub(super) fn received(&mut self, lane: &LaneKey, position: Position) {
        let fences = self.lanes.entry(lane.clone()).or_default();
        if fences.applied.is_some_and(|applied| position <= applied) {
            return;
        }
        fences.entries.entry(position).or_insert(Entry::Writing);
    }

    /// How many writes have returned: what a write granted now covers if it
    /// is caught up.
    pub(super) fn returned(&self) -> u64 {
        self.returned
    }

    /// A write returned, holding the items at `written` (each lane's
    /// position; one write may hold several lanes' items, merged). Each is
    /// written, due by the write's `visibility`. A caught-up write reporting
    /// `Immediate` makes every write that returned before `covers_below`
    /// searchable now, on every lane.
    pub(super) fn write_returned(
        &mut self,
        written: &[(LaneKey, Position)],
        visibility: Visibility,
        caught_up: Option<u64>,
        now: Instant,
    ) {
        let seq = self.returned;
        self.returned += 1;
        for (lane, position) in written {
            let Some(entry) = self
                .lanes
                .get_mut(lane)
                .and_then(|fences| fences.entries.get_mut(position))
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
        if let Some(covers_below) = caught_up {
            self.covered_below = self.covered_below.max(covers_below);
            if visibility == Visibility::Immediate && self.some_may_wait(now) {
                for fences in self.lanes.values_mut() {
                    for entry in fences.entries.values_mut() {
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

    /// `lane`'s new applied position, if its written run from the front
    /// moved.
    pub(super) fn advance_applied(&mut self, lane: &LaneKey) -> Option<Position> {
        let fences = self.lanes.get_mut(lane)?;
        let from = fences.applied.map_or(Bound::Unbounded, Bound::Excluded);
        let mut reached = None;
        for (position, entry) in fences.entries.range((from, Bound::Unbounded)) {
            match entry {
                Entry::Written { .. } => reached = Some(*position),
                Entry::Writing => break,
            }
        }
        if reached.is_some() {
            fences.applied = reached;
        }
        reached
    }

    /// For each lane whose searchable run moved, its last position now
    /// searchable, dropping it and everything before it.
    pub(super) fn advance_searchable(&mut self, now: Instant) -> Vec<(LaneKey, Position)> {
        let mut moved = Vec::new();
        for (lane, fences) in &mut self.lanes {
            let mut reached = None;
            while let Some((&position, entry)) = fences.entries.first_key_value() {
                match entry {
                    Entry::Written {
                        due: Due::At(at), ..
                    } if *at <= now => {
                        reached = Some(position);
                        fences.entries.pop_first();
                    }
                    _ => break,
                }
            }
            if let Some(position) = reached {
                moved.push((lane.clone(), position));
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
                    Entry::Written {
                        due: Due::At(at), ..
                    },
                )) => Some(*at),
                _ => None,
            })
            .min()
    }

    /// Whether a written position isn't searchable yet and no caught-up
    /// write that returned after it covered it: an idle engine sends an empty
    /// caught-up write.
    pub(super) fn wants_a_caught_up_write(&self, now: Instant) -> bool {
        self.some_may_wait(now)
            && self
                .lanes
                .values()
                .flat_map(|fences| fences.entries.values())
                .any(|entry| match entry {
                    Entry::Written { seq, due } => {
                        *seq >= self.covered_below
                            && match due {
                                Due::At(at) => *at > now,
                                Due::WithACaughtUpWrite => true,
                            }
                    }
                    Entry::Writing => false,
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

    #[test]
    fn applied_waits_for_the_oldest_write_and_searchable_follows_it() {
        let now = Instant::now();
        let users = lane("users");
        let mut fences = Fences::default();
        for position in 1..=3 {
            fences.received(&users, Position::new(position));
        }
        fences.write_returned(
            &[(users.clone(), Position::new(3))],
            Visibility::Immediate,
            None,
            now,
        );
        fences.write_returned(
            &[(users.clone(), Position::new(2))],
            Visibility::Within(Duration::from_secs(5)),
            None,
            now,
        );
        assert_eq!(fences.advance_applied(&users), None);
        assert!(fences.advance_searchable(now).is_empty());

        fences.write_returned(
            &[(users.clone(), Position::new(1))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(fences.advance_applied(&users), Some(Position::new(3)));
        assert_eq!(
            fences.advance_searchable(now),
            [(users.clone(), Position::new(1))]
        );
        assert_eq!(fences.next_due(), Some(now + Duration::from_secs(5)));
        let later = now + Duration::from_secs(5);
        assert_eq!(
            fences.advance_searchable(later),
            [(users, Position::new(3))]
        );
    }

    #[test]
    fn a_caught_up_write_covers_only_the_writes_that_returned_before_its_grant() {
        let now = Instant::now();
        let users = lane("users");
        let mut fences = Fences::default();
        fences.received(&users, Position::new(1));
        fences.received(&users, Position::new(2));
        fences.write_returned(
            &[(users.clone(), Position::new(1))],
            Visibility::Deferred,
            None,
            now,
        );
        let covers_below = fences.returned();
        fences.write_returned(
            &[(users.clone(), Position::new(2))],
            Visibility::Deferred,
            None,
            now,
        );
        assert!(fences.wants_a_caught_up_write(now));

        fences.write_returned(&[], Visibility::Immediate, Some(covers_below), now);
        assert_eq!(fences.advance_applied(&users), Some(Position::new(2)));
        assert_eq!(fences.advance_searchable(now), [(users, Position::new(1))]);
        assert!(
            fences.wants_a_caught_up_write(now),
            "2 returned after the caught-up write was granted"
        );
    }

    #[test]
    fn a_redelivered_position_keeps_its_place() {
        let now = Instant::now();
        let users = lane("users");
        let mut fences = Fences::default();
        fences.received(&users, Position::new(1));
        fences.received(&users, Position::new(2));
        fences.write_returned(
            &[(users.clone(), Position::new(2))],
            Visibility::Immediate,
            None,
            now,
        );
        fences.received(&users, Position::new(1));
        assert_eq!(fences.advance_applied(&users), None);
        fences.write_returned(
            &[(users.clone(), Position::new(1))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(fences.advance_applied(&users), Some(Position::new(2)));
    }

    /// Lanes keep their own order: a write still in flight on one index holds
    /// back neither the other index's applied nor its searchable position.
    #[test]
    fn a_write_in_flight_on_one_lane_holds_back_no_other_lane() {
        let now = Instant::now();
        let (users, orders) = (lane("users"), lane("orders"));
        let mut fences = Fences::default();
        fences.received(&users, Position::new(1));
        fences.received(&orders, Position::new(2));
        fences.write_returned(
            &[(orders.clone(), Position::new(2))],
            Visibility::Immediate,
            None,
            now,
        );
        assert_eq!(fences.advance_applied(&users), None);
        assert_eq!(fences.advance_applied(&orders), Some(Position::new(2)));
        assert_eq!(fences.advance_searchable(now), [(orders, Position::new(2))]);
    }
}
