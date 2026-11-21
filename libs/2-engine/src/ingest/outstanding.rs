//! What each index has waiting on lanes for an acknowledgement, and since
//! when nothing of it was acknowledged (ADR 0012 stage 8a, 6b-2, review
//! round 2 of #452): what tells the reader's exemption whether an index is
//! **free** (making progress, or quiet with nothing outstanding) or stuck.
//!
//! Every item the ingest engine publishes, live or backfill, carries a
//! [`Published`] guard of its index inside its [`stream::Hold`]: the guard
//! counts the item outstanding from its publish until every lane is done
//! with it (acknowledged and out of every ledger), when its drop marks the
//! index's progress. An index is **stuck** while it has items outstanding and
//! none was acknowledged for [`STUCK_AFTER`]; a lane that never receives the
//! item (a `backfill = false` lane), acknowledges it without applying it, or
//! is at rest holds nothing.
//!
//! ```text
//! publish a (0 → 1: since = now)   ack a (1 → 0: since = now)   free
//! publish b (since = 10 s) … 15 s, nothing acknowledged          stuck
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use kernel::IndexName;
use tokio::time::{Duration, Instant};

/// How long an index's items may wait with none acknowledged before it
/// counts as stuck: above a healthy sink's flush (hundreds of ms) and one
/// sink engine restart (1 s, then 2 s), below three failed attempts in a row
/// (1 + 2 + 4 s). See the stage note, §5.
pub(crate) const STUCK_AFTER: Duration = Duration::from_secs(5);

/// Each index's record, for the life of the ingest engine (an item outlives
/// the engine run that published it).
#[derive(Debug)]
pub(crate) struct Outstanding {
    began: Instant,
    indexes: BTreeMap<IndexName, Arc<Index>>,
}

/// One index's record.
#[derive(Debug, Default)]
struct Index {
    /// Items published and not yet let go by every lane.
    items: AtomicUsize,
    /// When an item was last acknowledged, or the first published after
    /// none was outstanding: milliseconds since `began`.
    since: AtomicU64,
}

/// An item's guard: outstanding while any lane holds the item.
#[derive(Debug)]
pub(crate) struct Published {
    index: Arc<Index>,
    began: Instant,
}

impl Drop for Published {
    fn drop(&mut self) {
        // An acknowledgement is progress, whatever is still outstanding.
        self.index
            .since
            .store(millis(self.began), Ordering::Relaxed);
        self.index.items.fetch_sub(1, Ordering::Release);
    }
}

impl Outstanding {
    /// A record for each of `indexes`.
    pub(crate) fn new(indexes: impl IntoIterator<Item = IndexName>) -> Self {
        Self {
            began: Instant::now(),
            indexes: indexes
                .into_iter()
                .map(|index| (index, Arc::default()))
                .collect(),
        }
    }

    /// The guard of an item of `index` being published; `None` for an index
    /// with no record.
    pub(crate) fn publish(&self, index: &IndexName) -> Option<Published> {
        let record = self.indexes.get(index)?;
        if record.items.fetch_add(1, Ordering::AcqRel) == 0 {
            record.since.store(millis(self.began), Ordering::Relaxed);
        }
        Some(Published {
            index: Arc::clone(record),
            began: self.began,
        })
    }

    /// Whether `index` has items outstanding and none was acknowledged for
    /// [`STUCK_AFTER`].
    pub(crate) fn stuck(&self, index: &IndexName) -> bool {
        let Some(record) = self.indexes.get(index) else {
            return false;
        };
        if record.items.load(Ordering::Acquire) == 0 {
            return false;
        }
        let since = record.since.load(Ordering::Relaxed);
        millis(self.began).saturating_sub(since) >= STUCK_AFTER.as_millis() as u64
    }
}

fn millis(began: Instant) -> u64 {
    u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn users() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    /// Quiet with nothing outstanding: free. An item waiting five seconds
    /// with nothing acknowledged: stuck. An acknowledgement: free again,
    /// whatever else is outstanding, until five more seconds pass.
    #[tokio::test(start_paused = true)]
    async fn an_index_is_stuck_while_its_items_wait_five_seconds_unacknowledged() {
        let outstanding = Outstanding::new([users()]);
        tokio::time::advance(Duration::from_secs(60)).await;
        assert!(!outstanding.stuck(&users()), "quiet, nothing outstanding");
        let first = outstanding.publish(&users()).unwrap();
        let second = outstanding.publish(&users()).unwrap();
        tokio::time::advance(Duration::from_millis(4_999)).await;
        assert!(!outstanding.stuck(&users()), "not yet five seconds");
        tokio::time::advance(Duration::from_millis(1)).await;
        assert!(
            outstanding.stuck(&users()),
            "five seconds, nothing acknowledged"
        );
        drop(first);
        assert!(
            !outstanding.stuck(&users()),
            "an acknowledgement is progress"
        );
        tokio::time::advance(STUCK_AFTER).await;
        assert!(outstanding.stuck(&users()), "the second still waits");
        drop(second);
        tokio::time::advance(Duration::from_secs(60)).await;
        assert!(!outstanding.stuck(&users()), "nothing outstanding");
    }
}
