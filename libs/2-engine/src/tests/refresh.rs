//! A lane's caught-up write and what lands beside it (ADR 0012).
//!
//! A quiet lane's empty caught-up write is a flight: other writes land while
//! it refreshes, on its own lane and on others. Whatever lands, every written
//! position ends searchable without new traffic, and the run never waits on a
//! flight only it drives. The sink here can hold a lock across every
//! caught-up write, as the OpenSearch sink does (one refresh at a time), and
//! reports an unforced write `Deferred`, so only a caught-up write makes a
//! position searchable.
//!
//! Each test drives one engine over real channel lanes on paused time.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, Position, SinkName};
use sink::{CaughtUp, Sink, Visibility, WriteReport};
use stream::{Batch, LaneItem, LaneKey, Producer, Stream, WalkMark};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;

use super::{mapping, sink_name, users};
use crate::sink_engine::Blame;
use crate::{FailurePolicies, FailurePolicy, Observer, Progress, Result, SinkControl, SinkEngine};

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

/// A sink whose caught-up writes may share one lock, as the OpenSearch
/// sink's refreshes do. Ids starting with `slow` take `slow_ms`; an unforced
/// write is `Deferred`; a caught-up one is `Immediate` (or `Deferred` too,
/// with `deferred_even_caught_up`, which no sink here would do but an ordered
/// one may).
#[derive(Debug, Default)]
struct Refresher {
    lock: Option<tokio::sync::Mutex<()>>,
    refresh_ms: u64,
    slow_ms: u64,
    caught_up_ms: u64,
    deferred_even_caught_up: bool,
    ordered: bool,
    rejects_once: Mutex<HashSet<String>>,
    /// The next empty caught-up write fails.
    fails_a_refresh: std::sync::atomic::AtomicBool,
    now: AtomicUsize,
    most: AtomicUsize,
    refreshes: AtomicUsize,
    seeded: Mutex<BTreeSet<String>>,
    records: Mutex<Vec<String>>,
    /// Each non-empty write's `index/id`s, sorted, in the order they started.
    writes: Mutex<Vec<Vec<String>>>,
}

impl Refresher {
    fn locked(refresh_ms: u64, slow_ms: u64) -> Self {
        Self {
            lock: Some(tokio::sync::Mutex::new(())),
            refresh_ms,
            slow_ms,
            ..Self::default()
        }
    }

    fn unlocked(refresh_ms: u64, slow_ms: u64) -> Self {
        Self {
            refresh_ms,
            slow_ms,
            ..Self::default()
        }
    }

    async fn written(
        &self,
        parts: &[&[Envelope]],
        caught_up: CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        let empty = parts.iter().all(|part| part.is_empty());
        let envelopes = || parts.iter().flat_map(|part| part.iter());
        if !empty {
            let mut labels: Vec<String> = envelopes()
                .map(|e| format!("{}/{}", e.index, e.id))
                .collect();
            labels.sort();
            self.writes.lock().unwrap().push(labels);
        }
        if envelopes().any(|e| e.id.starts_with("slow")) {
            tokio::time::sleep(Duration::from_millis(self.slow_ms)).await;
        }
        let rejected: Vec<sink::RejectedDocument> = {
            let mut rejects = self.rejects_once.lock().unwrap();
            envelopes()
                .filter(|e| rejects.remove(&e.id))
                .map(|e| sink::RejectedDocument {
                    index: e.index.to_string(),
                    id: e.id.clone(),
                    reason: "refused".to_owned(),
                })
                .collect()
        };
        if !caught_up.is_caught_up() {
            let visibility = if empty {
                Visibility::Immediate
            } else {
                Visibility::Deferred
            };
            return Ok(WriteReport::clean().rejecting(rejected).visible(visibility));
        }
        let _guard = match &self.lock {
            Some(lock) => Some(lock.lock().await),
            None => None,
        };
        let ms = if empty {
            self.refresh_ms
        } else {
            self.caught_up_ms
        };
        tokio::time::sleep(Duration::from_millis(ms)).await;
        if empty && self.fails_a_refresh.swap(false, Ordering::SeqCst) {
            return Err(sink::SinkError::Write("refresh refused".to_owned()));
        }
        if empty {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
        }
        let visibility = if self.deferred_even_caught_up && !empty {
            Visibility::Deferred
        } else {
            Visibility::Immediate
        };
        Ok(WriteReport::clean().rejecting(rejected).visible(visibility))
    }
}

#[async_trait]
impl Sink for Refresher {
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
        caught_up: CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        let report = self.written(parts, caught_up).await;
        self.now.fetch_sub(1, Ordering::SeqCst);
        report
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.seeded.lock().unwrap().insert(index.to_string());
        Ok(())
    }

    async fn record_walked(&self, _: &IndexName, mark: &str) -> sink::Result<()> {
        self.records.lock().unwrap().push(mark.to_owned());
        Ok(())
    }

    fn ordered(&self) -> bool {
        self.ordered
    }
}

/// The positions each lane reported searchable, by index.
#[derive(Debug, Default)]
struct Searchable {
    reached: Mutex<Vec<(String, u64)>>,
}

impl Searchable {
    fn on(&self, index: &IndexName) -> Vec<u64> {
        self.reached
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| at == index.as_ref())
            .map(|(_, seq)| *seq)
            .collect()
    }

    fn reached(&self, index: &IndexName, seq: u64) -> bool {
        self.on(index).contains(&seq)
    }
}

impl Observer for Searchable {
    fn on_position_searchable(&self, lane: &LaneKey, position: Position) {
        self.reached
            .lock()
            .unwrap()
            .push((lane.index.to_string(), position.seq));
    }
}

/// One sink engine over the lanes of `indexes`, each lane's producing end,
/// and what it reported searchable.
struct Rig {
    sink: Arc<Refresher>,
    engine: Arc<SinkEngine>,
    stream: Arc<ChannelStream>,
    producers: Vec<(IndexName, Box<dyn Producer<LaneItem>>)>,
    searchable: Arc<Searchable>,
    running: Option<tokio::task::JoinHandle<Result<()>>>,
    _control: mpsc::Sender<SinkControl>,
}

impl Rig {
    fn new(sink: Refresher, indexes: &[IndexName], writes: usize, write_documents: usize) -> Self {
        Self::with_policies(
            sink,
            indexes,
            writes,
            write_documents,
            FailurePolicies::default(),
        )
    }

    fn with_policies(
        sink: Refresher,
        indexes: &[IndexName],
        writes: usize,
        write_documents: usize,
        policies: FailurePolicies,
    ) -> Self {
        let name: SinkName = sink_name("primary");
        let stream = Arc::new(ChannelStream::over(
            64,
            indexes.iter().cloned(),
            [name.clone()],
        ));
        let producers = indexes
            .iter()
            .map(|index| {
                let producer = stream
                    .lane(&LaneKey::new(index.clone(), name.clone()))
                    .unwrap()
                    .producer;
                (index.clone(), producer)
            })
            .collect();
        let sink = Arc::new(sink);
        let searchable = Arc::new(Searchable::default());
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
                Arc::new(Progress::of(indexes.iter().cloned(), [name])),
            )
            .with_writes(writes)
            .with_write_documents(write_documents)
            .with_failure_policies(policies)
            .with_observer(Arc::clone(&searchable) as Arc<dyn Observer>),
        );
        engine.set_blame(Blame::Nobody);
        let (control, _) = mpsc::channel(1);
        Self {
            sink,
            engine,
            stream,
            producers,
            searchable,
            running: None,
            _control: control,
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

    fn producer(&self, index: &IndexName) -> &dyn Producer<LaneItem> {
        self.producers
            .iter()
            .find(|(at, _)| at == index)
            .map(|(_, producer)| producer.as_ref())
            .unwrap()
    }

    async fn publish(&self, index: &IndexName, id: &str, position: Option<u64>) {
        self.publish_walked(index, id, position, None).await;
    }

    async fn publish_walked(
        &self,
        index: &IndexName,
        id: &str,
        position: Option<u64>,
        walked: Option<&str>,
    ) {
        self.producer(index)
            .publish(LaneItem::Batch(Batch {
                key: position.map(stream::ItemKey::new),
                position: position.map(Position::new),
                envelopes: vec![Envelope::upsert(
                    index.clone(),
                    id.to_owned(),
                    GenericValue::Map(BTreeMap::new()),
                    position.map(Position::new),
                    chrono::Utc::now(),
                )]
                .into(),
                hold: Default::default(),
                walked: walked.map(WalkMark::new),
                ticket: None,
            }))
            .await
            .unwrap();
    }

    async fn complete(&self, index: &IndexName) {
        self.producer(index)
            .publish(LaneItem::SnapshotComplete {
                index: index.clone(),
                ticket: stream::Ticket::new(1),
            })
            .await
            .unwrap();
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.abort();
        }
    }
}

/// U1 lands unforced and its lane's refresh R starts; while R is out, U3
/// (unforced) and U4 (caught up) are granted, U4 lands, then U3 lands, not
/// searchable. With no lock between caught-up writes, U4's caught-up write
/// returned before U3: only a caught-up write after U3 can cover it.
#[tokio::test(start_paused = true)]
async fn a_write_landing_on_its_lane_during_the_lanes_refresh_becomes_searchable() {
    let mut rig = Rig::new(Refresher::unlocked(200, 50), &[users()], 3, 1);
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    rig.publish(&users(), "b", Some(2)).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    rig.publish(&users(), "slow-c", Some(3)).await;
    rig.publish(&users(), "d", Some(4)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 4),
        "{:?}",
        rig.searchable.on(&users())
    );
}

/// The same with a lock over every caught-up write, as the OpenSearch sink
/// holds: U4's caught-up write waits for R and then covers U3.
#[tokio::test(start_paused = true)]
async fn a_write_landing_during_its_lanes_refresh_becomes_searchable_behind_a_refresh_lock() {
    let mut rig = Rig::new(Refresher::locked(200, 50), &[users()], 3, 1);
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    rig.publish(&users(), "b", Some(2)).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    rig.publish(&users(), "slow-c", Some(3)).await;
    rig.publish(&users(), "d", Some(4)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 4),
        "{:?}",
        rig.searchable.on(&users())
    );
}

/// Several unforced writes land during one refresh of their lane: one more
/// caught-up write once it returns covers them all, and no further one is
/// sent for nothing.
#[tokio::test(start_paused = true)]
async fn several_writes_landing_during_one_refresh_all_become_searchable() {
    for locked in [false, true] {
        let sink = if locked {
            Refresher::locked(200, 50)
        } else {
            Refresher::unlocked(200, 50)
        };
        let mut rig = Rig::new(sink, &[users()], 4, 1);
        rig.start();
        rig.publish(&users(), "slow-a", Some(1)).await;
        rig.publish(&users(), "b", Some(2)).await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        rig.publish(&users(), "slow-c", Some(3)).await;
        rig.publish(&users(), "slow-e", Some(4)).await;
        rig.publish(&users(), "f", Some(5)).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(rig.stream.is_idle(), "locked {locked}");
        assert!(
            rig.searchable.reached(&users(), 5),
            "locked {locked}: {:?}",
            rig.searchable.on(&users())
        );
        assert!(
            rig.sink.refreshes.load(Ordering::SeqCst) <= 2,
            "locked {locked}: one refresh, then one for what landed during it"
        );
    }
}

/// A live write that isn't caught up, then a backfill write that is, landing
/// after it: the backfill part, the lane's last, still sends the caught-up
/// write the live position needs.
#[tokio::test(start_paused = true)]
async fn a_backfill_part_landing_last_on_its_lane_keeps_the_lanes_caught_up_write() {
    let mut sink = Refresher::locked(10, 50);
    sink.caught_up_ms = 100;
    let mut rig = Rig::new(sink, &[users()], 3, 1);
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    rig.publish(&users(), "b", None).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 1),
        "{:?}",
        rig.searchable.on(&users())
    );
}

/// `orders` finishes its backfill and seeds while `users`' refresh is out,
/// behind the refresh lock: both land, the lanes drain, `users` is searchable.
#[tokio::test(start_paused = true)]
async fn a_seed_landing_during_another_lanes_refresh() {
    let mut rig = Rig::new(Refresher::locked(200, 50), &[users(), orders()], 3, 1);
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    rig.publish(&users(), "b", Some(2)).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    rig.publish(&orders(), "k", None).await;
    rig.complete(&orders()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        rig.sink.seeded.lock().unwrap().contains("orders"),
        "the seed landed during the refresh"
    );
    assert!(
        !rig.searchable.reached(&users(), 2),
        "the refresh is still out"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 2),
        "{:?}",
        rig.searchable.on(&users())
    );
}

/// A walk's checkpoint is recorded while its lane's refresh is out (no lock,
/// so the walk's own writes land meanwhile): the record lands, the refresh
/// lands, the lane drains and is searchable.
#[tokio::test(start_paused = true)]
async fn a_checkpoint_landing_during_its_lanes_refresh() {
    let mut rig = Rig::new(Refresher::unlocked(15_000, 50), &[users()], 3, 1);
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    rig.publish(&users(), "b", Some(2)).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    rig.publish_walked(&users(), "k01", None, Some("k01")).await;
    rig.publish_walked(&users(), "k02", None, Some("k02")).await;
    tokio::time::sleep(Duration::from_secs(11)).await;
    rig.publish_walked(&users(), "k03", None, Some("k03")).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !rig.sink.records.lock().unwrap().is_empty(),
        "a checkpoint was recorded"
    );
    assert_eq!(
        rig.sink.refreshes.load(Ordering::SeqCst),
        0,
        "while the refresh was still out"
    );
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 2),
        "{:?}",
        rig.searchable.on(&users())
    );
}

/// A merged write lands `users` and has `orders` refused under `stop`, which
/// parks `orders`; `users`, quiet, is owed a caught-up write, which fails and
/// ends the run with `users` written but not searchable (only an empty
/// caught-up write makes a position searchable here). The next run, before
/// anything new arrives, sends `users` the caught-up write it is owed, and
/// `orders`' probe lands.
#[tokio::test(start_paused = true)]
async fn a_restarted_run_makes_a_quiet_lanes_written_positions_searchable() {
    let sink = Refresher {
        deferred_even_caught_up: true,
        fails_a_refresh: true.into(),
        ..Refresher::locked(10, 0)
    };
    sink.rejects_once.lock().unwrap().insert("o".to_owned());
    let mut rig = Rig::with_policies(
        sink,
        &[users(), orders()],
        1,
        crate::BatchPolicy::default().max_changes,
        FailurePolicies::new(FailurePolicy::Stop),
    );
    rig.publish(&users(), "u", Some(1)).await;
    rig.publish(&orders(), "o", Some(2)).await;
    rig.start();
    assert!(
        rig.ended().await.is_err(),
        "the failed refresh ends the run"
    );
    assert!(!rig.searchable.reached(&users(), 1));

    rig.start();
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 1),
        "{:?}",
        rig.searchable.on(&users())
    );
    assert!(rig.searchable.reached(&orders(), 2));
}

/// A merged write lands `users` and parks `orders` on a `stop` refusal: with
/// the run going on, the quiet `users` lane gets its caught-up write at once,
/// and `orders` once its probe lands.
#[tokio::test(start_paused = true)]
async fn a_quiet_lane_beside_a_parked_one_is_made_searchable() {
    let sink = Refresher {
        deferred_even_caught_up: true,
        ..Refresher::locked(10, 0)
    };
    sink.rejects_once.lock().unwrap().insert("o".to_owned());
    let mut rig = Rig::with_policies(
        sink,
        &[users(), orders()],
        1,
        crate::BatchPolicy::default().max_changes,
        FailurePolicies::new(FailurePolicy::Stop),
    );
    rig.publish(&users(), "u", Some(1)).await;
    rig.publish(&orders(), "o", Some(2)).await;
    rig.start();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        rig.searchable.reached(&users(), 1),
        "{:?}",
        rig.searchable.on(&users())
    );
    assert!(!rig.searchable.reached(&orders(), 2), "orders is parked");
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(rig.searchable.reached(&orders(), 2));
}

/// An ordered sink writes one thing at a time, its caught-up writes too: a
/// lane's refresh never runs beside another lane's write.
#[tokio::test(start_paused = true)]
async fn an_ordered_sinks_refresh_runs_alone() {
    let sink = Refresher {
        refresh_ms: 200,
        slow_ms: 50,
        deferred_even_caught_up: true,
        ordered: true,
        ..Refresher::default()
    };
    let mut rig = Rig::new(sink, &[users(), orders()], 2, 1);
    rig.start();
    rig.publish(&users(), "a", Some(1)).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    rig.publish(&orders(), "o", Some(2)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(rig.searchable.reached(&users(), 1));
    assert!(rig.searchable.reached(&orders(), 2));
    assert_eq!(
        rig.sink.most.load(Ordering::SeqCst),
        1,
        "one write at a time"
    );
}

/// An item waiting for its lane's caught-up write is still unfinished: one
/// landing behind it meanwhile is no progress, and the front landing once the
/// refresh returns is one step.
#[tokio::test(start_paused = true)]
async fn an_item_landing_behind_one_waiting_for_its_refresh_is_no_progress() {
    let mut rig = Rig::new(Refresher::unlocked(200, 50), &[users()], 3, 1);
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    rig.publish(&users(), "b", Some(2)).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    let before = rig.engine.advanced();
    rig.publish(&users(), "e", Some(3)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert_eq!(
        rig.engine.advanced() - before,
        1,
        "e landed behind a, which waited for its refresh"
    );
}

/// `users`' refresh is out; `orders`' slow write is out too, and an item of
/// each lane waits for it, then goes out merged while the refresh is still
/// out. The merged write's `users` part lands during `users`' refresh: once
/// it returns, `users` is checked again, and both lanes end searchable. No
/// lock here (behind one, every caught-up write would wait for the refresh
/// and the merge wait would end first), and every non-empty write is
/// `Deferred`, so a refresh is what makes a position searchable.
#[tokio::test(start_paused = true)]
async fn a_merged_write_landing_during_a_lanes_refresh() {
    let sink = Refresher {
        deferred_even_caught_up: true,
        ..Refresher::unlocked(200, 30)
    };
    let mut rig = Rig::new(
        sink,
        &[users(), orders()],
        4,
        crate::BatchPolicy::default().max_changes,
    );
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    tokio::time::sleep(Duration::from_millis(35)).await;
    rig.publish(&orders(), "slow-o", Some(3)).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    rig.publish(&users(), "u", Some(4)).await;
    rig.publish(&orders(), "o", Some(5)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        rig.sink
            .writes
            .lock()
            .unwrap()
            .contains(&vec!["orders/o".to_owned(), "users/u".to_owned()]),
        "u and o went out merged: {:?}",
        rig.sink.writes.lock().unwrap()
    );
    assert_eq!(
        rig.sink.refreshes.load(Ordering::SeqCst),
        0,
        "while users' refresh was still out"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 4),
        "{:?}",
        rig.searchable.on(&users())
    );
    assert!(
        rig.searchable.reached(&orders(), 5),
        "{:?}",
        rig.searchable.on(&orders())
    );
}

/// Behind the refresh lock, writes landing during a lane's refresh, merged or
/// not, still end with every lane searchable.
#[tokio::test(start_paused = true)]
async fn writes_during_a_lanes_refresh_behind_the_lock_end_searchable() {
    let mut rig = Rig::new(
        Refresher::locked(200, 50),
        &[users(), orders()],
        4,
        crate::BatchPolicy::default().max_changes,
    );
    rig.start();
    rig.publish(&users(), "slow-a", Some(1)).await;
    rig.publish(&users(), "b", Some(2)).await;
    tokio::time::sleep(Duration::from_millis(55)).await;
    rig.publish(&orders(), "slow-o", Some(3)).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    rig.publish(&users(), "u", Some(4)).await;
    rig.publish(&orders(), "o", Some(5)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.stream.is_idle());
    assert!(
        rig.searchable.reached(&users(), 4),
        "{:?}",
        rig.searchable.on(&users())
    );
    assert!(
        rig.searchable.reached(&orders(), 5),
        "{:?}",
        rig.searchable.on(&orders())
    );
}
