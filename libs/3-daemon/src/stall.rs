//! Stalled lanes: how long each `(index, sink)` lane has gone without landing
//! its oldest outstanding item, how much WAL it holds the slot back by, and
//! which lane pins the slot (ADR 0012 stage 8c).
//!
//! **A lane is stalled when it acknowledges nothing**, not when its backlog
//! is high: a busy lane lands its oldest item, and the next becomes the
//! oldest. So the age is measured from the moment the lane's oldest
//! outstanding position was first seen, and resets whenever it changes or the
//! lane clears. A lane with nothing outstanding has age 0.
//!
//! **Sampled, not timestamped.** [`watch`] samples every lane once a second
//! through [`Progress::lane`]: one lock per lane per second, nothing on the
//! path a change takes. The age is accurate to the sampling period.
//!
//! **Backfill items carry no position**, so they are not in [`Progress`]: a
//! lane holding only a backfill's items has age 0. A backfill pins no WAL; a
//! live item queued behind a stuck backfill item is registered and ages.
//!
//! **WAL behind.** The slot is confirmed to the least of the lanes
//! ([`Progress::landed`]), and the source reports how far behind the
//! server's WAL that is (the slot lag). A lane holds WAL from its own
//! position ([`ChangeCapture::reached`]), so its share is the lag less how
//! far it is ahead of the least. The lane `landed` is decided by
//! ([`Progress::pinned_by`]) **pins the slot**: one computation, so a tie at
//! one source position names the lane holding the slot back.
//!
//! ```
//! use std::time::Duration;
//! use daemon::Stalls;
//! use engine::Progress;
//! use kernel::{IndexName, Position, SinkName};
//! use source::cdc::{ChangeCapture, Landed};
//! use stream::LaneKey;
//! use tokio::time::Instant;
//!
//! # #[derive(Debug)] struct NoSource;
//! # #[async_trait::async_trait]
//! # impl ChangeCapture for NoSource {
//! #     async fn continuity(&self) -> source::Result<source::cdc::Continuity> { unimplemented!() }
//! #     async fn prepare(&self) -> source::Result<()> { unimplemented!() }
//! #     async fn live(&self) -> source::Result<futures::stream::BoxStream<'static, source::Result<source::cdc::LiveChange>>> { unimplemented!() }
//! #     fn confirm(&self, _: Landed) {}
//! # }
//! let lane = LaneKey::new(IndexName::try_new("users").unwrap(), SinkName::try_new("primary").unwrap());
//! let progress = Progress::new([lane.clone()]);
//! let stalls = Stalls::new([lane.clone()]);
//! let start = Instant::now();
//! progress.published(&lane, Position::new(1), Position::new(1));
//! stalls.sample(start, &progress, &NoSource, None);
//! stalls.sample(start + Duration::from_secs(30), &progress, &NoSource, None);
//! assert_eq!(stalls.lane(&lane).stalled_for, Duration::from_secs(30));
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use engine::{Level, Progress};
use source::cdc::{ChangeCapture, Landed};
use stream::LaneKey;
use tokio::time::{Instant, MissedTickBehavior, interval};

use crate::status::Status;

/// How long a lane goes without landing before it is logged as stalled, and
/// how often the warning repeats while it lasts: ADR 0012's alert threshold.
pub const STALL_WARN: Duration = Duration::from_secs(600);

/// How often every lane is sampled.
pub const SAMPLE_EVERY: Duration = Duration::from_secs(1);

/// One lane's stall, as of the last sample.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaneStall {
    /// How long the lane's oldest outstanding item has stayed its oldest;
    /// zero while nothing is outstanding.
    pub stalled_for: Duration,
    /// Bytes of WAL the slot holds because of this lane; `None` without a
    /// slot lag sample, or while nothing is outstanding on the lane.
    pub wal_behind: Option<u64>,
}

/// What a sample found worth telling an operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StallEvent {
    /// The lane passed [`STALL_WARN`], or another [`STALL_WARN`] passed
    /// since the last warning.
    Stalled {
        lane: LaneKey,
        stalled_for: Duration,
        wal_behind: Option<u64>,
    },
    /// A lane that was warned about landed again.
    Recovered {
        lane: LaneKey,
        stalled_for: Duration,
    },
}

/// Every lane's stall, updated by [`sample`](Self::sample).
#[derive(Debug, Default)]
pub struct Stalls {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    lanes: BTreeMap<LaneKey, Tracked>,
    pinned_by: Option<LaneKey>,
}

#[derive(Debug)]
struct Tracked {
    /// The seq of the lane's oldest outstanding position at the last sample.
    oldest: Option<u64>,
    since: Instant,
    warned: Option<Instant>,
    stall: LaneStall,
}

impl Stalls {
    /// Tracking `lanes`, none stalled.
    pub fn new(lanes: impl IntoIterator<Item = LaneKey>) -> Self {
        let now = Instant::now();
        Self {
            state: Mutex::new(State {
                lanes: lanes
                    .into_iter()
                    .map(|lane| {
                        (
                            lane,
                            Tracked {
                                oldest: None,
                                since: now,
                                warned: None,
                                stall: LaneStall::default(),
                            },
                        )
                    })
                    .collect(),
                pinned_by: None,
            }),
        }
    }

    /// Sample every lane at `now`: ages from `progress`, WAL behind from the
    /// `slot_lag` and `source`'s positions. Returns what to log.
    pub fn sample(
        &self,
        now: Instant,
        progress: &Progress,
        source: &dyn ChangeCapture,
        slot_lag: Option<u64>,
    ) -> Vec<StallEvent> {
        let least = source.reached(progress.landed()).map(|at| at.0);
        let mut events = Vec::new();
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        for (lane, tracked) in &mut state.lanes {
            let landed = progress.lane(lane, Level::Applied).unwrap_or(Landed::Idle);
            let oldest = match landed {
                Landed::Before(position) => Some(position.seq),
                Landed::Through(_) | Landed::Idle => None,
            };
            if oldest != tracked.oldest {
                if tracked.warned.take().is_some() {
                    events.push(StallEvent::Recovered {
                        lane: lane.clone(),
                        stalled_for: now.saturating_duration_since(tracked.since),
                    });
                }
                tracked.oldest = oldest;
                tracked.since = now;
            }
            if oldest.is_none() {
                tracked.stall = LaneStall::default();
                continue;
            }
            let at = source.reached(landed).map(|at| at.0);
            let wal_behind = slot_lag.map(|lag| match (at, least) {
                (Some(at), Some(least)) => lag.saturating_sub(at.saturating_sub(least)),
                _ => lag,
            });
            let stalled_for = now.saturating_duration_since(tracked.since);
            tracked.stall = LaneStall {
                stalled_for,
                wal_behind,
            };
            let due = tracked
                .warned
                .is_none_or(|warned| now.saturating_duration_since(warned) >= STALL_WARN);
            if stalled_for >= STALL_WARN && due {
                tracked.warned = Some(now);
                events.push(StallEvent::Stalled {
                    lane: lane.clone(),
                    stalled_for,
                    wal_behind,
                });
            }
        }
        // The lane `landed` itself is decided by: one computation, so a tie
        // (two lanes at one source position) names the lane that holds the
        // slot back, never its neighbour.
        state.pinned_by = progress.pinned_by();
        events
    }

    /// `lane`'s stall as of the last sample.
    pub fn lane(&self, lane: &LaneKey) -> LaneStall {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .lanes
            .get(lane)
            .map(|tracked| tracked.stall)
            .unwrap_or_default()
    }

    /// Every lane's stall as of the last sample.
    pub fn lanes(&self) -> Vec<(LaneKey, LaneStall)> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .lanes
            .iter()
            .map(|(lane, tracked)| (lane.clone(), tracked.stall))
            .collect()
    }

    /// The lane that holds the slot back the most, if any lane has something
    /// outstanding.
    pub fn pinned_by(&self) -> Option<LaneKey> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.pinned_by.clone()
    }
}

/// Sample every lane each [`SAMPLE_EVERY`] until aborted, logging when a lane
/// stalls (and every [`STALL_WARN`] while it does) and when it recovers.
pub(crate) async fn watch(
    stalls: Arc<Stalls>,
    progress: Arc<Progress>,
    source: Arc<dyn ChangeCapture>,
    status: Arc<Status>,
) {
    let mut ticker = interval(SAMPLE_EVERY);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let events = stalls.sample(Instant::now(), &progress, &*source, status.slot_lag());
        for event in events {
            match event {
                StallEvent::Stalled {
                    lane,
                    stalled_for,
                    wal_behind,
                } => tracing::warn!(
                    index = %lane.index,
                    sink = %lane.sink,
                    stalled_secs = stalled_for.as_secs(),
                    wal_behind_bytes = ?wal_behind,
                    "a lane has landed nothing for {}s: it pins the replication slot for \
                     every index; see the runbook for a stalled lane",
                    stalled_for.as_secs()
                ),
                StallEvent::Recovered { lane, stalled_for } => tracing::info!(
                    index = %lane.index,
                    sink = %lane.sink,
                    stalled_secs = stalled_for.as_secs(),
                    "a stalled lane is landing again"
                ),
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use kernel::{IndexName, Position, SinkName};
    use source::cdc::SourcePosition;

    use super::*;

    fn lane(index: &str) -> LaneKey {
        LaneKey::new(
            IndexName::try_new(index).unwrap(),
            SinkName::try_new("primary").unwrap(),
        )
    }

    /// A source whose position for a change is its seq × 100, the end of the
    /// last landed change for `Through`.
    #[derive(Debug)]
    struct Lsns;

    #[async_trait::async_trait]
    impl ChangeCapture for Lsns {
        async fn continuity(&self) -> source::Result<source::cdc::Continuity> {
            Ok(source::cdc::Continuity::Resumed)
        }
        async fn prepare(&self) -> source::Result<()> {
            Ok(())
        }
        async fn live(
            &self,
        ) -> source::Result<
            futures::stream::BoxStream<'static, source::Result<source::cdc::LiveChange>>,
        > {
            Ok(Box::pin(futures::stream::empty()))
        }
        fn confirm(&self, _: Landed) {}
        fn reached(&self, landed: Landed) -> Option<SourcePosition> {
            match landed {
                Landed::Before(oldest) => Some(SourcePosition(oldest.seq * 100)),
                Landed::Through(last) => Some(SourcePosition(last.seq * 100 + 100)),
                Landed::Idle => None,
            }
        }
    }

    fn at(start: Instant, secs: u64) -> Instant {
        start + Duration::from_secs(secs)
    }

    #[test]
    fn a_lane_with_nothing_outstanding_has_age_zero() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        stalls.sample(start, &progress, &Lsns, Some(10));
        stalls.sample(at(start, 900), &progress, &Lsns, Some(10));
        assert_eq!(stalls.lane(&users), LaneStall::default());
        assert_eq!(stalls.pinned_by(), None);
    }

    #[test]
    fn a_lane_whose_oldest_item_stays_ages_and_resets_when_it_lands() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        progress.published(&users, Position::new(1), Position::new(1));
        progress.published(&users, Position::new(2), Position::new(2));
        stalls.sample(start, &progress, &Lsns, None);
        stalls.sample(at(start, 40), &progress, &Lsns, None);
        assert_eq!(stalls.lane(&users).stalled_for, Duration::from_secs(40));

        progress.applied(&users, Position::new(1));
        stalls.sample(at(start, 41), &progress, &Lsns, None);
        assert_eq!(
            stalls.lane(&users).stalled_for,
            Duration::ZERO,
            "the next item is the oldest now: the lane moved"
        );
        progress.applied(&users, Position::new(2));
        stalls.sample(at(start, 50), &progress, &Lsns, None);
        assert_eq!(stalls.lane(&users), LaneStall::default());
    }

    #[test]
    fn a_lane_landing_steadily_never_ages_past_a_sample() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        for second in 1..=100u64 {
            progress.published(&users, Position::new(second), Position::new(second));
            if second > 1 {
                progress.applied(&users, Position::new(second - 1));
            }
            stalls.sample(at(start, second), &progress, &Lsns, None);
            assert!(stalls.lane(&users).stalled_for <= SAMPLE_EVERY);
        }
    }

    #[test]
    fn the_least_lane_pins_the_slot_and_holds_all_its_lag() {
        let users = lane("users");
        let orders = lane("orders");
        let progress = Progress::new([users.clone(), orders.clone()]);
        let stalls = Stalls::new([users.clone(), orders.clone()]);
        // `orders` holds change 3 (LSN 300); `users` change 7 (LSN 700).
        progress.published(&orders, Position::new(3), Position::new(3));
        progress.published(&users, Position::new(7), Position::new(7));
        stalls.sample(Instant::now(), &progress, &Lsns, Some(1_000));
        assert_eq!(stalls.pinned_by(), Some(orders.clone()));
        assert_eq!(stalls.lane(&orders).wal_behind, Some(1_000));
        assert_eq!(
            stalls.lane(&users).wal_behind,
            Some(600),
            "users is 400 bytes ahead of the least lane"
        );
    }

    /// A source where every outstanding change is in one commit: every lane
    /// with one is at the same position.
    #[derive(Debug)]
    struct OneCommit;

    #[async_trait::async_trait]
    impl ChangeCapture for OneCommit {
        async fn continuity(&self) -> source::Result<source::cdc::Continuity> {
            Ok(source::cdc::Continuity::Resumed)
        }
        async fn prepare(&self) -> source::Result<()> {
            Ok(())
        }
        async fn live(
            &self,
        ) -> source::Result<
            futures::stream::BoxStream<'static, source::Result<source::cdc::LiveChange>>,
        > {
            Ok(Box::pin(futures::stream::empty()))
        }
        fn confirm(&self, _: Landed) {}
        fn reached(&self, _: Landed) -> Option<SourcePosition> {
            Some(SourcePosition(500))
        }
    }

    /// Stage-8c review: two lanes at one source position (one commit, two
    /// changes); `pinned_by` names the lane `Progress::landed` is decided by,
    /// the one with the older change, not the first by name.
    #[test]
    fn a_tie_at_one_source_position_names_the_lane_landed_is_decided_by() {
        let aaa = lane("aaa");
        let zzz = lane("zzz");
        let progress = Progress::new([aaa.clone(), zzz.clone()]);
        let stalls = Stalls::new([aaa.clone(), zzz.clone()]);
        progress.published(&zzz, Position::new(5), Position::new(5));
        progress.published(&aaa, Position::new(9), Position::new(9));
        stalls.sample(Instant::now(), &progress, &OneCommit, Some(1_000));
        assert_eq!(progress.landed(), Landed::Before(Position::new(5)));
        assert_eq!(stalls.pinned_by(), Some(zzz));
        assert_eq!(progress.pinned_by(), stalls.pinned_by());
    }

    #[test]
    fn wal_behind_is_clamped_at_zero() {
        let users = lane("users");
        let orders = lane("orders");
        let progress = Progress::new([users.clone(), orders.clone()]);
        let stalls = Stalls::new([users.clone(), orders.clone()]);
        progress.published(&orders, Position::new(1), Position::new(1));
        progress.published(&users, Position::new(50), Position::new(50));
        stalls.sample(Instant::now(), &progress, &Lsns, Some(100));
        assert_eq!(stalls.lane(&users).wal_behind, Some(0));
    }

    #[test]
    fn a_stall_is_logged_once_past_the_threshold_then_every_threshold_then_on_recovery() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        progress.published(&users, Position::new(1), Position::new(1));
        assert!(stalls.sample(start, &progress, &Lsns, None).is_empty());
        assert!(
            stalls
                .sample(at(start, 599), &progress, &Lsns, None)
                .is_empty()
        );
        assert!(matches!(
            stalls
                .sample(at(start, 600), &progress, &Lsns, None)
                .as_slice(),
            [StallEvent::Stalled { .. }]
        ));
        assert!(
            stalls
                .sample(at(start, 900), &progress, &Lsns, None)
                .is_empty()
        );
        assert!(matches!(
            stalls
                .sample(at(start, 1200), &progress, &Lsns, None)
                .as_slice(),
            [StallEvent::Stalled { .. }]
        ));
        progress.applied(&users, Position::new(1));
        assert_eq!(
            stalls.sample(at(start, 1300), &progress, &Lsns, None),
            vec![StallEvent::Recovered {
                lane: users.clone(),
                stalled_for: Duration::from_secs(1300)
            }]
        );
        assert!(
            stalls
                .sample(at(start, 1400), &progress, &Lsns, None)
                .is_empty()
        );
    }

    #[test]
    fn a_lane_holding_only_backfill_items_is_not_stalled() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        // Backfill items are never registered: the lane answers Idle.
        stalls.sample(start, &progress, &Lsns, Some(5));
        stalls.sample(at(start, 3600), &progress, &Lsns, Some(5));
        assert_eq!(stalls.lane(&users), LaneStall::default());
    }
}
