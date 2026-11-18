//! One index's pending set: the document ids marked and not yet published,
//! and the floor they hold (ADR 0012 stage 8a).
//!
//! An id is **waiting** from its mark until a build takes it, then **in
//! flight** until that build is published to the index's lanes (or given
//! back). A mark is unconditional: an id already waiting keeps its older
//! position (one build covers both changes, a build reads the database as it
//! is now), and an id in flight is marked waiting again, since its build may
//! have read before the change. A take skips ids in flight, so a document's
//! next build starts only after the previous one was published.
//!
//! The **floor** is the multiset of positions the index still holds before
//! its lanes: each waiting id's, and each build's oldest. Its least entry is
//! what the index pins confirmation and fences at until the lanes have it.
//!
//! ```text
//! mark a@3 b@4      waiting {a@3, b@4}            floor {3, 4}
//! take              in flight {a, b} as build 0   floor {3}        (the build's oldest)
//! mark a@7          waiting {a@7}                 floor {3, 7}
//! take              nothing: a is in flight
//! publish 0         in flight {}                  floor {7}
//! take              a as build 1                  floor {7}
//! ```

use std::collections::{BTreeMap, HashMap};

use kernel::Position;
use source::document::DocumentId;

/// A build's share of a pending set: the ids it took and the oldest
/// position among them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// The token that publishes or gives back the build.
    pub token: u64,
    /// The ids to build, oldest first.
    pub ids: Vec<DocumentId>,
    /// The oldest position among them: what the item holds back on its
    /// lanes until it lands.
    pub oldest: Position,
}

/// One index's pending set.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    waiting: HashMap<DocumentId, Position>,
    /// Waiting ids by the seq they wait at; an entry whose id now waits at
    /// another seq, or no longer waits, is stale and dropped when met.
    order: BTreeMap<u64, Vec<DocumentId>>,
    in_flight: HashMap<DocumentId, u64>,
    builds: HashMap<u64, Build>,
    /// Seq → (how many entries, the position).
    floor: BTreeMap<u64, (usize, Position)>,
    next_token: u64,
}

#[derive(Debug)]
struct Build {
    oldest: Position,
    ids: Vec<(DocumentId, Position)>,
}

impl Pending {
    /// Mark `ids` changed at `at`; returns how many began waiting.
    pub(crate) fn mark(
        &mut self,
        ids: impl IntoIterator<Item = DocumentId>,
        at: Position,
    ) -> usize {
        let mut added = 0;
        for id in ids {
            if self.waiting.contains_key(&id) {
                continue;
            }
            self.wait(id, at);
            added += 1;
        }
        added
    }

    fn wait(&mut self, id: DocumentId, at: Position) {
        self.order.entry(at.seq).or_default().push(id.clone());
        self.waiting.insert(id, at);
        self.raise(at);
    }

    /// Take up to `max` waiting ids, oldest first, skipping ids in flight:
    /// the atomic move from waiting to in flight.
    pub(crate) fn take(&mut self, max: usize) -> Option<Taken> {
        let mut taken: Vec<(DocumentId, Position)> = Vec::new();
        let mut emptied: Vec<u64> = Vec::new();
        for (seq, bucket) in &mut self.order {
            let mut kept = Vec::new();
            for id in bucket.drain(..) {
                if taken.len() >= max {
                    kept.push(id);
                    continue;
                }
                match self.waiting.get(&id) {
                    Some(at) if at.seq == *seq => {
                        if self.in_flight.contains_key(&id) {
                            kept.push(id);
                        } else {
                            let at = *at;
                            taken.push((id, at));
                        }
                    }
                    _ => {}
                }
            }
            *bucket = kept;
            if bucket.is_empty() {
                emptied.push(*seq);
            }
            if taken.len() >= max {
                break;
            }
        }
        for seq in emptied {
            if self.order.get(&seq).is_some_and(Vec::is_empty) {
                self.order.remove(&seq);
            }
        }
        let oldest = taken.iter().map(|(_, at)| *at).min()?;
        let token = self.next_token;
        self.next_token += 1;
        for (id, at) in &taken {
            self.waiting.remove(id);
            self.lower(*at);
            self.in_flight.insert(id.clone(), token);
        }
        self.raise(oldest);
        let ids = taken.iter().map(|(id, _)| id.clone()).collect();
        self.builds.insert(token, Build { oldest, ids: taken });
        Some(Taken { token, ids, oldest })
    }

    /// The oldest position of the build `token`, while it is in flight.
    pub(crate) fn oldest_of(&self, token: u64) -> Option<Position> {
        self.builds.get(&token).map(|build| build.oldest)
    }

    /// The build `token` is on its lanes: its ids leave flight and its
    /// floor entry goes. Returns whether one of its ids waits again.
    pub(crate) fn published(&mut self, token: u64) -> bool {
        let Some(build) = self.builds.remove(&token) else {
            return false;
        };
        let mut again = false;
        for (id, _) in &build.ids {
            self.in_flight.remove(id);
            again |= self.waiting.contains_key(id);
        }
        self.lower(build.oldest);
        again
    }

    /// The build `token` failed or its run ended: its ids wait again, each
    /// at the older of its take position and a newer mark.
    pub(crate) fn give_back(&mut self, token: u64) {
        let Some(build) = self.builds.remove(&token) else {
            return;
        };
        for (id, at) in build.ids {
            self.in_flight.remove(&id);
            match self.waiting.get(&id).copied() {
                Some(marked) if marked <= at => {}
                Some(marked) => {
                    self.lower(marked);
                    self.waiting.remove(&id);
                    self.wait(id, at);
                }
                None => self.wait(id, at),
            }
        }
        self.lower(build.oldest);
    }

    /// Every build in flight given back.
    pub(crate) fn give_back_all(&mut self) {
        let tokens: Vec<u64> = self.builds.keys().copied().collect();
        for token in tokens {
            self.give_back(token);
        }
    }

    /// The least position the index holds before its lanes.
    pub(crate) fn floor(&self) -> Option<Position> {
        self.floor.first_key_value().map(|(_, (_, at))| *at)
    }

    /// How many ids wait.
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }

    fn raise(&mut self, at: Position) {
        self.floor.entry(at.seq).or_insert((0, at)).0 += 1;
    }

    fn lower(&mut self, at: Position) {
        if let Some((count, _)) = self.floor.get_mut(&at.seq) {
            *count -= 1;
            if *count == 0 {
                self.floor.remove(&at.seq);
            }
        }
    }
}
