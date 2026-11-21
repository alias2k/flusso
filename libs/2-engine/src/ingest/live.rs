//! The builder's side of live changes: the runs the reader handed over,
//! formed into batches as `main` formed them (ADR 0012 stage 8a, R3).

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use kernel::Position;
use source::document::DocumentId;
use tokio::time::Duration;

use super::reader::{Lead, Run};

/// The runs handed over and not yet granted, oldest first, and the position
/// a grant kept for documents the budget left waiting.
#[derive(Debug)]
pub(super) struct Runs {
    queue: VecDeque<Run>,
    /// The last grant's candidate, kept while the budget leaves some of its
    /// documents waiting: a later grant takes them even if no run comes.
    kept: Option<Candidate>,
    max_changes: usize,
    max_delay: Duration,
    lead: Arc<Lead>,
}

/// The live batch a grant would take: through which change, how many
/// changes it settles, and how many queued runs it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    /// Its newest change: the take reaches no document marked after it.
    pub(super) position: Position,
    /// How many changes had been read through it: its step closes there.
    pub(super) read: u64,
    /// The changes of its runs.
    pub(super) changes: usize,
    runs: usize,
    /// It can't grow: `max_changes` changes, or the next run doesn't fit.
    pub(super) full: bool,
}

impl Runs {
    pub(super) fn new(max_changes: usize, max_delay: Duration, lead: Arc<Lead>) -> Self {
        Self {
            queue: VecDeque::new(),
            kept: None,
            max_changes: max_changes.max(1),
            max_delay,
            lead,
        }
    }

    pub(super) fn push(&mut self, run: Run) {
        self.queue.push_back(run);
    }

    /// The changes handed over and not yet granted.
    pub(super) fn changes(&self) -> usize {
        self.queue.iter().map(|run| run.changes).sum()
    }

    /// The batch a grant would take now: the oldest run, joined by each
    /// next run while the changes stay within `max_changes` and the run's
    /// first change arrived before the batch's deadline (its first run's
    /// first change plus `max_delay`); or, with no run queued, what the last
    /// grant kept for the budget's leftovers.
    pub(super) fn candidate(&self) -> Option<Candidate> {
        let Some(first) = self.queue.front() else {
            return self.kept.map(|kept| Candidate {
                changes: 0,
                runs: 0,
                full: false,
                ..kept
            });
        };
        let deadline = first.started + self.max_delay;
        let mut candidate = Candidate {
            position: first.position,
            read: first.read,
            changes: first.changes,
            runs: 1,
            full: first.changes >= self.max_changes,
        };
        for run in self.queue.iter().skip(1) {
            if candidate.full || run.started >= deadline {
                break;
            }
            if candidate.changes + run.changes > self.max_changes {
                candidate.full = true;
                break;
            }
            candidate.changes += run.changes;
            candidate.position = run.position;
            candidate.read = run.read;
            candidate.runs += 1;
            candidate.full = candidate.changes >= self.max_changes;
        }
        Some(candidate)
    }

    /// `candidate` was granted and its documents taken: its runs leave the
    /// queue, the reader may read on, and when the budget left some of its
    /// documents `waiting`, it is kept for them.
    pub(super) fn granted(&mut self, candidate: Candidate, waiting: bool) {
        for _ in 0..candidate.runs {
            self.queue.pop_front();
        }
        self.kept = waiting.then_some(candidate);
        self.lead.granted(candidate.read);
    }
}

/// A snapshot batch in the making: the deduplicated document ids of the
/// rows read and how many.
#[derive(Debug)]
pub(super) struct PendingBatch {
    pub(super) ids: Vec<DocumentId>,
    pub(super) seen: HashSet<DocumentId>,
    pub(super) count: usize,
    pub(super) capacity: usize,
}

impl PendingBatch {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
            seen: HashSet::with_capacity(capacity),
            count: 0,
            capacity,
        }
    }

    /// Buffer one change's affected documents, deduplicated.
    pub(super) fn add(&mut self, affected: impl IntoIterator<Item = DocumentId>) {
        for id in affected {
            if self.seen.insert(id.clone()) {
                self.ids.push(id);
            }
        }
        self.count += 1;
    }

    pub(super) fn is_full(&self) -> bool {
        self.count >= self.capacity
    }

    pub(super) fn clear(&mut self) {
        self.ids.clear();
        self.seen.clear();
        self.count = 0;
    }
}
