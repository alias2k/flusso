//! Detaching a lane (ADR 0012 stage 10, slice 2 step 2): asked for, it
//! happens at the lane's next refused probe or its next staging; the sink
//! records it first; then the lane's fence freezes, it leaves the slot's
//! minimum, and everything it holds or receives is acknowledged unwritten.
//! Staging brings a detached index back detached. All on paused time.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, Position};
use sink::{
    DetachRecord, Detachment, MarkerAt, RecordVersion, Recorded, Sink, SinkError, WriteReport,
};
use source::cdc::{Continuity, Landed};
use stream::{Batch, LaneItem, LaneKey, Stream};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::sink_name;
use crate::sink_engine::Blame;
use crate::{LaneHealth, Level, Observer, Progress, SinkControl, SinkEngine};

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

fn users() -> IndexName {
    index("users")
}

fn orders() -> IndexName {
    index("orders")
}

fn lane(of: IndexName) -> LaneKey {
    LaneKey::new(of, sink_name("primary"))
}

fn mapping(of: IndexName) -> IndexMapping {
    IndexMapping {
        index: of,
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// How the sink answers a detach.
#[derive(Debug, Clone, Copy)]
enum Record {
    Kept,
    NotKept,
    Fail,
    /// The record lands, and the call still fails (a timeout after
    /// OpenSearch applied it).
    FailAfterWriting,
}

/// A sink that blocks, or misses, the indexes it is told to, records every
/// write and every detach, and answers staging's read with what it is given.
#[derive(Debug, Default)]
struct Gate {
    blocked: Mutex<BTreeSet<String>>,
    missing: Mutex<BTreeSet<String>>,
    /// Each write: when, and the index of each document.
    writes: Mutex<Vec<(Instant, Vec<String>)>>,
    ensured: Mutex<Vec<String>>,
    detaches: Mutex<Vec<(String, sink::Detach)>>,
    /// How the next detaches are answered, in order; then `Kept`.
    records: Mutex<VecDeque<Record>>,
    /// Each detach takes this long.
    detach_takes: Mutex<Option<Duration>>,
    /// What staging reads of each index's records.
    kept: Mutex<BTreeMap<String, Detachment>>,
    forgotten: Mutex<Vec<String>>,
    /// Indexes `is_seeded` answers unseeded.
    unseeded: Mutex<BTreeSet<String>>,
    /// Every document written, as `index/id`, in write order.
    ids: Mutex<Vec<String>>,
    /// A write holding this id takes this long.
    slow: Mutex<Option<(String, Duration)>>,
    /// Each rebuild begun, by index.
    rebuilds: Mutex<Vec<String>>,
    /// Each demotion, by index, with whether live only.
    demoted: Mutex<Vec<(String, bool)>>,
    /// Each index marked seeded.
    seeded: Mutex<Vec<String>>,
    /// The gap a reattach keeps with each index's seed marker.
    marker_gaps: Mutex<BTreeMap<String, sink::Gap>>,
    /// Each gap written, by index, in order.
    gaps: Mutex<Vec<(String, sink::Gap)>>,
    /// The seed meets a block on the serving generation (#479).
    retire_blocked: Mutex<bool>,
    /// Calls that fail from now on, by method name: the process died
    /// before them.
    dead: Mutex<BTreeSet<&'static str>>,
    /// While set, `ensure_index` waits for a permit here.
    ensure_gate: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
}

impl Gate {
    fn dies_before(&self, call: &'static str) -> sink::Result<()> {
        if self.dead.lock().unwrap().contains(call) {
            return Err(SinkError::Write(format!("the process died before {call}")));
        }
        Ok(())
    }
}

impl Gate {
    fn block(&self, of: &IndexName, blocked: bool) {
        let mut set = self.blocked.lock().unwrap();
        if blocked {
            set.insert(of.to_string());
        } else {
            set.remove(of.as_ref());
        }
    }

    /// When each write holding `of` went out.
    fn writes_of(&self, of: &IndexName) -> Vec<Instant> {
        self.writes
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, indexes)| indexes.iter().any(|at| at == of.as_ref()))
            .map(|(when, _)| *when)
            .collect()
    }

    fn detaches(&self) -> Vec<(String, sink::Detach)> {
        self.detaches.lock().unwrap().clone()
    }
}

#[async_trait]
impl Sink for Gate {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        self.dies_before("ensure_index")?;
        let gate = self.ensure_gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.acquire().await.unwrap().forget();
        }
        self.ensured.lock().unwrap().push(mapping.index.to_string());
        Ok(sink::Ensured::default())
    }

    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        let caught_up = if caught_up {
            sink::CaughtUp::All
        } else {
            sink::CaughtUp::No
        };
        self.write_parts(&[envelopes], caught_up).await
    }

    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        _: sink::CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        let slow = self.slow.lock().unwrap().clone();
        if let Some((id, takes)) = slow
            && parts
                .iter()
                .flat_map(|part| part.iter())
                .any(|e| e.id == id)
        {
            tokio::time::sleep(takes).await;
        }
        let mut report = WriteReport::clean();
        let mut indexes = Vec::new();
        let mut named = BTreeSet::new();
        for envelope in parts.iter().flat_map(|part| part.iter()) {
            indexes.push(envelope.index.to_string());
            self.ids
                .lock()
                .unwrap()
                .push(format!("{}/{}", envelope.index, envelope.id));
            if !named.insert(envelope.index.clone()) {
                continue;
            }
            if self
                .blocked
                .lock()
                .unwrap()
                .contains(envelope.index.as_ref())
            {
                report = report.blocking(
                    envelope.index.clone(),
                    format!(
                        "index [{}] blocked by: [FORBIDDEN/8/index write (api)]",
                        envelope.index
                    ),
                );
            } else if self
                .missing
                .lock()
                .unwrap()
                .contains(envelope.index.as_ref())
            {
                report = report.missing_target(
                    envelope.index.clone(),
                    format!("no such index [{}]", envelope.index),
                );
            }
        }
        if !indexes.is_empty() {
            self.writes.lock().unwrap().push((Instant::now(), indexes));
        }
        Ok(report)
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(!self.unseeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn detachment(&self, mapping: &IndexMapping) -> sink::Result<Detachment> {
        let mut kept = self
            .kept
            .lock()
            .unwrap()
            .get(mapping.index.as_ref())
            .cloned()
            .unwrap_or_default();
        kept.gap = self
            .marker_gaps
            .lock()
            .unwrap()
            .get(mapping.index.as_ref())
            .cloned();
        Ok(kept)
    }

    async fn record_gap(&self, mapping: &IndexMapping, gap: &sink::Gap) -> sink::Result<()> {
        self.dies_before("record_gap")?;
        self.gaps
            .lock()
            .unwrap()
            .push((mapping.index.to_string(), gap.clone()));
        self.marker_gaps
            .lock()
            .unwrap()
            .insert(mapping.index.to_string(), gap.clone());
        Ok(())
    }

    async fn detach(
        &self,
        mapping: &IndexMapping,
        detach: &sink::Detach,
    ) -> sink::Result<Recorded> {
        self.detaches
            .lock()
            .unwrap()
            .push((mapping.index.to_string(), detach.clone()));
        let takes = *self.detach_takes.lock().unwrap();
        if let Some(takes) = takes {
            tokio::time::sleep(takes).await;
        }
        let answer = self.records.lock().unwrap().pop_front();
        match answer.unwrap_or(Record::Kept) {
            // Kept as a real sink keeps it: the next staging reads it
            // (review of step 3, S2).
            Record::Kept => {
                let mut kept = self.kept.lock().unwrap();
                let attempts = kept
                    .get(mapping.index.as_ref())
                    .and_then(|kept| kept.attempts);
                let marker = if detach.live_only {
                    MarkerAt::Serving { seeded: true }
                } else {
                    MarkerAt::Newer { seeded: false }
                };
                let mut record = Detachment::recorded(
                    DetachRecord::Detached {
                        live_only: detach.live_only,
                    },
                    Some(marker),
                    version(),
                );
                record.attempts = attempts;
                record.through = detach.through.clone();
                kept.insert(mapping.index.to_string(), record);
                Ok(Recorded::Kept)
            }
            Record::NotKept => Ok(Recorded::NotKept),
            Record::Fail => Err(SinkError::Write(
                "the meta index refused the record".to_owned(),
            )),
            Record::FailAfterWriting => {
                self.kept.lock().unwrap().insert(
                    mapping.index.to_string(),
                    Detachment::recorded(
                        DetachRecord::Detached { live_only: false },
                        Some(MarkerAt::Newer { seeded: false }),
                        version(),
                    ),
                );
                Err(SinkError::Write("the record write timed out".to_owned()))
            }
        }
    }

    async fn begin_rebuild(&self, mapping: &IndexMapping) -> sink::Result<u64> {
        let mut kept = self.kept.lock().unwrap();
        let Some(record) = kept.get_mut(mapping.index.as_ref()) else {
            return Err(SinkError::Write("no detach record".to_owned()));
        };
        let attempts = record.attempts.unwrap_or(0) + 1;
        record.record = Some(DetachRecord::Rebuilding { live_only: false });
        record.attempts = Some(attempts);
        record.marker = Some(MarkerAt::Newer { seeded: false });
        self.rebuilds
            .lock()
            .unwrap()
            .push(mapping.index.to_string());
        Ok(attempts)
    }

    async fn demote(&self, mapping: &IndexMapping, live_only: bool) -> sink::Result<()> {
        if let Some(record) = self.kept.lock().unwrap().get_mut(mapping.index.as_ref()) {
            record.record = Some(DetachRecord::Detached { live_only });
            // R14: live only, the marker is back on the generation reads see.
            if live_only {
                record.marker = Some(MarkerAt::Serving { seeded: true });
            }
        }
        self.demoted
            .lock()
            .unwrap()
            .push((mapping.index.to_string(), live_only));
        Ok(())
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        if *self.retire_blocked.lock().unwrap() {
            return Err(SinkError::RetireBlocked {
                new: format!("{index}_2"),
                old: format!("{index}_1"),
                block: "index write (api)".to_owned(),
                deletable: false,
            });
        }
        // r4: the seed deletes a finished rebuild's record.
        self.kept.lock().unwrap().remove(index.as_ref());
        self.seeded.lock().unwrap().push(index.to_string());
        Ok(())
    }

    async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> sink::Result<()> {
        self.dies_before("forget_detachment")?;
        self.forgotten
            .lock()
            .unwrap()
            .push(format!("{}@{}", mapping.index, version.as_str()));
        self.kept.lock().unwrap().remove(mapping.index.as_ref());
        Ok(())
    }
}

/// Every lane health the observers heard, in order.
#[derive(Debug, Default)]
struct Heard {
    health: Mutex<Vec<(LaneKey, LaneHealth)>>,
    /// The deployment's progress, read as each detach is heard.
    progress: std::sync::OnceLock<Arc<Progress>>,
    /// For each detach heard, whether its lane had already left the
    /// minimum: never, or the fence would freeze where `Free` reaches (R5).
    left_first: Mutex<Vec<bool>>,
    /// Each gap heard, with how many lane healths were heard before it.
    gaps: Mutex<Vec<(LaneKey, sink::Gap, usize)>>,
}

impl Heard {
    fn kinds(&self, of: &LaneKey) -> Vec<&'static str> {
        self.health
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| at == of)
            .map(|(_, health)| health.kind())
            .collect()
    }
}

impl Observer for Heard {
    fn on_lane_gap(&self, lane: &LaneKey, gap: &sink::Gap) {
        let heard = self.health.lock().unwrap().len();
        self.gaps
            .lock()
            .unwrap()
            .push((lane.clone(), gap.clone(), heard));
    }

    fn on_lane_health(&self, lane: &LaneKey, health: &LaneHealth) {
        if health.is_detached()
            && let Some(progress) = self.progress.get()
        {
            self.left_first
                .lock()
                .unwrap()
                .push(progress.is_detached(lane));
        }
        self.health
            .lock()
            .unwrap()
            .push((lane.clone(), health.clone()));
    }
}

fn item(of: &IndexName, id: i64, position: u64) -> LaneItem {
    LaneItem::Batch(Batch {
        key: Some(stream::ItemKey::new(position)),
        position: Some(Position::new(position)),
        envelopes: vec![Envelope::upsert(
            of.clone(),
            id.to_string(),
            GenericValue::Map(BTreeMap::new()),
            Some(Position::new(position)),
            chrono::Utc::now(),
        )]
        .into(),
        ..Default::default()
    })
}

/// A source whose answer for what landed is the `seq` it covers, as text,
/// once it is live.
#[derive(Debug, Default)]
struct Seqs {
    down: std::sync::atomic::AtomicBool,
}

impl crate::SourcePositions for Seqs {
    fn place(&self, landed: Landed) -> Option<String> {
        if self.down.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        Some(
            landed
                .covered_seq()
                .map_or_else(|| "nothing".to_owned(), |seq| seq.to_string()),
        )
    }
}

/// Two lanes, `orders` and `users`, of one sink `primary`.
struct Rig {
    stream: Arc<ChannelStream>,
    progress: Arc<Progress>,
    sink: Arc<Gate>,
    heard: Arc<Heard>,
    positions: Arc<Seqs>,
    rereads: Arc<crate::Rereads>,
}

impl Rig {
    fn new(capacity: usize) -> Self {
        Self::over(capacity, &["primary"])
    }

    /// Both indexes on each of `sinks`; `sink` is `primary`'s.
    fn over(capacity: usize, sinks: &[&str]) -> Self {
        let indexes = [orders(), users()];
        let sinks: Vec<_> = sinks.iter().map(|name| sink_name(name)).collect();
        let progress = Arc::new(Progress::of(indexes.clone(), sinks.clone()));
        let heard = Arc::new(Heard::default());
        let _ = heard.progress.set(Arc::clone(&progress));
        Self {
            stream: Arc::new(ChannelStream::over(
                capacity,
                indexes.clone(),
                sinks.clone(),
            )),
            progress,
            sink: Arc::new(Gate::default()),
            heard,
            positions: Arc::new(Seqs::default()),
            rereads: Arc::new(crate::Rereads::default()),
        }
    }

    /// A live item of `of` at `position`, registered as the ingest engine
    /// would.
    async fn live(&self, of: &IndexName, id: i64, position: u64) {
        self.progress.published(
            &lane(of.clone()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        self.stream
            .lane(&lane(of.clone()))
            .unwrap()
            .producer
            .publish(item(of, id, position))
            .await
            .unwrap();
    }

    fn engine(&self) -> Arc<SinkEngine> {
        self.engine_of("primary", Arc::clone(&self.sink))
    }

    /// The engine of sink `name`, writing to `sink`.
    fn engine_of(&self, name: &str, sink: Arc<Gate>) -> Arc<SinkEngine> {
        Arc::new(self.built(name, sink))
    }

    /// `primary`'s engine with `backfill = false`: the sink a reattach is for.
    fn live_only_engine(&self) -> Arc<SinkEngine> {
        Arc::new(
            self.built("primary", Arc::clone(&self.sink))
                .with_options(sink::SinkOptions { backfill: false }),
        )
    }

    fn built(&self, name: &str, sink: Arc<Gate>) -> SinkEngine {
        let engine = SinkEngine::new(
            sink_name(name),
            sink as Arc<dyn Sink>,
            Arc::clone(&self.stream) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&self.progress),
        )
        .with_writes(4)
        .with_observer(Arc::clone(&self.heard) as Arc<dyn Observer>)
        .with_positions(Arc::clone(&self.positions) as Arc<dyn crate::SourcePositions>)
        .with_rereads(Arc::clone(&self.rereads));
        engine.set_blame(Blame::Nobody);
        engine
    }

    fn applied(&self, of: &IndexName) -> Option<Landed> {
        self.progress.lane(&lane(of.clone()), Level::Applied)
    }
}

/// A run of `engine`, and the sender of its controls.
fn spawn(
    engine: &Arc<SinkEngine>,
) -> (
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
) {
    let (send, mut control) = mpsc::channel::<SinkControl>(4);
    let engine = Arc::clone(engine);
    let task = tokio::spawn(async move { engine.run(&mut control).await });
    (task, send)
}

/// Wait, on paused time, until `condition` holds; a minute of it at most.
async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !condition() {
        assert!(Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn through(position: u64) -> Option<Landed> {
    Some(Landed::Through(Position::new(position)))
}

/// `users` blocked and asked to detach, with an item parked on: the first
/// write parks it, and its first refused probe detaches it.
async fn detached_on_a_block(
    rig: &Rig,
) -> (Arc<SinkEngine>, tokio::task::JoinHandle<crate::Result<()>>) {
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    (engine, task)
}

/// Asked to detach, a parked lane detaches at its next refused probe: the
/// sink records it first, why and the item's keys, then the lane is detached,
/// its fence frozen (the observers hear it) and out of the slot's minimum;
/// the sibling lane keeps landing, and nothing of the detached index is
/// written again (R3–R5, R12).
#[tokio::test(start_paused = true)]
async fn a_refused_probe_detaches_a_lane_asked_to_detach() {
    let rig = Rig::new(64);
    let start = Instant::now();
    let (engine, task) = detached_on_a_block(&rig).await;

    let detaches = rig.sink.detaches();
    assert_eq!(detaches.len(), 1);
    let (index, detach) = &detaches[0];
    assert_eq!(index, "users");
    assert!(
        detach.why.starts_with("blocked: index [users] blocked by"),
        "{detach:?}"
    );
    assert!(!detach.live_only);
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        2,
        "the first write and one refused probe"
    );
    assert!(
        rig.sink
            .writes_of(&users())
            .iter()
            .all(|at| *at < start + Duration::from_secs(2)),
        "the probe at 1 s, then the detach"
    );
    assert_eq!(
        rig.heard.kinds(&lane(users())),
        ["blocked", "blocked", "detached"],
        "parked, its probe refused, detached"
    );
    assert!(rig.progress.is_detached(&lane(users())));

    rig.live(&orders(), 1, 2).await;
    rig.live(&users(), 2, 3).await;
    eventually("orders lands beside the detached lane", || {
        rig.applied(&orders()) == through(2)
    })
    .await;
    assert_eq!(
        rig.progress.landed(),
        Landed::Through(Position::new(2)),
        "only orders answers: the detached lane holds nothing back"
    );
    tokio::time::sleep(Duration::from_secs(120)).await;
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        2,
        "nothing of the detached index reaches write"
    );
    assert!(engine.lane_health(&lane(orders())).is_open());
    task.abort();
}

/// A detached lane is always taken from and each item acknowledged
/// unwritten, so a send blocked on a full lane completes (R6), at once: no
/// item waits for the run's next wake-up (review B1: one item per idle
/// tick).
#[tokio::test(start_paused = true)]
async fn a_detached_lane_is_drained() {
    let rig = Rig::new(2);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    // The producer is ahead of the parked lane, blocked on it, when it
    // detaches.
    let producer = rig.stream.lane(&lane(users())).unwrap().producer;
    let sending = tokio::spawn(async move {
        for position in 1..1001 {
            producer
                .publish(item(&users(), i64::try_from(position).unwrap(), position))
                .await
                .unwrap();
        }
    });
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    let sent = tokio::time::timeout(Duration::from_millis(100), sending).await;
    assert!(
        sent.is_ok(),
        "a thousand items through a lane of two within 100 ms: each was taken at once"
    );
    assert_eq!(rig.sink.writes_of(&users()).len(), 2);
    task.abort();
}

/// The index's builder publishes each item to every lane of the index, the
/// detached one first: a healthy second sink of the same index is not slowed
/// by the detached lane (review B1).
#[tokio::test(start_paused = true)]
async fn a_detached_lane_slows_no_healthy_sink_of_its_index() {
    let rig = Rig::over(2, &["primary", "secondary"]);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    let healthy = rig.engine_of("secondary", Arc::new(Gate::default()));
    let (healthy_task, _control) = spawn(&healthy);
    let secondary = LaneKey::new(users(), sink_name("secondary"));
    let detached = rig.stream.lane(&lane(users())).unwrap().producer;
    let other = rig.stream.lane(&secondary).unwrap().producer;
    let progress = Arc::clone(&rig.progress);
    let registered = secondary.clone();
    let builder = tokio::spawn(async move {
        for position in 1..1001 {
            let id = i64::try_from(position).unwrap();
            detached
                .publish(item(&users(), id, position))
                .await
                .unwrap();
            progress.published(
                &registered,
                stream::ItemKey::new(position),
                Position::new(position),
                Position::new(position),
            );
            other.publish(item(&users(), id, position)).await.unwrap();
        }
    });
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    let start = Instant::now();
    eventually("the healthy sink lands the last item", || {
        rig.progress.lane(&secondary, Level::Applied) == through(1000)
    })
    .await;
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "a thousand items in {:?}",
        start.elapsed()
    );
    builder.await.unwrap();
    task.abort();
    healthy_task.abort();
}

/// A restaging lane has no probe: asked to detach, it detaches when its
/// staging comes due, and the staging never runs.
#[tokio::test(start_paused = true)]
async fn a_restaging_lane_asked_to_detach_detaches_instead_of_staging() {
    let rig = Rig::new(64);
    rig.sink.missing.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    let detaches = rig.sink.detaches();
    assert_eq!(detaches.len(), 1);
    assert!(
        detaches[0].1.why.starts_with("missing: "),
        "{:?}",
        detaches[0]
    );
    assert!(
        rig.sink.ensured.lock().unwrap().is_empty(),
        "the staging the detach replaced never ran"
    );
    assert_eq!(rig.heard.kinds(&lane(users())), ["restaging", "detached"]);
    task.abort();
}

/// A probe that lands drops the ask: the lane opens, and a later park and
/// refused probe leave it parked, with no detach.
#[tokio::test(start_paused = true)]
async fn a_landed_probe_drops_the_ask() {
    let rig = Rig::new(64);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("users parks", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(engine.lane_state(&lane(users())).detach_asked);
    rig.sink.block(&users(), false);
    eventually("the probe lands", || rig.applied(&users()) == through(1)).await;
    assert!(!engine.lane_state(&lane(users())).detach_asked, "dropped");

    rig.sink.block(&users(), true);
    rig.live(&users(), 2, 2).await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(rig.sink.detaches().is_empty());
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked { .. }
    ));
    task.abort();
}

/// A sink that keeps no records never detaches: the ask is dropped and the
/// lane stays parked, holding the slot (R4, O7).
#[tokio::test(start_paused = true)]
async fn a_sink_without_records_never_detaches() {
    let rig = Rig::new(64);
    rig.sink.records.lock().unwrap().push_back(Record::NotKept);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the sink was asked", || rig.sink.detaches().len() == 1).await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(rig.sink.detaches().len(), 1, "asked once only");
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked { .. }
    ));
    assert!(!rig.progress.is_detached(&lane(users())));
    assert_eq!(
        rig.applied(&users()),
        Some(Landed::Before(Position::new(1))),
        "still in the minimum"
    );
    assert!(!engine.lane_state(&lane(users())).detach_asked);

    // The parked item was neither acknowledged nor dropped while the sink
    // was asked: lifted, the block lets that very item land.
    let before = rig.sink.writes_of(&users()).len();
    rig.sink.block(&users(), false);
    eventually("the parked item lands", || {
        rig.applied(&users()) == through(1)
    })
    .await;
    assert!(
        rig.sink.writes_of(&users()).len() > before,
        "written, not dropped"
    );
    task.abort();
}

/// A record that fails leaves the lane parked, in the minimum, with its ask:
/// the next refused probe records again, and then it detaches.
#[tokio::test(start_paused = true)]
async fn a_failed_record_leaves_the_lane_parked_until_its_next_refused_probe() {
    let rig = Rig::new(64);
    rig.sink.records.lock().unwrap().push_back(Record::Fail);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the first record failed", || rig.sink.detaches().len() == 1).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked { .. }
    ));
    assert!(!rig.progress.is_detached(&lane(users())));
    eventually("the next refused probe detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    assert_eq!(rig.sink.detaches().len(), 2);
    assert!(rig.progress.is_detached(&lane(users())));
    task.abort();
}

/// A run that dies while the sink records the detach leaves the lane in the
/// minimum, parked, with its ask: the next run detaches it at its next
/// refused probe (the kill walk's d1, d2 inside a process).
#[tokio::test(start_paused = true)]
async fn a_run_dying_while_the_detach_is_recorded_asks_again() {
    let rig = Rig::new(64);
    *rig.sink.detach_takes.lock().unwrap() = Some(Duration::from_secs(30));
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the record is under way", || rig.sink.detaches().len() == 1).await;
    task.abort();
    let _ = task.await;
    assert!(!rig.progress.is_detached(&lane(users())));
    assert_eq!(
        rig.applied(&users()),
        Some(Landed::Before(Position::new(1))),
        "still in the minimum"
    );
    assert!(engine.lane_state(&lane(users())).detach_asked);

    *rig.sink.detach_takes.lock().unwrap() = None;
    let (task, _control) = spawn(&engine);
    eventually("the next run detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    assert_eq!(rig.sink.detaches().len(), 2);
    assert!(rig.progress.is_detached(&lane(users())));
    task.abort();
}

/// A run restarted after the detach keeps the lane detached: what the lane
/// delivers again, and what comes after, is acknowledged unwritten (H3).
#[tokio::test(start_paused = true)]
async fn a_restarted_run_never_writes_what_a_detached_lane_holds() {
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    rig.sink.block(&users(), false);
    rig.live(&users(), 2, 2).await;
    rig.live(&orders(), 1, 3).await;
    let (task, _control) = spawn(&engine);
    eventually("orders lands", || rig.applied(&orders()) == through(3)).await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        2,
        "nothing more of users"
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    task.abort();
}

fn unreadable() -> DetachRecord {
    DetachRecord::Unreadable {
        reason: "a newer state".to_owned(),
    }
}

fn version() -> Option<RecordVersion> {
    Some(RecordVersion::new("7:1"))
}

/// What a staging does with each record and marker pair (§4): detached
/// indexes are neither ensured nor backfilled and their lanes leave the
/// minimum; a record a rebuild outlived is deleted; a record whose marker is
/// on serving detaches again; anything this step doesn't act on is staged as
/// before.
#[tokio::test(start_paused = true)]
async fn staging_classifies_every_record_and_marker_pair() {
    let detached = |live_only| DetachRecord::Detached { live_only };
    let rebuilding = DetachRecord::Rebuilding { live_only: false };
    let elsewhere = |seeded| Some(MarkerAt::Elsewhere { seeded });
    let serving = |seeded| Some(MarkerAt::Serving { seeded });
    let newer = |seeded| Some(MarkerAt::Newer { seeded });
    // (what staging reads, ensured, detached after, detach recorded, forgotten)
    let cases: Vec<(&str, Detachment, bool, bool, bool, bool)> = vec![
        (
            "nothing kept",
            Detachment::default(),
            true,
            false,
            false,
            false,
        ),
        (
            "detached, marker unseeded elsewhere",
            Detachment::recorded(detached(false), newer(false), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "detached, a seeded marker on no known serving copy: not finished",
            Detachment::recorded(detached(false), elsewhere(true), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "live only, detached while restaging, nothing serving (review S2)",
            Detachment::recorded(detached(true), elsewhere(true), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "live only never reads as finished (O16)",
            Detachment::recorded(detached(true), newer(true), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "detached with no marker",
            Detachment::recorded(detached(false), None, version()),
            false,
            true,
            false,
            false,
        ),
        (
            "live only, marker on serving",
            Detachment::recorded(detached(true), serving(true), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "detached, marker back on serving",
            Detachment::recorded(detached(false), serving(true), version()),
            false,
            true,
            true,
            false,
        ),
        (
            "detached, a rebuild seeded the marker's generation",
            Detachment::recorded(detached(false), newer(true), version()),
            true,
            false,
            false,
            true,
        ),
        (
            "rebuilding, finished",
            Detachment::recorded(rebuilding.clone(), newer(true), version()),
            true,
            false,
            false,
            true,
        ),
        (
            "rebuilding: staged as before until step 4",
            Detachment::recorded(rebuilding, elsewhere(false), version()),
            true,
            false,
            false,
            false,
        ),
        (
            "rebuilding, live only, marker on serving: detached again (O30)",
            Detachment::recorded(
                DetachRecord::Rebuilding { live_only: true },
                serving(true),
                version(),
            ),
            false,
            true,
            true,
            false,
        ),
        (
            "rebuilding, marker on serving: detached again (O30)",
            Detachment::recorded(
                DetachRecord::Rebuilding { live_only: false },
                serving(true),
                version(),
            ),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, marker on a newer unseeded generation: a rebuild, as before (O30)",
            Detachment::recorded(unreadable(), newer(false), version()),
            true,
            false,
            false,
            false,
        ),
        (
            "unreadable, marker on serving: detached again (O30)",
            Detachment::recorded(unreadable(), serving(true), version()),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, serving unknown: detached again (O30)",
            Detachment::recorded(unreadable(), elsewhere(false), version()),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, no marker: detached again (O30)",
            Detachment::recorded(unreadable(), None, version()),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, a newer seeded marker: detached again (O30)",
            Detachment::recorded(unreadable(), newer(true), version()),
            false,
            true,
            true,
            false,
        ),
    ];
    for (name, kept, ensured, is_detached, recorded, forgotten) in cases {
        let rig = Rig::new(64);
        rig.sink
            .kept
            .lock()
            .unwrap()
            .insert("users".to_owned(), kept);
        let engine = rig.engine();
        engine.stage(Continuity::Resumed).await.unwrap();
        assert_eq!(
            rig.sink
                .ensured
                .lock()
                .unwrap()
                .contains(&"users".to_owned()),
            ensured,
            "{name}: ensured"
        );
        assert_eq!(
            engine.lane_health(&lane(users())).is_detached(),
            is_detached,
            "{name}: health"
        );
        assert_eq!(
            rig.progress.is_detached(&lane(users())),
            is_detached,
            "{name}: progress"
        );
        assert_eq!(
            !rig.sink.detaches().is_empty(),
            recorded,
            "{name}: recorded again"
        );
        assert_eq!(
            *rig.sink.forgotten.lock().unwrap() == ["users@7:1"],
            forgotten,
            "{name}: forgotten"
        );
        assert!(
            rig.sink
                .ensured
                .lock()
                .unwrap()
                .contains(&"orders".to_owned()),
            "{name}: orders is staged as ever"
        );
        if is_detached {
            assert_eq!(rig.heard.kinds(&lane(users())), ["detached"], "{name}");
        }
    }
}

/// A lane staging brings back detached is detached in the next run too:
/// what its lane delivers is acknowledged unwritten.
#[tokio::test(start_paused = true)]
async fn a_lane_staged_detached_writes_nothing() {
    let rig = Rig::new(64);
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Elsewhere { seeded: false }),
            version(),
        ),
    );
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.live(&users(), 1, 1).await;
    rig.live(&orders(), 1, 2).await;
    let (task, _control) = spawn(&engine);
    eventually("orders lands", || rig.applied(&orders()) == through(2)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.sink.writes_of(&users()).is_empty());
    assert_eq!(rig.progress.landed(), Landed::Through(Position::new(2)));
    task.abort();
}

/// The fence hears a detach before the lane leaves the minimum, in a run and
/// at staging alike: heard after, it would freeze at what `Free` reaches,
/// the source's horizon, and vouch for changes the index never got (R5,
/// review S4).
#[tokio::test(start_paused = true)]
async fn the_fence_hears_a_detach_before_the_lane_leaves_the_minimum() {
    let rig = Rig::new(64);
    let (_engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    assert_eq!(*rig.heard.left_first.lock().unwrap(), [false], "in a run");

    let rig = Rig::new(64);
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            version(),
        ),
    );
    rig.engine().stage(Continuity::Resumed).await.unwrap();
    assert!(rig.progress.is_detached(&lane(users())));
    assert_eq!(*rig.heard.left_first.lock().unwrap(), [false], "at staging");
}

/// A record that landed although its call failed (a timeout after the
/// write) is deleted before the lane's next probe, so a lane whose probe
/// then lands is not detached by the next staging (§5, review S1).
#[tokio::test(start_paused = true)]
async fn a_record_that_landed_although_its_call_failed_goes_before_the_lane_opens() {
    let rig = Rig::new(64);
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::FailAfterWriting);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the record was written and the call failed", || {
        rig.sink.detaches().len() == 1
    })
    .await;
    rig.sink.block(&users(), false);
    eventually("the probe lands", || rig.applied(&users()) == through(1)).await;
    assert_eq!(*rig.sink.forgotten.lock().unwrap(), ["users@7:1"]);
    assert!(rig.sink.kept.lock().unwrap().is_empty());
    assert!(engine.lane_health(&lane(users())).is_open());
    task.abort();
    let _ = task.await;

    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(
        engine.lane_health(&lane(users())).is_open(),
        "the next staging finds no record"
    );
    assert!(!rig.progress.is_detached(&lane(users())));
}

/// A record that says detached beside a marker on serving is detached again
/// at staging; a sink that answers it keeps no record fails the staging
/// step, never staging the index attached over the gap (review m2).
#[tokio::test(start_paused = true)]
async fn a_detach_again_the_sink_keeps_no_record_of_fails_the_staging_step() {
    let rig = Rig::new(64);
    rig.sink.records.lock().unwrap().push_back(Record::NotKept);
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Serving { seeded: true }),
            version(),
        ),
    );
    let engine = rig.engine();
    let error = engine.stage(Continuity::Fresh).await.unwrap_err();
    assert!(error.to_string().contains("keeps no record"), "{error}");
    assert!(
        !rig.sink
            .ensured
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    );
    assert!(!rig.progress.is_detached(&lane(users())));
}

/// The detach records an earlier process left, for `users`.
fn detached_record() -> Detachment {
    Detachment::recorded(
        DetachRecord::Detached { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    )
}

/// Items of `users` published past a lane of two, within a second: what
/// only a run that takes the lane as detached lets through.
async fn drains_within_a_second(rig: &Rig) -> bool {
    let producer = rig.stream.lane(&lane(users())).unwrap().producer;
    tokio::time::timeout(Duration::from_secs(1), async {
        for position in 100..140 {
            producer
                .publish(item(&users(), i64::try_from(position).unwrap(), position))
                .await
                .unwrap();
        }
    })
    .await
    .is_ok()
}

/// A lane restaged during a run whose staging finds the index detached is
/// taken as detached by the run: drained, never reopened (`adopt_detached`).
#[tokio::test(start_paused = true)]
async fn a_restage_during_a_run_that_finds_the_index_detached_detaches_the_lane() {
    let rig = Rig::new(2);
    rig.sink.missing.lock().unwrap().insert("users".to_owned());
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    let engine = rig.engine();
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("the restage finds it detached", || {
        rig.progress.is_detached(&lane(users()))
    })
    .await;
    assert!(
        drains_within_a_second(&rig).await,
        "the run drains the lane"
    );
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        1,
        "only the first write"
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    task.abort();
}

/// A reindex whose staging finds the index detached detaches the lane in
/// the run: nothing ensured, nothing written (`adopt_detached`).
#[tokio::test(start_paused = true)]
async fn a_reindex_whose_staging_finds_the_index_detached_detaches_the_lane() {
    let rig = Rig::new(2);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("users lands", || rig.applied(&users()) == through(1)).await;
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the reindex finds it detached", || {
        rig.progress.is_detached(&lane(users()))
    })
    .await;
    assert!(
        drains_within_a_second(&rig).await,
        "the run drains the lane"
    );
    assert_eq!(rig.sink.writes_of(&users()).len(), 1);
    assert!(rig.sink.ensured.lock().unwrap().is_empty());
    task.abort();
}

/// A lane the process holds detached is asked no backfill by a later
/// staging that finds no record (removed by hand): `Progress` decides inside
/// a process, and the fence keeps answering `detached` (review m5).
#[tokio::test(start_paused = true)]
async fn a_lane_held_detached_is_asked_no_backfill_by_a_later_staging() {
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    engine.stage(Continuity::Resumed).await.unwrap();
    let mut requests = rig.stream.requests().unwrap().consumer;
    let asked = tokio::time::timeout(Duration::from_millis(1), requests.recv()).await;
    assert!(asked.is_err(), "no backfill asked: {asked:?}");
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(rig.progress.is_detached(&lane(users())));
}

/// The attach request the sink engine sent for `users`: its attempt.
async fn attach_asked(rig: &Rig) -> u64 {
    let mut requests = rig.stream.requests().unwrap().consumer;
    let delivery = tokio::time::timeout(Duration::from_secs(5), requests.recv())
        .await
        .expect("an attach is asked for")
        .unwrap()
        .unwrap();
    let (request, handle) = delivery.into_parts();
    handle.ack().await.unwrap();
    let stream::Request::Attach { index, attempt, .. } = request else {
        panic!("an attach request, got {request:?}");
    };
    assert_eq!(index, users());
    attempt
}

/// `users` detached on a block, the block lifted, and an attach asked for:
/// the engine and the attempt it waits for.
async fn attaching(
    rig: &Rig,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
    u64,
) {
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    control
        .send(SinkControl::Attach { index: users() })
        .await
        .unwrap();
    let attempt = attach_asked(rig).await;
    (engine, task, control, attempt)
}

async fn publish(rig: &Rig, item: LaneItem) {
    rig.stream
        .lane(&lane(users()))
        .unwrap()
        .producer
        .publish(item)
        .await
        .unwrap();
}

fn written(rig: &Rig) -> Vec<String> {
    rig.sink
        .ids
        .lock()
        .unwrap()
        .iter()
        .filter(|id| id.starts_with("users/"))
        .cloned()
        .collect()
}

/// R6, R8: what reaches the lane before its `Attached` (built while it was
/// detached) is acknowledged unwritten; the `Attached` of the attempt asked
/// for opens the lane, and what follows is written.
#[tokio::test(start_paused = true)]
async fn items_before_attached_are_discarded_and_after_it_written() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = attaching(&rig).await;
    let before = written(&rig);
    publish(&rig, item(&users(), 10, 10)).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    eventually("the item after the attach is written", || {
        written(&rig).contains(&"users/11".to_owned())
    })
    .await;
    assert!(engine.lane_health(&lane(users())).is_open());
    let after: Vec<String> = written(&rig).into_iter().skip(before.len()).collect();
    assert_eq!(
        after,
        ["users/11"],
        "the item before the attach is not written"
    );
    task.abort();
}

/// An `Attached` of another attempt than the one asked for (an older one)
/// is discarded: the lane stays detached until its own.
#[tokio::test(start_paused = true)]
async fn an_older_attempts_attached_is_discarded() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = attaching(&rig).await;
    publish(
        &rig,
        LaneItem::Attached {
            attempt: attempt - 1,
        },
    )
    .await;
    publish(&rig, item(&users(), 10, 10)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(!written(&rig).contains(&"users/10".to_owned()));
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    eventually("its own attach opens the lane", || {
        written(&rig).contains(&"users/11".to_owned())
    })
    .await;
    task.abort();
}

/// A second `Attached` of the same attempt (published again by a run that
/// died between the attach and the first) discards nothing (H2).
#[tokio::test(start_paused = true)]
async fn a_second_attached_of_the_same_attempt_discards_nothing() {
    let rig = Rig::new(64);
    let (_engine, task, _control, attempt) = attaching(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 12, 12)).await;
    eventually("both items are written", || {
        let written = written(&rig);
        written.contains(&"users/11".to_owned()) && written.contains(&"users/12".to_owned())
    })
    .await;
    task.abort();
}

/// A sink run restarted after the `Attached` took effect never writes an
/// item delivered before it: those were acknowledged in line before the
/// lane opened, so nothing earlier comes back; what was after it and not
/// acknowledged is delivered again and written (H3).
#[tokio::test(start_paused = true)]
async fn a_run_restart_after_attached_never_writes_a_redelivered_earlier_item() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = attaching(&rig).await;
    *rig.sink.slow.lock().unwrap() = Some(("11".to_owned(), Duration::from_secs(30)));
    publish(&rig, item(&users(), 10, 10)).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    task.abort();
    let _ = task.await;
    *rig.sink.slow.lock().unwrap() = None;
    assert!(
        !written(&rig).contains(&"users/11".to_owned()),
        "its write was cut by the restart"
    );
    let (task, _control) = spawn(&engine);
    eventually(
        "the item after the attach is written, delivered again",
        || written(&rig).contains(&"users/11".to_owned()),
    )
    .await;
    assert!(
        !written(&rig).contains(&"users/10".to_owned()),
        "{:?}",
        written(&rig)
    );
    assert!(engine.lane_health(&lane(users())).is_open());
    task.abort();
}

/// The next request the sink engine sent.
async fn next_request(rig: &Rig) -> stream::Request {
    let mut requests = rig.stream.requests().unwrap().consumer;
    let delivery = tokio::time::timeout(Duration::from_secs(10), requests.recv())
        .await
        .expect("a request is sent")
        .unwrap()
        .unwrap();
    let (request, handle) = delivery.into_parts();
    handle.ack().await.unwrap();
    request
}

/// `users` detached on a block, the block lifted, and a reindex asked:
/// the rebuild begins and the engine asks for the attach; its attempt.
async fn rebuilding_on_reindex(
    rig: &Rig,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
    u64,
) {
    let (engine, task) = detached_on_a_block(rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(rig).await else {
        panic!("the rebuild asks for its attach");
    };
    (engine, task, control, attempt)
}

/// R13 and r1–r4: a reindex of a detached index rebuilds it. The sink begins
/// the rebuild (the record says `rebuilding` before the attach is asked:
/// S2), the generation is ensured, the attach is asked; once `Attached` is
/// taken the lane is open, rebuilding, and asks its backfill; the backfill's
/// seed finishes it: the record is gone and the lane is current.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_a_detached_index_rebuilds_it_to_the_seed() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    assert_eq!(*rig.sink.rebuilds.lock().unwrap(), ["users"]);
    assert!(
        matches!(
            rig.sink
                .kept
                .lock()
                .unwrap()
                .get("users")
                .and_then(|kept| kept.record.clone()),
            Some(DetachRecord::Rebuilding { .. })
        ),
        "rebuilding is recorded before the attach is asked"
    );
    assert!(
        rig.sink
            .ensured
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    );
    assert!(
        engine.lane_health(&lane(users())).is_detached(),
        "until Attached"
    );

    publish(&rig, LaneItem::Attached { attempt }).await;
    let stream::Request::Backfill { index, ticket, .. } = next_request(&rig).await else {
        panic!("the rebuild asks its backfill once attached");
    };
    assert_eq!(index, users());
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));

    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("the rebuild is seeded", || {
        rig.sink
            .seeded
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    })
    .await;
    eventually("the lane is current", || {
        engine.lane_state(&lane(users())).rebuilding.is_none()
    })
    .await;
    assert!(
        rig.sink.kept.lock().unwrap().get("users").is_none(),
        "record gone"
    );
    task.abort();
}

/// Review of step 3, S2: a lane attached by its rebuild stays attached when
/// the daemon stages the sink again before restarting its run: the record
/// says `rebuilding`, read through the real staging path, never `detached`.
#[tokio::test(start_paused = true)]
async fn an_attached_lane_stays_attached_across_a_sink_run_restart() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    // What the index's builder does before it publishes `Attached`.
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    task.abort();
    let _ = task.await;
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(engine.lane_health(&lane(users())).is_open());
    assert!(!rig.progress.is_detached(&lane(users())));
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 11, 11).await;
    eventually("the lane writes after the restart", || {
        written(&rig).contains(&"users/11".to_owned())
    })
    .await;
    task.abort();
}

/// Review of step 3, m5: an attach with no backfill (the test path, as
/// `reattach`) clears the cut backfill entry the detach left, so a later
/// reindex of the index is staged, not taken for one still outstanding.
#[tokio::test(start_paused = true)]
async fn a_reindex_after_an_attach_is_staged() {
    let rig = Rig::new(64);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let _backfill = next_request(&rig).await;
    rig.sink.block(&users(), true);
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    control
        .send(SinkControl::Attach { index: users() })
        .await
        .unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("an attach");
    };
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    // The record goes with the attach, as `reattach` deletes it before.
    rig.sink.kept.lock().unwrap().remove("users");
    let ensured = rig.sink.ensured.lock().unwrap().len();
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the reindex is staged", || {
        rig.sink.ensured.lock().unwrap().len() > ensured
    })
    .await;
    task.abort();
}

/// O20(c): a reindex of a rebuilding index waiting for its attach, or open,
/// is ignored; one of a rebuild that parked starts it again: detached
/// again, then a second rebuild begins.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_a_rebuild_is_ignored_unless_it_parked() {
    let rig = Rig::new(64);
    let (engine, task, control, attempt) = rebuilding_on_reindex(&rig).await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        rig.sink.rebuilds.lock().unwrap().len(),
        1,
        "waiting for Attached: ignored"
    );

    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        rig.sink.rebuilds.lock().unwrap().len(),
        1,
        "open and rebuilding: ignored"
    );

    rig.sink.block(&users(), true);
    rig.live(&users(), 2, 2).await;
    eventually("the rebuilding lane parks", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the rebuild starts again", || {
        rig.sink.rebuilds.lock().unwrap().len() == 2
    })
    .await;
    assert!(
        engine.lane_health(&lane(users())).is_detached(),
        "detached again, attaching"
    );
    let stream::Request::Attach { attempt: again, .. } = next_request(&rig).await else {
        panic!("a new attach");
    };
    assert!(again > attempt);
    task.abort();
}

/// The process-start attach of a `rebuilding` record: the lane starts
/// attached and rebuilding, its index ensured and its backfill asked; under
/// `--skip-backfill` it is demoted (O26), and on a sink with `backfill =
/// false` demoted live only (R14): detached.
#[tokio::test(start_paused = true)]
async fn a_start_attaches_a_rebuild_or_demotes_it() {
    let rebuilding = || {
        let mut record = Detachment::recorded(
            DetachRecord::Rebuilding { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            version(),
        );
        record.attempts = Some(3);
        record
    };
    let rig = Rig::new(64);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), rebuilding());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(3));
    assert!(
        rig.sink
            .ensured
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    );
    let stream::Request::Backfill { index, .. } = next_request(&rig).await else {
        panic!("its backfill");
    };
    assert_eq!(index, users());

    for (skip_backfill, backfill) in [(true, true), (false, false)] {
        let rig = Rig::new(64);
        rig.sink
            .kept
            .lock()
            .unwrap()
            .insert("users".to_owned(), rebuilding());
        let engine = SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&rig.sink) as Arc<dyn Sink>,
            Arc::clone(&rig.stream) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&rig.progress),
        )
        .skip_backfill(skip_backfill)
        .with_options(sink::SinkOptions { backfill });
        engine.stage(Continuity::Resumed).await.unwrap();
        assert!(
            engine.lane_health(&lane(users())).is_detached(),
            "{skip_backfill}"
        );
        assert!(rig.progress.is_detached(&lane(users())));
        assert_eq!(
            *rig.sink.demoted.lock().unwrap(),
            [("users".to_owned(), !backfill)]
        );
    }
}

/// A reattach of `index` whose verdict nobody hears.
fn reattach(index: IndexName) -> SinkControl {
    SinkControl::Reattach {
        index,
        reply: crate::Reply::ignored(),
    }
}

fn gap(from: Option<&str>, to: Option<&str>) -> sink::Gap {
    sink::Gap {
        from: from.map(str::to_owned),
        to: to.map(str::to_owned),
    }
}

/// `primary` with `backfill = false`, staged and running: `users/1` lands,
/// then `users` is blocked and asked to detach, and detaches on `users/2`;
/// the block is lifted.
async fn detached_live_only(
    rig: &Rig,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
) {
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("users/1 lands", || rig.applied(&users()) == through(1)).await;
    rig.sink.block(&users(), true);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 2, 2).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    (engine, task, control)
}

/// O16, §6a, S2 and S3: `reattach --accept-gap` on a live-only lane. The
/// detach recorded where reads stood (`from`); the reattach ensures the
/// index, keeps the gap with `to` unknown, deletes the record (the commit
/// point), and asks for the attach. At `Attached` the fence hears the whole
/// gap before the lane opens, the sink keeps it, and the lane is written as
/// usual.
#[tokio::test(start_paused = true)]
async fn a_reattach_keeps_the_gap_deletes_the_record_and_attaches_the_lane() {
    let rig = Rig::new(64);
    let (engine, task, control) = detached_live_only(&rig).await;
    let detaches = rig.sink.detaches();
    let (_, detach) = detaches.last().unwrap();
    assert!(detach.live_only);
    assert_eq!(detach.through.as_deref(), Some("1"), "users/2 never landed");
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Detached { through: Some(ref at), .. } if at == "1"
    ));

    let ensured = rig.sink.ensured.lock().unwrap().len();
    control.send(reattach(users())).await.unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("the reattach asks for its attach");
    };
    assert!(rig.sink.ensured.lock().unwrap().len() > ensured);
    assert_eq!(
        *rig.sink.gaps.lock().unwrap(),
        [("users".to_owned(), gap(Some("1"), None))],
        "kept before the record goes"
    );
    assert!(
        rig.sink.kept.lock().unwrap().get("users").is_none(),
        "the record is gone: no staging detaches the lane again"
    );
    assert!(
        engine.lane_health(&lane(users())).is_detached(),
        "until Attached"
    );
    assert!(rig.heard.gaps.lock().unwrap().is_empty());

    // What the index's builder does: the lane answers through the newest
    // change routed while it was detached, then `Attached` is published.
    rig.progress.routed(Position::new(3));
    assert_eq!(
        rig.progress.attach(&lane(users()), attempt),
        crate::Attach::Attached
    );
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    let heard = rig.heard.gaps.lock().unwrap().clone();
    assert_eq!(heard.len(), 1);
    let (at, heard_gap, healths) = heard.first().cloned().unwrap();
    assert_eq!((at, heard_gap), (lane(users()), gap(Some("1"), Some("3"))));
    assert_eq!(
        rig.heard.kinds(&lane(users())).last(),
        Some(&"open"),
        "the lane opened"
    );
    assert!(
        rig.heard
            .health
            .lock()
            .unwrap()
            .get(healths)
            .is_some_and(|(_, health)| health.is_open()),
        "the gap is heard before the lane opens"
    );
    eventually("the whole gap is kept", || {
        rig.sink.gaps.lock().unwrap().last()
            == Some(&("users".to_owned(), gap(Some("1"), Some("3"))))
    })
    .await;
    rig.live(&users(), 4, 4).await;
    eventually("written as usual", || {
        written(&rig).contains(&"users/4".to_owned())
    })
    .await;
    task.abort();
}

/// O16, §6a: a reattach is refused, and changes nothing, on a sink that can
/// rebuild (a reindex does), on a lane that is not detached, and when the
/// records don't say detached live only with the marker on the seeded
/// generation reads see.
#[tokio::test(start_paused = true)]
async fn a_reattach_is_refused_where_it_does_not_apply() {
    let quiet = |rig: &Rig| {
        rig.sink.gaps.lock().unwrap().is_empty() && rig.sink.forgotten.lock().unwrap().is_empty()
    };

    // A sink that can rebuild.
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    let (task, control) = spawn(&engine);
    control.send(reattach(users())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(quiet(&rig), "a sink that can rebuild");
    assert!(engine.lane_health(&lane(users())).is_detached());
    task.abort();

    // A lane not detached.
    let rig = Rig::new(64);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    control.send(reattach(users())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(quiet(&rig), "a lane not detached");
    task.abort();

    // Records that don't allow it: the marker moved off the generation
    // reads see.
    let rig = Rig::new(64);
    let (engine, task, control) = detached_live_only(&rig).await;
    rig.sink
        .kept
        .lock()
        .unwrap()
        .get_mut("users")
        .unwrap()
        .marker = Some(MarkerAt::Newer { seeded: false });
    control.send(reattach(users())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(quiet(&rig), "records that don't allow it");
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(engine.lane_state(&lane(users())).attaching.is_none());
    task.abort();
}

/// S3's crash window: a reattach that died between deleting its record and
/// the attach left a gap whose end is unknown. A start before the slot is
/// held stages the lane attached and hears the open gap; once the slot is
/// held the read again completes it where the lane stands, before anything
/// of it is written, and keeps it.
#[tokio::test(start_paused = true)]
async fn a_reattach_cut_short_before_its_attach_completes_its_gap_once_the_slot_is_held() {
    let rig = Rig::new(64);
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), gap(Some("1"), None));
    rig.positions
        .down
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(
        rig.heard
            .gaps
            .lock()
            .unwrap()
            .iter()
            .map(|(at, gap, _)| (at.clone(), gap.clone()))
            .collect::<Vec<_>>(),
        [(lane(users()), gap(Some("1"), None))]
    );
    assert!(rig.sink.gaps.lock().unwrap().is_empty());

    rig.positions
        .down
        .store(false, std::sync::atomic::Ordering::SeqCst);
    rig.rereads.live_opened();
    assert!(!rig.rereads.current(&sink_name("primary")));
    let (task, _control) = spawn(&engine);
    eventually("the gap is completed and kept", || {
        rig.sink.gaps.lock().unwrap().as_slice()
            == [("users".to_owned(), gap(Some("1"), Some("nothing")))]
    })
    .await;
    assert_eq!(
        rig.heard
            .gaps
            .lock()
            .unwrap()
            .last()
            .map(|(_, gap, _)| gap.clone()),
        Some(gap(Some("1"), Some("nothing")))
    );
    eventually("the records count as read", || {
        rig.rereads.current(&sink_name("primary"))
    })
    .await;
    task.abort();
}

/// O29: a lane another process detached after this one staged is taken as
/// detached once the slot is held, before anything of it is written: the
/// sink engine reads its records again first.
#[tokio::test(start_paused = true)]
async fn a_lane_the_records_say_detached_once_the_slot_is_held_is_taken_as_detached() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    rig.rereads.live_opened();
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("the lane is taken as detached", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    assert!(rig.progress.is_detached(&lane(users())));
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(written(&rig).is_empty(), "nothing of it is written");
    assert!(rig.rereads.current(&sink_name("primary")));
    task.abort();
}

/// O29: a staging after the slot opened reads every record: it is the read
/// again, and the run reads nothing more.
#[tokio::test(start_paused = true)]
async fn a_staging_once_the_slot_is_held_counts_as_the_read() {
    let rig = Rig::new(64);
    rig.rereads.live_opened();
    let engine = rig.engine();
    assert!(!rig.rereads.current(&sink_name("primary")));
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(rig.rereads.current(&sink_name("primary")));
}

/// A lane reattached, detached again and reattached again keeps one gap
/// spanning both: the earlier gap's positions are never vouched for again.
#[tokio::test(start_paused = true)]
async fn a_second_reattach_keeps_the_first_gaps_start() {
    let rig = Rig::new(64);
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), gap(Some("0"), Some("0")));
    let (_engine, task, control) = detached_live_only(&rig).await;
    control.send(reattach(users())).await.unwrap();
    let _attach = next_request(&rig).await;
    assert_eq!(
        rig.sink.gaps.lock().unwrap().last(),
        Some(&("users".to_owned(), gap(Some("0"), None)))
    );
    task.abort();
}

/// Review of step 4, S1: a rebuild an operator forced under
/// `--skip-backfill` survives its sink's re-staging inside the process (a
/// run restart after a transient failure): O26 demotes only at a process's
/// start, where no backfill would finish it.
#[tokio::test(start_paused = true)]
async fn a_rebuild_forced_under_skip_backfill_survives_a_sink_run_restart() {
    let rig = Rig::new(64);
    rig.sink.block(&users(), true);
    let engine = Arc::new(
        rig.built("primary", Arc::clone(&rig.sink))
            .skip_backfill(true),
    );
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("the forced rebuild asks for its attach");
    };
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    task.abort();
    let _ = task.await;
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(
        rig.sink.demoted.lock().unwrap().is_empty(),
        "no demotion inside the process"
    );
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
}

/// Review of step 4, S2: a rebuild whose seed meets a block on the serving
/// generation (#479) waits with its complete generation, parked `blocked`.
/// A reindex then would delete that generation and meet the same block, the
/// cost O28 rejected: it is ignored, and the rebuild stays parked as it was.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_a_rebuild_waiting_on_its_seeds_retire_block_is_refused() {
    let rig = Rig::new(64);
    let (engine, task, control, attempt) = rebuilding_on_reindex(&rig).await;
    *rig.sink.retire_blocked.lock().unwrap() = true;
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    let stream::Request::Backfill { ticket, .. } = next_request(&rig).await else {
        panic!("the rebuild asks its backfill");
    };
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("the seed parks on the retire block", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked {
                reason: crate::ParkReason::SeedBlocked { .. },
                ..
            }
        )
    })
    .await;
    let detaches = rig.sink.detaches().len();
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        rig.sink.rebuilds.lock().unwrap().len(),
        1,
        "no second rebuild"
    );
    assert_eq!(rig.sink.detaches().len(), detaches, "not detached again");
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked {
            reason: crate::ParkReason::SeedBlocked { .. },
            ..
        }
    ));
    task.abort();
}

/// Review of step 4, S3: the reattach killed before each of its writes, and
/// what the next start finds. Until the record is deleted the lane comes
/// back detached (the fence frozen at the detach); from the delete on it
/// comes back attached with the gap, never without it, so the fence never
/// vouches inside it. The gap must be kept before the record goes.
#[tokio::test(start_paused = true)]
async fn a_reattach_killed_before_any_step_comes_back_with_its_gap_or_detached() {
    // The call the process died before, and whether the next start finds
    // the lane detached (else attached, carrying the open gap).
    for (died_before, detached) in [
        ("ensure_index", true),
        ("record_gap", true),
        ("forget_detachment", true),
        ("attached", false),
    ] {
        let rig = Rig::new(64);
        let (_engine, task, control) = detached_live_only(&rig).await;
        if died_before != "attached" {
            rig.sink.dead.lock().unwrap().insert(died_before);
        }
        control.send(reattach(users())).await.unwrap();
        if died_before == "attached" {
            let _attach = next_request(&rig).await;
        } else {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        task.abort();
        let _ = task.await;

        // The next start, before the slot is held, over what the sink kept.
        let mut next = Rig::new(64);
        next.sink = Arc::clone(&rig.sink);
        next.sink.dead.lock().unwrap().clear();
        next.positions
            .down
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let engine = next.live_only_engine();
        engine.stage(Continuity::Resumed).await.unwrap();
        let heard: Vec<sink::Gap> = next
            .heard
            .gaps
            .lock()
            .unwrap()
            .iter()
            .map(|(_, gap, _)| gap.clone())
            .collect();
        if detached {
            assert!(
                engine.lane_health(&lane(users())).is_detached(),
                "{died_before}: the record still says detached"
            );
        } else {
            assert!(
                engine.lane_health(&lane(users())).is_open(),
                "{died_before}: no record, the lane attached"
            );
            assert_eq!(
                heard,
                [gap(Some("1"), None)],
                "{died_before}: the fence hears the open gap"
            );
        }
    }
}

/// Send a reattach of `users` on `control` and wait for its verdict.
async fn verdict(control: &mpsc::Sender<SinkControl>) -> crate::ReattachVerdict {
    let (reply, verdict) = crate::Reply::new();
    control
        .send(SinkControl::Reattach {
            index: users(),
            reply,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), verdict)
        .await
        .expect("a verdict")
        .expect("answered")
}

/// Review of step 4, S4: the operator hears the truth. A reattach answers
/// `Started` once the record is gone; a refusal, with why and what works
/// instead; a sink failing a step before the record's delete, `Failed`.
#[tokio::test(start_paused = true)]
async fn a_reattach_answers_its_verdict() {
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    let (task, control) = spawn(&engine);
    let crate::ReattachVerdict::Refused(why) = verdict(&control).await else {
        panic!("a sink that can rebuild refuses");
    };
    assert!(why.contains("flusso reindex users --sink primary"), "{why}");
    task.abort();

    let rig = Rig::new(64);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    assert_eq!(
        verdict(&control).await,
        crate::ReattachVerdict::Refused("the lane is not detached".to_owned())
    );
    task.abort();

    let rig = Rig::new(64);
    let (_engine, task, control) = detached_live_only(&rig).await;
    rig.sink.dead.lock().unwrap().insert("ensure_index");
    assert!(matches!(
        verdict(&control).await,
        crate::ReattachVerdict::Failed(_)
    ));
    rig.sink.dead.lock().unwrap().clear();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .get_mut("users")
        .unwrap()
        .marker = Some(MarkerAt::Newer { seeded: false });
    let crate::ReattachVerdict::Refused(why) = verdict(&control).await else {
        panic!("records that don't allow it refuse");
    };
    assert!(why.contains("restart flusso"), "{why}");
    rig.sink
        .kept
        .lock()
        .unwrap()
        .get_mut("users")
        .unwrap()
        .marker = Some(MarkerAt::Serving { seeded: true });
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    task.abort();
}

/// Review of step 4, S4(b): a `detached` record written while the sink could
/// rebuild, found by a start on a sink now `backfill = false`, is put back
/// to detached live only (R14) with its marker on the generation reads see:
/// a reattach then applies.
#[tokio::test(start_paused = true)]
async fn a_detached_record_on_a_sink_switched_to_backfill_false_is_put_back_live_only() {
    let rig = Rig::new(64);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(
        *rig.sink.demoted.lock().unwrap(),
        [("users".to_owned(), true)]
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    let (task, control) = spawn(&engine);
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    task.abort();
}

/// Review of step 4, S5: the slot can open again while a turn awaits (here a
/// reindex's staging of another index), and another process may have
/// detached a lane meanwhile. What the turn then takes was published after
/// the opening: it is read again before anything is written.
#[tokio::test(start_paused = true)]
async fn a_slot_reopened_while_a_reindex_stages_is_read_before_the_items_it_brought() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *rig.sink.ensure_gate.lock().unwrap() = Some(Arc::clone(&gate));
    let ensured = rig.sink.ensured.lock().unwrap().len();
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        rig.sink.ensured.lock().unwrap().len(),
        ensured,
        "the reindex waits in ensure_index"
    );
    rig.rereads.live_opened();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    rig.live(&users(), 1, 1).await;
    *rig.sink.ensure_gate.lock().unwrap() = None;
    gate.add_permits(1);
    eventually("the lane is taken as detached", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(written(&rig).is_empty(), "nothing of it is written");
    task.abort();
}
