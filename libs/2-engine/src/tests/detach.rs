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
        let mut report = WriteReport::clean();
        let mut indexes = Vec::new();
        let mut named = BTreeSet::new();
        for envelope in parts.iter().flat_map(|part| part.iter()) {
            indexes.push(envelope.index.to_string());
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

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    async fn detachment(&self, mapping: &IndexMapping) -> sink::Result<Detachment> {
        Ok(self
            .kept
            .lock()
            .unwrap()
            .get(mapping.index.as_ref())
            .cloned()
            .unwrap_or_default())
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
            Record::Kept => Ok(Recorded::Kept),
            Record::NotKept => Ok(Recorded::NotKept),
            Record::Fail => Err(SinkError::Write(
                "the meta index refused the record".to_owned(),
            )),
        }
    }

    async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> sink::Result<()> {
        self.forgotten
            .lock()
            .unwrap()
            .push(format!("{}@{}", mapping.index, version.as_str()));
        Ok(())
    }
}

/// Every lane health the observers heard, in order.
#[derive(Debug, Default)]
struct Heard {
    health: Mutex<Vec<(LaneKey, LaneHealth)>>,
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
    fn on_lane_health(&self, lane: &LaneKey, health: &LaneHealth) {
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

/// Two lanes, `orders` and `users`, of one sink `primary`.
struct Rig {
    stream: Arc<ChannelStream>,
    progress: Arc<Progress>,
    sink: Arc<Gate>,
    heard: Arc<Heard>,
}

impl Rig {
    fn new(capacity: usize) -> Self {
        let indexes = [orders(), users()];
        Self {
            stream: Arc::new(ChannelStream::over(
                capacity,
                indexes.clone(),
                [sink_name("primary")],
            )),
            progress: Arc::new(Progress::of(indexes, [sink_name("primary")])),
            sink: Arc::new(Gate::default()),
            heard: Arc::new(Heard::default()),
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
        let engine = SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&self.sink) as Arc<dyn Sink>,
            Arc::clone(&self.stream) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&self.progress),
        )
        .with_writes(4)
        .with_observer(Arc::clone(&self.heard) as Arc<dyn Observer>);
        engine.set_blame(Blame::Nobody);
        Arc::new(engine)
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
/// unwritten, so a send blocked on a full lane completes (R6).
#[tokio::test(start_paused = true)]
async fn a_detached_lane_is_drained() {
    let rig = Rig::new(2);
    let (_engine, task) = detached_on_a_block(&rig).await;
    let producer = rig.stream.lane(&lane(users())).unwrap().producer;
    let sent = tokio::time::timeout(Duration::from_secs(30), async {
        for position in 10..60 {
            producer
                .publish(item(&users(), i64::try_from(position).unwrap(), position))
                .await
                .unwrap();
        }
    })
    .await;
    assert!(
        sent.is_ok(),
        "fifty items through a lane of two: each was taken"
    );
    assert_eq!(rig.sink.writes_of(&users()).len(), 2);
    task.abort();
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

/// A reindex of a detached index is not staged: this step doesn't rebuild.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_a_detached_index_is_ignored() {
    let rig = Rig::new(64);
    let (engine, first) = detached_on_a_block(&rig).await;
    first.abort();
    let _ = first.await;
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.sink.ensured.lock().unwrap().is_empty());
    assert!(engine.lane_health(&lane(users())).is_detached());
    task.abort();
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
            Detachment::recorded(detached(false), elsewhere(false), version()),
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
            Detachment::recorded(detached(false), elsewhere(true), version()),
            true,
            false,
            false,
            true,
        ),
        (
            "rebuilding, finished",
            Detachment::recorded(rebuilding.clone(), elsewhere(true), version()),
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
            "unreadable",
            Detachment::recorded(
                DetachRecord::Unreadable {
                    reason: "a newer state".to_owned(),
                },
                elsewhere(false),
                version(),
            ),
            true,
            false,
            false,
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
