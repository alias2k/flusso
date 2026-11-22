//! One index's pending documents: the ids marked for a build and not yet
//! published, each with the oldest change that touched it (ADR 0012 stage 8a).
//!
//! **Waiting** ids are marked and not taken; a **build** takes them under a
//! [`Token`] and is released once published on every lane. What holds
//! confirmation back is the oldest of both: the first waiting id in position
//! order, or the oldest change of a build.
//!
//! A build goes through three steps, each changing who answers for it:
//!
//! | Step | Admission (a take skips its ids) | Holds a lane back as |
//! | --- | --- | --- |
//! | taken, running | yes | the index's **backlog** |
//! | landed | no, as on `main`: a later build of an id gets a later slot, so it publishes after | backlog |
//! | sending | no | the **lane's own**, until its item is registered there |
//!
//! - A mark keeps an id's **oldest** position: one build covers every change
//!   before it, since a build reads the database as it is when it runs.
//! - A mark is **unconditional**: an id building is marked waiting again, since
//!   that build may have read before the change.
//! - A take **skips** ids building and ids the caller says are held (a running
//!   backfill build of them), so a taken id is never built twice at once.
//! - What a run leaves behind (it failed before publishing) is **forgotten**
//!   when the next run starts: confirmation stayed before all of it, so the
//!   replayed stream marks it again, in batches of `max_changes`, as `main`
//!   rebuilt it.
//! - A build holds a lane back only until its item is **registered** on that
//!   lane, or the lane is told the batch had nothing for it (the item then
//!   holds it, from the same oldest change), so a full lane holds back only
//!   itself; once sending, a lane that hasn't taken the item answers for it.
//!
//! ```text
//! mark a@1 b@2 a@3   waiting a@1 b@2            minimum 1
//! take               building {a@1, b@2}        minimum 1
//! mark a@4           waiting a@4, building …    minimum 1
//! release            waiting a@4                minimum 4
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use kernel::{IndexName, Position, SinkName};
use source::RowKey;
use source::document::DocumentId;

// How many waiting entries this thread's pending sets looked at to answer
// a grant check (`scan`): what a test bounds, since time can't be.
#[cfg(test)]
thread_local! {
    pub(crate) static LOOKED_AT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn looked_at(entries: usize) {
    LOOKED_AT.with(|looked| looked.set(looked.get() + entries));
}

/// Names one build of an index's pending documents, from its take to its
/// release or give-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token(u64);

/// The ids a running build holds outside a pending set (a backfill
/// build's), offered one by one to `found` until it answers `true`;
/// whether it did.
pub(crate) type Held<'a> = &'a dyn Fn(&mut dyn FnMut(&RowKey) -> bool) -> bool;

/// What a take moved from waiting to building.
#[derive(Debug)]
pub struct Taken {
    /// The build's token: release it once the build is published on every
    /// lane, give it back if the build is not.
    pub token: Token,
    /// The ids to build, oldest change first.
    pub ids: Vec<DocumentId>,
    /// The oldest change among them.
    pub oldest: Position,
    /// How many changes had been read through the one before the oldest
    /// document the take left waiting, or, with none left, every change read
    /// when it was taken: every document of the index touched by a change
    /// through it is in this build or an earlier one (ADR 0012 stage 8a,
    /// §3.9 I5). What the build's step carries.
    pub frontier: u64,
}

/// What [`Pending::forget`] dropped.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Forgotten {
    /// How many ids were waiting.
    pub(crate) waiting: usize,
    /// The least read number left unbuilt.
    pub(crate) least_read: Option<u64>,
}

/// One index's waiting and building ids.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    /// Each waiting id: its oldest change not yet built, the order counter,
    /// and how many changes had been read through that change (its read
    /// number).
    waiting: HashMap<Arc<RowKey>, (Position, u64, u64)>,
    /// The waiting ids by `(seq, counter)`: oldest first; ids sharing a seq
    /// stay apart by the counter.
    order: BTreeMap<(u64, u64), Arc<RowKey>>,
    counter: u64,
    building: HashSet<Arc<RowKey>>,
    builds: Vec<Build>,
    next_token: u64,
}

#[derive(Debug)]
struct Build {
    token: Token,
    oldest: Position,
    /// The least read number among its ids: what a run ending before it is
    /// released leaves unbuilt.
    least_read: u64,
    ids: Vec<(Arc<RowKey>, Position)>,
    /// The sinks whose lane of the index has the build's item registered,
    /// or was told the batch had nothing for it.
    registered: Vec<SinkName>,
    /// The build returned: its ids are no longer admission's.
    landed: bool,
    /// Its batch is being published to the index's lanes.
    sending: bool,
}

impl Pending {
    /// Mark `key` as touched by the change at `at`, the `read`th read.
    /// Returns whether it started waiting (it wasn't already).
    pub(crate) fn mark(&mut self, key: RowKey, at: Position, read: u64) -> bool {
        match self.waiting.get(&key) {
            Some((held, _, _)) if held.seq <= at.seq => false,
            Some(&(held, counter, _)) => {
                if let Some(arc) = self.order.remove(&(held.seq, counter)) {
                    self.insert(arc, at, read);
                }
                false
            }
            None => {
                self.insert(Arc::new(key), at, read);
                true
            }
        }
    }

    fn insert(&mut self, key: Arc<RowKey>, at: Position, read: u64) {
        let counter = self.counter;
        self.counter += 1;
        self.order.insert((at.seq, counter), Arc::clone(&key));
        self.waiting.insert(key, (at, counter, read));
    }

    /// The newest position among the documents waiting.
    pub(crate) fn newest_waiting(&self) -> Option<Position> {
        self.order
            .last_key_value()
            .and_then(|(_, key)| self.waiting.get(key))
            .map(|(position, _, _)| *position)
    }

    /// The read number of the oldest waiting id; `None` while none waits.
    /// Within one engine run read numbers follow positions, so the oldest by
    /// position is the least.
    pub(crate) fn least_waiting_read(&self) -> Option<u64> {
        self.order
            .first_key_value()
            .and_then(|(_, key)| self.waiting.get(key))
            .map(|(_, _, read)| *read)
    }

    /// The least read number of what is not yet on its lanes: a document
    /// waiting, or one in a build taken and not yet released.
    pub(crate) fn least_unbuilt(&self) -> Option<u64> {
        // The oldest waiting by position is the least read: reads follow
        // the stream's order, and a document marked again keeps its older
        // change.
        self.least_waiting_read()
            .into_iter()
            .chain(self.builds.iter().map(|build| build.least_read))
            .min()
    }

    /// Whether every document marked at or before `last` has been taken
    /// into `token`'s build or one already published: none waits, and no
    /// other build not yet released holds one.
    pub(crate) fn all_taken_through(&self, last: Position, token: Option<Token>) -> bool {
        self.oldest_waiting()
            .is_none_or(|oldest| oldest.seq > last.seq)
            && self
                .builds
                .iter()
                .filter(|build| Some(build.token) != token)
                .all(|build| build.oldest.seq > last.seq)
    }

    /// Nothing waits and no build is taken and not yet released: every
    /// document marked is on its lanes.
    pub(crate) fn is_idle(&self) -> bool {
        self.waiting.is_empty() && self.builds.is_empty()
    }

    /// Move up to `max` waiting ids of `index`, oldest first, to a new build,
    /// skipping ids building and ids `held` says a running build holds, and
    /// ids marked after `through` (a newer run's: ADR 0012 stage 8a, R4).
    /// `None` when nothing could be taken.
    pub(crate) fn take(
        &mut self,
        index: &IndexName,
        max: usize,
        through: Option<Position>,
        held: &dyn Fn(&RowKey) -> bool,
    ) -> Option<Taken> {
        let chosen: Vec<(u64, u64)> = self
            .through(through)
            .filter(|(_, key)| !self.building.contains(*key) && !held(key))
            .take(max)
            .map(|(at, _)| *at)
            .collect();
        if chosen.is_empty() {
            return None;
        }
        let mut ids = Vec::with_capacity(chosen.len());
        let mut taken = Vec::with_capacity(chosen.len());
        let mut oldest: Option<Position> = None;
        let mut least_read = u64::MAX;
        for at in chosen {
            let Some(key) = self.order.remove(&at) else {
                continue;
            };
            let Some((position, _, read)) = self.waiting.remove(&key) else {
                continue;
            };
            least_read = least_read.min(read);
            oldest = Some(oldest.map_or(position, |o| o.min(position)));
            ids.push(DocumentId {
                index: index.clone(),
                key: (*key).clone(),
            });
            self.building.insert(Arc::clone(&key));
            taken.push((key, position));
        }
        let oldest = oldest?;
        let token = Token(self.next_token);
        self.next_token += 1;
        self.builds.push(Build {
            token,
            oldest,
            least_read,
            ids: taken,
            registered: Vec::new(),
            landed: false,
            sending: false,
        });
        Some(Taken {
            token,
            ids,
            oldest,
            frontier: 0,
        })
    }

    /// The build under `token` returned: its ids leave admission, so a later
    /// change to one of them can be built in a later slot (which publishes
    /// after this one).
    pub(crate) fn land(&mut self, token: Token) {
        let Some(build) = self.builds.iter_mut().find(|build| build.token == token) else {
            return;
        };
        if std::mem::replace(&mut build.landed, true) {
            return;
        }
        for (key, _) in &build.ids {
            self.building.remove(key);
        }
    }

    /// The build under `token` is being published: a lane that hasn't
    /// registered its item now answers for it.
    pub(crate) fn sending(&mut self, token: Token) {
        if let Some(build) = self.builds.iter_mut().find(|build| build.token == token) {
            build.sending = true;
        }
    }

    /// The build under `token` is published on every lane: it holds nothing
    /// back. Returns how many ids it held.
    pub(crate) fn release(&mut self, token: Token) -> usize {
        let Some(build) = self.remove_build(token) else {
            return 0;
        };
        if !build.landed {
            for (key, _) in &build.ids {
                self.building.remove(key);
            }
        }
        build.ids.len()
    }

    /// Forget everything: the run that marked and took it ended before
    /// publishing it. Confirmation never passed any of it, so the next run's
    /// stream replays every change and marks the documents again. Returns how
    /// many ids were waiting, and the least read number left unbuilt
    /// (waiting or in a build never released).
    pub(crate) fn forget(&mut self) -> Forgotten {
        let forgotten = Forgotten {
            waiting: self.waiting.len(),
            least_read: self
                .waiting
                .values()
                .map(|(_, _, read)| *read)
                .chain(self.builds.iter().map(|build| build.least_read))
                .min(),
        };
        self.waiting.clear();
        self.order.clear();
        self.building.clear();
        self.builds.clear();
        forgotten
    }

    fn remove_build(&mut self, token: Token) -> Option<Build> {
        let at = self.builds.iter().position(|build| build.token == token)?;
        Some(self.builds.remove(at))
    }

    /// The build under `token` has its item registered on `sink`'s lane.
    pub(crate) fn registered(&mut self, token: Token, sink: &SinkName) {
        if let Some(build) = self.builds.iter_mut().find(|build| build.token == token)
            && !build.registered.contains(sink)
        {
            build.registered.push(sink.clone());
        }
    }

    /// The oldest change the index's backlog holds back on `sink`'s lane:
    /// the first waiting id, or the oldest of a build not yet being published
    /// nor registered there.
    pub(crate) fn backlog_for(&self, sink: &SinkName) -> Option<Position> {
        let waiting = self
            .order
            .first_key_value()
            .and_then(|(_, key)| self.waiting.get(key))
            .map(|(position, _, _)| *position);
        self.builds
            .iter()
            .filter(|build| !build.sending && !build.registered.contains(sink))
            .map(|build| build.oldest)
            .chain(waiting)
            .min()
    }

    /// The oldest change of a build being published that `sink`'s lane
    /// hasn't registered: the lane's to answer for.
    pub(crate) fn sent_to(&self, sink: &SinkName) -> Option<Position> {
        self.builds
            .iter()
            .filter(|build| build.sending && !build.registered.contains(sink))
            .map(|build| build.oldest)
            .min()
    }

    /// The oldest change of an id not built yet: one waiting, or in a build
    /// no lane has been sent yet (slice 7's backlog age).
    pub(crate) fn oldest_unbuilt(&self) -> Option<Position> {
        self.builds
            .iter()
            .filter(|build| !build.sending)
            .map(|build| build.oldest)
            .chain(self.oldest_waiting())
            .min()
    }

    /// The oldest change any of its ids holds back on any lane.
    #[cfg(test)]
    pub(crate) fn minimum(&self) -> Option<Position> {
        let waiting = self
            .order
            .first_key_value()
            .and_then(|(_, key)| self.waiting.get(key))
            .map(|(position, _, _)| *position);
        self.builds
            .iter()
            .map(|build| build.oldest)
            .chain(waiting)
            .min()
    }

    /// The waiting ids marked at or before `through` (every one for
    /// `None`), oldest first.
    fn through(
        &self,
        through: Option<Position>,
    ) -> std::collections::btree_map::Range<'_, (u64, u64), Arc<RowKey>> {
        let last = through.map_or(u64::MAX, |position| position.seq);
        self.order.range(..=(last, u64::MAX))
    }

    /// Whether some waiting id marked at or before `through` (any, for
    /// `None`) is building, or is one `held` names (a running build's ids
    /// outside this set): a take now would leave it behind. Answered from
    /// the builds' side, one lookup per id building or held, never a walk
    /// of what waits: a change touching many documents keeps parts of it
    /// building while the rest waits.
    pub(crate) fn any_held(&self, held: Option<Held<'_>>, through: Option<Position>) -> bool {
        let last = through.map_or(u64::MAX, |position| position.seq);
        let mut blocks = |key: &RowKey| {
            #[cfg(test)]
            looked_at(1);
            self.waiting
                .get(key)
                .is_some_and(|(position, _, _)| position.seq <= last)
        };
        self.building.iter().any(|key| blocks(key)) || held.is_some_and(|held| held(&mut blocks))
    }

    /// How many ids wait that were marked at or before `through`, counted up
    /// to `most`, or `None` when one of them is building or `held` names it
    /// ([`any_held`](Self::any_held)): work bounded by what builds and
    /// `most`, not by what waits.
    pub(crate) fn scan(
        &self,
        through: Position,
        held: Option<Held<'_>>,
        most: usize,
    ) -> Option<usize> {
        if self.any_held(held, Some(through)) {
            return None;
        }
        Some(self.waiting_through_at_most(Some(through), most))
    }

    /// How many ids wait.
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// How many ids wait that were marked at or before `through`: every one,
    /// at no cost, when none was marked after it.
    #[cfg(test)]
    pub(crate) fn waiting_through(&self, through: Option<Position>) -> usize {
        self.waiting_through_at_most(through, usize::MAX)
    }

    /// How many ids wait that were marked at or before `through`, counted up
    /// to `most`: every one, at no cost, when none was marked after it.
    pub(crate) fn waiting_through_at_most(&self, through: Option<Position>, most: usize) -> usize {
        let newest = self.order.last_key_value().map(|((seq, _), _)| *seq);
        match (through, newest) {
            (Some(through), Some(newest)) if newest > through.seq => {
                let count = self.through(Some(through)).take(most).count();
                #[cfg(test)]
                looked_at(count);
                count
            }
            _ => self.waiting.len().min(most),
        }
    }

    /// Whether some id marked at or before `through` waits.
    pub(crate) fn waits_through(&self, through: Position) -> bool {
        self.oldest_waiting()
            .is_some_and(|oldest| oldest.seq <= through.seq)
    }

    /// The position of the oldest waiting id; `None` while none waits.
    pub(crate) fn oldest_waiting(&self) -> Option<Position> {
        self.order
            .first_key_value()
            .and_then(|(_, key)| self.waiting.get(key))
            .map(|(position, _, _)| *position)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use kernel::{ColumnName, GenericValue};

    use super::*;

    fn key(id: i64) -> RowKey {
        RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::BigInt(id),
        )])
    }

    fn p(seq: u64) -> Position {
        Position::new(seq)
    }

    fn index() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    fn none(_: &RowKey) -> bool {
        false
    }

    fn ids(taken: &Taken) -> Vec<RowKey> {
        taken.ids.iter().map(|id| id.key.clone()).collect()
    }

    #[test]
    fn a_document_marked_twice_keeps_its_oldest_position() {
        let mut pending = Pending::default();
        assert!(pending.mark(key(1), p(3), 3));
        assert!(!pending.mark(key(1), p(5), 5));
        assert_eq!(pending.waiting(), 1);
        assert_eq!(pending.minimum(), Some(p(3)));
        let taken = pending.take(&index(), usize::MAX, None, &none).unwrap();
        assert_eq!(ids(&taken), [key(1)]);
        assert_eq!(taken.oldest, p(3));
    }

    #[test]
    fn ids_are_taken_oldest_first_and_ids_sharing_a_change_are_both_kept() {
        let mut pending = Pending::default();
        pending.mark(key(2), p(4), 4);
        pending.mark(key(1), p(1), 1);
        pending.mark(key(3), p(1), 1);
        let taken = pending.take(&index(), 2, None, &none).unwrap();
        assert_eq!(ids(&taken), [key(1), key(3)]);
        assert_eq!(pending.waiting(), 1);
        assert_eq!(pending.minimum(), Some(p(1)), "the build holds change 1");
    }

    /// A change to a document whose build is running waits again: that
    /// build may have read before the change. The waiting id is not taken
    /// while the build runs, and is once it is released.
    #[test]
    fn a_mark_while_its_document_builds_waits_for_the_next_build() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1), 1);
        let first = pending.take(&index(), usize::MAX, None, &none).unwrap();
        assert!(pending.mark(key(1), p(3), 3));
        assert!(
            pending.take(&index(), usize::MAX, None, &none).is_none(),
            "never built twice at once"
        );
        assert_eq!(pending.minimum(), Some(p(1)));
        assert_eq!(pending.release(first.token), 1);
        assert_eq!(pending.minimum(), Some(p(3)));
        let second = pending.take(&index(), usize::MAX, None, &none).unwrap();
        assert_eq!(second.oldest, p(3));
    }

    #[test]
    fn a_take_skips_what_a_running_build_holds() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1), 1);
        pending.mark(key(2), p(2), 2);
        let held = |key_: &RowKey| *key_ == key(1);
        let taken = pending.take(&index(), usize::MAX, None, &held).unwrap();
        assert_eq!(ids(&taken), [key(2)]);
        assert_eq!(pending.minimum(), Some(p(1)), "1 still waits");
    }

    #[test]
    fn forgetting_leaves_nothing_waiting_or_building() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1), 1);
        let taken = pending.take(&index(), usize::MAX, None, &none).unwrap();
        pending.mark(key(2), p(2), 2);
        assert_eq!(
            pending.forget(),
            Forgotten {
                waiting: 1,
                least_read: Some(1),
            },
            "the build of read 1 never landed"
        );
        assert_eq!(pending.minimum(), None);
        assert_eq!(pending.release(taken.token), 0, "the build is gone");
        pending.mark(key(1), p(3), 3);
        let again = pending.take(&index(), usize::MAX, None, &none).unwrap();
        assert_eq!(again.oldest, p(3), "marked afresh by the replay");
        assert_ne!(again.token, taken.token);
    }

    /// A landed build leaves admission: a later change to one of its ids is
    /// taken into a later build while it waits to be published.
    #[test]
    fn a_landed_build_lets_its_documents_be_taken_again() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1), 1);
        let first = pending.take(&index(), usize::MAX, None, &none).unwrap();
        pending.mark(key(1), p(2), 2);
        assert!(pending.take(&index(), usize::MAX, None, &none).is_none());
        pending.land(first.token);
        let second = pending.take(&index(), usize::MAX, None, &none).unwrap();
        assert_eq!(second.oldest, p(2));
        pending.release(first.token);
        assert_eq!(
            pending.minimum(),
            Some(p(2)),
            "the second build still holds it"
        );
        assert!(
            pending.take(&index(), usize::MAX, None, &none).is_none(),
            "releasing the first leaves the second's document building"
        );
    }

    #[test]
    fn the_minimum_follows_waiting_and_building_ids_and_clears() {
        let mut pending = Pending::default();
        assert_eq!(pending.minimum(), None);
        pending.mark(key(1), p(4), 4);
        let taken = pending.take(&index(), usize::MAX, None, &none).unwrap();
        pending.mark(key(2), p(6), 6);
        assert_eq!(pending.minimum(), Some(p(4)));
        pending.release(taken.token);
        assert_eq!(pending.minimum(), Some(p(6)));
        let last = pending.take(&index(), usize::MAX, None, &none).unwrap();
        pending.release(last.token);
        assert_eq!(pending.minimum(), None);
        assert_eq!(pending.release(last.token), 0, "a token is released once");
    }

    /// R4 of ADR 0012 stage 8a: a take reaches no id marked after its
    /// candidate's last position, so a step closed through that run never
    /// claims a newer run's change. An id a newer run marks again keeps its
    /// older mark, and so is taken: the build reads it after both changes.
    #[test]
    fn a_take_through_a_position_leaves_newer_marks_waiting() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1), 1);
        pending.mark(key(2), p(2), 2);
        pending.mark(key(3), p(5), 5);
        pending.mark(key(1), p(6), 6);
        assert_eq!(pending.waiting_through(Some(p(2))), 2);
        assert_eq!(pending.waiting_through(Some(p(5))), 3);
        assert_eq!(pending.waiting_through(None), 3);
        let three = |found: &mut dyn FnMut(&RowKey) -> bool| found(&key(3));
        assert!(!pending.any_held(Some(&three), Some(p(2))));
        assert!(pending.any_held(Some(&three), Some(p(5))));
        let taken = pending
            .take(&index(), usize::MAX, Some(p(2)), &none)
            .unwrap();
        assert_eq!(ids(&taken), [key(1), key(2)]);
        assert_eq!(pending.waiting(), 1, "3, a newer run's, waits");
        assert_eq!(pending.waiting_through(Some(p(4))), 0);
        assert!(
            pending
                .take(&index(), usize::MAX, Some(p(4)), &none)
                .is_none()
        );
    }

    /// The one walk a grant pass makes per index: how many wait through a
    /// position, or `None` when one of those is building or held; ids marked
    /// after the position are neither counted nor looked at.
    #[test]
    fn a_scan_counts_through_a_position_and_stops_at_a_held_id() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1), 1);
        pending.mark(key(2), p(2), 2);
        pending.mark(key(3), p(5), 5);
        assert_eq!(pending.scan(p(2), None, usize::MAX), Some(2));
        assert_eq!(pending.scan(p(9), None, usize::MAX), Some(3));
        assert_eq!(pending.scan(p(9), None, 2), Some(2), "counted up to 2");
        let three = |found: &mut dyn FnMut(&RowKey) -> bool| found(&key(3));
        assert_eq!(
            pending.scan(p(2), Some(&three), usize::MAX),
            Some(2),
            "3 is past it"
        );
        assert_eq!(pending.scan(p(5), Some(&three), usize::MAX), None);
        let taken = pending.take(&index(), 1, None, &none).unwrap();
        pending.mark(key(1), p(6), 6);
        assert_eq!(
            pending.scan(p(9), None, usize::MAX),
            None,
            "1 builds and waits again"
        );
        pending.release(taken.token);
        assert_eq!(pending.scan(p(9), None, usize::MAX), Some(3));
    }
}
