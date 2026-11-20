//! Items ready while a write is out wait for it, bounded, and go out merged
//! (ADR 0012, stage 7's merging reopened).
//!
//! A live item that becomes ready while some write is out waits until the
//! writes out return, then goes out in one write with whatever else is ready,
//! as `main` wrote one batch over every index. The wait is bounded hard by
//! the merge wait (the batch policy's `max_delay`, 50 ms by default) from when
//! the item became ready, and a write out longer than that is not waited for,
//! so a stalled or slow lane never holds another index's writes longer than
//! the bound. Backfill is never held, an ordered sink is unaffected, and a
//! change on a quiet engine is written at once.
//!
//! Each test drives one engine over real channel lanes on paused time.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, Position, SinkName};
use sink::{CaughtUp, Sink, WriteReport};
use stream::{Batch, LaneItem, LaneKey, Producer, Stream};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::{mapping, sink_name, users};
use crate::sink_engine::Blame;
use crate::{FailurePolicies, FailurePolicy, Observer, Progress, Result, SinkControl, SinkEngine};

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

/// A sink that records each write's parts (`index/id`) and when it started.
/// Ids starting with `stuck` never return, `slow` take 30 ms; a rejected id is
/// refused item by item, and a failing one fails its whole write once.
#[derive(Debug, Default)]
struct Recorder {
    started_at: Mutex<Option<Instant>>,
    writes: Mutex<Vec<(Duration, Vec<String>)>>,
    rejects_once: Mutex<HashSet<String>>,
    fail_once: Mutex<HashSet<String>>,
    ordered: bool,
}

impl Recorder {
    fn ordered() -> Self {
        Self {
            ordered: true,
            ..Self::default()
        }
    }

    fn rejecting_once(self, ids: &[&str]) -> Self {
        self.rejects_once
            .lock()
            .unwrap()
            .extend(ids.iter().map(|id| (*id).to_owned()));
        self
    }

    fn failing_once(self, ids: &[&str]) -> Self {
        self.fail_once
            .lock()
            .unwrap()
            .extend(ids.iter().map(|id| (*id).to_owned()));
        self
    }

    /// Every write so far: when it started (from the rig's start), and its
    /// `index/id`s sorted.
    fn writes(&self) -> Vec<(Duration, Vec<String>)> {
        self.writes.lock().unwrap().clone()
    }

    /// When the write holding `index/id` first started.
    fn started(&self, label: &str) -> Option<Duration> {
        self.writes()
            .into_iter()
            .find(|(_, ids)| ids.iter().any(|id| id == label))
            .map(|(at, _)| at)
    }

    /// Whether one write held both `a` and `b`.
    fn merged(&self, a: &str, b: &str) -> bool {
        self.writes()
            .iter()
            .any(|(_, ids)| ids.iter().any(|id| id == a) && ids.iter().any(|id| id == b))
    }

    /// How many writes held `index/id`.
    fn times_written(&self, label: &str) -> usize {
        self.writes()
            .iter()
            .filter(|(_, ids)| ids.iter().any(|id| id == label))
            .count()
    }
}

#[async_trait]
impl Sink for Recorder {
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        let scope = if caught_up {
            CaughtUp::All
        } else {
            CaughtUp::No
        };
        self.write_parts(&[envelopes], scope).await
    }

    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        _: CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        let envelopes: Vec<&Envelope> = parts.iter().flat_map(|part| part.iter()).collect();
        if envelopes.is_empty() {
            return Ok(WriteReport::clean());
        }
        let mut labels: Vec<String> = envelopes
            .iter()
            .map(|e| format!("{}/{}", e.index, e.id))
            .collect();
        labels.sort();
        let since = self
            .started_at
            .lock()
            .unwrap()
            .map_or(Duration::ZERO, |at| at.elapsed());
        self.writes.lock().unwrap().push((since, labels));
        if envelopes.iter().any(|e| e.id.starts_with("stuck")) {
            std::future::pending::<()>().await;
        }
        if envelopes.iter().any(|e| e.id.starts_with("slow")) {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        if envelopes
            .iter()
            .any(|e| self.fail_once.lock().unwrap().remove(&e.id))
        {
            return Err(sink::SinkError::Write("the write failed".to_owned()));
        }
        let rejected: Vec<sink::RejectedDocument> = {
            let mut rejects = self.rejects_once.lock().unwrap();
            envelopes
                .iter()
                .filter(|e| rejects.remove(&e.id))
                .map(|e| sink::RejectedDocument {
                    index: e.index.to_string(),
                    id: e.id.clone(),
                    reason: "refused".to_owned(),
                })
                .collect()
        };
        Ok(WriteReport::clean().rejecting(rejected))
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    fn ordered(&self) -> bool {
        self.ordered
    }
}

/// The documents a sink engine quarantined, as `index/id`.
#[derive(Debug, Default)]
struct Quarantined {
    seen: Mutex<BTreeSet<String>>,
}

impl Observer for Quarantined {
    fn on_document_quarantined(&self, _: &SinkName, index: &str, id: &str, _: &str) {
        self.seen.lock().unwrap().insert(format!("{index}/{id}"));
    }
}

/// One sink engine over the `users` and `orders` lanes, with the default
/// merge wait and merge limit, and each lane's producing end.
struct Rig {
    sink: Arc<Recorder>,
    engine: Arc<SinkEngine>,
    stream: Arc<ChannelStream>,
    users: Box<dyn Producer<LaneItem>>,
    orders: Option<Box<dyn Producer<LaneItem>>>,
    quarantined: Arc<Quarantined>,
    running: Option<tokio::task::JoinHandle<Result<()>>>,
    began: Instant,
}

impl Rig {
    fn new(sink: Recorder, writes: usize) -> Self {
        Self::with_policies(sink, writes, FailurePolicies::default())
    }

    fn with_policies(sink: Recorder, writes: usize, policies: FailurePolicies) -> Self {
        Self::over(sink, writes, policies, &[users(), orders()])
    }

    /// A rig over `indexes`' lanes: `users`, and `orders` when listed.
    fn over(
        sink: Recorder,
        writes: usize,
        policies: FailurePolicies,
        indexes: &[IndexName],
    ) -> Self {
        let name: SinkName = sink_name("primary");
        let indexes = indexes.to_vec();
        let stream = Arc::new(ChannelStream::over(64, indexes.clone(), [name.clone()]));
        let lane = |index: IndexName| {
            stream
                .lane(&LaneKey::new(index, name.clone()))
                .unwrap()
                .producer
        };
        let users_lane = lane(users());
        let orders_lane = indexes.contains(&orders()).then(|| lane(orders()));
        let began = Instant::now();
        *sink.started_at.lock().unwrap() = Some(began);
        let sink = Arc::new(sink);
        let quarantined = Arc::new(Quarantined::default());
        let engine = Arc::new(
            SinkEngine::new(
                name.clone(),
                Arc::clone(&sink) as Arc<dyn Sink>,
                Arc::clone(&stream) as Arc<dyn Stream>,
                indexes
                    .iter()
                    .map(|index| IndexMapping {
                        index: index.clone(),
                        ..mapping()
                    })
                    .collect(),
                Arc::new(Progress::of(indexes.clone(), [name])),
            )
            .with_writes(writes)
            .with_failure_policies(policies)
            .with_observer(Arc::clone(&quarantined) as Arc<dyn Observer>),
        );
        engine.set_blame(Blame::Nobody);
        Self {
            sink,
            engine,
            stream,
            users: users_lane,
            orders: orders_lane,
            quarantined,
            running: None,
            began,
        }
    }

    fn start(&mut self) {
        let engine = Arc::clone(&self.engine);
        self.running = Some(tokio::spawn(async move {
            let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
            engine.run(&mut control).await
        }));
    }

    async fn ended(&mut self) -> Result<()> {
        self.running.take().unwrap().await.unwrap()
    }

    /// Publish one document on `index`'s lane, live at `position` or backfill.
    async fn publish(&self, index: &IndexName, id: &str, position: Option<u64>) -> Duration {
        let producer = if *index == users() {
            &self.users
        } else {
            self.orders.as_ref().unwrap()
        };
        producer
            .publish(LaneItem::Batch(Batch {
                key: position.map(stream::ItemKey::new),
                position: position.map(Position::new),
                changes: 1,
                envelopes: vec![Envelope::upsert(
                    index.clone(),
                    id.to_owned(),
                    GenericValue::Map(BTreeMap::new()),
                    position.map(Position::new),
                    chrono::Utc::now(),
                )]
                .into(),
                hold: Default::default(),
                walked: None,
                ticket: None,
            }))
            .await
            .unwrap();
        self.began.elapsed()
    }

    fn quarantined(&self) -> BTreeSet<String> {
        self.quarantined.seen.lock().unwrap().clone()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.abort();
        }
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// The default merge wait: the batch policy's `max_delay`.
fn bound() -> Duration {
    crate::BatchPolicy::default().max_delay
}

/// `users`' write is out; `orders` and `users` each get an item meanwhile.
/// They wait for it, then go out together in one write.
#[tokio::test(start_paused = true)]
async fn items_ready_while_a_write_is_out_go_out_together_once_it_returns() {
    let mut rig = Rig::new(Recorder::default(), 4);
    rig.start();
    rig.publish(&users(), "slow-u1", Some(1)).await;
    tokio::time::sleep(ms(5)).await;
    rig.publish(&orders(), "o1", Some(2)).await;
    rig.publish(&users(), "u2", Some(3)).await;
    tokio::time::sleep(ms(200)).await;
    assert!(rig.stream.is_idle());
    let writes: Vec<Vec<String>> = rig.sink.writes().into_iter().map(|(_, ids)| ids).collect();
    assert_eq!(
        writes,
        [
            vec!["users/slow-u1".to_owned()],
            vec!["orders/o1".to_owned(), "users/u2".to_owned()],
        ],
    );
    assert_eq!(
        rig.sink.started("orders/o1"),
        Some(ms(30)),
        "once the write out returned"
    );
}

/// With one lane there is nothing to merge with: an item ready while that
/// lane's write is out starts at once, not held for the merge wait.
#[tokio::test(start_paused = true)]
async fn a_single_lane_item_is_never_held_for_a_merge() {
    let mut rig = Rig::over(
        Recorder::default(),
        4,
        FailurePolicies::default(),
        &[users()],
    );
    rig.start();
    rig.publish(&users(), "slow-u1", Some(1)).await;
    tokio::time::sleep(ms(5)).await;
    let published = rig.publish(&users(), "u2", Some(2)).await;
    tokio::time::sleep(ms(200)).await;
    assert!(rig.stream.is_idle());
    assert_eq!(rig.sink.started("users/slow-u1"), Some(ms(0)));
    assert_eq!(rig.sink.started("users/u2"), Some(published));
}

/// A change on a quiet engine is written at once: nothing is out to wait for.
#[tokio::test(start_paused = true)]
async fn a_lone_change_on_a_quiet_engine_is_written_at_once() {
    let mut rig = Rig::new(Recorder::default(), 4);
    rig.start();
    tokio::time::sleep(ms(10)).await;
    let published = rig.publish(&users(), "u", Some(1)).await;
    tokio::time::sleep(ms(1)).await;
    assert_eq!(rig.sink.started("users/u"), Some(published));
}

/// `orders`' write never returns. Every `users` item, the first and every one
/// after, goes out within the bound of becoming ready, and once the stuck
/// write is older than the bound, at once: it is not waited for.
#[tokio::test(start_paused = true)]
async fn a_stalled_lanes_write_holds_no_other_lanes_item_beyond_the_bound() {
    let mut rig = Rig::new(Recorder::default(), 4);
    rig.start();
    rig.publish(&orders(), "stuck-o", Some(1)).await;
    for k in 0..6 {
        tokio::time::sleep(if k == 0 { ms(10) } else { ms(100) }).await;
        let id = format!("u{k}");
        let published = rig.publish(&users(), &id, Some(2 + k)).await;
        tokio::time::sleep(bound() + ms(1)).await;
        let started = rig
            .sink
            .started(&format!("users/{id}"))
            .unwrap_or_else(|| panic!("{id} is written within the bound"));
        assert!(
            started - published <= bound(),
            "{id} waited {:?}",
            started - published
        );
        if k > 0 {
            assert_eq!(
                started, published,
                "{id}: a write out longer than the bound is not waited for"
            );
        }
    }
}

/// `orders` keeps a fresh slow write out all the time. Each `users` item may
/// wait for one, but never longer than the bound.
#[tokio::test(start_paused = true)]
async fn a_busy_sibling_lane_holds_no_item_beyond_the_bound() {
    let mut rig = Rig::new(Recorder::default(), 4);
    rig.start();
    let mut published = Vec::new();
    for k in 0..30u64 {
        rig.publish(&orders(), &format!("slow-o{k}"), Some(100 + 2 * k))
            .await;
        if k % 3 == 1 {
            let id = format!("u{k}");
            let at = rig.publish(&users(), &id, Some(101 + 2 * k)).await;
            published.push((id, at));
        }
        tokio::time::sleep(ms(20)).await;
    }
    tokio::time::sleep(ms(500)).await;
    assert!(rig.stream.is_idle());
    for (id, at) in published {
        let started = rig.sink.started(&format!("users/{id}")).unwrap();
        assert!(started - at <= bound(), "{id} waited {:?}", started - at);
    }
}

/// Backfill is never held: with a backfill item waiting, nothing is, even
/// with a write out and a live item ready beside it.
#[tokio::test(start_paused = true)]
async fn a_backfill_item_is_never_held() {
    let mut rig = Rig::new(Recorder::default(), 4);
    rig.start();
    rig.publish(&users(), "slow-u1", Some(1)).await;
    tokio::time::sleep(ms(5)).await;
    let published = rig.publish(&orders(), "b", None).await;
    rig.publish(&users(), "u2", Some(2)).await;
    tokio::time::sleep(ms(1)).await;
    assert_eq!(rig.sink.started("orders/b"), Some(published));
}

/// An ordered sink writes one item at a time, in position order, with no
/// wait added between them.
#[tokio::test(start_paused = true)]
async fn an_ordered_sink_is_unaffected() {
    let mut rig = Rig::new(Recorder::ordered(), 4);
    rig.start();
    rig.publish(&users(), "slow-u1", Some(1)).await;
    tokio::time::sleep(ms(5)).await;
    rig.publish(&orders(), "o1", Some(2)).await;
    rig.publish(&users(), "u2", Some(3)).await;
    tokio::time::sleep(ms(200)).await;
    assert!(rig.stream.is_idle());
    assert_eq!(
        rig.sink.writes(),
        [
            (ms(0), vec!["users/slow-u1".to_owned()]),
            (ms(30), vec!["orders/o1".to_owned()]),
            (ms(30), vec!["users/u2".to_owned()]),
        ],
    );
}

/// A write merged by the wait: `users/u2` is refused under `skip` and
/// quarantined as `users`', `orders/o1` lands; both are acknowledged.
#[tokio::test(start_paused = true)]
async fn a_rejection_in_a_merged_write_is_its_own_lanes() {
    let mut rig = Rig::with_policies(
        Recorder::default().rejecting_once(&["u2"]),
        4,
        FailurePolicies::new(FailurePolicy::Skip),
    );
    rig.start();
    rig.publish(&users(), "slow-u1", Some(1)).await;
    tokio::time::sleep(ms(5)).await;
    rig.publish(&orders(), "o1", Some(2)).await;
    rig.publish(&users(), "u2", Some(3)).await;
    tokio::time::sleep(ms(200)).await;
    assert!(rig.stream.is_idle());
    assert_eq!(rig.sink.times_written("orders/o1"), 1);
    assert!(
        rig.sink.merged("orders/o1", "users/u2"),
        "one merged write: {:?}",
        rig.sink.writes()
    );
    assert_eq!(rig.quarantined(), BTreeSet::from(["users/u2".to_owned()]));
}

/// A merged write failing as a whole fails both its lanes; the restarted
/// run writes both items again.
#[tokio::test(start_paused = true)]
async fn a_failed_merged_write_is_blamed_on_its_lanes_and_both_come_back() {
    let mut rig = Rig::new(Recorder::default().failing_once(&["o1"]), 4);
    rig.start();
    rig.publish(&users(), "slow-u1", Some(1)).await;
    tokio::time::sleep(ms(5)).await;
    rig.publish(&orders(), "o1", Some(2)).await;
    rig.publish(&users(), "u2", Some(3)).await;
    assert!(rig.ended().await.is_err());
    assert!(
        rig.sink.merged("orders/o1", "users/u2"),
        "one merged write: {:?}",
        rig.sink.writes()
    );
    assert_eq!(
        rig.engine.blame(),
        Blame::Lanes(
            [
                LaneKey::new(users(), sink_name("primary")),
                LaneKey::new(orders(), sink_name("primary")),
            ]
            .into()
        ),
    );
    rig.start();
    tokio::time::sleep(ms(200)).await;
    assert!(rig.stream.is_idle());
    assert_eq!(rig.sink.times_written("orders/o1"), 2);
    assert_eq!(rig.sink.times_written("users/u2"), 2);
}

/// A `stop` rejection of `orders/o1` in a merged write parks `orders` alone;
/// `users/u2` landed and is not written again, and `orders/o1` lands on its
/// probe, with no restart.
#[tokio::test(start_paused = true)]
async fn a_stop_rejection_in_a_merged_write_loses_no_other_part() {
    let mut rig = Rig::with_policies(
        Recorder::default().rejecting_once(&["o1"]),
        4,
        FailurePolicies::new(FailurePolicy::Stop),
    );
    rig.start();
    rig.publish(&users(), "slow-u1", Some(1)).await;
    tokio::time::sleep(ms(5)).await;
    rig.publish(&orders(), "o1", Some(2)).await;
    rig.publish(&users(), "u2", Some(3)).await;
    tokio::time::sleep(ms(200)).await;
    assert!(
        rig.sink.merged("orders/o1", "users/u2"),
        "one merged write: {:?}",
        rig.sink.writes()
    );
    assert!(
        !rig.engine
            .lane_health(&LaneKey::new(orders(), sink_name("primary")))
            .is_open()
    );
    tokio::time::sleep(ms(1_500)).await;
    assert!(rig.stream.is_idle());
    assert_eq!(rig.sink.times_written("orders/o1"), 2);
    assert_eq!(rig.sink.times_written("users/u2"), 1);
}

/// When the wait ends, another lane's item may go first (lanes take turns),
/// alone, and its write is young: the item still waiting is not held again
/// for it. Its wait counts from when it became ready, and ends there.
#[tokio::test(start_paused = true)]
async fn an_item_waits_no_longer_than_the_bound_when_another_write_starts_meanwhile() {
    let mut rig = Rig::new(Recorder::default(), 4);
    rig.start();
    rig.publish(&users(), "stuck-u0", Some(1)).await;
    tokio::time::sleep(ms(10)).await;
    let published = rig.publish(&users(), "u1", Some(2)).await;
    rig.publish(&orders(), "stuck-o1", Some(3)).await;
    tokio::time::sleep(ms(200)).await;
    let started = rig.sink.started("users/u1").expect("u1 is written");
    assert!(
        rig.sink.started("orders/stuck-o1") <= Some(started),
        "orders went first: {:?}",
        rig.sink.writes()
    );
    assert!(
        started - published <= bound(),
        "u1 waited {:?}",
        started - published
    );
}

/// Runs `body` on a paused current-thread runtime in its own
/// thread, and fail if it doesn't finish within `real` wall-clock time (a
/// task that never yields to an idle runtime keeps paused time from moving).
fn within_real_time<F>(real: std::time::Duration, body: impl FnOnce() -> F + Send + 'static)
where
    F: std::future::Future<Output = ()>,
{
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap();
        runtime.block_on(body());
        let _ = done.send(());
    });
    assert!(
        finished.recv_timeout(real).is_ok(),
        "the paused runtime never went idle: something spins"
    );
}

/// A hold set before the run failed is not decided again (a failed run
/// grants nothing), so it must end when its instant passes: left in place it
/// is ready on every turn while a write is still out, and the loop spins.
#[test]
fn a_hold_outliving_a_failure_does_not_spin() {
    within_real_time(std::time::Duration::from_secs(10), || async {
        let mut rig = Rig::new(Recorder::default().failing_once(&["slow-f1"]), 4);
        rig.start();
        rig.publish(&orders(), "stuck-o", Some(1)).await;
        tokio::time::sleep(ms(60)).await;
        rig.publish(&users(), "slow-f1", Some(2)).await;
        tokio::time::sleep(ms(5)).await;
        rig.publish(&users(), "u2", Some(3)).await;
        tokio::time::sleep(ms(300)).await;
        assert!(rig.sink.started("users/slow-f1").is_some());
    });
}

/// A pending reindex grants nothing either, so a hold set before it must end
/// when its instant passes, as after a failure.
#[test]
fn a_hold_outliving_a_pending_reindex_does_not_spin() {
    within_real_time(std::time::Duration::from_secs(10), || async {
        let mut rig = Rig::new(Recorder::default(), 4);
        let engine = Arc::clone(&rig.engine);
        let (control_tx, mut control) = mpsc::channel::<SinkControl>(1);
        rig.running = Some(tokio::spawn(async move { engine.run(&mut control).await }));
        rig.publish(&orders(), "stuck-o", Some(1)).await;
        tokio::time::sleep(ms(60)).await;
        rig.publish(&users(), "slow-u1", Some(2)).await;
        tokio::time::sleep(ms(5)).await;
        rig.publish(&users(), "u2", Some(3)).await;
        tokio::time::sleep(ms(1)).await;
        control_tx
            .send(SinkControl::Reindex {
                indexes: vec![users()],
            })
            .await
            .unwrap();
        tokio::time::sleep(ms(300)).await;
        assert!(rig.sink.started("users/slow-u1").is_some());
    });
}
