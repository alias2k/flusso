//! The sink's two fence levels, kept in position order while writes finish
//! in any order.
//!
//! Every positioned lane item is entered when it is received and marked when
//! its write returns. *Applied* is the last position before the oldest one
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
//! The state outlives a restart of the engine's run: a released item comes
//! back under the same position and takes its old place.

use std::collections::BTreeMap;
use std::ops::Bound;

use kernel::Position;
use sink::Visibility;
use tokio::time::Instant;

/// The fence levels of one sink, over the positions it has received.
#[derive(Debug, Default)]
pub(super) struct Fences {
    entries: BTreeMap<Position, Entry>,
    /// The last position reported applied.
    applied: Option<Position>,
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
    /// A positioned item arrived: it is being written until
    /// [`write_returned`](Self::write_returned). A redelivered item keeps its
    /// place.
    pub(super) fn received(&mut self, position: Position) {
        if self.applied.is_some_and(|applied| position <= applied) {
            return;
        }
        self.entries.entry(position).or_insert(Entry::Writing);
    }

    /// How many writes have returned: what a write granted now covers if it
    /// is caught up.
    pub(super) fn returned(&self) -> u64 {
        self.returned
    }

    /// A write returned. Its position, if it has one, is written, due by the
    /// write's `visibility`. A caught-up write reporting `Immediate` makes
    /// every write that returned before `covers_below` searchable now.
    pub(super) fn write_returned(
        &mut self,
        position: Option<Position>,
        visibility: Visibility,
        caught_up: Option<u64>,
        now: Instant,
    ) {
        let seq = self.returned;
        self.returned += 1;
        if let Some(position) = position
            && let Some(entry) = self.entries.get_mut(&position)
        {
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
                for entry in self.entries.values_mut() {
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

    /// Whether some written position may still wait to be searchable: false
    /// only when none can.
    fn some_may_wait(&self, now: Instant) -> bool {
        self.deferred > 0 || self.latest_due.is_some_and(|at| at > now)
    }

    /// The new applied position, if the written run from the front moved.
    pub(super) fn advance_applied(&mut self) -> Option<Position> {
        let from = self.applied.map_or(Bound::Unbounded, Bound::Excluded);
        let mut reached = None;
        for (position, entry) in self.entries.range((from, Bound::Unbounded)) {
            match entry {
                Entry::Written { .. } => reached = Some(*position),
                Entry::Writing => break,
            }
        }
        if reached.is_some() {
            self.applied = reached;
        }
        reached
    }

    /// The last position now searchable, in order, dropping it and everything
    /// before it.
    pub(super) fn advance_searchable(&mut self, now: Instant) -> Option<Position> {
        let mut reached = None;
        while let Some((&position, entry)) = self.entries.first_key_value() {
            match entry {
                Entry::Written {
                    due: Due::At(at), ..
                } if *at <= now => {
                    reached = Some(position);
                    self.entries.pop_first();
                }
                _ => break,
            }
        }
        reached
    }

    /// When the oldest position becomes searchable, if that is a matter of
    /// time.
    pub(super) fn next_due(&self) -> Option<Instant> {
        match self.entries.first_key_value() {
            Some((
                _,
                Entry::Written {
                    due: Due::At(at), ..
                },
            )) => Some(*at),
            _ => None,
        }
    }

    /// Whether a written position isn't searchable yet and no caught-up
    /// write that returned after it covered it: an idle engine sends an empty
    /// caught-up write.
    pub(super) fn wants_a_caught_up_write(&self, now: Instant) -> bool {
        self.some_may_wait(now)
            && self.entries.values().any(|entry| match entry {
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

    use super::*;

    #[test]
    fn applied_waits_for_the_oldest_write_and_searchable_follows_it() {
        let now = Instant::now();
        let mut fences = Fences::default();
        for position in 1..=3 {
            fences.received(Position(position));
        }
        fences.write_returned(Some(Position(3)), Visibility::Immediate, None, now);
        fences.write_returned(
            Some(Position(2)),
            Visibility::Within(Duration::from_secs(5)),
            None,
            now,
        );
        assert_eq!(fences.advance_applied(), None);
        assert_eq!(fences.advance_searchable(now), None);

        fences.write_returned(Some(Position(1)), Visibility::Immediate, None, now);
        assert_eq!(fences.advance_applied(), Some(Position(3)));
        assert_eq!(fences.advance_searchable(now), Some(Position(1)));
        assert_eq!(fences.next_due(), Some(now + Duration::from_secs(5)));
        let later = now + Duration::from_secs(5);
        assert_eq!(fences.advance_searchable(later), Some(Position(3)));
    }

    #[test]
    fn a_caught_up_write_covers_only_the_writes_that_returned_before_its_grant() {
        let now = Instant::now();
        let mut fences = Fences::default();
        fences.received(Position(1));
        fences.received(Position(2));
        fences.write_returned(Some(Position(1)), Visibility::Deferred, None, now);
        let covers_below = fences.returned();
        fences.write_returned(Some(Position(2)), Visibility::Deferred, None, now);
        assert!(fences.wants_a_caught_up_write(now));

        fences.write_returned(None, Visibility::Immediate, Some(covers_below), now);
        assert_eq!(fences.advance_applied(), Some(Position(2)));
        assert_eq!(fences.advance_searchable(now), Some(Position(1)));
        assert!(
            fences.wants_a_caught_up_write(now),
            "2 returned after the caught-up write was granted"
        );
    }

    #[test]
    fn a_redelivered_position_keeps_its_place() {
        let now = Instant::now();
        let mut fences = Fences::default();
        fences.received(Position(1));
        fences.received(Position(2));
        fences.write_returned(Some(Position(2)), Visibility::Immediate, None, now);
        fences.received(Position(1));
        assert_eq!(fences.advance_applied(), None);
        fences.write_returned(Some(Position(1)), Visibility::Immediate, None, now);
        assert_eq!(fences.advance_applied(), Some(Position(2)));
    }
}
