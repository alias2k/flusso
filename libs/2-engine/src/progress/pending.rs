//! One index's pending documents: the ids marked for a build and not yet
//! published, each with the oldest change that touched it (ADR 0012 stage 8a).
//!
//! **Waiting** ids are marked and not taken; **building** ids were taken under
//! a [`Token`] and are released once that build is published on every lane.
//! What holds confirmation back is the oldest of both: the first waiting id in
//! position order, or the oldest change of a build.
//!
//! - A mark keeps an id's **oldest** position: one build covers every change
//!   before it, since a build reads the database as it is when it runs.
//! - A mark is **unconditional**: an id building is marked waiting again, since
//!   that build may have read before the change.
//! - A take **skips** ids building and ids the caller says are held (a running
//!   backfill build of them), so a taken id is never built twice at once.
//! - A build **given back** (it failed, or its run ended unpublished) returns
//!   each id at the older of its taken position and any newer mark.
//! - A build holds a lane back only until its item is **registered** on that
//!   lane (the item then holds it, from the same oldest change), so a full
//!   lane holds back only itself.
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

/// Names one build of an index's pending documents, from its take to its
/// release or give-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token(u64);

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
    /// The newest change any of them waited at.
    pub newest: Position,
}

/// One index's waiting and building ids.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    waiting: HashMap<Arc<RowKey>, (Position, u64)>,
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
    ids: Vec<(Arc<RowKey>, Position)>,
    /// The sinks whose lane of the index has the build's item registered.
    registered: Vec<SinkName>,
}

impl Pending {
    /// Mark `key` as touched by the change at `at`. Returns whether it
    /// started waiting (it wasn't already).
    pub(crate) fn mark(&mut self, key: RowKey, at: Position) -> bool {
        match self.waiting.get(&key) {
            Some((held, _)) if held.seq <= at.seq => false,
            Some(&(held, counter)) => {
                if let Some(arc) = self.order.remove(&(held.seq, counter)) {
                    self.insert(arc, at);
                }
                false
            }
            None => {
                self.insert(Arc::new(key), at);
                true
            }
        }
    }

    fn insert(&mut self, key: Arc<RowKey>, at: Position) {
        let counter = self.counter;
        self.counter += 1;
        self.order.insert((at.seq, counter), Arc::clone(&key));
        self.waiting.insert(key, (at, counter));
    }

    /// Move up to `max` waiting ids of `index`, oldest first, to a new build,
    /// skipping ids building and ids `held` says a running build holds.
    /// `None` when nothing could be taken.
    pub(crate) fn take(
        &mut self,
        index: &IndexName,
        max: usize,
        held: &dyn Fn(&RowKey) -> bool,
    ) -> Option<Taken> {
        let chosen: Vec<(u64, u64)> = self
            .order
            .iter()
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
        let mut newest: Option<Position> = None;
        for at in chosen {
            let Some(key) = self.order.remove(&at) else {
                continue;
            };
            let Some((position, _)) = self.waiting.remove(&key) else {
                continue;
            };
            oldest = Some(oldest.map_or(position, |o| o.min(position)));
            newest = Some(newest.map_or(position, |n| n.max(position)));
            ids.push(DocumentId {
                index: index.clone(),
                key: (*key).clone(),
            });
            self.building.insert(Arc::clone(&key));
            taken.push((key, position));
        }
        let (oldest, newest) = oldest.zip(newest)?;
        let token = Token(self.next_token);
        self.next_token += 1;
        self.builds.push(Build {
            token,
            oldest,
            ids: taken,
            registered: Vec::new(),
        });
        Some(Taken {
            token,
            ids,
            oldest,
            newest,
        })
    }

    /// The build under `token` is published on every lane: its ids are no
    /// longer building. Returns how many ids it held.
    pub(crate) fn release(&mut self, token: Token) -> usize {
        let Some(build) = self.remove_build(token) else {
            return 0;
        };
        for (key, _) in &build.ids {
            self.building.remove(key);
        }
        build.ids.len()
    }

    /// The build under `token` won't be published: each id waits again, at
    /// the older of its taken position and any newer mark. Returns how many
    /// ids started waiting again.
    pub(crate) fn give_back(&mut self, token: Token) -> usize {
        let Some(build) = self.remove_build(token) else {
            return 0;
        };
        let mut waiting = 0;
        for (key, position) in build.ids {
            self.building.remove(&key);
            match self.waiting.get(&key) {
                Some(&(held, counter)) => {
                    if position.seq < held.seq
                        && let Some(arc) = self.order.remove(&(held.seq, counter))
                    {
                        self.insert(arc, position);
                    }
                }
                None => {
                    self.insert(key, position);
                    waiting += 1;
                }
            }
        }
        waiting
    }

    /// Every build's token, oldest first.
    pub(crate) fn tokens(&self) -> Vec<Token> {
        self.builds.iter().map(|build| build.token).collect()
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

    /// The oldest change any of its ids holds back on `sink`'s lane: the
    /// first waiting id, or the oldest of a build not yet registered there.
    pub(crate) fn minimum_for(&self, sink: &SinkName) -> Option<Position> {
        let waiting = self
            .order
            .first_key_value()
            .and_then(|(_, key)| self.waiting.get(key))
            .map(|(position, _)| *position);
        self.builds
            .iter()
            .filter(|build| !build.registered.contains(sink))
            .map(|build| build.oldest)
            .chain(waiting)
            .min()
    }

    /// The oldest change any of its ids holds back on any lane.
    #[cfg(test)]
    pub(crate) fn minimum(&self) -> Option<Position> {
        let waiting = self
            .order
            .first_key_value()
            .and_then(|(_, key)| self.waiting.get(key))
            .map(|(position, _)| *position);
        self.builds
            .iter()
            .map(|build| build.oldest)
            .chain(waiting)
            .min()
    }

    /// Whether some waiting id is building, or `held` says a running build
    /// holds it: a take now would leave it behind.
    pub(crate) fn any_held(&self, held: Option<&dyn Fn(&RowKey) -> bool>) -> bool {
        if self.building.is_empty() && held.is_none() {
            return false;
        }
        self.order
            .values()
            .any(|key| self.building.contains(key) || held.is_some_and(|held| held(key)))
    }

    /// How many ids wait.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
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
        assert!(pending.mark(key(1), p(3)));
        assert!(!pending.mark(key(1), p(5)));
        assert_eq!(pending.waiting(), 1);
        assert_eq!(pending.minimum(), Some(p(3)));
        let taken = pending.take(&index(), usize::MAX, &none).unwrap();
        assert_eq!(ids(&taken), [key(1)]);
        assert_eq!(taken.oldest, p(3));
    }

    #[test]
    fn ids_are_taken_oldest_first_and_ids_sharing_a_change_are_both_kept() {
        let mut pending = Pending::default();
        pending.mark(key(2), p(4));
        pending.mark(key(1), p(1));
        pending.mark(key(3), p(1));
        let taken = pending.take(&index(), 2, &none).unwrap();
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
        pending.mark(key(1), p(1));
        let first = pending.take(&index(), usize::MAX, &none).unwrap();
        assert!(pending.mark(key(1), p(3)));
        assert!(
            pending.take(&index(), usize::MAX, &none).is_none(),
            "never built twice at once"
        );
        assert_eq!(pending.minimum(), Some(p(1)));
        assert_eq!(pending.release(first.token), 1);
        assert_eq!(pending.minimum(), Some(p(3)));
        let second = pending.take(&index(), usize::MAX, &none).unwrap();
        assert_eq!(second.oldest, p(3));
    }

    #[test]
    fn a_take_skips_what_a_running_build_holds() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1));
        pending.mark(key(2), p(2));
        let held = |key_: &RowKey| *key_ == key(1);
        let taken = pending.take(&index(), usize::MAX, &held).unwrap();
        assert_eq!(ids(&taken), [key(2)]);
        assert_eq!(pending.minimum(), Some(p(1)), "1 still waits");
    }

    #[test]
    fn a_build_given_back_waits_again_at_its_older_position() {
        let mut pending = Pending::default();
        pending.mark(key(1), p(1));
        pending.mark(key(2), p(2));
        let taken = pending.take(&index(), usize::MAX, &none).unwrap();
        pending.mark(key(1), p(5));
        assert_eq!(
            pending.give_back(taken.token),
            1,
            "2 waits again; 1 already did"
        );
        assert_eq!(
            pending.minimum(),
            Some(p(1)),
            "1 is back at its older position"
        );
        let again = pending.take(&index(), usize::MAX, &none).unwrap();
        assert_eq!(ids(&again), [key(1), key(2)]);
        assert_eq!(again.oldest, p(1));
    }

    #[test]
    fn the_minimum_follows_waiting_and_building_ids_and_clears() {
        let mut pending = Pending::default();
        assert_eq!(pending.minimum(), None);
        pending.mark(key(1), p(4));
        let taken = pending.take(&index(), usize::MAX, &none).unwrap();
        pending.mark(key(2), p(6));
        assert_eq!(pending.minimum(), Some(p(4)));
        assert_eq!(pending.tokens(), [taken.token]);
        pending.release(taken.token);
        assert_eq!(pending.minimum(), Some(p(6)));
        let last = pending.take(&index(), usize::MAX, &none).unwrap();
        pending.release(last.token);
        assert_eq!(pending.minimum(), None);
        assert_eq!(pending.release(last.token), 0, "a token is released once");
    }
}
