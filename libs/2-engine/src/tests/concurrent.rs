//! A sink engine writes several lane items at once (ADR 0012 stage 2), and
//! keeps every guarantee it had writing one at a time:
//!
//! - **per document**: never two writes of one document at once, and the
//!   versions written never go down (a later build lands after an earlier one);
//! - **per lane**: confirmation and the fence never pass an earlier item that
//!   is still being written, whatever finishes first;
//! - **selection**: live items before backfill, except every fourth contested
//!   grant; `SnapshotComplete` and a reindex are barriers;
//! - **failure and stop**: the writes in flight finish before the run ends.
//!
//! Each test drives one engine over a real channel lane, publishing items by
//! hand, on paused time. The randomized test replays with
//! `FLUSSO_SCHEDULE_SEED=<seed>`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, Position, SinkName};
use sink::{Sink, Visibility, WriteReport};
use stream::{Batch, LaneItem, Producer, Stream};
use stream_channel::ChannelStream;
use tokio::sync::Notify;

use super::{PositionObserver, mapping, sink_name, users};
use crate::{
    FailurePolicies, FailurePolicy, Observer, Progress, Result, SinkControl, SinkEngine, Stop,
};

/// A small deterministic generator, so a schedule replays from its seed.
#[derive(Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A sink that records everything a test asks about: each write's start and
/// end, two writes of one document at once, versions going down, how many
/// writes ran at once. A write can be held until released, delayed at random,
/// or failed once.
#[derive(Debug)]
struct Probe {
    log: Mutex<Vec<String>>,
    writing: Mutex<HashSet<String>>,
    overlapped: AtomicBool,
    versions: Mutex<HashMap<String, i64>>,
    regressed: AtomicBool,
    now: AtomicUsize,
    most: AtomicUsize,
    held: Mutex<HashSet<String>>,
    released: Notify,
    fail_once: Mutex<HashSet<String>>,
    /// Up to this many writes fail at random (one in eight), from the delays'
    /// generator, so a seed replays them.
    random_failures: AtomicUsize,
    rejects: Mutex<HashSet<String>>,
    delays: Option<Mutex<Rng>>,
    caught_up: Mutex<Vec<bool>>,
    visibility: fn(bool) -> Visibility,
    ordered: bool,
}

impl Probe {
    fn new() -> Self {
        Self {
            log: Mutex::default(),
            writing: Mutex::default(),
            overlapped: AtomicBool::new(false),
            versions: Mutex::default(),
            regressed: AtomicBool::new(false),
            now: AtomicUsize::new(0),
            most: AtomicUsize::new(0),
            held: Mutex::default(),
            released: Notify::new(),
            fail_once: Mutex::default(),
            random_failures: AtomicUsize::new(0),
            rejects: Mutex::default(),
            delays: None,
            caught_up: Mutex::default(),
            visibility: |_| Visibility::Immediate,
            ordered: false,
        }
    }

    fn with_delays(mut self, seed: u64) -> Self {
        self.delays = Some(Mutex::new(Rng(seed.max(1))));
        self
    }

    fn holding(self, ids: &[&str]) -> Self {
        self.held
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

    fn failing_at_random(self, writes: usize) -> Self {
        self.random_failures.store(writes, Ordering::SeqCst);
        self
    }

    /// Reject these documents item by item, on every write: the engine's
    /// failure policy decides them.
    fn rejecting(self, ids: &[&str]) -> Self {
        self.rejects
            .lock()
            .unwrap()
            .extend(ids.iter().map(|id| (*id).to_owned()));
        self
    }

    /// A sink whose readers consume its output in order.
    fn ordered(mut self) -> Self {
        self.ordered = true;
        self
    }

    fn visible(mut self, visibility: fn(bool) -> Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    fn release(&self, id: &str) {
        self.held.lock().unwrap().remove(id);
        self.released.notify_waiters();
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    /// The order writes started in, by their documents.
    fn starts(&self) -> Vec<String> {
        self.log()
            .into_iter()
            .filter_map(|line| line.strip_prefix("start ").map(str::to_owned))
            .collect()
    }

    fn most_at_once(&self) -> usize {
        self.most.load(Ordering::SeqCst)
    }

    async fn wait_while_held(&self, ids: &[String]) {
        loop {
            let released = self.released.notified();
            let held = {
                let held = self.held.lock().unwrap();
                ids.iter().any(|id| held.contains(id))
            };
            if !held {
                return;
            }
            released.await;
        }
    }
}

#[async_trait]
impl Sink for Probe {
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        let ids: Vec<String> = envelopes.iter().map(|e| e.id.clone()).collect();
        let label = ids.join(",");
        {
            let mut writing = self.writing.lock().unwrap();
            for id in &ids {
                if !writing.insert(id.clone()) {
                    self.overlapped.store(true, Ordering::SeqCst);
                }
            }
        }
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        self.caught_up.lock().unwrap().push(caught_up);
        self.log.lock().unwrap().push(format!("start {label}"));

        self.wait_while_held(&ids).await;
        let delay = self.delays.as_ref().map(|rng| rng.lock().unwrap().below(8));
        if let Some(delay) = delay {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let failed = {
            let mut fail_once = self.fail_once.lock().unwrap();
            ids.iter().any(|id| fail_once.remove(id))
        } || (self.random_failures.load(Ordering::SeqCst) > 0
            && self
                .delays
                .as_ref()
                .is_some_and(|rng| rng.lock().unwrap().below(8) == 0)
            && self
                .random_failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok());

        if !failed {
            let mut versions = self.versions.lock().unwrap();
            for envelope in envelopes {
                let version = match &envelope.document {
                    Some(GenericValue::Map(fields)) => match fields.get("v") {
                        Some(GenericValue::BigInt(v)) => *v,
                        _ => 0,
                    },
                    _ => 0,
                };
                let last = versions.entry(envelope.id.clone()).or_insert(version);
                if version < *last {
                    self.regressed.store(true, Ordering::SeqCst);
                }
                *last = version;
            }
        }
        {
            let mut writing = self.writing.lock().unwrap();
            for id in &ids {
                writing.remove(id);
            }
        }
        self.now.fetch_sub(1, Ordering::SeqCst);
        self.log
            .lock()
            .unwrap()
            .push(format!("{} {label}", if failed { "failed" } else { "end" }));
        if failed {
            return Err(sink::SinkError::Write(format!("{label} failed")));
        }
        let rejected: Vec<sink::RejectedDocument> = {
            let rejects = self.rejects.lock().unwrap();
            envelopes
                .iter()
                .filter(|e| rejects.contains(&e.id))
                .map(|e| sink::RejectedDocument {
                    index: e.index.to_string(),
                    id: e.id.clone(),
                    reason: "simulated rejection".to_owned(),
                })
                .collect()
        };
        Ok(WriteReport::clean()
            .rejecting(rejected)
            .visible((self.visibility)(caught_up)))
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.log.lock().unwrap().push(format!("seeded {index}"));
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

/// One sink engine over a channel lane, its observer, and the lane's
/// producing end, so a test publishes items by hand.
struct Rig {
    probe: Arc<Probe>,
    /// One engine for the whole test: a restart runs it again, as the daemon
    /// does, so what it keeps across runs (fences, a failed run) carries over.
    sink_engine: Arc<SinkEngine>,
    stream: Arc<ChannelStream>,
    progress: Arc<Progress>,
    name: SinkName,
    observer: Arc<PositionObserver>,
    lane: Box<dyn Producer<LaneItem>>,
    control: tokio::sync::mpsc::Sender<SinkControl>,
    engine: Option<tokio::task::JoinHandle<Result<()>>>,
    stop: Stop,
    version: i64,
}

impl Rig {
    fn new(probe: Probe, writes: usize) -> Self {
        Self::with_policies(probe, writes, FailurePolicies::default())
    }

    fn with_policies(probe: Probe, writes: usize, policies: FailurePolicies) -> Self {
        let name = sink_name("primary");
        let stream = Arc::new(ChannelStream::new(1024, [name.clone()]));
        let lane = stream.lane(&name).unwrap().producer;
        let (control, _) = tokio::sync::mpsc::channel(4);
        let probe = Arc::new(probe);
        let observer = Arc::new(PositionObserver::default());
        let stop = Stop::new();
        let progress = Arc::new(Progress::new([name.clone()]));
        let sink_engine = Arc::new(
            SinkEngine::new(
                name.clone(),
                Arc::clone(&probe) as Arc<dyn Sink>,
                Arc::clone(&stream) as Arc<dyn Stream>,
                vec![mapping()],
                Arc::clone(&progress),
            )
            .with_writes(writes)
            .with_failure_policies(policies)
            .with_stop(stop.clone())
            .with_observer(Arc::clone(&observer) as Arc<dyn Observer>),
        );
        Self {
            probe,
            sink_engine,
            stream,
            progress,
            name,
            observer,
            lane,
            control,
            engine: None,
            stop,
            version: 0,
        }
    }

    fn start(mut self) -> Self {
        let engine = Arc::clone(&self.sink_engine);
        let (control, mut rx) = tokio::sync::mpsc::channel(4);
        self.control = control;
        self.engine = Some(tokio::spawn(async move { engine.run(&mut rx).await }));
        self
    }

    /// Start, and land one write at position 0 before the test's own items:
    /// a fresh engine writes its first item alone (a previous process may
    /// have stopped on it), so a test of several writes at once starts after
    /// one has landed. The warm-up leaves no trace in the logs.
    async fn warm(self) -> Self {
        let mut rig = self.start();
        rig.live(0, &["warm"]).await;
        until("the warm-up write", || {
            super::covered(&rig.progress) == Some(Position::new(0))
        })
        .await;
        rig.probe.log.lock().unwrap().clear();
        rig.observer.forget();
        rig
    }

    /// The outcome of the running engine, once it ends.
    async fn ended(&mut self) -> Result<()> {
        self.engine.take().unwrap().await.unwrap()
    }

    fn envelopes(&mut self, ids: &[&str], position: Option<Position>) -> Arc<[Envelope]> {
        self.version += 1;
        let version = self.version;
        ids.iter()
            .map(|id| {
                Envelope::upsert(
                    users(),
                    *id,
                    GenericValue::Map(BTreeMap::from([(
                        "v".to_owned(),
                        GenericValue::BigInt(version),
                    )])),
                    position,
                    chrono::Utc::now(),
                )
            })
            .collect::<Vec<_>>()
            .into()
    }

    /// Publish a live item, then register it as the ingest engine does.
    async fn live(&mut self, position: u64, ids: &[&str]) {
        let position = Position::new(position);
        let envelopes = self.envelopes(ids, Some(position));
        self.lane
            .publish(LaneItem::Batch(Batch {
                position: Some(position),
                changes: 1,
                envelopes,
            }))
            .await
            .unwrap();
        self.progress.published(&self.name, position, position);
    }

    async fn backfill(&mut self, ids: &[&str]) {
        let envelopes = self.envelopes(ids, None);
        self.lane
            .publish(LaneItem::Batch(Batch {
                position: None,
                changes: 0,
                envelopes,
            }))
            .await
            .unwrap();
    }

    async fn complete(&self) {
        self.lane
            .publish(LaneItem::SnapshotComplete {
                indexes: vec![users()],
            })
            .await
            .unwrap();
    }

    fn applied(&self) -> Vec<String> {
        self.observer
            .events()
            .into_iter()
            .filter(|e| e.starts_with("applied"))
            .collect()
    }

    fn searchable(&self) -> Vec<String> {
        self.observer
            .events()
            .into_iter()
            .filter(|e| e.starts_with("searchable"))
            .collect()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(engine) = self.engine.take() {
            engine.abort();
        }
    }
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

fn seeds() -> Vec<u64> {
    match std::env::var("FLUSSO_SCHEDULE_SEED") {
        Ok(seed) => vec![seed.parse().expect("FLUSSO_SCHEDULE_SEED is a number")],
        Err(_) => (1..=40).collect(),
    }
}

/// Random lane traffic over a few documents: live batches and backfill pages
/// in any mix, each holding a random set of ids, every write taking a random
/// time, and up to three writes failing at random: each failure ends the run
/// and the engine restarts, as the daemon restarts it, so an earlier item
/// fails while later ones sharing its documents wait. Builds are numbered in
/// lane order (the version), as the one ingest task builds them. Whatever the
/// schedule, no document is written twice at once, its versions never go
/// down, every document ends at its last version, and the lane drains with
/// the confirmation at the last position.
#[tokio::test(start_paused = true)]
async fn a_document_is_never_written_twice_at_once_and_its_versions_never_go_down() {
    const IDS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
    let mut concurrent = false;
    let mut restarted = false;
    for writes in [1, 2, 4] {
        for seed in seeds() {
            let mut rng = Rng(seed * 7919 + writes as u64);
            let mut rig =
                Rig::new(Probe::new().with_delays(seed).failing_at_random(3), writes).start();
            let mut last: HashMap<&str, i64> = HashMap::new();
            let mut position = 0;
            for _ in 0..60 {
                let mut ids: Vec<&str> =
                    IDS.iter().copied().filter(|_| rng.below(3) == 0).collect();
                if ids.is_empty() {
                    ids.push(IDS[rng.below(IDS.len() as u64) as usize]);
                }
                if rng.below(3) == 0 {
                    rig.backfill(&ids).await;
                } else {
                    position += 1;
                    rig.live(position, &ids).await;
                }
                for id in &ids {
                    last.insert(id, rig.version);
                }
                if rng.below(4) == 0 {
                    tokio::time::sleep(Duration::from_millis(rng.below(5))).await;
                }
            }
            for _ in 0..60_000 {
                if rig.stream.is_idle() {
                    break;
                }
                if rig.engine.as_ref().is_some_and(|run| run.is_finished()) {
                    assert!(rig.ended().await.is_err(), "only a failure ends the run");
                    restarted = true;
                    rig = rig.start();
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            assert!(
                rig.stream.is_idle(),
                "seed {seed}, writes {writes}: the lane drains"
            );

            let probe = &rig.probe;
            assert!(
                !probe.overlapped.load(Ordering::SeqCst),
                "seed {seed}, writes {writes}: a document was written twice at once"
            );
            assert!(
                !probe.regressed.load(Ordering::SeqCst),
                "seed {seed}, writes {writes}: a document's version went down"
            );
            let written = probe.versions.lock().unwrap().clone();
            for (id, version) in &last {
                assert_eq!(
                    written.get(*id),
                    Some(version),
                    "seed {seed}, writes {writes}: {id} ends at its last version"
                );
            }
            assert_eq!(super::covered(&rig.progress), Some(Position::new(position)));
            assert!(probe.most_at_once() <= writes);
            concurrent |= probe.most_at_once() > 1;
        }
    }
    assert!(concurrent, "some schedule wrote several items at once");
    assert!(restarted, "some schedule failed a write and restarted");
}

#[tokio::test(start_paused = true)]
async fn writes_run_at_once_up_to_the_setting() {
    let rig_probe = Probe::new().holding(&["a", "b", "c", "d"]);
    let mut rig = Rig::new(rig_probe, 3).warm().await;
    for (position, id) in ["a", "b", "c", "d"].into_iter().enumerate() {
        rig.live(position as u64 + 1, &[id]).await;
    }
    settle().await;
    assert_eq!(
        rig.probe.starts(),
        ["a", "b", "c"],
        "three at once, no more"
    );

    rig.probe.release("b");
    settle().await;
    assert_eq!(
        rig.probe.starts(),
        ["a", "b", "c", "d"],
        "a freed slot is taken"
    );
    assert_eq!(rig.probe.most_at_once(), 3);
}

/// An item holding a document an earlier, unfinished item holds waits for
/// it, while items holding other documents go ahead.
#[tokio::test(start_paused = true)]
async fn an_item_waits_for_an_earlier_one_sharing_a_document() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]), 4).warm().await;
    rig.live(1, &["a", "b"]).await;
    rig.live(2, &["a"]).await;
    rig.live(3, &["c"]).await;
    settle().await;
    assert_eq!(rig.probe.starts(), ["a,b", "c"]);

    rig.probe.release("a");
    until("the second write of a", || rig.probe.starts().len() == 3).await;
    assert_eq!(rig.probe.starts(), ["a,b", "c", "a"]);
}

/// Item 3 and 2 finish while item 1 is still being written: the lane's
/// confirmation and the sink's applied fence stay before 1, then move to 3 once
/// it lands.
#[tokio::test(start_paused = true)]
async fn a_later_write_finishing_first_moves_neither_confirmation_nor_the_fence() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]), 4).warm().await;
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    rig.live(3, &["c"]).await;
    until("2 and 3 to be written", || {
        rig.probe
            .log()
            .iter()
            .filter(|l| l.starts_with("end"))
            .count()
            == 2
    })
    .await;
    settle().await;
    assert_eq!(
        super::covered(&rig.progress),
        Some(Position::new(0)),
        "1 is still being written"
    );
    assert!(rig.applied().is_empty(), "{:?}", rig.applied());

    rig.probe.release("a");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(super::covered(&rig.progress), Some(Position::new(3)));
    assert_eq!(rig.applied(), ["applied primary 3"]);
}

/// With writes = 1, live items go before backfill items queued ahead of them,
/// except every fourth grant both could take, which goes to the oldest
/// backfill item. Once one kind runs out the other takes every grant. Held
/// gate writes fill every slot while the items queue, so the engine sees them
/// all before it grants any.
#[tokio::test(start_paused = true)]
async fn live_goes_first_except_every_fourth_contested_grant() {
    for writes in [1, 2] {
        let gates: Vec<String> = (0..writes).map(|n| format!("gate{n}")).collect();
        let gates: Vec<&str> = gates.iter().map(String::as_str).collect();
        let mut rig = Rig::new(Probe::new().holding(&gates), writes).warm().await;
        for (n, gate) in gates.iter().enumerate() {
            rig.live(n as u64 + 1, &[gate]).await;
        }
        until("the gates to be written", || {
            rig.probe.starts().len() == writes
        })
        .await;
        for page in 0..8 {
            rig.backfill(&[&format!("b{page}")]).await;
        }
        for position in 0..8 {
            rig.live(position + 10, &[&format!("l{position}")]).await;
        }
        settle().await;
        for gate in &gates {
            rig.probe.release(gate);
        }
        until("the lane to drain", || rig.stream.is_idle()).await;
        let kinds: String = rig
            .probe
            .starts()
            .iter()
            .filter(|ids| !ids.starts_with("gate"))
            .map(|ids| if ids.starts_with('b') { 'B' } else { 'L' })
            .collect();
        assert_eq!(kinds, "LLLBLLLBLLBBBBBB", "writes = {writes}");
    }
}

/// `SnapshotComplete` starts only once everything before it is written, and
/// nothing after it starts before it is done, live items included.
#[tokio::test(start_paused = true)]
async fn a_snapshot_complete_is_a_barrier() {
    let mut rig = Rig::new(Probe::new().holding(&["b1"]), 4).start();
    rig.backfill(&["b1"]).await;
    rig.backfill(&["b2"]).await;
    rig.complete().await;
    rig.live(1, &["l1"]).await;
    settle().await;
    assert!(!rig.probe.log().contains(&"seeded users".to_owned()));
    assert!(!rig.probe.starts().contains(&"l1".to_owned()));

    rig.probe.release("b1");
    until("the lane to drain", || rig.stream.is_idle()).await;
    let log = rig.probe.log();
    let at = |line: &str| log.iter().position(|l| l == line).unwrap();
    assert!(at("seeded users") > at("end b1"));
    assert!(at("seeded users") > at("end b2"));
    assert!(at("start l1") > at("seeded users"));
}

/// A reindex arriving while writes are in flight is staged only once they
/// have returned.
#[tokio::test(start_paused = true)]
async fn a_reindex_waits_for_the_writes_in_flight() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]), 4).start();
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    settle().await;
    rig.control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    settle().await;
    assert!(!rig.probe.log().contains(&"reindex users".to_owned()));

    rig.probe.release("a");
    until("the reindex", || {
        rig.probe.log().contains(&"reindex users".to_owned())
    })
    .await;
    let log = rig.probe.log();
    let at = |line: &str| log.iter().position(|l| l == line).unwrap();
    assert!(at("reindex users") > at("end a"));
}

/// Item 1's write fails while 2 and 3 are being written: they finish and are
/// acknowledged, the run ends with 1's error, and the restarted engine
/// rewrites 1 alone.
#[tokio::test(start_paused = true)]
async fn a_failed_write_lets_the_others_finish_and_comes_back_alone() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]).failing_once(&["a"]), 4)
        .warm()
        .await;
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    rig.live(3, &["c"]).await;
    until("2 and 3 to be written", || {
        rig.probe
            .log()
            .iter()
            .filter(|l| l.starts_with("end"))
            .count()
            == 2
    })
    .await;
    rig.probe.release("a");
    assert!(rig.ended().await.is_err(), "the failed write ends the run");
    assert_eq!(
        super::covered(&rig.progress),
        Some(Position::new(0)),
        "1 is not written"
    );

    let rig = rig.start();
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.probe.starts(), ["a", "b", "c", "a"]);
    assert_eq!(super::covered(&rig.progress), Some(Position::new(3)));
}

/// After a run that ended in a failed write, the restarted engine writes the
/// oldest redelivered item alone, as a barrier: a write that fails every time
/// (a `stop` rejection, a destination refusing it) then halts the sink, as
/// with one write at a time, instead of letting later items land beside it on
/// every restart. Once that item lands, writes run at once again.
#[tokio::test(start_paused = true)]
async fn after_a_failed_run_the_oldest_item_is_written_alone_until_it_lands() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]).failing_once(&["a"]), 4)
        .warm()
        .await;
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    until("b to be written", || {
        rig.probe.log().iter().any(|l| l == "end b")
    })
    .await;
    rig.probe.release("a");
    assert!(rig.ended().await.is_err(), "the failed write ends the run");

    rig.probe
        .held
        .lock()
        .unwrap()
        .extend(["a".to_owned(), "c".to_owned()]);
    let mut rig = rig.start();
    rig.live(3, &["c"]).await;
    rig.live(4, &["d"]).await;
    settle().await;
    assert_eq!(
        rig.probe.starts(),
        ["a", "b", "a"],
        "nothing starts beside the redelivered item until it lands"
    );

    rig.probe.release("a");
    until("d to be written", || {
        rig.probe.log().iter().any(|l| l == "end d")
    })
    .await;
    assert_eq!(rig.probe.starts(), ["a", "b", "a", "c", "d"]);
    assert!(
        !rig.probe.log().contains(&"end c".to_owned()),
        "once the barrier landed, d was written while c was still held"
    );
    rig.probe.release("c");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(super::covered(&rig.progress), Some(Position::new(4)));
}

/// A stop lets the live writes in flight land and skips the backfill still
/// waiting: its index isn't marked seeded.
#[tokio::test(start_paused = true)]
async fn a_stop_finishes_the_writes_in_flight_and_skips_waiting_backfill() {
    let mut rig = Rig::new(Probe::new().holding(&["a", "b"]), 2).warm().await;
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    rig.backfill(&["p"]).await;
    rig.complete().await;
    settle().await;
    assert_eq!(rig.probe.starts(), ["a", "b"]);

    rig.stop.request();
    rig.probe.release("a");
    rig.probe.release("b");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(
        rig.probe.starts(),
        ["a", "b"],
        "the backfill page is skipped"
    );
    assert!(!rig.probe.log().contains(&"seeded users".to_owned()));
    assert_eq!(super::covered(&rig.progress), Some(Position::new(2)));
}

/// A stop requested while the run waits on its only write, with nothing
/// waiting and no room (the narrow wait, which doesn't watch the stop): the
/// write lands, and backfill arriving meanwhile is acknowledged unwritten,
/// its index left unseeded.
#[tokio::test(start_paused = true)]
async fn a_stop_during_the_only_write_still_skips_the_backfill_behind_it() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]), 1).warm().await;
    rig.live(1, &["a"]).await;
    until("a to be written", || rig.probe.starts() == ["a"]).await;
    rig.stop.request();
    rig.backfill(&["p"]).await;
    rig.complete().await;
    settle().await;
    rig.probe.release("a");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.probe.starts(), ["a"], "the backfill page is skipped");
    assert!(!rig.probe.log().contains(&"seeded users".to_owned()));
    assert_eq!(super::covered(&rig.progress), Some(Position::new(1)));
}

/// Three writes in flight; the last one granted is caught up and returns
/// first, so its refresh can't cover the two that return after it. Once the
/// engine is idle it sends an empty caught-up write, and every position is
/// searchable without waiting out the refresh interval.
#[tokio::test(start_paused = true)]
async fn an_idle_engine_makes_its_last_writes_searchable() {
    let probe = Probe::new().holding(&["a", "b"]).visible(|caught_up| {
        if caught_up {
            Visibility::Immediate
        } else {
            Visibility::Within(Duration::from_secs(10))
        }
    });
    let mut rig = Rig::new(probe, 4).warm().await;
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    rig.live(3, &["c"]).await;
    until("3 to be written", || {
        rig.probe.log().contains(&"end c".to_owned())
    })
    .await;
    rig.probe.release("a");
    rig.probe.release("b");
    until("the lane to drain", || rig.stream.is_idle()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    settle().await;

    assert_eq!(
        rig.searchable().last().map(String::as_str),
        Some("searchable primary 3")
    );
    assert_eq!(
        rig.probe.caught_up.lock().unwrap().last(),
        Some(&true),
        "the idle write is caught up"
    );
}

/// A stream over a channel whose acknowledgement of any item holding
/// document `fail` errs, as a broker might.
#[derive(Debug)]
struct FailingAcks {
    inner: Arc<ChannelStream>,
    fail: &'static str,
}

impl Stream for FailingAcks {
    fn lane(&self, sink: &kernel::SinkName) -> stream::Result<stream::Lane> {
        let lane = self.inner.lane(sink)?;
        Ok(stream::Lane {
            producer: lane.producer,
            consumer: Box::new(FailingConsumer {
                inner: lane.consumer,
                fail: self.fail,
            }),
        })
    }

    fn requests(&self) -> stream::Result<stream::RequestLane> {
        self.inner.requests()
    }

    fn is_idle(&self) -> bool {
        self.inner.is_idle()
    }
}

#[derive(Debug)]
struct FailingConsumer {
    inner: Box<dyn stream::Consumer<LaneItem>>,
    fail: &'static str,
}

#[async_trait]
impl stream::Consumer<LaneItem> for FailingConsumer {
    async fn recv(&mut self) -> stream::Result<Option<stream::Delivery<LaneItem>>> {
        let Some(delivery) = self.inner.recv().await? else {
            return Ok(None);
        };
        let fails = matches!(
            delivery.item(),
            LaneItem::Batch(batch) if batch.envelopes.iter().any(|e| e.id == self.fail)
        );
        if !fails {
            return Ok(Some(delivery));
        }
        let (item, handle) = delivery.into_parts();
        Ok(Some(stream::Delivery::new(
            item,
            Box::new(FailingAck(handle)),
        )))
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[derive(Debug)]
struct FailingAck(Box<dyn stream::AckHandle>);

#[async_trait]
impl stream::AckHandle for FailingAck {
    async fn ack(self: Box<Self>) -> stream::Result<()> {
        Err(stream::QueueError::Consume(
            "simulated acknowledgement failure".to_owned(),
        ))
    }

    async fn nack(self: Box<Self>) -> stream::Result<()> {
        self.0.nack().await
    }
}

/// An acknowledgement that errs ends the run like a failed write: the writes
/// in flight beside it are awaited first, so none is abandoned mid-request
/// for any stream adapter.
#[tokio::test(start_paused = true)]
async fn a_failed_acknowledgement_still_waits_for_the_writes_in_flight() {
    let name = sink_name("primary");
    let channel = Arc::new(ChannelStream::new(1024, [name.clone()]));
    let stream = Arc::new(FailingAcks {
        inner: Arc::clone(&channel),
        fail: "b",
    });
    let probe = Arc::new(Probe::new().holding(&["a"]));
    let engine = SinkEngine::new(
        name.clone(),
        Arc::clone(&probe) as Arc<dyn Sink>,
        stream as Arc<dyn Stream>,
        vec![mapping()],
        Arc::new(crate::Progress::new([name.clone()])),
    )
    .with_writes(4);
    let lane = channel.lane(&name).unwrap().producer;
    let run = tokio::spawn(async move {
        let (_control, mut rx) = tokio::sync::mpsc::channel(1);
        engine.run(&mut rx).await
    });
    for (position, id) in [(0, "warm"), (1, "a"), (2, "b")] {
        let envelopes: Arc<[Envelope]> = vec![Envelope::upsert(
            users(),
            id,
            GenericValue::Map(BTreeMap::new()),
            Some(Position::new(position)),
            chrono::Utc::now(),
        )]
        .into();
        lane.publish(LaneItem::Batch(Batch {
            position: Some(Position::new(position)),
            changes: 1,
            envelopes,
        }))
        .await
        .unwrap();
    }
    until("b to be written", || {
        probe.log().contains(&"end b".to_owned())
    })
    .await;
    settle().await;
    assert!(
        !run.is_finished(),
        "the run ended with a's write still in flight"
    );

    probe.release("a");
    assert!(
        run.await.unwrap().is_err(),
        "the failed acknowledgement ends the run"
    );
    assert!(probe.log().contains(&"end a".to_owned()));
}

/// Lands `z` first: a fresh engine writes its first item alone (a previous
/// process may have stopped on it), so a test of several writes at once
/// starts after one has landed.
async fn warm_up(rig: &mut Rig) {
    rig.live(1, &["z"]).await;
    until("the warm-up write", || {
        rig.probe.log().contains(&"end z".to_owned())
    })
    .await;
}

/// Under `stop`, a rejection the sink reports at its write's first poll
/// fails the run at once: nothing granted after it starts, the write still
/// in flight beside it (`a`) lands, and the confirmation stops before the
/// rejected item. The probe answers `b` on its first poll: that is the
/// precondition of "nothing after it starts".
#[tokio::test(start_paused = true)]
async fn a_stop_rejection_seen_at_its_first_poll_starts_nothing_after_it() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]).rejecting(&["b"]), 4).start();
    warm_up(&mut rig).await;
    rig.live(2, &["a"]).await;
    rig.live(3, &["b"]).await;
    rig.live(4, &["c"]).await;
    until("b to be written", || {
        rig.probe.log().contains(&"end b".to_owned())
    })
    .await;
    settle().await;
    assert!(
        rig.engine.as_ref().is_some_and(|run| !run.is_finished()),
        "the run waits for a"
    );
    assert!(
        !rig.probe.starts().contains(&"c".to_owned()),
        "nothing starts once the rejection failed the run"
    );
    rig.probe.release("a");
    assert!(
        rig.ended().await.is_err(),
        "the rejection under stop fails the run"
    );
    assert_eq!(
        super::covered(&rig.progress),
        Some(Position::new(2)),
        "a landed; b is unacknowledged, so the lane confirms nothing past 2"
    );
}

/// Under `stop`, while the rejected write is still in flight the run doesn't
/// know yet: disjoint items behind it start and land. What is promised is
/// that the lane's confirmation stays before the rejected item, and the next
/// run writes it alone (the barrier).
#[tokio::test(start_paused = true)]
async fn a_stop_rejection_still_in_flight_lets_disjoint_writes_land_but_holds_confirmation() {
    let mut rig = Rig::new(Probe::new().holding(&["b"]).rejecting(&["b"]), 4).start();
    warm_up(&mut rig).await;
    rig.live(2, &["b"]).await;
    rig.live(3, &["c"]).await;
    until("c to be written while b is in flight", || {
        rig.probe.log().contains(&"end c".to_owned())
    })
    .await;
    assert_eq!(
        super::covered(&rig.progress),
        Some(Position::new(1)),
        "c landed, but b before it is still unacknowledged"
    );
    rig.probe.release("b");
    assert!(rig.ended().await.is_err());
    assert_eq!(super::covered(&rig.progress), Some(Position::new(1)));

    let rig = rig.start();
    settle().await;
    assert_eq!(
        rig.probe.starts(),
        ["z", "b", "c", "b"],
        "the restarted run writes b alone, and nothing behind it"
    );
}

/// A fresh engine writes its first item alone: a sink that stopped the
/// previous process on a `stop` rejection writes nothing past it on the next
/// start either, even while the rejected write is still in flight (it awaits
/// here, so the rejection isn't known when the next grant could start).
#[tokio::test(start_paused = true)]
async fn a_fresh_engine_writes_nothing_past_a_persistent_stop_rejection() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]).rejecting(&["a"]), 4);
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    rig.live(3, &["c"]).await;
    let mut rig = rig.start();
    settle().await;
    assert_eq!(rig.probe.starts(), ["a"], "nothing starts beside a");
    rig.probe.release("a");
    assert!(rig.ended().await.is_err());
    assert_eq!(rig.probe.starts(), ["a"], "nothing past the rejected item");
    assert_eq!(
        rig.progress.landed(),
        source::cdc::Landed::Before(Position::new(1))
    );
}

/// The oldest item fails first and a later one lands after it: the later one
/// landing is no progress, since the failed item comes back first.
#[tokio::test(start_paused = true)]
async fn a_write_landing_after_an_older_one_failed_is_no_progress() {
    let mut rig = Rig::new(Probe::new().holding(&["a", "b"]).failing_once(&["a"]), 4).start();
    warm_up(&mut rig).await;
    let before = rig.sink_engine.advanced();
    rig.live(2, &["a"]).await;
    rig.live(3, &["b"]).await;
    until("a and b in flight", || {
        let starts = rig.probe.starts();
        starts.contains(&"a".to_owned()) && starts.contains(&"b".to_owned())
    })
    .await;
    rig.probe.release("a");
    until("a to fail", || {
        rig.probe.log().contains(&"failed a".to_owned())
    })
    .await;
    rig.probe.release("b");
    assert!(rig.ended().await.is_err());
    assert_eq!(
        rig.sink_engine.advanced(),
        before,
        "b landing behind a failure is no progress"
    );
}

/// A write that fails at its first poll, with a later item sharing its
/// document waiting: the later item never starts before the failed one is
/// written again, so the document's version never goes down.
#[tokio::test(start_paused = true)]
async fn a_write_failing_at_its_first_poll_holds_back_a_later_item_sharing_its_document() {
    let mut rig = Rig::new(Probe::new().failing_once(&["x"]), 4).start();
    warm_up(&mut rig).await;
    rig.live(2, &["x"]).await;
    rig.live(3, &["x"]).await;
    assert!(rig.ended().await.is_err());
    assert_eq!(
        rig.probe.starts(),
        ["z", "x"],
        "the later x waits for the failed one"
    );

    let rig = rig.start();
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.probe.starts(), ["z", "x", "x", "x"]);
    assert!(!rig.probe.regressed.load(Ordering::SeqCst));
    assert_eq!(super::covered(&rig.progress), Some(Position::new(3)));
}

/// A stop with backfill still on the lane and every write slot busy: the
/// writes in flight land, the backfill is acknowledged unwritten as it is
/// taken, and its index isn't marked seeded.
#[tokio::test(start_paused = true)]
async fn a_stop_with_backfill_on_the_lane_and_every_slot_busy_writes_none_of_it() {
    let mut rig = Rig::new(Probe::new().holding(&["a", "b"]), 2).start();
    warm_up(&mut rig).await;
    rig.live(2, &["a"]).await;
    rig.live(3, &["b"]).await;
    rig.backfill(&["p"]).await;
    rig.backfill(&["q"]).await;
    rig.complete().await;
    settle().await;
    assert_eq!(rig.probe.starts(), ["z", "a", "b"]);

    rig.stop.request();
    rig.probe.release("a");
    rig.probe.release("b");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(
        rig.probe.starts(),
        ["z", "a", "b"],
        "no backfill page is written"
    );
    assert!(!rig.probe.log().contains(&"seeded users".to_owned()));
    assert_eq!(super::covered(&rig.progress), Some(Position::new(3)));
}

/// Under `skip`, a rejected document is quarantined and its write
/// acknowledged while other writes run beside it: the run goes on and the
/// lane drains.
#[tokio::test(start_paused = true)]
async fn a_skip_rejection_beside_writes_in_flight_is_quarantined_and_the_run_goes_on() {
    let mut rig = Rig::with_policies(
        Probe::new().holding(&["a"]).rejecting(&["b"]),
        4,
        FailurePolicies::new(FailurePolicy::Skip),
    )
    .warm()
    .await;
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    rig.live(3, &["c"]).await;
    until("b and c to be written", || {
        let log = rig.probe.log();
        log.contains(&"end b".to_owned()) && log.contains(&"end c".to_owned())
    })
    .await;
    assert_eq!(
        super::covered(&rig.progress),
        Some(Position::new(0)),
        "a is still being written, so nothing is confirmed yet"
    );
    rig.probe.release("a");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(super::covered(&rig.progress), Some(Position::new(3)));
    assert!(
        rig.engine.as_ref().is_some_and(|run| !run.is_finished()),
        "a skipped rejection does not end the run"
    );
}

/// A reindex taken off the control channel while a write is in flight
/// survives that write failing: the restarted engine stages it and requests
/// its backfill exactly once. (A reindex still pending when a stop ends the
/// deployment is dropped with it: the daemon refuses reindexes once stopping.)
#[tokio::test(start_paused = true)]
async fn a_reindex_taken_before_a_failed_write_is_staged_after_the_restart() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]).failing_once(&["a"]), 4).start();
    rig.live(1, &["a"]).await;
    settle().await;
    rig.control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    settle().await;
    rig.probe.release("a");
    assert!(rig.ended().await.is_err(), "the failed write ends the run");

    let rig = rig.start();
    until("the reindex", || {
        rig.probe.log().contains(&"reindex users".to_owned())
    })
    .await;
    until("the lane to drain", || rig.stream.is_idle()).await;
    settle().await;
    let reindexes = rig
        .probe
        .log()
        .iter()
        .filter(|line| *line == "reindex users")
        .count();
    assert_eq!(reindexes, 1, "staged once");
    let mut requests = rig.stream.requests().unwrap().consumer;
    // Held, not dropped: a dropped delivery is redelivered.
    let mut asked = Vec::new();
    while let Some(Ok(Some(request))) = requests.recv().now_or_never() {
        asked.push(request);
    }
    assert_eq!(asked.len(), 1, "its backfill is requested once");
}

/// The supervisor resets its restart backoff after a run that made progress.
/// Progress is the sink's oldest unfinished item landing, not any
/// acknowledgement: a later item landing beside an older one that then fails
/// leaves the sink where it was, so that run made none.
#[tokio::test(start_paused = true)]
async fn only_the_oldest_unfinished_item_landing_counts_as_progress() {
    let mut rig = Rig::new(Probe::new().holding(&["a"]).failing_once(&["a"]), 4)
        .warm()
        .await;
    let warmed = rig.sink_engine.advanced();
    rig.live(1, &["a"]).await;
    rig.live(2, &["b"]).await;
    until("b to be written", || {
        rig.probe.log().contains(&"end b".to_owned())
    })
    .await;
    rig.probe.release("a");
    assert!(rig.ended().await.is_err());
    assert_eq!(
        rig.sink_engine.advanced(),
        warmed,
        "b landed behind a, which failed: no progress"
    );

    let rig = rig.start();
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert!(rig.sink_engine.advanced() > warmed, "a landing is progress");
}

/// An ordered sink (an emitting one, such as stdout) gets its items in exact
/// lane order, one at a time, whatever `writes` says: no later disjoint item
/// starts beside a held one, and no live item overtakes an earlier backfill
/// page.
#[tokio::test(start_paused = true)]
async fn an_ordered_sink_gets_its_items_in_lane_order_one_at_a_time() {
    let mut rig = Rig::new(Probe::new().ordered().holding(&["a"]), 4).start();
    rig.live(1, &["a"]).await;
    rig.backfill(&["p"]).await;
    rig.live(2, &["c"]).await;
    rig.live(3, &["d"]).await;
    settle().await;
    assert_eq!(
        rig.probe.starts(),
        ["a"],
        "nothing starts beside the held item"
    );

    rig.probe.release("a");
    until("the lane to drain", || rig.stream.is_idle()).await;
    assert_eq!(rig.probe.starts(), ["a", "p", "c", "d"], "exact lane order");
    assert_eq!(rig.probe.most_at_once(), 1);
}
