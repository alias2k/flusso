//! The builder's side of live changes: the runs the reader handed over,
//! cut into batches as `main` cut them (ADR 0012 stage 8a, R3): a batch
//! may end inside a run, since the reader's runs end where the stream would
//! block, not where `main`'s batch closed.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use kernel::Position;
use source::document::DocumentId;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use super::reader::{Lead, Run};
use crate::observer::{Observer, Stage, StageTiming, Work};

/// The runs handed over and not yet granted, oldest first, and the position
/// a grant kept for documents the budget left waiting.
#[derive(Debug)]
pub(super) struct Runs {
    queue: VecDeque<Run>,
    /// How many changes of the front run earlier grants took.
    taken: usize,
    /// How many changes are queued and not yet granted.
    queued: usize,
    /// The batch is known not to fill: its deadline cut it short of
    /// `max_changes`, and only a grant changes that.
    short: bool,
    /// When the last live batch was granted: the next batch's deadline counts
    /// from here when its first change was read before it.
    last_grant: Option<Instant>,
    /// The last grant's candidate, kept while the budget leaves some of its
    /// documents waiting: a later grant takes them even if no run comes.
    kept: Option<Candidate>,
    max_changes: usize,
    max_delay: Duration,
    lead: Arc<Lead>,
    /// The reader's hand-over: drained before any batch is formed, so no
    /// batch misses a change already read (R3, review round 4).
    handed: mpsc::UnboundedReceiver<Run>,
    observer: Arc<dyn Observer>,
}

/// The live batch a grant would take: through which change, how many
/// changes it settles, and where it ends in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    /// Its newest change: the take reaches no document marked after it.
    pub(super) position: Position,
    /// How many changes had been read through it: its step closes there.
    pub(super) read: u64,
    /// The changes it takes.
    pub(super) changes: usize,
    /// It holds `max_changes` changes: a live build may start beside a
    /// running one.
    pub(super) full: bool,
    /// The queue position just past its last change: (run, change in run).
    end: Option<(usize, usize)>,
}

impl Runs {
    pub(super) fn new(
        max_changes: usize,
        max_delay: Duration,
        lead: Arc<Lead>,
        handed: mpsc::UnboundedReceiver<Run>,
        observer: Arc<dyn Observer>,
    ) -> Self {
        Self {
            queue: VecDeque::new(),
            taken: 0,
            queued: 0,
            short: false,
            last_grant: None,
            kept: None,
            max_changes: max_changes.max(1),
            max_delay,
            lead,
            handed,
            observer,
        }
    }

    /// The reader's next run, `None` once it ended. Not queued yet: the
    /// caller closes its idle wait first, then [`push`](Self::push)es it.
    /// Cancel-safe.
    pub(super) async fn next(&mut self) -> Option<Run> {
        self.handed.recv().await
    }

    /// Queue a run, reporting how long the reader took to read it.
    pub(super) fn push(&mut self, run: Run) {
        self.observer
            .on_stage(StageTiming::new(Stage::Resolve, run.resolved).work(Work::Live));
        self.queued += run.changes.len();
        self.queue.push_back(run);
    }

    /// Queue every run already handed over. Every way to form a batch
    /// starts here.
    fn drain(&mut self) {
        while let Ok(run) = self.handed.try_recv() {
            self.push(run);
        }
    }

    /// The changes handed over and not yet granted.
    pub(super) fn changes(&self) -> usize {
        self.queued
    }

    /// [`candidate`](Self::candidate) when it is full, for a grant beside a
    /// running live build, which only a full batch may start. `None` at once
    /// while fewer than `max_changes` changes are queued, or while the
    /// batch's deadline is known to cut it short: a hand-over that can't
    /// start a build costs no walk.
    pub(super) fn full_candidate(&mut self) -> Option<Candidate> {
        self.drain();
        if self.short || self.queued < self.max_changes {
            return None;
        }
        let candidate = self.form(true).filter(|candidate| candidate.full);
        // With `max_changes` queued, a batch that isn't full was cut by its
        // deadline, which later changes can't move.
        self.short = candidate.is_none();
        candidate
    }

    /// The batch a grant would take now, as `main` formed one (R3): from the
    /// oldest change not yet granted, every change read before the batch's
    /// deadline, up to `max_changes`. The deadline is `max_delay` past the
    /// moment the batch could first take a change: its first change's read,
    /// or the last live grant when that change was read before it (`main`
    /// stopped reading at a closed batch and read the backlog the moment the
    /// grant reopened it). `settle` passes `deadline: false`: at the end
    /// every change read is built, in batches of `max_changes`. With no
    /// change queued, what the last grant kept for the budget's leftovers.
    ///
    /// Every run already handed over is queued first, so the batch misses no
    /// change read before it, whichever wake of the builder forms it.
    pub(super) fn candidate(&mut self, deadline: bool) -> Option<Candidate> {
        self.drain();
        self.form(deadline)
    }

    /// [`candidate`](Self::candidate) over what is queued.
    fn form(&self, deadline: bool) -> Option<Candidate> {
        let first = self
            .queue
            .front()
            .and_then(|run| run.changes.get(self.taken))
            .map(|(_, at)| *at);
        let Some(first) = first else {
            return self.kept.map(|kept| Candidate {
                changes: 0,
                full: false,
                end: None,
                ..kept
            });
        };
        let closes = self.last_grant.map_or(first, |grant| grant.max(first)) + self.max_delay;
        let mut candidate: Option<Candidate> = None;
        let mut count = 0;
        'runs: for (at_run, run) in self.queue.iter().enumerate() {
            let skip = if at_run == 0 { self.taken } else { 0 };
            for (at_change, (position, read_at)) in run.changes.iter().enumerate().skip(skip) {
                if count == self.max_changes || (deadline && count > 0 && *read_at >= closes) {
                    break 'runs;
                }
                count += 1;
                let unread_after = (run.changes.len() - at_change - 1) as u64;
                candidate = Some(Candidate {
                    position: *position,
                    read: run.read - unread_after,
                    changes: count,
                    full: count == self.max_changes,
                    end: Some((at_run, at_change + 1)),
                });
            }
        }
        candidate
    }

    /// `candidate` was granted and its documents taken: its changes leave
    /// the queue, the reader may read on, and when the budget left some of
    /// its documents `waiting`, it is kept for them.
    pub(super) fn granted(&mut self, candidate: Candidate, waiting: bool) {
        self.queued -= candidate.changes;
        self.short = false;
        if let Some((at_run, at_change)) = candidate.end {
            for _ in 0..at_run {
                self.queue.pop_front();
            }
            self.taken = at_change;
            if self
                .queue
                .front()
                .is_some_and(|run| run.changes.len() == at_change)
            {
                self.queue.pop_front();
                self.taken = 0;
            }
        }
        self.last_grant = Some(Instant::now());
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::observer::NoopObserver;
    use crate::policy::MaxWaiting;

    fn run(seq: u64, read: u64) -> Run {
        Run {
            changes: vec![(Position::new(seq), Instant::now())],
            read,
            resolved: Duration::ZERO,
        }
    }

    /// R3 (review round 4): a batch is formed only after the hand-over is
    /// drained, whichever wake of the builder forms it. Runs sent and never
    /// pushed by the loop are in the very next candidate, full or not.
    #[tokio::test(start_paused = true)]
    async fn a_batch_is_formed_only_after_every_run_handed_over_is_queued() {
        let (sender, handed) = mpsc::unbounded_channel();
        let lead = Lead::new(
            MaxWaiting {
                per_index: 100,
                total: 100,
            },
            0,
        );
        let mut runs = Runs::new(
            2,
            Duration::from_secs(1),
            lead,
            handed,
            Arc::new(NoopObserver),
        );
        sender.send(run(0, 1)).unwrap();
        sender.send(run(1, 2)).unwrap();
        let candidate = runs.candidate(true).unwrap();
        assert_eq!(
            (candidate.position, candidate.changes),
            (Position::new(1), 2)
        );
        assert!(candidate.full);
        runs.granted(candidate, false);
        sender.send(run(2, 3)).unwrap();
        sender.send(run(3, 4)).unwrap();
        let candidate = runs.full_candidate().unwrap();
        assert_eq!(
            (candidate.position, candidate.changes),
            (Position::new(3), 2)
        );
    }
}
