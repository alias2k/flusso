//! The exemption's log lines (ADR 0012 stage 8a, 6b-2, review round 3 of
//! #452): one line per index naming its state (not exempt, exempt and
//! growing, exempt and frozen) and one naming the pending cap (reading
//! stopped at `max_pending_documents`, or reading on).
//!
//! **Kept by the engine, checked on the tick.** The ingest engine keeps one
//! [`ExemptionLog`] for its life, as it keeps the outstanding record, so an
//! engine run that restarts goes on from the last line logged. The
//! dispatcher's 1 s tick checks every line ([`Lead::tick`](super::Lead::tick)),
//! whether or not anything is read, so an idle stream still logs a change.
//! The check only logs: the reader decides on its own checks.
//!
//! **Throttled per line.** At most one line a minute for an index (whichever
//! state it names), and one for the cap; the first always. Once the minute
//! is over a state that differs from the last line is logged at the next
//! tick. So within a minute and a second of any change, the last line names
//! the current state.
//!
//! ```text
//! 0 s exempt (logged) · 20 s not · 40 s exempt · 61 s tick: nothing (exempt is the last line)
//! 0 s exempt (logged) · 20 s not (throttled) · 61 s tick: "no longer exempt"
//! ```

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use kernel::IndexName;
use tokio::time::{Duration, Instant};

/// How often a line may log.
const LOG_EVERY: Duration = Duration::from_secs(60);

/// The state a line names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(in crate::ingest) enum Shown {
    /// Not exempt (an index), or reading on (the cap): what holds before any
    /// line.
    Quiet = 0,
    /// Exempt and growing; the cap: reading stopped.
    Raised = 1,
    /// Exempt and frozen at the count it stopped growing at.
    Frozen = 2,
}

/// The exemption's lines, for the life of the ingest engine.
#[derive(Debug)]
pub(in crate::ingest) struct ExemptionLog {
    began: Instant,
    indexes: BTreeMap<IndexName, Line>,
    cap: Line,
}

/// One line's last state logged, and when.
#[derive(Debug, Default)]
struct Line {
    shown: AtomicU8,
    /// Milliseconds since the log began, `0` never.
    at: AtomicU64,
}

impl Line {
    /// Whether the line naming `state` is due `since` the log began; marks
    /// it logged.
    fn due(&self, state: Shown, since: Duration) -> bool {
        let state = state as u8;
        if self.shown.load(Ordering::Relaxed) == state {
            return false;
        }
        let now = u64::try_from(since.as_millis()).unwrap_or(u64::MAX).max(1);
        let last = self.at.load(Ordering::Relaxed);
        if last != 0 && now.saturating_sub(last) < LOG_EVERY.as_millis() as u64 {
            return false;
        }
        self.shown.store(state, Ordering::Relaxed);
        self.at.store(now, Ordering::Relaxed);
        true
    }
}

impl ExemptionLog {
    /// A line for each of `indexes`, and the cap's.
    pub(in crate::ingest) fn new(indexes: impl IntoIterator<Item = IndexName>) -> Self {
        Self {
            began: Instant::now(),
            indexes: indexes
                .into_iter()
                .map(|index| (index, Line::default()))
                .collect(),
            cap: Line::default(),
        }
    }

    /// `index`, with `waiting` documents, is in `state`: log it if due.
    pub(super) fn index(&self, index: &IndexName, state: Shown, waiting: usize) {
        let Some(line) = self.indexes.get(index) else {
            return;
        };
        if !line.due(state, self.began.elapsed()) {
            return;
        }
        match state {
            Shown::Raised => tracing::warn!(
                %index,
                waiting,
                "index held at max_waiting_per_index: exempt, so reading goes on for the other \
                 indexes; its changes are kept as documents waiting, up to max_pending_documents \
                 for every exempt index, and the replication slot stays pinned by it, so WAL grows"
            ),
            Shown::Frozen => tracing::warn!(
                %index,
                waiting,
                "exempt index frozen: its hold ended or no other index is making progress, so it \
                 may drain but not grow; reading stops for every index while it has this many \
                 documents waiting or more"
            ),
            Shown::Quiet => tracing::info!(
                %index,
                waiting,
                "index no longer exempt: it paces the reading again at max_waiting_per_index"
            ),
        }
    }

    /// The exempt indexes have `documents` waiting, `full` at `max`: log it
    /// if due.
    pub(super) fn cap(&self, full: bool, documents: usize, max: usize) {
        let state = if full { Shown::Raised } else { Shown::Quiet };
        if !self.cap.due(state, self.began.elapsed()) {
            return;
        }
        if full {
            tracing::warn!(
                documents,
                max_pending_documents = max,
                "reading stopped for every index: the exempt indexes have \
                 max_pending_documents documents waiting to be built; it reads on as they are built"
            );
        } else {
            tracing::info!(
                documents,
                "the exempt indexes are below max_pending_documents: reading on"
            );
        }
    }
}
