//! How far a lane's backfill has durably got: the walk mark of its newest
//! snapshot item that landed with every earlier one of the walk (ADR 0012
//! stage 8b, `docs/adr/0012-stage-notes/stage-8b-resume.md`).
//!
//! Items land out of order (several writes at once, merged writes), so the
//! mark follows a contiguous prefix, as the fences do for positions: an item
//! counts once it and everything received before it on the lane landed.
//!
//! - **Landed** is a write that returned with the item acknowledged: a
//!   document quarantined under `skip` counts, a part rejected under `stop`
//!   does not, and neither does an item a stop acknowledged unwritten.
//! - **A release holds the mark.** Once any item of the lane is released,
//!   nacked or skipped, the mark moves no further until the walk ends: the
//!   item may come back after others, under a new delivery.
//! - **A walk ends at its `SnapshotComplete`**, and only the items before it
//!   are its own: items of the next walk may already be in the window (the
//!   run reads ahead), and they start the next walk's prefix. A release among
//!   them holds the next walk.
//! - **Recording** is every [`RECORD_EVERY`], never ahead of the prefix, one
//!   record in flight at a time.
//!
//! ```text
//! received  a1 a2 a3 a4         (lane order)
//! landed       a2    a4  a1      → prefix a2 (a3 still out)
//! ```

use std::collections::VecDeque;
use std::time::Duration;

use stream::WalkMark;
use tokio::time::Instant;

/// How often a lane records how far its backfill got, at most.
pub(super) const RECORD_EVERY: Duration = Duration::from_secs(10);

/// One lane's backfill progress within a run.
#[derive(Debug, Default)]
pub(super) struct Walked {
    /// The walk's items received and not yet in the prefix, in lane order.
    items: VecDeque<Item>,
    /// The newest mark of the landed prefix.
    reached: Option<WalkMark>,
    /// The mark last recorded, or being recorded.
    recorded: Option<WalkMark>,
    /// When the walk's first item arrived or the last record went out.
    since: Option<Instant>,
    recording: bool,
    /// The lane order of the latest item released: until the walk holding
    /// it ends, the prefix stops where it is.
    held: Option<u64>,
    /// The last item's lane order, for the debug check.
    last_seq: Option<u64>,
}

#[derive(Debug)]
struct Item {
    seq: u64,
    mark: Option<WalkMark>,
    landed: bool,
}

impl Walked {
    /// A snapshot item of the walk was received, `seq` its lane order.
    pub(super) fn received(&mut self, seq: u64, mark: Option<WalkMark>, now: Instant) {
        debug_assert!(
            self.last_seq.is_none_or(|last| seq > last),
            "a walk's items reach the tracker in lane order"
        );
        self.last_seq = Some(seq);
        self.since.get_or_insert(now);
        self.items.push_back(Item {
            seq,
            mark,
            landed: false,
        });
    }

    /// The item at `seq` landed. Moves the prefix over every landed item at
    /// its front.
    pub(super) fn landed(&mut self, seq: u64) {
        if let Some(item) = self.items.iter_mut().find(|item| item.seq == seq) {
            item.landed = true;
        }
        if self.held.is_some() {
            return;
        }
        while self.items.front().is_some_and(|item| item.landed) {
            if let Some(item) = self.items.pop_front()
                && item.mark.is_some()
            {
                self.reached = item.mark;
            }
        }
    }

    /// The item at `seq` was released, nacked or acknowledged unwritten: the
    /// prefix moves no further this walk.
    pub(super) fn hold(&mut self, seq: u64) {
        self.held = Some(self.held.map_or(seq, |held| held.max(seq)));
    }

    /// The walk whose `SnapshotComplete` is at `seq` ended: nothing of it is
    /// recorded any more. Items received after it (read ahead) start the next
    /// walk, which records from now.
    pub(super) fn ended(&mut self, seq: u64, now: Instant) {
        self.items.retain(|item| item.seq > seq);
        self.reached = None;
        self.recorded = None;
        self.since = (!self.items.is_empty()).then_some(now);
        self.held = self.held.filter(|held| *held > seq);
    }

    /// Forget every walk: what the lane's target held is gone, so no mark of
    /// any item received so far means anything in the new one.
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// The mark to record now, if one is due: the prefix moved past what was
    /// recorded, [`RECORD_EVERY`] has passed, and no record is in flight.
    pub(super) fn due(&mut self, now: Instant) -> Option<WalkMark> {
        if self.recording
            || self.reached.is_none()
            || self.reached == self.recorded
            || self
                .since
                .is_some_and(|since| now.duration_since(since) < RECORD_EVERY)
        {
            return None;
        }
        self.recording = true;
        self.since = Some(now);
        self.recorded.clone_from(&self.reached);
        self.reached.clone()
    }

    /// The record in flight returned; a failed one is tried again when the
    /// next is due.
    pub(super) fn record_returned(&mut self, ok: bool) {
        self.recording = false;
        if !ok {
            self.recorded = None;
        }
    }

    #[cfg(test)]
    fn reached(&self) -> Option<&str> {
        self.reached.as_ref().map(WalkMark::as_str)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn mark(text: &str) -> Option<WalkMark> {
        Some(WalkMark::new(text))
    }

    #[test]
    fn the_mark_never_passes_an_item_still_out() {
        let now = Instant::now();
        let mut walked = Walked::default();
        for (seq, key) in ["a1", "a2", "a3", "a4"].iter().enumerate() {
            walked.received(seq as u64, mark(key), now);
        }
        walked.landed(1);
        walked.landed(3);
        assert_eq!(walked.reached(), None, "a1 is still out");
        walked.landed(0);
        assert_eq!(walked.reached(), Some("a2"), "a3 is still out");
        walked.landed(2);
        assert_eq!(walked.reached(), Some("a4"));
    }

    #[test]
    fn a_released_item_holds_the_mark_back_without_the_run_ending() {
        let now = Instant::now();
        let mut walked = Walked::default();
        walked.received(0, mark("a1"), now);
        walked.received(1, mark("a2"), now);
        walked.landed(0);
        assert_eq!(walked.reached(), Some("a1"));
        walked.hold(1);
        walked.received(2, mark("a3"), now);
        walked.landed(1);
        walked.landed(2);
        assert_eq!(
            walked.reached(),
            Some("a1"),
            "a2 was released: whatever lands after it stays behind it"
        );
    }

    #[test]
    fn a_record_is_due_every_ten_seconds_once_the_prefix_moved() {
        let start = Instant::now();
        let mut walked = Walked::default();
        walked.received(0, mark("a1"), start);
        walked.landed(0);
        assert_eq!(walked.due(start + Duration::from_secs(9)), None);
        let first = walked.due(start + RECORD_EVERY);
        assert_eq!(first.as_ref().map(WalkMark::as_str), Some("a1"));
        assert_eq!(
            walked.due(start + RECORD_EVERY * 3),
            None,
            "one record in flight at a time"
        );
        walked.record_returned(true);
        assert_eq!(
            walked.due(start + RECORD_EVERY * 3),
            None,
            "nothing new landed"
        );
        walked.received(1, mark("a2"), start);
        walked.landed(1);
        assert_eq!(
            walked.due(start + RECORD_EVERY + Duration::from_secs(5)),
            None,
            "ten seconds since the last record"
        );
        assert_eq!(
            walked
                .due(start + RECORD_EVERY * 2)
                .as_ref()
                .map(WalkMark::as_str),
            Some("a2")
        );
        walked.record_returned(false);
        assert_eq!(
            walked
                .due(start + RECORD_EVERY * 3)
                .as_ref()
                .map(WalkMark::as_str),
            Some("a2"),
            "a failed record is tried again"
        );
    }

    #[test]
    fn a_walks_end_forgets_it() {
        let now = Instant::now();
        let mut walked = Walked::default();
        walked.received(0, mark("a1"), now);
        walked.landed(0);
        walked.hold(0);
        walked.ended(1, now);
        walked.received(2, mark("b1"), now);
        walked.landed(2);
        assert_eq!(walked.reached(), Some("b1"));
    }

    /// The run read b1 and b2 before a's `SnapshotComplete` (seq 1) ran: they
    /// are the next walk's, and its prefix waits for b1.
    #[test]
    fn the_next_walks_items_read_ahead_survive_the_end_of_the_last() {
        let now = Instant::now();
        let mut walked = Walked::default();
        walked.received(0, mark("a1"), now);
        walked.landed(0);
        walked.received(2, mark("b1"), now);
        walked.received(3, mark("b2"), now);
        walked.ended(1, now);
        assert_eq!(walked.reached(), None, "a1 is the old walk's");
        walked.landed(3);
        assert_eq!(walked.reached(), None, "b1 is still out");
        walked.landed(2);
        assert_eq!(walked.reached(), Some("b2"));
    }

    /// b1, read ahead of a's end, was released: the next walk is held too.
    #[test]
    fn a_release_read_ahead_holds_the_next_walk() {
        let now = Instant::now();
        let mut walked = Walked::default();
        walked.received(0, mark("a1"), now);
        walked.received(2, mark("b1"), now);
        walked.hold(2);
        walked.landed(0);
        walked.ended(1, now);
        walked.received(3, mark("b2"), now);
        walked.landed(2);
        walked.landed(3);
        assert_eq!(walked.reached(), None);
    }

    /// Staging the lane again forgets every walk received so far.
    #[test]
    fn a_reset_forgets_every_walk() {
        let now = Instant::now();
        let mut walked = Walked::default();
        walked.received(0, mark("a1"), now);
        walked.landed(0);
        walked.received(1, mark("a2"), now);
        walked.reset();
        walked.landed(1);
        assert_eq!(walked.reached(), None);
        walked.received(2, mark("b1"), now);
        walked.landed(2);
        assert_eq!(walked.reached(), Some("b1"));
    }

    /// A released item holds the rest of its walk, which can never count
    /// again: none of it is kept, however long the walk goes on, and landing
    /// one is a search, not a scan.
    #[test]
    fn a_held_walk_keeps_nothing_it_can_never_count() {
        let now = Instant::now();
        let mut walked = Walked::default();
        walked.received(0, mark("a0"), now);
        walked.received(1, mark("a1"), now);
        walked.hold(1);
        for seq in 2..20_000 {
            walked.received(seq, mark("a"), now);
            walked.landed(seq);
        }
        walked.landed(0);
        assert_eq!(walked.items.len(), 0, "kept items the walk can never count");
        assert_eq!(walked.reached(), None);
    }

    /// A seq the walk never received (a live item's, released before any
    /// backfill) holds nothing.
    #[test]
    fn releasing_an_item_the_walk_never_received_holds_nothing() {
        let now = Instant::now();
        let mut walked = Walked::default();
        walked.hold(7);
        walked.received(8, mark("a1"), now);
        walked.landed(8);
        assert_eq!(walked.reached(), Some("a1"));
    }

    /// Items land in a random order; at every step the mark is the newest
    /// key whose item and every earlier one landed (a model of the rule).
    #[test]
    fn random_landing_orders_keep_the_mark_on_the_landed_prefix() {
        let now = Instant::now();
        for seed in 1..=300u64 {
            let mut state = seed;
            let mut next = || {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            };
            let count = 1 + (next() % 40) as usize;
            let mut walked = Walked::default();
            for seq in 0..count {
                walked.received(seq as u64, mark(&format!("{seq:03}")), now);
            }
            let mut order: Vec<usize> = (0..count).collect();
            for i in (1..count).rev() {
                order.swap(i, (next() % (i as u64 + 1)) as usize);
            }
            let mut landed = vec![false; count];
            for seq in order {
                walked.landed(seq as u64);
                landed[seq] = true;
                let prefix = landed.iter().take_while(|done| **done).count();
                let expected = prefix.checked_sub(1).map(|last| format!("{last:03}"));
                assert_eq!(walked.reached().map(str::to_owned), expected, "seed {seed}");
            }
        }
    }
}
