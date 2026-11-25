//! A sink seeds several indexes at once (ADR 0012, seeding beside writes):
//! a lane's `SnapshotComplete`, once nothing before it on its lane is waiting
//! or being written, runs `mark_seeded` as a flight holding one write slot.
//!
//! - other lanes keep writing, landing and seeding meanwhile;
//! - the seeding lane's later items wait for its seed;
//! - a failed seed fails the run once the flights beside it are done, and the
//!   redelivered `SnapshotComplete` seeds the index once;
//! - a stop lets a seed in flight finish; a reindex waits for it;
//! - an ordered sink seeds one index at a time, between its writes.
//!
//! Each test drives one engine over real channel lanes, publishing items by
//! hand, on paused time. The randomized test replays with
//! `FLUSSO_SCHEDULE_SEED=<seed>`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{ContentHash, Envelope, GenericValue, IndexMapping, IndexName, Position};
use sink::{Sink, WriteReport};
use stream::{Batch, LaneItem, LaneKey, Producer, Stream};
use stream_channel::ChannelStream;
use tokio::sync::Notify;

use super::sink_name;
use crate::{Progress, Result, SinkControl, SinkEngine, Stop};

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

/// A small deterministic generator, so a schedule replays from its seed.
#[derive(Debug)]
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// A sink that logs each write and seed as it starts and ends, by index,
/// counts how many run at once, and holds or fails the ones a test names:
/// `write <index>` or `seed <index>`.
#[derive(Debug, Default)]
struct Gate {
    log: Mutex<Vec<String>>,
    held: Mutex<HashSet<String>>,
    released: Notify,
    fail_once: Mutex<HashSet<String>>,
    now: AtomicUsize,
    most: AtomicUsize,
    seeds_now: AtomicUsize,
    seeds_most: AtomicUsize,
    seeded: Mutex<HashMap<String, usize>>,
    delays: Option<Mutex<Rng>>,
    ordered: bool,
    /// Indexes whose seed the sink refuses because the generation it would
    /// retire is blocked (#479), and how often each was tried.
    retire_blocked: Mutex<HashSet<String>>,
    seed_tries: Mutex<HashMap<String, usize>>,
    /// When each refused seed was tried, by index.
    refused_at: Mutex<HashMap<String, Vec<tokio::time::Instant>>>,
}

impl Gate {
    fn holding(self, what: &[&str]) -> Self {
        self.held
            .lock()
            .unwrap()
            .extend(what.iter().map(|w| (*w).to_owned()));
        self
    }

    fn failing_once(self, what: &[&str]) -> Self {
        self.fail_once
            .lock()
            .unwrap()
            .extend(what.iter().map(|w| (*w).to_owned()));
        self
    }

    /// Refuse `indexes`' seeds as a blocked old generation would.
    fn retire_blocked(self, indexes: &[&str]) -> Self {
        self.retire_blocked
            .lock()
            .unwrap()
            .extend(indexes.iter().map(|i| (*i).to_owned()));
        self
    }

    /// Lift the block on `index`'s old generation.
    fn lift(&self, index: &str) {
        self.retire_blocked.lock().unwrap().remove(index);
    }

    fn seed_tries(&self, index: &str) -> usize {
        self.seed_tries
            .lock()
            .unwrap()
            .get(index)
            .copied()
            .unwrap_or(0)
    }

    fn release(&self, what: &str) {
        self.held.lock().unwrap().remove(what);
        self.released.notify_waiters();
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn logged(&self, line: &str) -> bool {
        self.log().iter().any(|l| l == line)
    }

    fn at(&self, line: &str) -> usize {
        let log = self.log();
        log.iter()
            .position(|l| l == line)
            .unwrap_or_else(|| panic!("{line} not in {log:?}"))
    }

    fn seeded(&self, index: &str) -> usize {
        self.seeded.lock().unwrap().get(index).copied().unwrap_or(0)
    }

    /// Log `what` started, wait while it is held, take a random delay, and
    /// log how it ended: whether it fails.
    async fn pass(&self, what: &str) -> bool {
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        self.log.lock().unwrap().push(format!("start {what}"));
        loop {
            let released = self.released.notified();
            if !self.held.lock().unwrap().contains(what) {
                break;
            }
            released.await;
        }
        let delay = self.delays.as_ref().map(|rng| rng.lock().unwrap().below(6));
        if let Some(delay) = delay {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let failed = self.fail_once.lock().unwrap().remove(what);
        self.now.fetch_sub(1, Ordering::SeqCst);
        self.log
            .lock()
            .unwrap()
            .push(format!("{} {what}", if failed { "failed" } else { "end" }));
        failed
    }
}

#[async_trait]
impl Sink for Gate {
    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        let Some(first) = envelopes.first() else {
            return Ok(WriteReport::clean());
        };
        let what = format!("write {}", first.index);
        if self.pass(&what).await {
            return Err(sink::SinkError::Write(format!("{what} failed")));
        }
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        *self
            .seed_tries
            .lock()
            .unwrap()
            .entry(index.to_string())
            .or_default() += 1;
        if self.retire_blocked.lock().unwrap().contains(index.as_ref()) {
            self.refused_at
                .lock()
                .unwrap()
                .entry(index.to_string())
                .or_default()
                .push(tokio::time::Instant::now());
            return Err(sink::SinkError::RetireBlocked {
                new: format!("{index}_1_2"),
                old: format!("{index}_1_1"),
                block: "index [old] blocked by: [TOO_MANY_REQUESTS/12/disk usage exceeded \
                        flood-stage watermark, index has read-only-allow-delete block]"
                    .to_owned(),
                deletable: true,
            });
        }
        let now = self.seeds_now.fetch_add(1, Ordering::SeqCst) + 1;
        self.seeds_most.fetch_max(now, Ordering::SeqCst);
        let what = format!("seed {index}");
        let failed = self.pass(&what).await;
        self.seeds_now.fetch_sub(1, Ordering::SeqCst);
        if failed {
            return Err(sink::SinkError::Write(format!("{what} failed")));
        }
        *self
            .seeded
            .lock()
            .unwrap()
            .entry(index.to_string())
            .or_default() += 1;
        Ok(())
    }

    fn ordered(&self) -> bool {
        self.ordered
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("reindex {}", mapping.index));
        Ok(())
    }
}

/// One sink engine over one lane per index.
struct Rig {
    gate: Arc<Gate>,
    engine: Arc<SinkEngine>,
    stream: Arc<ChannelStream>,
    progress: Arc<Progress>,
    lanes: BTreeMap<String, Box<dyn Producer<LaneItem>>>,
    control: tokio::sync::mpsc::Sender<SinkControl>,
    running: Option<tokio::task::JoinHandle<Result<()>>>,
    stop: Stop,
}

impl Rig {
    fn new(gate: Gate, indexes: &[&str], writes: usize) -> Self {
        let name = sink_name("primary");
        let names: Vec<IndexName> = indexes.iter().map(|i| index(i)).collect();
        let stream = Arc::new(ChannelStream::over(1024, names.clone(), [name.clone()]));
        let lanes = names
            .iter()
            .map(|index| {
                let producer = stream
                    .lane(&LaneKey::new(index.clone(), name.clone()))
                    .unwrap()
                    .producer;
                (index.to_string(), producer)
            })
            .collect();
        let gate = Arc::new(gate);
        let progress = Arc::new(Progress::of(names.clone(), [name.clone()]));
        let stop = Stop::new();
        let mappings = names
            .iter()
            .map(|index| IndexMapping {
                index: index.clone(),
                hash: ContentHash::new(1),
                fields: Vec::new(),
            })
            .collect();
        let engine = Arc::new(
            SinkEngine::new(
                name,
                Arc::clone(&gate) as Arc<dyn Sink>,
                Arc::clone(&stream) as Arc<dyn Stream>,
                mappings,
                Arc::clone(&progress),
            )
            .with_writes(writes)
            .with_stop(stop.clone()),
        );
        let (control, _) = tokio::sync::mpsc::channel(4);
        Self {
            gate,
            engine,
            stream,
            progress,
            lanes,
            control,
            running: None,
            stop,
        }
    }

    fn start(&mut self) {
        let engine = Arc::clone(&self.engine);
        let (control, mut rx) = tokio::sync::mpsc::channel(4);
        self.control = control;
        self.running = Some(tokio::spawn(async move { engine.run(&mut rx).await }));
    }

    async fn ended(&mut self) -> Result<()> {
        self.running.take().unwrap().await.unwrap()
    }

    fn health(&self, lane: &str) -> crate::LaneHealth {
        self.engine
            .lane_health(&LaneKey::new(index(lane), sink_name("primary")))
    }

    /// What `lane` is parked for, as `/status` and the log show it.
    fn parked_reason(&self, lane: &str) -> Option<String> {
        match self.health(lane) {
            crate::LaneHealth::Parked { reason, .. } => Some(reason.to_string()),
            _ => None,
        }
    }

    fn finished(&self) -> bool {
        self.running.as_ref().is_some_and(|run| run.is_finished())
    }

    async fn publish(&self, lane: &str, item: LaneItem) {
        self.lanes[lane].publish(item).await.unwrap();
    }

    async fn backfill(&self, lane: &str, id: &str) {
        self.publish(lane, batch(lane, id, None)).await;
    }

    async fn live(&self, lane: &str, id: &str, position: u64) {
        let position = Position::new(position);
        self.publish(lane, batch(lane, id, Some(position))).await;
        self.progress.published(
            &LaneKey::new(index(lane), sink_name("primary")),
            stream::ItemKey::new(position.seq),
            position,
            position,
        );
    }

    async fn complete(&self, lane: &str) {
        self.publish(
            lane,
            LaneItem::SnapshotComplete {
                index: index(lane),
                ticket: stream::Ticket::new(1),
            },
        )
        .await;
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            running.abort();
        }
    }
}

fn batch(lane: &str, id: &str, position: Option<Position>) -> LaneItem {
    LaneItem::Batch(Batch {
        key: position.map(|position| stream::ItemKey::new(position.seq)),
        position,
        envelopes: vec![Envelope::upsert(
            index(lane),
            id,
            GenericValue::Map(BTreeMap::new()),
            position,
            chrono::Utc::now(),
        )]
        .into(),
        hold: Default::default(),
        walked: None,
        ticket: None,
        ..Default::default()
    })
}

/// Wait (in paused time) until `condition` holds, failing after a minute.
async fn until(what: &str, mut condition: impl FnMut() -> bool) {
    for _ in 0..60_000 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("timed out waiting for {what}");
}

/// Let the engine run what it can without the clock moving.
async fn settle() {
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
}

/// A's seed is held; B's seed starts and finishes meanwhile.
#[tokio::test(start_paused = true)]
async fn two_lanes_seed_at_once() {
    let mut rig = Rig::new(Gate::default().holding(&["seed a"]), &["a", "b"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a's seed", || rig.gate.logged("start seed a")).await;
    rig.backfill("b", "b1").await;
    rig.complete("b").await;
    until("b's seed to finish", || rig.gate.logged("end seed b")).await;
    assert!(!rig.gate.logged("end seed a"));

    rig.gate.release("seed a");
    until("a's seed to finish", || rig.gate.seeded("a") == 1).await;
    assert_eq!(rig.gate.seeded("b"), 1);
    assert_eq!(rig.gate.seeds_most.load(Ordering::SeqCst), 2);
}

/// A write on another lane lands while A seeds.
#[tokio::test(start_paused = true)]
async fn a_write_on_another_lane_lands_while_a_lane_seeds() {
    let mut rig = Rig::new(Gate::default().holding(&["seed a"]), &["a", "c"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a's seed", || rig.gate.logged("start seed a")).await;
    rig.live("c", "c1", 1).await;
    until("c's write", || rig.gate.logged("end write c")).await;
    assert!(!rig.gate.logged("end seed a"));
    rig.gate.release("seed a");
    until("a's seed to finish", || rig.gate.seeded("a") == 1).await;
}

/// A's own item after its `SnapshotComplete` waits for A's seed.
#[tokio::test(start_paused = true)]
async fn a_lanes_later_item_waits_for_its_seed() {
    let mut rig = Rig::new(Gate::default().holding(&["seed a"]), &["a"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    rig.live("a", "a2", 1).await;
    until("a's seed", || rig.gate.logged("start seed a")).await;
    settle().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        rig.gate.log(),
        ["start write a", "end write a", "start seed a"],
        "a2 waits for the seed"
    );
    rig.gate.release("seed a");
    until("a2", || rig.gate.log().len() == 6).await;
    assert!(rig.gate.at("start write a") < rig.gate.at("start seed a"));
    let log = rig.gate.log();
    assert_eq!(log.get(3).map(String::as_str), Some("end seed a"));
    assert_eq!(log.get(4).map(String::as_str), Some("start write a"));
}

/// A's seed fails while B's is held: the run ends only once B's seed
/// returned, B is seeded, and the restarted run seeds A once.
#[tokio::test(start_paused = true)]
async fn a_failed_seed_waits_for_the_others_and_seeds_once_on_redelivery() {
    let gate = Gate::default()
        .holding(&["seed b"])
        .failing_once(&["seed a"]);
    let mut rig = Rig::new(gate, &["a", "b"], 4);
    rig.start();
    rig.backfill("b", "b1").await;
    rig.complete("b").await;
    until("b's seed", || rig.gate.logged("start seed b")).await;
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a's seed to fail", || rig.gate.logged("failed seed a")).await;
    settle().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!rig.finished(), "the run waits for b's seed");

    rig.gate.release("seed b");
    assert!(rig.ended().await.is_err(), "the failed seed ends the run");
    assert_eq!(rig.gate.seeded("b"), 1);
    assert_eq!(rig.gate.seeded("a"), 0);

    rig.start();
    until("the lanes to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.gate.seeded("a"), 1);
    assert_eq!(rig.gate.seeded("b"), 1);
}

/// A stop lets the seed in flight finish, and leaves unseeded an index whose
/// backfill it cut short.
#[tokio::test(start_paused = true)]
async fn a_stop_lets_a_seed_in_flight_finish_and_skips_a_cut_backfill() {
    let gate = Gate::default().holding(&["seed a", "write c"]);
    let mut rig = Rig::new(gate, &["a", "b", "c"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a's seed", || rig.gate.logged("start seed a")).await;
    rig.live("c", "c1", 1).await;
    until("c's write", || rig.gate.logged("start write c")).await;

    rig.stop.request();
    rig.backfill("b", "b1").await;
    rig.complete("b").await;
    settle().await;
    rig.gate.release("seed a");
    rig.gate.release("write c");
    until("the lanes to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.gate.seeded("a"), 1, "the seed in flight finished");
    assert_eq!(rig.gate.seeded("b"), 0, "b's backfill was cut short");
    assert!(!rig.gate.logged("start write b"));
}

/// A reindex arriving while a seed runs is staged once it returned.
#[tokio::test(start_paused = true)]
async fn a_reindex_waits_for_a_seed_in_flight() {
    let mut rig = Rig::new(Gate::default().holding(&["seed a"]), &["a", "b"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a's seed", || rig.gate.logged("start seed a")).await;
    rig.control
        .send(SinkControl::Reindex {
            indexes: vec![index("b")],
        })
        .await
        .unwrap();
    settle().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!rig.gate.logged("reindex b"));

    rig.gate.release("seed a");
    until("the reindex", || rig.gate.logged("reindex b")).await;
    assert!(rig.gate.at("reindex b") > rig.gate.at("end seed a"));
}

/// A seed holds a write slot: with one slot, B's write waits for A's seed.
#[tokio::test(start_paused = true)]
async fn a_seed_holds_a_write_slot() {
    let mut rig = Rig::new(Gate::default().holding(&["seed a"]), &["a", "b"], 1);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a's seed", || rig.gate.logged("start seed a")).await;
    rig.live("b", "b1", 1).await;
    settle().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!rig.gate.logged("start write b"));
    rig.gate.release("seed a");
    until("b's write", || rig.gate.logged("end write b")).await;
}

/// An ordered sink seeds one index at a time, never beside a write.
#[tokio::test(start_paused = true)]
async fn an_ordered_sink_seeds_one_index_at_a_time() {
    let gate = Gate {
        ordered: true,
        ..Gate::default()
    };
    let mut rig = Rig::new(gate, &["a", "b", "c"], 4);
    rig.start();
    for lane in ["a", "b", "c"] {
        rig.backfill(lane, &format!("{lane}1")).await;
        rig.complete(lane).await;
        rig.live(lane, &format!("{lane}2"), 1).await;
    }
    until("the lanes to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.gate.most.load(Ordering::SeqCst), 1);
    for lane in ["a", "b", "c"] {
        assert_eq!(rig.gate.seeded(lane), 1);
    }
}

fn seeds() -> Vec<u64> {
    match std::env::var("FLUSSO_SCHEDULE_SEED") {
        Ok(seed) => vec![seed.parse().expect("FLUSSO_SCHEDULE_SEED is a number")],
        Err(_) => (1..=100).collect(),
    }
}

/// Snapshots on several lanes, live items before and after them, every write
/// and seed taking a random time: on every lane the backfill lands before
/// its seed starts, nothing after the `SnapshotComplete` starts before the
/// seed ended, each index is seeded once, and seeds overlap somewhere.
#[test]
fn random_schedules_seed_each_lane_once_in_its_lane_order() {
    let mut overlapped = false;
    for seed in seeds() {
        let runtime = super::paused_runtime();
        overlapped |= runtime.block_on(one_schedule(seed));
    }
    assert!(overlapped, "no schedule seeded two indexes at once");
}

async fn one_schedule(seed: u64) -> bool {
    let lanes = ["a", "b", "c", "d"];
    let gate = Gate {
        delays: Some(Mutex::new(Rng(seed.max(1)))),
        ..Gate::default()
    };
    let mut rig = Rig::new(gate, &lanes, 4);
    rig.start();
    let mut rng = Rng(seed.wrapping_mul(7919).max(1));
    let mut position = 0;
    for lane in lanes {
        for page in 0..=rng.below(3) {
            rig.backfill(lane, &format!("{lane}b{page}")).await;
        }
        position += 1;
        rig.live(lane, &format!("{lane}l{position}"), position)
            .await;
        rig.complete(lane).await;
        for after in 0..=rng.below(2) {
            position += 1;
            rig.live(lane, &format!("{lane}a{after}"), position).await;
        }
    }
    until("the lanes to drain", || rig.stream.is_idle()).await;
    let log = rig.gate.log();
    for lane in lanes {
        assert_eq!(rig.gate.seeded(lane), 1, "seed {seed}: {lane} seeded once");
        let seed_start = rig.gate.at(&format!("start seed {lane}"));
        let seed_end = rig.gate.at(&format!("end seed {lane}"));
        let writes: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(_, l)| *l == &format!("start write {lane}"))
            .map(|(at, _)| at)
            .collect();
        let ends: Vec<usize> = log
            .iter()
            .enumerate()
            .filter(|(_, l)| *l == &format!("end write {lane}"))
            .map(|(at, _)| at)
            .collect();
        assert_eq!(
            writes.iter().filter(|at| **at < seed_start).count(),
            ends.iter().filter(|at| **at < seed_start).count(),
            "seed {seed}: every write of {lane} before its seed landed first: {log:?}"
        );
        assert!(
            writes.iter().all(|at| *at < seed_start || *at > seed_end),
            "seed {seed}: no write of {lane} starts while it seeds: {log:?}"
        );
        assert!(
            ends.iter()
                .filter(|at| **at > seed_start)
                .all(|at| *at > seed_end),
            "seed {seed}: no write of {lane} is in flight while it seeds: {log:?}"
        );
    }
    rig.gate.seeds_most.load(Ordering::SeqCst) >= 2
}

/// #479: a seed the sink refuses because the generation it would retire is
/// blocked parks its lane `blocked`. The reason names the new generation as
/// built, the old one and its block, and both ways out. The run goes on:
/// another lane lands. The `SnapshotComplete` stays unacknowledged. Once the
/// block is lifted, the lane's probe seeds the index and acknowledges it.
#[tokio::test(start_paused = true)]
async fn a_seed_refused_by_a_blocked_old_generation_parks_its_lane_until_a_probe_seeds_it() {
    let mut rig = Rig::new(Gate::default().retire_blocked(&["a"]), &["a", "c"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a parked blocked", || rig.health("a").kind() == "blocked").await;
    let reason = rig.parked_reason("a").unwrap();
    for named in [
        "blocked: ",
        "a_1_2 is built",
        "a_1_1 can't be retired",
        "read-only-allow-delete",
        "the block clears when the disk has room again",
        "lift the block on a_1_1",
        "delete a_1_1",
        "takes reads of this index offline until the seed's next probe",
        "finishes on its own",
    ] {
        assert!(reason.contains(named), "{named} in {reason}");
    }

    rig.live("c", "c1", 1).await;
    until("c's write", || rig.gate.logged("end write c")).await;
    settle().await;
    assert!(!rig.finished(), "the run goes on");
    assert_eq!(rig.gate.seeded("a"), 0);
    assert!(!rig.stream.is_idle(), "the completion is not acknowledged");

    rig.gate.lift("a");
    until("a's probe seeds it", || rig.gate.seeded("a") == 1).await;
    until("a opens", || rig.health("a").is_open()).await;
    until("everything acknowledged", || rig.stream.is_idle()).await;
    assert!(!rig.finished());
}

/// A stop while a lane is parked on a refused seed: its completion is
/// acknowledged unseeded, so it doesn't hold the drain, and the index stays
/// unseeded for the next start to finish.
#[tokio::test(start_paused = true)]
async fn a_stop_while_parked_on_a_refused_seed_leaves_the_index_unseeded() {
    let mut rig = Rig::new(Gate::default().retire_blocked(&["a"]), &["a"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a parked blocked", || rig.health("a").kind() == "blocked").await;
    rig.stop.request();
    until("the lanes to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.gate.seeded("a"), 0);
    assert!(
        rig.gate.seed_tries("a") >= 2,
        "the stop probes the parked seed once more"
    );
}

/// A reindex of the index while its lane is parked on a refused seed stages
/// a new generation; the parked completion is acknowledged without seeding
/// and its seed is not tried again.
#[tokio::test(start_paused = true)]
async fn a_reindex_while_parked_on_a_refused_seed_drops_that_seed() {
    let mut rig = Rig::new(Gate::default().retire_blocked(&["a"]), &["a"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    // A completion asked for before the reindex, whose own request then
    // carries a later ticket.
    rig.publish(
        "a",
        LaneItem::SnapshotComplete {
            index: index("a"),
            ticket: stream::Ticket::default(),
        },
    )
    .await;
    until("a parked blocked", || rig.health("a").kind() == "blocked").await;
    let tries = rig.gate.seed_tries("a");
    rig.control
        .send(SinkControl::Reindex {
            indexes: vec![index("a")],
        })
        .await
        .unwrap();
    until("the reindex", || rig.gate.logged("reindex a")).await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    settle().await;
    assert_eq!(
        rig.gate.seed_tries("a"),
        tries,
        "the refused seed is dropped"
    );
    assert_eq!(rig.gate.seeded("a"), 0);
    assert!(!rig.finished());

    // The reindex's own backfill: its completion seeds the index once the
    // block is lifted, and the lane opens.
    rig.gate.lift("a");
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("the new backfill seeds a", || rig.gate.seeded("a") == 1).await;
    until("a opens", || rig.health("a").is_open()).await;
    until("everything acknowledged", || rig.stream.is_idle()).await;
}

/// The probes of a refused seed are spaced by the lane's backoff, which
/// doubles after each refusal (stage 9's parking rules): 1 s, then 2 s, 4 s.
#[tokio::test(start_paused = true)]
async fn the_probes_of_a_refused_seed_back_off_doubling() {
    let mut rig = Rig::new(Gate::default().retire_blocked(&["a"]), &["a"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("four tries", || rig.gate.seed_tries("a") >= 4).await;
    let at = rig.gate.refused_at.lock().unwrap()["a"].clone();
    let gaps: Vec<Duration> = at.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert_eq!(
        gaps[..3],
        [
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4)
        ],
        "probes at {at:?}"
    );
}

/// A run that ends for another reason while a lane is parked on a refused
/// seed: the restarted run is redelivered the completion and parks on it
/// again, without the run failing.
#[tokio::test(start_paused = true)]
async fn a_restarted_run_parks_on_a_refused_seed_again() {
    let gate = Gate::default()
        .retire_blocked(&["a"])
        .failing_once(&["write c"]);
    let mut rig = Rig::new(gate, &["a", "c"], 4);
    rig.start();
    rig.backfill("a", "a1").await;
    rig.complete("a").await;
    until("a parked blocked", || rig.health("a").kind() == "blocked").await;
    rig.live("c", "c1", 1).await;
    until("the run fails on c", || rig.finished()).await;
    assert!(rig.ended().await.is_err());
    let tries = rig.gate.seed_tries("a");

    rig.start();
    until("c lands", || {
        rig.gate
            .log()
            .iter()
            .filter(|l| *l == "end write c")
            .count()
            == 1
    })
    .await;
    until("the seed tried again", || rig.gate.seed_tries("a") > tries).await;
    until("a parked blocked", || rig.health("a").kind() == "blocked").await;
    settle().await;
    assert!(!rig.finished(), "a refused seed fails no run");
}
