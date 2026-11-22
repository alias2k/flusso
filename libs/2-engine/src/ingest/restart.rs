//! Restarting one index's builder alone, and retrying its walk alone (ADR
//! 0012 stage 8a, slice 8; the design is the stage note's §3.10).
//!
//! **Whose failure.** With two or more builders, an error a builder returns
//! from its own index's live work restarts that builder alone: a live build
//! that failed or panicked, a live document `Failed` under `on_error =
//! "stop"`. Those are the [`EngineError::Source`] and
//! [`EngineError::DocumentFailed`] errors a builder returns: it reaches the
//! source only through its builds and its walk, and its walk's failures (a
//! key read, a backfill batch) it handles in its loop, retrying the walk
//! alone (fix round 2). Everything else is the run's (a lane or the request
//! lane, a bug, the reader, the dispatcher), as is any failure while
//! stopping (a stop never restarts anything) and any failure of a lone
//! builder (one index keeps the whole-run restart and its replay).
//!
//! **The wait.** Per builder, the `[backoff]` schedule, one per kind of work
//! that failed: the first wait, then doubling to the longest, started over
//! once that kind moved since its last failure. A live failure starts over
//! once the index published a live item; a walk failure once the walk
//! published a backfill batch carrying a walk mark (some lane's resume
//! point moved; a source that can't mark walks starts every retry from the
//! first key, which is no progress): distinct
//! one-off failures along a walk each wait the first wait, while a poison
//! row, which the walk never publishes past, doubles to the longest.
//! Progress of the other kind is no sign the failure is past (review of
//! #456). A stop cuts a live wait short; a walk's wait it drops with the
//! walk.
//!
//! ```text
//! live fail ─▶ 1 s ─▶ live fail ─▶ 2 s ─▶ … published live ─▶ live fail ─▶ 1 s
//! walk fail ─▶ 1 s ─▶ fail ─▶ 2 s ─▶ 4 s (nothing published past it; live flows)
//! walk fail ─▶ 1 s ─▶ published batches ─▶ fail further on ─▶ 1 s
//! ```

use tokio::time::Duration;

use crate::error::EngineError;
use crate::observer::Work;
use crate::policy::Backoff;

/// Whether a builder's `error` restarts that builder alone: it is one of
/// `several` builders, the run isn't `stopping`, and the error came from its
/// own index's work (see the module docs).
pub(super) fn restarts_alone(several: bool, stopping: bool, error: &EngineError) -> bool {
    several
        && !stopping
        && matches!(
            error,
            EngineError::Source(_) | EngineError::DocumentFailed { .. }
        )
}

/// What an index has done so far, by kind: what restarts a kind's waits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Done {
    /// Live items of the index numbered (published).
    pub(super) live: u64,
    /// Backfill batches of the index published with a walk mark: some
    /// lane's resume point moved.
    pub(super) walk_batches: u64,
}

/// One builder's waits between restarts, for the life of an engine run: one
/// schedule per kind of work.
#[derive(Debug)]
pub(super) struct Retry {
    live: Schedule,
    walk: Schedule,
}

/// One kind's waits, and what that kind had done at its last failure.
#[derive(Debug)]
struct Schedule {
    backoff: Backoff,
    next: Duration,
    done: u64,
}

impl Schedule {
    fn failed(&mut self, done: u64) -> Duration {
        if done > self.done {
            self.next = self.backoff.initial();
        }
        self.done = done;
        let wait = self.next;
        self.next = self.backoff.after(wait);
        wait
    }
}

impl Retry {
    pub(super) fn new(backoff: Backoff) -> Self {
        let schedule = || Schedule {
            backoff,
            next: backoff.initial(),
            done: 0,
        };
        Self {
            live: schedule(),
            walk: schedule(),
        }
    }

    /// The builder failed on `work`, its index having `done` so much: how
    /// long it waits before its restart.
    pub(super) fn failed(&mut self, work: Work, done: Done) -> Duration {
        match work {
            Work::Live => self.live.failed(done.live),
            Work::Backfill => self.walk.failed(done.walk_batches),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use source::SourceError;

    use super::*;

    fn source() -> EngineError {
        EngineError::Source(SourceError::Query("refused".to_owned()))
    }

    fn document() -> EngineError {
        EngineError::DocumentFailed {
            index: "orders".to_owned(),
            id: "1".to_owned(),
            failure: Box::new(source::document::BuildFailure {
                field: "label".to_owned(),
                message: "boom".to_owned(),
                position: None,
            }),
        }
    }

    /// R1, R2, R10, R11, R14: an index's own failure restarts its builder
    /// alone; a lane's, a bug, a stop and a lone builder end the run.
    #[test]
    fn only_an_indexs_own_failure_beside_others_and_not_stopping_restarts_it_alone() {
        assert!(restarts_alone(true, false, &source()));
        assert!(restarts_alone(true, false, &document()));
        assert!(!restarts_alone(false, false, &source()), "one index");
        assert!(!restarts_alone(true, true, &source()), "stopping");
        assert!(!restarts_alone(true, true, &document()), "stopping");
        assert!(
            !restarts_alone(
                true,
                false,
                &EngineError::Stream(stream::QueueError::Closed)
            ),
            "a lane"
        );
        assert!(
            !restarts_alone(
                true,
                false,
                &EngineError::UnregisteredLiveItem("orders/primary".to_owned())
            ),
            "a bug"
        );
        assert!(
            !restarts_alone(
                true,
                false,
                &EngineError::DocumentsLeft("orders: 1".to_owned())
            ),
            "the end check"
        );
    }

    /// R8: the waits double up to the longest and start over once the index
    /// published since its last failure.
    #[test]
    fn the_waits_double_and_start_over_after_a_published_item() {
        let mut retry =
            Retry::new(Backoff::new(Duration::from_secs(1), Duration::from_secs(5)).unwrap());
        let mut waits = Vec::new();
        for _ in 0..4 {
            waits.push(retry.failed(Work::Live, done(0, 0)).as_secs());
        }
        waits.push(retry.failed(Work::Live, done(3, 0)).as_secs());
        waits.push(retry.failed(Work::Live, done(3, 0)).as_secs());
        assert_eq!(waits, [1, 2, 4, 5, 1, 2]);
    }

    fn done(live: u64, walk_batches: u64) -> Done {
        Done { live, walk_batches }
    }

    /// R8 per kind (review of #456, rounds 1 and 2): a walk's failure starts
    /// over once the walk published a batch since its last failure; live
    /// items published meanwhile don't start it over, and a walk's batches
    /// don't start a live failure's waits over.
    #[test]
    fn each_kind_starts_over_only_on_its_own_progress() {
        let mut retry =
            Retry::new(Backoff::new(Duration::from_secs(1), Duration::from_secs(60)).unwrap());
        let mut walk = Vec::new();
        for live in 0..4 {
            walk.push(retry.failed(Work::Backfill, done(live * 10, 5)).as_secs());
        }
        assert_eq!(
            walk,
            [1, 2, 4, 8],
            "live published between, the walk nothing"
        );
        assert_eq!(
            retry.failed(Work::Backfill, done(40, 6)).as_secs(),
            1,
            "the walk published past its last failure"
        );
        assert_eq!(retry.failed(Work::Live, done(40, 6)).as_secs(), 1);
        assert_eq!(
            retry.failed(Work::Live, done(40, 9)).as_secs(),
            2,
            "a walk's batches don't start the live waits over"
        );
    }
}
