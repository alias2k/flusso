//! Stalled lanes: how long each `(index, sink)` lane has gone without landing
//! its oldest outstanding item, how much WAL it holds the slot back by, and
//! which lane pins the slot (ADR 0012 stage 8c).
//!
//! **A lane is stalled when it acknowledges nothing**, not when its backlog
//! is high: a busy lane lands its oldest item, and the next becomes the
//! oldest. So the age is measured from the moment the lane's oldest
//! outstanding item was first seen, and resets whenever it changes or the
//! lane clears. A lane with nothing outstanding has age 0. Only the lane's
//! own items count ([`Progress::own_oldest`]): documents its index has
//! pending and not yet published are on no lane, so they stall none.
//!
//! **Sampled, not timestamped.** [`watch`] samples every lane once a second
//! through [`Progress::own_oldest`]: one lock per lane per second, nothing on the
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
//! far it is ahead of the least. What `landed` is decided by
//! ([`Progress::pinned_by`]) **pins the slot**: one computation, so a tie at
//! one source position names what holds the slot back. That is a lane, or an
//! index's pending documents ([`Pin::Backlog`]) when they are older than every
//! lane's own items.
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
//! progress.published(&lane, stream::ItemKey::new(1), Position::new(1), Position::new(1));
//! stalls.sample(start, &progress, &NoSource, None);
//! stalls.sample(start + Duration::from_secs(30), &progress, &NoSource, None);
//! assert_eq!(stalls.lane(&lane).stalled_for, Duration::from_secs(30));
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use engine::{Pin, Progress};
use kernel::IndexName;
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
    /// Each index's backlog age as of the last sample.
    backlogs: BTreeMap<IndexName, Duration>,
    /// How many changes had been read at each sample, oldest first: what
    /// tells when a read number was read. Only what an unbuilt change may
    /// need is kept, at most [`BACKLOG_SPAN`] samples.
    reads: std::collections::VecDeque<(u64, Instant)>,
    pinned_by: Option<Pin>,
}

/// The most samples a backlog age is measured over: an hour at one a
/// second. An older oldest unbuilt change reads as an hour.
pub const BACKLOG_SPAN: usize = 3600;

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
        let lanes: Vec<LaneKey> = lanes.into_iter().collect();
        let backlogs = lanes
            .iter()
            .map(|lane| (lane.index.clone(), Duration::ZERO))
            .collect();
        Self {
            state: Mutex::new(State {
                backlogs,
                reads: std::collections::VecDeque::new(),
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
            let own = progress.own_oldest(lane);
            let oldest = own.map(|position| position.seq);
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
            let at = own.and_then(|own| source.reached(Landed::Before(own)).map(|at| at.0));
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
        // Each index's backlog age (slice 7, review of #454): how long ago
        // its oldest unbuilt change was read, to the sample (a change read
        // since the last one counts as read now).
        let read = progress.changes_read();
        let state = &mut *state;
        state.reads.push_back((read, now));
        if state.reads.len() > BACKLOG_SPAN {
            state.reads.pop_front();
        }
        let mut needed = u64::MAX;
        for (index, age) in &mut state.backlogs {
            let oldest = progress.oldest_unbuilt(index);
            *age = oldest.map_or(Duration::ZERO, |oldest| {
                needed = needed.min(oldest);
                let at = state.reads.partition_point(|(read, _)| *read < oldest);
                state.reads.get(at).map_or(Duration::ZERO, |(_, when)| {
                    now.saturating_duration_since(*when)
                })
            });
        }
        // Keep the first sample that had read the oldest unbuilt change, and
        // everything after it.
        while state.reads.get(1).is_some_and(|(read, _)| *read < needed) {
            state.reads.pop_front();
        }
        // What `landed` itself is decided by: one computation, so a tie (two
        // lanes at one source position) names what holds the slot back, never
        // its neighbour.
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

    /// Forget every backlog age and the reads kept for them: an engine run
    /// started or ended, and what it left is the next run's to read again.
    pub fn forget_backlog(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.reads.clear();
        for age in state.backlogs.values_mut() {
            *age = Duration::ZERO;
        }
    }

    /// How long ago `index`'s oldest change not built yet was read, as of
    /// the last sample (1 s resolution, at most [`BACKLOG_SPAN`] samples): 0
    /// while nothing waits to be built, or for an index with no lane.
    pub fn backlog(&self, index: &IndexName) -> Duration {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.backlogs.get(index).copied().unwrap_or(Duration::ZERO)
    }

    /// What holds the slot back the most, a lane or an index's pending
    /// documents, while anything is outstanding.
    pub fn pinned_by(&self) -> Option<Pin> {
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
                Landed::Free => Some(SourcePosition(u64::MAX)),
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

    /// Documents an index has pending, on no lane yet, stall none of its
    /// lanes: a healthy lane keeps age 0 while its index's backlog is what
    /// pins the slot, named by the index.
    #[test]
    fn a_healthy_lane_is_not_stalled_by_its_index_backlog() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        progress.mark([(
            source::document::DocumentId {
                index: users.index.clone(),
                key: source::RowKey(Vec::new()),
            },
            Position::new(1),
        )]);
        stalls.sample(start, &progress, &Lsns, Some(10));
        stalls.sample(at(start, 900), &progress, &Lsns, Some(10));
        assert_eq!(stalls.lane(&users), LaneStall::default());
        assert_eq!(stalls.pinned_by(), Some(Pin::Backlog(users.index.clone())));
        assert!(
            stalls
                .sample(at(start, 1300), &progress, &Lsns, Some(10))
                .is_empty(),
            "no lane is warned about"
        );
    }

    /// Slice 7's backlog age: how long ago the index's oldest unbuilt change
    /// was read: it ages a second a second while it stays; taken into a
    /// build it is still unbuilt; once its build is sent to the lanes nothing
    /// is unbuilt and the age is 0.
    #[test]
    fn an_index_backlog_ages_while_its_oldest_change_stays_unbuilt() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        let id = |key: i64| source::document::DocumentId {
            index: users.index.clone(),
            key: source::RowKey(vec![(
                kernel::ColumnName::try_new("id").unwrap(),
                kernel::GenericValue::BigInt(key),
            )]),
        };
        stalls.sample(start, &progress, &Lsns, None);
        assert_eq!(stalls.backlog(&users.index), Duration::ZERO, "none");
        progress.mark([(id(1), Position::new(1))]);
        progress.read_changes(1);
        stalls.sample(at(start, 1), &progress, &Lsns, None);
        progress.mark([(id(2), Position::new(2))]);
        progress.read_changes(1);
        stalls.sample(at(start, 31), &progress, &Lsns, None);
        assert_eq!(
            stalls.backlog(&users.index),
            Duration::from_secs(30),
            "the oldest stayed: its age, not the newest's"
        );
        let taken = progress.take(&users.index, 10, &|_| false).unwrap();
        stalls.sample(at(start, 40), &progress, &Lsns, None);
        assert_eq!(
            stalls.backlog(&users.index),
            Duration::from_secs(39),
            "taken into a build, it is still unbuilt"
        );
        progress.build_sending(&users.index, taken.token);
        stalls.sample(at(start, 41), &progress, &Lsns, None);
        assert_eq!(stalls.backlog(&users.index), Duration::ZERO, "built");
    }

    /// Reads `read` changes and builds `built` (oldest first, sent to the
    /// lanes) in each second of `seconds`, sampling at its end; returns the
    /// backlog age at each sample.
    fn backlog_over(seconds: &[(u64, usize)]) -> Vec<u64> {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        let mut next = 0i64;
        let mut ages = Vec::new();
        for (second, (read, built)) in seconds.iter().enumerate() {
            for _ in 0..*read {
                next += 1;
                progress.mark([(
                    source::document::DocumentId {
                        index: users.index.clone(),
                        key: source::RowKey(vec![(
                            kernel::ColumnName::try_new("id").unwrap(),
                            kernel::GenericValue::BigInt(next),
                        )]),
                    },
                    Position::new(next as u64),
                )]);
                progress.read_changes(1);
            }
            if *built > 0
                && let Some(taken) = progress.take(&users.index, *built, &|_| false)
            {
                progress.build_sending(&users.index, taken.token);
            }
            stalls.sample(at(start, second as u64 + 1), &progress, &Lsns, None);
            ages.push(stalls.backlog(&users.index).as_secs());
        }
        ages
    }

    /// The hour cap: a stuck index's backlog grows a second a second up to
    /// the samples kept (`BACKLOG_SPAN`), then reads an hour; nothing older is
    /// kept.
    #[test]
    fn a_backlog_older_than_the_samples_kept_reads_their_span() {
        let mut seconds = vec![(10, 0)];
        seconds.extend(vec![(0, 0); BACKLOG_SPAN + 99]);
        let ages = backlog_over(&seconds);
        let span = (BACKLOG_SPAN - 1) as u64;
        assert_eq!(ages.get(BACKLOG_SPAN - 1), Some(&span));
        assert_eq!(ages.last(), Some(&span), "capped at the span");
    }

    /// Steady healthy load: everything read in a second is built in it; the
    /// backlog stays 0.
    #[test]
    fn a_steady_index_has_no_backlog() {
        assert!(backlog_over(&[(10, 10); 30]).iter().all(|age| *age == 0));
    }

    /// A stuck index (or an exempt one growing): changes read, none built:
    /// the backlog grows a second a second from the first one read, whether
    /// the reading goes on or stopped at the lead.
    #[test]
    fn a_stuck_or_exempt_index_backlog_grows_a_second_a_second() {
        let growing = backlog_over(&[(10, 0); 20]);
        assert_eq!(growing, (0..20).collect::<Vec<u64>>());
        let mut stopped = vec![(10, 0)];
        stopped.extend([(0, 0); 19]);
        assert_eq!(backlog_over(&stopped), (0..20).collect::<Vec<u64>>());
    }

    /// An exempt index draining slowly: 100 changes read over ten seconds,
    /// then built at 20 a second: the backlog falls as it catches up (by a
    /// second a second, since it drains twice as fast as it was read), and
    /// is 0 once everything is built.
    #[test]
    fn a_draining_index_backlog_falls_as_it_catches_up() {
        let mut seconds = vec![(10, 0); 10];
        seconds.extend([(0, 20); 6]);
        let ages = backlog_over(&seconds);
        let draining = &ages[10..];
        // 9 s at the last second of reading; each second of draining takes
        // two seconds' reads; the fifth builds the last.
        assert_eq!(ages[9], 9);
        assert_eq!(draining, [8, 7, 6, 5, 0, 0], "{ages:?}");
    }

    #[test]
    fn a_lane_whose_oldest_item_stays_ages_and_resets_when_it_lands() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        progress.published(
            &users,
            stream::ItemKey::new(1),
            Position::new(1),
            Position::new(1),
        );
        progress.published(
            &users,
            stream::ItemKey::new(2),
            Position::new(2),
            Position::new(2),
        );
        stalls.sample(start, &progress, &Lsns, None);
        stalls.sample(at(start, 40), &progress, &Lsns, None);
        assert_eq!(stalls.lane(&users).stalled_for, Duration::from_secs(40));

        progress.applied(&users, stream::ItemKey::new(1));
        stalls.sample(at(start, 41), &progress, &Lsns, None);
        assert_eq!(
            stalls.lane(&users).stalled_for,
            Duration::ZERO,
            "the next item is the oldest now: the lane moved"
        );
        progress.applied(&users, stream::ItemKey::new(2));
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
            progress.published(
                &users,
                stream::ItemKey::new(second),
                Position::new(second),
                Position::new(second),
            );
            if second > 1 {
                progress.applied(&users, stream::ItemKey::new(second - 1));
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
        progress.published(
            &orders,
            stream::ItemKey::new(3),
            Position::new(3),
            Position::new(3),
        );
        progress.published(
            &users,
            stream::ItemKey::new(7),
            Position::new(7),
            Position::new(7),
        );
        stalls.sample(Instant::now(), &progress, &Lsns, Some(1_000));
        assert_eq!(stalls.pinned_by(), Some(Pin::Lane(orders.clone())));
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

    /// Two lanes at one source position (one commit, two
    /// changes); `pinned_by` names the lane `Progress::landed` is decided by,
    /// the one with the older change, not the first by name.
    #[test]
    fn a_tie_at_one_source_position_names_the_lane_landed_is_decided_by() {
        let aaa = lane("aaa");
        let zzz = lane("zzz");
        let progress = Progress::new([aaa.clone(), zzz.clone()]);
        let stalls = Stalls::new([aaa.clone(), zzz.clone()]);
        progress.published(
            &zzz,
            stream::ItemKey::new(5),
            Position::new(5),
            Position::new(5),
        );
        progress.published(
            &aaa,
            stream::ItemKey::new(9),
            Position::new(9),
            Position::new(9),
        );
        stalls.sample(Instant::now(), &progress, &OneCommit, Some(1_000));
        assert_eq!(progress.landed(), Landed::Before(Position::new(5)));
        assert_eq!(stalls.pinned_by(), Some(Pin::Lane(zzz)));
        assert_eq!(progress.pinned_by(), stalls.pinned_by());
    }

    #[test]
    fn wal_behind_is_clamped_at_zero() {
        let users = lane("users");
        let orders = lane("orders");
        let progress = Progress::new([users.clone(), orders.clone()]);
        let stalls = Stalls::new([users.clone(), orders.clone()]);
        progress.published(
            &orders,
            stream::ItemKey::new(1),
            Position::new(1),
            Position::new(1),
        );
        progress.published(
            &users,
            stream::ItemKey::new(50),
            Position::new(50),
            Position::new(50),
        );
        stalls.sample(Instant::now(), &progress, &Lsns, Some(100));
        assert_eq!(stalls.lane(&users).wal_behind, Some(0));
    }

    #[test]
    fn a_stall_is_logged_once_past_the_threshold_then_every_threshold_then_on_recovery() {
        let users = lane("users");
        let progress = Progress::new([users.clone()]);
        let stalls = Stalls::new([users.clone()]);
        let start = Instant::now();
        progress.published(
            &users,
            stream::ItemKey::new(1),
            Position::new(1),
            Position::new(1),
        );
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
        progress.applied(&users, stream::ItemKey::new(1));
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
