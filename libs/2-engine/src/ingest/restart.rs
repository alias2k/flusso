//! Restarting one index's builder alone (ADR 0012 stage 8a, slice 8; the
//! design is the stage note's §3.10).
//!
//! **Whose failure.** With two or more builders, an error a builder returns
//! from its own index's work restarts that builder alone: a build that failed
//! or panicked, a document `Failed` under `on_error = "stop"`, its walk's key
//! read. Those are the [`EngineError::Source`] and
//! [`EngineError::DocumentFailed`] errors a builder returns: it reaches the
//! source only through its builds and its walk. Everything else is the run's
//! (a lane or the request lane, a bug, the reader, the dispatcher), as is any
//! failure while stopping (a stop never restarts anything) and any failure of
//! a lone builder (one index keeps the whole-run restart and its replay).
//!
//! **The wait.** Per builder, the `[backoff]` schedule: the first wait, then
//! doubling to the longest, started over once the restarted builder published
//! a live item of its index (a later failure is a new outage, not the same one
//! repeating). A stop cuts the wait short.
//!
//! ```text
//! fail ─▶ wait 1 s ─▶ restart ─▶ fail ─▶ wait 2 s ─▶ restart ─▶ publish … fail ─▶ wait 1 s
//! ```

use tokio::time::Duration;

use crate::error::EngineError;
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

/// One builder's waits between restarts, for the life of an engine run.
#[derive(Debug)]
pub(super) struct Retry {
    schedule: Backoff,
    next: Duration,
    /// How many live items of the index had been numbered when it last
    /// restarted: more at its next failure means it published since.
    numbered: u64,
}

impl Retry {
    pub(super) fn new(schedule: Backoff) -> Self {
        Self {
            schedule,
            next: schedule.initial(),
            numbered: 0,
        }
    }

    /// The builder failed with `numbered` live items of its index numbered
    /// so far: how long it waits before its restart.
    pub(super) fn failed(&mut self, numbered: u64) -> Duration {
        if numbered > self.numbered {
            self.next = self.schedule.initial();
        }
        let wait = self.next;
        self.next = self.schedule.after(wait);
        wait
    }

    /// The builder restarted with `numbered` live items of its index
    /// numbered so far.
    pub(super) fn restarted(&mut self, numbered: u64) {
        self.numbered = numbered;
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
    /// published since its last restart.
    #[test]
    fn the_waits_double_and_start_over_after_a_published_item() {
        let mut retry =
            Retry::new(Backoff::new(Duration::from_secs(1), Duration::from_secs(5)).unwrap());
        let mut waits = Vec::new();
        for _ in 0..4 {
            waits.push(retry.failed(0).as_secs());
            retry.restarted(0);
        }
        waits.push(retry.failed(3).as_secs());
        retry.restarted(3);
        waits.push(retry.failed(3).as_secs());
        assert_eq!(waits, [1, 2, 4, 5, 1, 2]);
    }
}
