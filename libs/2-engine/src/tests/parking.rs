//! A failure about one index parks its lane, not the sink (ADR 0012 stage 9):
//! a refusal under `stop` or a write block parks the lane and probes its front
//! item with a backoff while the other lanes keep writing; a missing target
//! stages that one index again. All on paused time.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, Position};
use sink::{RejectedDocument, Sink, SinkError, WriteReport};
use source::cdc::Landed;
use stream::{Batch, LaneItem, LaneKey, Request, Stream};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::sink_name;
use crate::sink_engine::Blame;
use crate::{
    FailurePolicies, FailurePolicy, LaneHealth, Level, Observer, ParkReason, ProbeOutcome,
    Progress, SinkControl, SinkEngine, Stop,
};

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

/// What the sink answers for an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    Refuse,
    Block,
    Missing,
    MissingForGood,
}

/// One write as the sink got it: when, and each part's index and ids.
type Write = (Instant, Vec<(String, Vec<String>)>);

/// A sink answering each index as told: refused, blocked, missing (until the
/// index is ensured again), or landed. `fail` fails every write as a whole.
#[derive(Debug, Default)]
struct Gate {
    answers: Mutex<BTreeMap<String, Answer>>,
    fail: std::sync::atomic::AtomicBool,
    ordered: bool,
    writes: Mutex<Vec<Write>>,
    ensured: Mutex<Vec<String>>,
    /// Indexes recreated empty by `ensure_index`: unseeded from then on.
    unseeded: Mutex<BTreeSet<String>>,
    /// When each `ensure_index` ran.
    ensured_at: Mutex<Vec<Instant>>,
    /// A write holding this id takes 5 s.
    slow_id: Mutex<Option<String>>,
    /// Ids whose upsert is refused, whatever their index answers: a row a
    /// fix makes indexable again (stage 9, fresh builds).
    refused_ids: Mutex<BTreeSet<String>>,
    /// Indexes whose walk checkpoint was forgotten.
    forgotten: Mutex<Vec<String>>,
    /// `ensure_index` fails (a cluster-wide block).
    ensure_fails: std::sync::atomic::AtomicBool,
    /// `ensure_index` fails for these indexes only.
    ensure_fails_for: Mutex<BTreeSet<String>>,
    /// `forget_walked` fails (its write refused by a cluster-wide block).
    forget_fails: std::sync::atomic::AtomicBool,
    /// Each `ensure_index` takes this long.
    ensure_delay: Mutex<Option<Duration>>,
    /// Review r2: a write holding one of these ids takes that long.
    slow_ids: Mutex<BTreeMap<String, Duration>>,
    /// Review r2: a write holding this id fails as a whole.
    fail_id: Mutex<Option<String>>,
    /// Review r2: writes report `Within(1 s)`; a caught-up empty write then
    /// takes 5 s and fails while `refresh_fails` is set.
    within: std::sync::atomic::AtomicBool,
    refresh_fails: std::sync::atomic::AtomicBool,
}

impl Gate {
    fn answer(&self, index: &IndexName, answer: Option<Answer>) {
        let mut answers = self.answers.lock().unwrap();
        match answer {
            Some(answer) => answers.insert(index.to_string(), answer),
            None => answers.remove(index.as_ref()),
        };
    }

    fn writes(&self) -> Vec<Write> {
        self.writes.lock().unwrap().clone()
    }

    /// The times of the writes holding `of`, from `start`.
    fn writes_of(&self, of: &IndexName, start: Instant) -> Vec<Duration> {
        self.writes()
            .iter()
            .filter(|(_, parts)| parts.iter().any(|(at, _)| at == of.as_ref()))
            .map(|(when, _)| *when - start)
            .collect()
    }
}

#[async_trait]
impl Sink for Gate {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        if self.ensure_fails.load(std::sync::atomic::Ordering::SeqCst)
            || self
                .ensure_fails_for
                .lock()
                .unwrap()
                .contains(mapping.index.as_ref())
        {
            return Err(SinkError::Write(
                "cluster_block_exception: [FORBIDDEN/12/index read-only / allow delete (api)]"
                    .to_owned(),
            ));
        }
        let delay = *self.ensure_delay.lock().unwrap();
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        let name = mapping.index.to_string();
        self.ensured.lock().unwrap().push(name.clone());
        self.ensured_at.lock().unwrap().push(Instant::now());
        let mut answers = self.answers.lock().unwrap();
        if answers.get(&name) == Some(&Answer::Missing) {
            answers.remove(&name);
            self.unseeded.lock().unwrap().insert(name);
            return Ok(sink::Ensured::created(None));
        }
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
        if parts.iter().all(|part| part.is_empty()) {
            if self.refresh_fails.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_secs(5)).await;
                return Err(SinkError::Write("refresh: connection reset".to_owned()));
            }
            return Ok(WriteReport::clean());
        }
        let started = Instant::now();
        let slow = self.slow_id.lock().unwrap().clone();
        if let Some(slow) = slow
            && parts.iter().any(|part| part.iter().any(|e| e.id == slow))
        {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        let slow_ids = self.slow_ids.lock().unwrap().clone();
        if let Some(wait) = parts
            .iter()
            .flat_map(|part| part.iter())
            .filter_map(|e| slow_ids.get(&e.id))
            .max()
        {
            tokio::time::sleep(*wait).await;
        }
        let mut write = Vec::new();
        let mut report = WriteReport::clean();
        let answers = self.answers.lock().unwrap().clone();
        let mut named: BTreeSet<IndexName> = BTreeSet::new();
        for part in parts {
            let Some(first) = part.first() else { continue };
            write.push((
                first.index.to_string(),
                part.iter().map(|e| e.id.clone()).collect(),
            ));
            let refused = self.refused_ids.lock().unwrap().clone();
            report = report.rejecting(
                part.iter()
                    .filter(|e| e.op == kernel::Op::Upsert && refused.contains(&e.id))
                    .map(|e| RejectedDocument {
                        index: e.index.to_string(),
                        id: e.id.clone(),
                        reason: "mapper_parsing_exception".to_owned(),
                    }),
            );
            match answers.get(first.index.as_ref()) {
                Some(Answer::Refuse) => {
                    report = report.rejecting(part.iter().map(|e| RejectedDocument {
                        index: e.index.to_string(),
                        id: e.id.clone(),
                        reason: "mapper_parsing_exception".to_owned(),
                    }));
                }
                Some(Answer::Block) if named.insert(first.index.clone()) => {
                    report = report.blocking(
                        first.index.clone(),
                        format!(
                            "index [{}] blocked by: [FORBIDDEN/8/index write (api)]",
                            first.index
                        ),
                    );
                }
                Some(Answer::Missing | Answer::MissingForGood)
                    if named.insert(first.index.clone()) =>
                {
                    report = report.missing_target(
                        first.index.clone(),
                        format!("no such index [{}]", first.index),
                    );
                }
                _ => {}
            }
        }
        self.writes.lock().unwrap().push((started, write));
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SinkError::Write("connection refused".to_owned()));
        }
        let fail_id = self.fail_id.lock().unwrap().clone();
        if let Some(fail_id) = fail_id
            && parts
                .iter()
                .any(|part| part.iter().any(|e| e.id == fail_id))
        {
            return Err(SinkError::Write("connection reset".to_owned()));
        }
        if self.within.load(std::sync::atomic::Ordering::SeqCst) {
            report = report.visible(sink::Visibility::Within(Duration::from_secs(1)));
        }
        Ok(report)
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(!self.unseeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn forget_walked(&self, index: &IndexName) -> sink::Result<()> {
        if self.forget_fails.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SinkError::Write(
                "cluster_block_exception: [FORBIDDEN/12/index read-only / allow delete (api)]"
                    .to_owned(),
            ));
        }
        self.forgotten.lock().unwrap().push(index.to_string());
        Ok(())
    }

    fn ordered(&self) -> bool {
        self.ordered
    }
}

/// Records lane health changes, probes and quarantines.
#[derive(Debug, Default)]
struct Health {
    health: Mutex<Vec<(LaneKey, LaneHealth)>>,
    probes: Mutex<Vec<(LaneKey, ProbeOutcome)>>,
    quarantined: Mutex<Vec<String>>,
}

impl Health {
    fn kinds(&self, of: &LaneKey) -> Vec<&'static str> {
        self.health
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| at == of)
            .map(|(_, health)| health.kind())
            .collect()
    }

    fn probes(&self, of: &LaneKey) -> Vec<ProbeOutcome> {
        self.probes
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| at == of)
            .map(|(_, outcome)| *outcome)
            .collect()
    }
}

impl Observer for Health {
    fn on_lane_health(&self, lane: &LaneKey, health: &LaneHealth) {
        self.health
            .lock()
            .unwrap()
            .push((lane.clone(), health.clone()));
    }

    fn on_lane_probe(&self, lane: &LaneKey, outcome: ProbeOutcome) {
        self.probes.lock().unwrap().push((lane.clone(), outcome));
    }

    fn on_document_quarantined(&self, _: &kernel::SinkName, _: &str, id: &str, _: &str) {
        self.quarantined.lock().unwrap().push(id.to_owned());
    }
}

fn item(of: &IndexName, ids: std::ops::Range<i64>, position: Option<u64>) -> LaneItem {
    LaneItem::Batch(Batch {
        key: position.map(stream::ItemKey::new),
        position: position.map(Position::new),
        envelopes: ids
            .map(|id| {
                Envelope::upsert(
                    of.clone(),
                    id.to_string(),
                    GenericValue::Map(BTreeMap::new()),
                    position.map(Position::new),
                    chrono::Utc::now(),
                )
            })
            .collect::<Vec<_>>()
            .into(),
        hold: Default::default(),
        walked: None,
        ticket: None,
        ..Default::default()
    })
}

/// Two lanes, `orders` and `users`, of one sink `primary`.
struct Rig {
    stream: Arc<ChannelStream>,
    progress: Arc<Progress>,
    sink: Arc<Gate>,
    observer: Arc<Health>,
}

impl Rig {
    fn new(sink: Gate) -> Self {
        let indexes = [orders(), users()];
        Self {
            stream: Arc::new(ChannelStream::over(
                1024,
                indexes.clone(),
                [sink_name("primary")],
            )),
            progress: Arc::new(Progress::of(indexes, [sink_name("primary")])),
            sink: Arc::new(sink),
            observer: Arc::new(Health::default()),
        }
    }

    /// A live item of `of` at `position`, registered as the ingest engine
    /// would.
    async fn live(&self, of: &IndexName, ids: std::ops::Range<i64>, position: u64) {
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
            .publish(item(of, ids, Some(position)))
            .await
            .unwrap();
    }

    /// An engine past its first landing, `stop` on every index.
    fn engine(&self, policy: FailurePolicy) -> SinkEngine {
        let engine = SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&self.sink) as Arc<dyn Sink>,
            Arc::clone(&self.stream) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&self.progress),
        )
        .with_writes(4)
        .with_failure_policies(FailurePolicies::new(policy))
        .with_observer(Arc::clone(&self.observer) as Arc<dyn Observer>);
        engine.set_blame(Blame::Nobody);
        engine
    }

    fn applied(&self, of: &IndexName) -> Option<Landed> {
        self.progress.lane(&lane(of.clone()), Level::Applied)
    }
}

fn spawn(engine: Arc<SinkEngine>) -> tokio::task::JoinHandle<crate::Result<()>> {
    tokio::spawn(async move {
        let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
        engine.run(&mut control).await
    })
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

fn before(position: u64) -> Option<Landed> {
    Some(Landed::Before(Position::new(position)))
}

/// A refusal under `stop` parks its own lane: the run goes on, the sibling
/// lane keeps writing and its positions keep being applied, and the parked
/// lane holds its marks and the source's resume point.
#[tokio::test(start_paused = true)]
async fn a_refusal_parks_only_its_lane_and_the_sibling_keeps_landing() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 0..2, 0).await;
    rig.live(&users(), 0..2, 1).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));

    eventually("users lands beside the refusal", || {
        rig.applied(&users()) == through(1)
    })
    .await;
    rig.live(&users(), 2..4, 2).await;
    eventually("users keeps landing", || {
        rig.applied(&users()) == through(2)
    })
    .await;
    assert!(!task.is_finished(), "a refusal no longer ends the run");

    assert_eq!(rig.applied(&orders()), before(0), "the parked lane holds");
    assert_eq!(
        rig.progress.landed(),
        Landed::Before(Position::new(0)),
        "and so holds the source's resume point"
    );
    assert_eq!(
        rig.progress.pinned_by(),
        Some(crate::Pin::Lane(lane(orders())))
    );
    match engine.lane_health(&lane(orders())) {
        LaneHealth::Parked {
            reason: ParkReason::Refused { ids, .. },
            ..
        } => assert_eq!(ids, ["0", "1"]),
        other => panic!("orders should be parked on its refusal, is {other:?}"),
    }
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(rig.observer.kinds(&lane(orders())), ["refused"]);
    task.abort();
}

/// A parked lane's front item is written again alone, at 1 s, then with the
/// backoff doubling up to the engine's maximum, the same documents each time;
/// the sibling lane landing in between never resets it.
#[tokio::test(start_paused = true)]
async fn a_parked_lane_probes_its_front_item_alone_with_a_backoff_no_sibling_resets() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    let start = Instant::now();
    rig.live(&orders(), 0..2, 0).await;
    rig.live(&orders(), 5..7, 1).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(
        crate::Backoff::new(Duration::from_secs(1), Duration::from_secs(4)).unwrap(),
    ));
    let task = spawn(Arc::clone(&engine));
    for (at, position) in (0..24).zip(2..) {
        tokio::time::sleep_until(start + Duration::from_millis(500) * at).await;
        rig.live(&users(), at.into()..i64::from(at) + 1, position)
            .await;
    }
    tokio::time::sleep_until(start + Duration::from_millis(12_500)).await;

    let probes: Vec<u128> = rig
        .sink
        .writes_of(&orders(), start)
        .iter()
        .map(Duration::as_millis)
        .collect();
    assert_eq!(
        probes,
        [0, 1_000, 3_000, 7_000, 11_000],
        "the first write, then probes at 1, 2, 4 and 4 s (the maximum)"
    );
    for (_, parts) in rig
        .sink
        .writes()
        .iter()
        .filter(|(_, parts)| parts.iter().any(|(at, _)| at == "orders"))
        .skip(1)
    {
        assert_eq!(
            parts,
            &[("orders".to_owned(), vec!["0".to_owned(), "1".to_owned()])],
            "each probe is the parked item, alone"
        );
    }
    assert_eq!(rig.applied(&users()), through(25), "users landed meanwhile");
    assert_eq!(
        rig.observer.probes(&lane(orders())),
        [ProbeOutcome::Refused; 4]
    );
    task.abort();
}

/// A write block parks the lane as blocked whatever `on_error` says, and
/// quarantines nothing (#404): lifted, the next probe lands the same item,
/// the lane opens and its later items follow, with no restart.
#[tokio::test(start_paused = true)]
async fn a_lifted_block_opens_the_lane_and_a_block_loses_nothing_under_either_policy() {
    for policy in [FailurePolicy::Skip, FailurePolicy::Stop] {
        let rig = Rig::new(Gate::default());
        rig.sink.answer(&orders(), Some(Answer::Block));
        rig.live(&orders(), 0..2, 0).await;
        rig.live(&orders(), 2..4, 1).await;
        rig.live(&users(), 0..2, 2).await;
        let engine = Arc::new(rig.engine(policy));
        let task = spawn(Arc::clone(&engine));

        eventually("users lands beside the block", || {
            rig.applied(&users()) == through(2)
        })
        .await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(rig.applied(&orders()), before(0), "{policy:?}");
        assert!(
            rig.observer.quarantined.lock().unwrap().is_empty(),
            "{policy:?}: a blocked document is never quarantined"
        );
        assert!(
            matches!(
                engine.lane_health(&lane(orders())),
                LaneHealth::Parked {
                    reason: ParkReason::Blocked { ref reason },
                    ..
                } if reason.contains("FORBIDDEN/8")
            ),
            "{policy:?}: {:?}",
            engine.lane_health(&lane(orders()))
        );

        rig.sink.answer(&orders(), None);
        eventually("orders catches up once the block is lifted", || {
            rig.applied(&orders()) == through(1)
        })
        .await;
        assert!(engine.lane_health(&lane(orders())).is_open());
        assert_eq!(
            rig.observer.kinds(&lane(orders())).last(),
            Some(&"open"),
            "{policy:?}"
        );
        assert_eq!(
            rig.observer.probes(&lane(orders())).last(),
            Some(&ProbeOutcome::Landed)
        );
        assert!(rig.observer.quarantined.lock().unwrap().is_empty());
        assert!(!task.is_finished());
        task.abort();
    }
}

/// A write failing as a whole is still the sink's failure: the run ends.
#[tokio::test(start_paused = true)]
async fn a_write_failing_as_a_whole_still_fails_the_sink() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.live(&orders(), 0..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let result = spawn(Arc::clone(&engine)).await.unwrap();
    assert!(result.is_err(), "a write failing as a whole ends the run");
    assert!(engine.lane_health(&lane(orders())).is_open());
}

/// A target gone mid-run stages that one index again (`ensure_index` for it
/// alone, a backfill requested for it), then writes the item again; the
/// sibling lane is untouched and the run goes on.
#[tokio::test(start_paused = true)]
async fn a_missing_target_restages_its_lane_alone() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Missing));
    let mut requests = rig.stream.requests().unwrap();
    rig.live(&orders(), 0..2, 0).await;
    rig.live(&users(), 0..2, 1).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));

    eventually("orders is written again once staged", || {
        rig.applied(&orders()) == through(0)
    })
    .await;
    assert_eq!(rig.applied(&users()), through(1));
    assert_eq!(*rig.sink.ensured.lock().unwrap(), ["orders"]);
    let request = tokio::time::timeout(Duration::from_secs(1), requests.consumer.recv())
        .await
        .expect("a backfill is requested")
        .unwrap()
        .unwrap();
    let (request, _ack) = request.into_parts();
    assert!(
        matches!(&request, Request::Backfill { index, .. } if *index == orders()),
        "{request:?}"
    );
    let orders_writes = rig
        .sink
        .writes_of(&orders(), Instant::now() - Duration::from_secs(3600));
    assert_eq!(
        orders_writes.len(),
        2,
        "the missing write, then once staged"
    );
    assert_eq!(rig.observer.kinds(&lane(orders())), ["restaging", "open"]);
    assert!(!task.is_finished());
    task.abort();
}

/// One refused part of a merged write parks its lane only; the other parts
/// land and are acknowledged.
#[tokio::test(start_paused = true)]
async fn a_refused_part_of_a_merged_write_parks_only_its_lane() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&users(), 0..2, 0).await;
    rig.live(&orders(), 0..2, 1).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_writes(1));
    let task = spawn(Arc::clone(&engine));
    eventually("users lands", || rig.applied(&users()) == through(0)).await;
    let first = rig.sink.writes().first().cloned().unwrap();
    assert_eq!(first.1.len(), 2, "one merged write: {first:?}");
    assert_eq!(rig.applied(&orders()), before(1));
    assert!(!engine.lane_health(&lane(orders())).is_open());
    task.abort();
}

/// With one write at a time, and on an ordered sink, a parked lane takes no
/// slot: the other lane keeps writing.
#[tokio::test(start_paused = true)]
async fn one_write_at_a_time_and_an_ordered_sink_keep_writing_other_lanes() {
    for ordered in [false, true] {
        let rig = Rig::new(Gate {
            ordered,
            ..Gate::default()
        });
        rig.sink.answer(&orders(), Some(Answer::Block));
        rig.live(&orders(), 0..2, 0).await;
        let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_writes(1));
        let task = spawn(Arc::clone(&engine));
        for position in 1..6 {
            rig.live(&users(), 0..1, position).await;
            eventually("users keeps landing", || {
                rig.applied(&users()) == through(position)
            })
            .await;
        }
        assert_eq!(rig.applied(&orders()), before(0), "ordered: {ordered}");
        task.abort();
    }
}

/// A stop doesn't wait out a parked lane's backoff: one probe goes at once,
/// so a block lifted meanwhile still drains.
#[tokio::test(start_paused = true)]
async fn a_stop_probes_a_parked_lane_at_once() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Block));
    let start = Instant::now();
    rig.live(&orders(), 0..2, 0).await;
    let stop = Stop::new();
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_stop(stop.clone()));
    let task = spawn(Arc::clone(&engine));
    // Probes at 1, 3 and 7 s; the next is due at 15 s.
    tokio::time::sleep_until(start + Duration::from_millis(8_000)).await;
    assert!(!engine.lane_health(&lane(orders())).is_open());
    rig.sink.answer(&orders(), None);
    stop.request();
    eventually("the parked lane drains on the stop", || {
        rig.applied(&orders()) == through(0)
    })
    .await;
    assert!(
        Instant::now() < start + Duration::from_secs(9),
        "the stop's probe went at once, not at 15 s"
    );
    eventually("the lane is acknowledged", || rig.stream.is_idle()).await;
    task.abort();
}

/// A stop with a lane still blocked drains everything else; the blocked lane
/// keeps its item unacknowledged and holds the resume point, so the drain
/// ends only at the caller's deadline.
#[tokio::test(start_paused = true)]
async fn a_stop_with_a_lane_still_blocked_drains_the_rest_and_holds_that_lane() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Block));
    rig.live(&orders(), 0..2, 0).await;
    rig.live(&users(), 0..2, 1).await;
    let stop = Stop::new();
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_stop(stop.clone()));
    let task = spawn(Arc::clone(&engine));
    eventually("users lands", || rig.applied(&users()) == through(1)).await;
    stop.request();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(
        !rig.stream.is_idle(),
        "the blocked item is never acknowledged"
    );
    assert_eq!(rig.progress.landed(), Landed::Before(Position::new(0)));
    assert!(!task.is_finished(), "the deadline is the caller's");
    task.abort();
}

/// A lane parked when the run fails for another reason stays parked in the
/// next run, on its schedule: its item is not written at the restart.
#[tokio::test(start_paused = true)]
async fn a_parked_lane_stays_parked_across_a_failed_run() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Block));
    let start = Instant::now();
    rig.live(&orders(), 0..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(
        crate::Backoff::new(Duration::from_secs(1), Duration::from_secs(30)).unwrap(),
    ));
    let task = spawn(Arc::clone(&engine));
    // Probes at 1, 3 and 7 s; the next is due at 15 s.
    tokio::time::sleep_until(start + Duration::from_millis(8_000)).await;
    rig.sink
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.live(&users(), 0..1, 1).await;
    assert!(
        task.await.unwrap().is_err(),
        "the users write fails the run"
    );
    rig.sink
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let restarted = Instant::now();
    let task = spawn(Arc::clone(&engine));
    eventually("users lands on the next run", || {
        rig.applied(&users()) == through(1)
    })
    .await;
    tokio::time::sleep_until(start + Duration::from_millis(14_000)).await;
    assert!(
        rig.sink
            .writes_of(&orders(), start)
            .iter()
            .all(|at| start + *at < restarted),
        "the parked lane waits for its own schedule: {:?}",
        rig.sink.writes_of(&orders(), start)
    );
    assert!(!engine.lane_health(&lane(orders())).is_open());
    task.abort();
}

/// A target staging can't bring back (an ingest pipeline routing to an index
/// that auto-create may not make): the lane is staged again with a backoff,
/// 1 s doubling, kept once it reopens until one of its writes lands, never in
/// a loop of writes and stagings.
#[tokio::test(start_paused = true)]
async fn a_missing_target_staging_cannot_cure_is_retried_with_a_backoff() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::MissingForGood));
    rig.live(&orders(), 0..2, 0).await;
    rig.live(&users(), 0..2, 1).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    let start = Instant::now();
    for n in 0..1000u64 {
        let id = i64::try_from(n).unwrap() + 10;
        rig.live(&users(), id..id + 1, 2 + n).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let writes = rig
        .sink
        .writes_of(&orders(), start - Duration::from_secs(1))
        .len();
    let ensures = rig.sink.ensured.lock().unwrap().len();
    assert!(
        writes < 10 && ensures < 10,
        "in 10 s: {writes} writes of orders, {ensures} ensure_index calls"
    );
    assert_eq!(rig.applied(&users()), through(1001), "users keeps landing");
    task.abort();
}

/// A lane's index is staged again only once nothing of the lane is in
/// flight: a slow write of the lane started before the target went missing
/// returns first (a write never holds a document of an index being staged).
#[tokio::test(start_paused = true)]
async fn restaging_waits_for_the_lanes_writes_in_flight() {
    let rig = Rig::new(Gate::default());
    *rig.sink.slow_id.lock().unwrap() = Some("9".to_owned());
    rig.sink.answer(&orders(), Some(Answer::Missing));
    let start = Instant::now();
    rig.live(&orders(), 9..10, 0).await;
    rig.live(&orders(), 0..1, 1).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_write_documents(1));
    let task = spawn(Arc::clone(&engine));
    eventually("orders lands", || rig.applied(&orders()) == through(1)).await;
    let ensured: Vec<Duration> = rig
        .sink
        .ensured_at
        .lock()
        .unwrap()
        .iter()
        .map(|at| *at - start)
        .collect();
    assert!(!ensured.is_empty());
    assert!(
        ensured.iter().all(|at| *at >= Duration::from_secs(5)),
        "staged while the slow write was in flight: {ensured:?}"
    );
    task.abort();
}

/// A parked lane's items neither hold nor end another lane's merge wait: with
/// a backfill item waiting on a blocked lane, a live item ready while a write
/// of an open lane is out still waits for it (up to the merge wait), as it
/// would with no lane parked.
#[tokio::test(start_paused = true)]
async fn a_parked_lanes_waiting_backfill_does_not_end_the_merge_wait() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Block));
    *rig.sink.slow_id.lock().unwrap() = Some("9".to_owned());
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(item(&orders(), 0..2, None))
        .await
        .unwrap();
    rig.live(&users(), 9..10, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("orders parks", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    let ready = Instant::now();
    rig.live(&users(), 1..2, 1).await;
    eventually("users 1 is written", || {
        rig.sink
            .writes()
            .iter()
            .any(|(_, parts)| parts.iter().any(|(_, ids)| ids == &["1".to_owned()]))
    })
    .await;
    let started = rig
        .sink
        .writes()
        .iter()
        .find(|(_, parts)| parts.iter().any(|(_, ids)| ids == &["1".to_owned()]))
        .map(|(at, _)| *at)
        .unwrap();
    assert!(
        started >= ready + Duration::from_millis(40),
        "users 1 waited for the write out, as with no lane parked: started {:?} after it was ready",
        started - ready
    );
    task.abort();
}

/// A backfill item refused under `stop` parks its lane like a live one: it
/// carries no position, so the lane gets no stall age, and the lane's health
/// (`flusso.lane.parked`, `FlussoLaneParked`) is what says so.
#[tokio::test(start_paused = true)]
async fn a_refused_backfill_item_parks_its_lane() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(item(&orders(), 0..2, None))
        .await
        .unwrap();
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(engine.lane_health(&lane(orders())).kind(), "refused");
    assert_eq!(
        rig.observer.kinds(&lane(orders())).first(),
        Some(&"refused")
    );
    assert!(
        !rig.stream.is_idle(),
        "the refused page stays unacknowledged"
    );
    task.abort();
}

/// A parked lane's `SnapshotComplete` still runs once nothing is in front
/// of it: a stop acknowledges the refused page unwritten, and the completion
/// behind it is acknowledged (seeding nothing, as a cut-short backfill), so
/// the lane drains. Only a lane being staged again holds its completion.
#[tokio::test(start_paused = true)]
async fn a_parked_lanes_completion_runs_once_a_stop_skipped_the_page_in_front() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    let producer = rig.stream.lane(&lane(orders())).unwrap().producer;
    producer.publish(item(&orders(), 0..2, None)).await.unwrap();
    producer
        .publish(LaneItem::SnapshotComplete {
            index: orders(),
            ticket: stream::Ticket::new(1),
        })
        .await
        .unwrap();
    let stop = Stop::new();
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_stop(stop.clone()));
    let task = spawn(Arc::clone(&engine));
    eventually("the lane parks on the refusal", || {
        engine.lane_health(&lane(orders())).kind() == "refused"
    })
    .await;
    stop.request();
    eventually("the parked lane's completion is acknowledged", || {
        rig.stream.is_idle()
    })
    .await;
    task.abort();
}

/// A lane staged again whose write then lands is cured: when its target goes
/// missing again later, it is staged again at once, not after the backoff it
/// kept while unproven.
#[tokio::test(start_paused = true)]
async fn a_lane_whose_write_landed_after_staging_is_staged_at_once_next_time() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Missing));
    rig.live(&orders(), 0..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("orders is staged and lands", || {
        rig.applied(&orders()) == through(0)
    })
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    rig.sink.answer(&orders(), Some(Answer::Missing));
    let missing_again = Instant::now();
    rig.live(&orders(), 2..4, 1).await;
    eventually("orders is staged again and lands", || {
        rig.applied(&orders()) == through(1)
    })
    .await;
    let ensured = rig.sink.ensured_at.lock().unwrap().clone();
    assert_eq!(ensured.len(), 2, "two stagings");
    let waited = ensured[1] - missing_again;
    assert!(
        waited < Duration::from_millis(500),
        "a cured lane is staged again at once, not after a backoff: {waited:?}"
    );
    task.abort();
}

fn configured() -> crate::Backoff {
    crate::Backoff::new(Duration::from_millis(200), Duration::from_millis(800)).unwrap()
}

/// A configured schedule is what a parked lane's probes wait: 200 ms
/// doubling to 800 ms.
#[tokio::test(start_paused = true)]
async fn a_parked_lane_probes_on_the_configured_schedule() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    let start = Instant::now();
    rig.live(&orders(), 0..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(configured()));
    let task = spawn(Arc::clone(&engine));
    tokio::time::sleep_until(start + Duration::from_millis(3_100)).await;

    let probes: Vec<u128> = rig
        .sink
        .writes_of(&orders(), start)
        .iter()
        .map(Duration::as_millis)
        .collect();
    assert_eq!(probes, [0, 200, 600, 1_400, 2_200, 3_000]);
    task.abort();
}

/// A configured schedule is what a re-staged lane waits between stagings
/// while its target stays gone: 200 ms doubling to 800 ms, counted from the
/// write that found it gone again.
#[tokio::test(start_paused = true)]
async fn a_restaged_lane_retries_on_the_configured_schedule() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::MissingForGood));
    let start = Instant::now();
    rig.live(&orders(), 0..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(configured()));
    let task = spawn(Arc::clone(&engine));
    tokio::time::sleep_until(start + Duration::from_millis(4_500)).await;

    let ensured: Vec<u128> = rig
        .sink
        .ensured_at
        .lock()
        .unwrap()
        .iter()
        .map(|at| (*at - start).as_millis())
        .collect();
    let writes: Vec<u128> = rig
        .sink
        .writes_of(&orders(), start)
        .iter()
        .map(Duration::as_millis)
        .collect();
    let waits: Vec<u128> = ensured
        .iter()
        .skip(1)
        .filter_map(|staged| {
            let found_missing = writes.iter().rev().find(|at| *at < staged)?;
            Some(staged - found_missing)
        })
        .collect();
    assert_eq!(
        waits,
        [200, 400, 800, 800, 800],
        "each staging after the write that found the target gone: stagings at {ensured:?}, writes at {writes:?}"
    );
    task.abort();
}

/// A schedule as long as a configured `u64::MAX` ms parks the lane once:
/// nothing panics, the parked item is not written again, and the sibling
/// lane keeps landing.
#[tokio::test(start_paused = true)]
async fn a_lane_parked_on_the_longest_schedule_neither_panics_nor_spins() {
    let longest = Duration::from_millis(u64::MAX);
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Block));
    let start = Instant::now();
    rig.live(&orders(), 0..2, 0).await;
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(crate::Backoff::new(longest, longest).unwrap()),
    );
    let task = spawn(Arc::clone(&engine));
    for at in 0..10u32 {
        tokio::time::sleep_until(start + Duration::from_secs(at.into())).await;
        rig.live(&users(), at.into()..i64::from(at) + 1, u64::from(at) + 1)
            .await;
    }
    tokio::time::sleep_until(start + Duration::from_secs(30)).await;

    assert_eq!(rig.sink.writes_of(&orders(), start).len(), 1);
    assert_eq!(rig.applied(&users()), through(10));
    assert!(!engine.lane_health(&lane(orders())).is_open());
    assert!(!task.is_finished(), "the run goes on");
    task.abort();
}

// Fresh builds (ADR 0012 stage 9, second half, §14.1 of the note): a lane
// parked on a refusal asks its index's builder for a fresh build of its item's
// documents, and writes it in the item's place. `Builder` stands in for the
// ingest engine's builders: it answers each `Rebuild` on the substitute lane.

/// An item of `of` whose envelopes carry their keys, numbered `build`.
fn keyed(of: &IndexName, ids: &[i64], position: Option<u64>, build: u64) -> LaneItem {
    LaneItem::Batch(Batch {
        key: position.map(stream::ItemKey::new),
        position: position.map(Position::new),
        envelopes: ids
            .iter()
            .map(|id| {
                Envelope::upsert(
                    of.clone(),
                    id.to_string(),
                    GenericValue::Map(BTreeMap::new()),
                    position.map(Position::new),
                    chrono::Utc::now(),
                )
            })
            .collect::<Vec<_>>()
            .into(),
        build: Some(build),
        ids: Some(Arc::new(
            ids.iter()
                .map(|id| kernel::DocumentId {
                    index: of.clone(),
                    key: key(*id),
                })
                .collect(),
        )),
        ..Default::default()
    })
}

fn key(id: i64) -> kernel::RowKey {
    kernel::RowKey(vec![(
        kernel::ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

fn id_of(key: &kernel::RowKey) -> i64 {
    match key.0.first() {
        Some((_, GenericValue::BigInt(id))) => *id,
        _ => -1,
    }
}

/// Answers each `Rebuild` with a substitute numbered from `build`: a delete
/// for a row in `deleted`, else an upsert. Records when each was asked.
#[derive(Debug, Default)]
struct Builder {
    asked: Mutex<Vec<(Instant, stream::Ticket, Vec<i64>)>>,
    deleted: Mutex<BTreeSet<i64>>,
    /// Hold every answer back.
    silent: std::sync::atomic::AtomicBool,
}

impl Builder {
    fn asked(&self) -> Vec<(Instant, stream::Ticket, Vec<i64>)> {
        self.asked.lock().unwrap().clone()
    }
}

fn answering(rig: &Rig, builder: Arc<Builder>, first_build: u64) -> tokio::task::JoinHandle<()> {
    let stream = Arc::clone(&rig.stream);
    tokio::spawn(async move {
        let mut requests = stream.requests().unwrap().consumer;
        let substitutes = stream.substitutes(&sink_name("primary")).unwrap().producer;
        let mut build = first_build;
        while let Ok(Some(delivery)) = requests.recv().await {
            let (request, handle) = delivery.into_parts();
            handle.ack().await.unwrap();
            let Request::Rebuild {
                index,
                sink,
                keys,
                ticket,
            } = request
            else {
                continue;
            };
            let ids: Vec<i64> = keys.iter().map(id_of).collect();
            builder
                .asked
                .lock()
                .unwrap()
                .push((Instant::now(), ticket, ids.clone()));
            if builder.silent.load(std::sync::atomic::Ordering::SeqCst) {
                continue;
            }
            let deleted = builder.deleted.lock().unwrap().clone();
            let envelopes: Vec<Envelope> = ids
                .iter()
                .map(|id| {
                    if deleted.contains(id) {
                        Envelope::delete(index.clone(), id.to_string(), None, chrono::Utc::now())
                    } else {
                        Envelope::upsert(
                            index.clone(),
                            id.to_string(),
                            GenericValue::Map(BTreeMap::new()),
                            None,
                            chrono::Utc::now(),
                        )
                    }
                })
                .collect();
            let built_ids = ids
                .iter()
                .map(|id| kernel::DocumentId {
                    index: index.clone(),
                    key: key(*id),
                })
                .collect();
            substitutes
                .publish(stream::Substitute {
                    lane: LaneKey::new(index, sink),
                    ticket,
                    batch: Batch {
                        envelopes: envelopes.into(),
                        build: Some(build),
                        ids: Some(Arc::new(built_ids)),
                        ..Default::default()
                    },
                })
                .await
                .unwrap();
            build += 1;
        }
    })
}

/// The ids of each write holding `of`, from `start`.
fn written(rig: &Rig, of: &IndexName) -> Vec<Vec<String>> {
    rig.sink
        .writes()
        .iter()
        .flat_map(|(_, parts)| parts.iter().filter(|(at, _)| at == of.as_ref()))
        .map(|(_, ids)| ids.clone())
        .collect()
}

/// F1, F5, F6, F12: a refused lane's due probe asks for a fresh build instead
/// of writing its item; the substitute is written in the item's place. Still
/// refused, the backoff doubles and the next probe asks again; once the row
/// is fixed, the substitute lands, the parked item is acknowledged (its
/// position applied, for a live item) and the lane opens. The same for a
/// refused backfill page.
#[tokio::test(start_paused = true)]
async fn a_fixed_row_unparks_its_lane() {
    for position in [Some(0), None] {
        let rig = Rig::new(Gate::default());
        rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
        let start = Instant::now();
        if position.is_some() {
            rig.progress.published(
                &lane(orders()),
                stream::ItemKey::new(0),
                Position::new(0),
                Position::new(0),
            );
        }
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(keyed(&orders(), &[0, 1], position, 1))
            .await
            .unwrap();
        let builder = Arc::new(Builder::default());
        let answers = answering(&rig, Arc::clone(&builder), 10);
        let engine = Arc::new(rig.engine(FailurePolicy::Stop));
        let task = spawn(Arc::clone(&engine));
        eventually("a second fresh build is asked", || {
            builder.asked().len() >= 2
        })
        .await;
        let asked: Vec<u128> = builder
            .asked()
            .iter()
            .map(|(at, _, _)| (*at - start).as_millis())
            .collect();
        assert_eq!(
            asked,
            [1_000, 3_000],
            "{position:?}: at 1 s, then 2 s later"
        );
        let asked_ids: Vec<Vec<i64>> = builder.asked().into_iter().map(|(_, _, ids)| ids).collect();
        assert_eq!(
            asked_ids,
            [vec![0, 1], vec![1]],
            "0 landed with the first substitute: the item is built again without it"
        );
        assert_eq!(engine.lane_health(&lane(orders())).kind(), "refused");
        rig.sink.refused_ids.lock().unwrap().clear();
        eventually("the fixed row unparks the lane", || {
            engine.lane_health(&lane(orders())).is_open()
        })
        .await;
        if position.is_some() {
            assert_eq!(rig.applied(&orders()), through(0));
        }
        eventually("the item and its substitutes are acknowledged", || {
            rig.stream.is_idle()
        })
        .await;
        assert_eq!(
            written(&rig, &orders()).len(),
            4,
            "{position:?}: the item, then one substitute per probe (1, 3 and 7 s)"
        );
        assert_eq!(
            rig.observer.probes(&lane(orders())),
            [
                ProbeOutcome::Refused,
                ProbeOutcome::Refused,
                ProbeOutcome::Landed
            ]
        );
        task.abort();
        answers.abort();
    }
}

/// F7: a row deleted meanwhile is fresh-built as a delete, which lands.
#[tokio::test(start_paused = true)]
async fn a_deleted_row_unparks_its_lane_with_a_delete() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    rig.progress.published(
        &lane(orders()),
        stream::ItemKey::new(0),
        Position::new(0),
        Position::new(0),
    );
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(keyed(&orders(), &[1], Some(0), 1))
        .await
        .unwrap();
    let builder = Arc::new(Builder::default());
    builder.deleted.lock().unwrap().insert(1);
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    eventually("the delete unparks the lane", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    assert_eq!(rig.applied(&orders()), through(0));
    assert_eq!(builder.asked().len(), 1);
    task.abort();
    answers.abort();
}

/// F8, F15: a substitute naming a ticket the lane doesn't wait for is
/// acknowledged and dropped, never written; a fresh build nobody answers is
/// waited for one backoff (at most `SUBSTITUTE_WAIT`), then the item itself
/// is the probe, and the next probe asks again.
#[tokio::test(start_paused = true)]
async fn a_stale_or_unanswered_fresh_build_falls_back_to_the_item() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    let start = Instant::now();
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(keyed(&orders(), &[1], None, 1))
        .await
        .unwrap();
    let builder = Arc::new(Builder::default());
    builder
        .silent
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    rig.stream
        .substitutes(&sink_name("primary"))
        .unwrap()
        .producer
        .publish(stream::Substitute {
            lane: lane(orders()),
            ticket: stream::Ticket::new(999),
            batch: Batch {
                envelopes: vec![Envelope::upsert(
                    orders(),
                    "99".to_owned(),
                    GenericValue::Map(BTreeMap::new()),
                    None,
                    chrono::Utc::now(),
                )]
                .into(),
                ..Batch::default()
            },
        })
        .await
        .unwrap();
    eventually("a second fresh build is asked", || {
        builder.asked().len() == 2
    })
    .await;
    let probes: Vec<u128> = rig
        .sink
        .writes_of(&orders(), start)
        .iter()
        .map(Duration::as_millis)
        .collect();
    assert_eq!(
        probes,
        [0, 2_000],
        "the item, then itself after the unanswered fresh build's 1 s"
    );
    assert!(
        written(&rig, &orders()).iter().all(|ids| ids == &["1"]),
        "the stale substitute is never written"
    );
    let asked: Vec<u128> = builder
        .asked()
        .iter()
        .map(|(at, _, _)| (*at - start).as_millis())
        .collect();
    assert_eq!(
        asked,
        [1_000, 4_000],
        "asked again after the doubled backoff"
    );
    assert_eq!(engine.lane_health(&lane(orders())).kind(), "refused");
    task.abort();
    answers.abort();
}

/// F9: once a substitute numbered 10 lands, an item behind the parked one
/// built before it (number 2) is written without the substitute's
/// documents, and one built after it (number 11) whole.
#[tokio::test(start_paused = true)]
async fn an_older_build_queued_behind_the_parked_item_is_not_written_over_its_substitute() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    for (ids, position, build) in [(&[1][..], 0, 1), (&[1, 2][..], 1, 2), (&[1, 3][..], 2, 11)] {
        rig.progress.published(
            &lane(orders()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(keyed(&orders(), ids, Some(position), build))
            .await
            .unwrap();
    }
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("every item lands", || rig.applied(&orders()) == through(2)).await;
    let writes = written(&rig, &orders());
    let strings = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
    assert!(
        writes.contains(&strings(&["2"])),
        "the older build of 1 is left out: {writes:?}"
    );
    assert!(
        !writes.contains(&strings(&["1", "2"])),
        "never written whole: {writes:?}"
    );
    assert!(
        writes.contains(&strings(&["1", "3"])),
        "the newer build is written whole: {writes:?}"
    );
    task.abort();
    answers.abort();
}

/// F13: a stop doesn't wait for a fresh build it asked for: the refused
/// lane's last probe writes its item as it is, at once.
#[tokio::test(start_paused = true)]
async fn a_stop_probes_a_refused_lane_with_its_own_item() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    rig.progress.published(
        &lane(orders()),
        stream::ItemKey::new(0),
        Position::new(0),
        Position::new(0),
    );
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(keyed(&orders(), &[1], Some(0), 1))
        .await
        .unwrap();
    let builder = Arc::new(Builder::default());
    builder
        .silent
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let stop = Stop::new();
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_stop(stop.clone()));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    rig.sink.refused_ids.lock().unwrap().clear();
    stop.request();
    eventually("the item itself lands on the stop", || {
        rig.applied(&orders()) == through(0)
    })
    .await;
    assert_eq!(
        builder.asked().len(),
        1,
        "no fresh build asked once stopping"
    );
    task.abort();
    answers.abort();
}

// Review of #458 (F9's record, B1 and B2).

/// B2: F9's record is dropped when a newer item is *received*, but an
/// older item received before it can still be redelivered whole after the run
/// fails: the older build of 1 (number 2) is then written over the substitute
/// (number 10).
#[tokio::test(start_paused = true)]
async fn a_redelivered_older_item_is_still_written_without_the_substitutes_documents() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    // Built before the substitute: 1 and 2, then a backlog past the window's
    // lookahead; built after it (number 2000): 3.
    let mut items: Vec<(Vec<i64>, u64)> = vec![(vec![1], 1), (vec![1, 2], 2)];
    for k in 0..40 {
        items.push((vec![100 + k], 3 + k as u64));
    }
    items.push((vec![3], 2000));
    let last = items.len() as u64 - 1;
    for (position, (ids, build)) in items.iter().enumerate() {
        let position = position as u64;
        rig.progress.published(
            &lane(orders()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(keyed(&orders(), ids, Some(position), *build))
            .await
            .unwrap();
    }
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 1000);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    // The item numbered 2 (now [2]) takes 5 s to write and then fails as a
    // whole; meanwhile the backlog drains and the item numbered 2000 is
    // received.
    *rig.sink.slow_id.lock().unwrap() = Some("2".to_owned());
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("the substitute unparks the lane", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed write");
    assert!(ended.unwrap().is_err());
    rig.sink
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    *rig.sink.slow_id.lock().unwrap() = None;
    // The daemon's restart: stage the same engine again, then run it again
    // (`supervise.rs`); its lanes' F9 maps live on it.
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let task = spawn(Arc::clone(&engine));
    eventually("every item lands after the restart", || {
        rig.applied(&orders()) == through(last)
    })
    .await;
    let writes = written(&rig, &orders());
    let strings = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();
    assert!(
        !writes.contains(&strings(&["1", "2"])),
        "the older build of 1 (number 2) was written over the substitute (number 1000): {writes:?}"
    );
    task.abort();
    answers.abort();
}

/// B1: a second substitute on the same lane replaces F9's record, so an
/// item still to come that was built before the first substitute writes the
/// first substitute's documents back to an older build.
#[tokio::test(start_paused = true)]
async fn a_second_substitute_keeps_what_the_first_one_supersedes() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    for (ids, position, build) in [(&[1][..], 0, 1), (&[1, 2][..], 1, 2)] {
        rig.progress.published(
            &lane(orders()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(keyed(&orders(), ids, Some(position), build))
            .await
            .unwrap();
    }
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    // 1 is fixed, 2 is now refused: the item numbered 2 parks the lane again.
    {
        let mut refused = rig.sink.refused_ids.lock().unwrap();
        refused.clear();
        refused.insert("2".to_owned());
    }
    eventually("a second fresh build is asked", || {
        builder.asked().iter().any(|(_, _, ids)| ids == &[2])
    })
    .await;
    // Built before either substitute (number 3), still in the channel: the
    // window was not taking from the parked lane.
    rig.progress.published(
        &lane(orders()),
        stream::ItemKey::new(2),
        Position::new(2),
        Position::new(2),
    );
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(keyed(&orders(), &[1], Some(2), 3))
        .await
        .unwrap();
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("every item lands", || rig.applied(&orders()) == through(2)).await;
    let writes = written(&rig, &orders());
    let last_two = writes
        .iter()
        .rposition(|ids| ids == &["2".to_owned()])
        .expect("the substitute of 2 is written");
    assert!(
        writes
            .iter()
            .skip(last_two + 1)
            .all(|ids| !ids.contains(&"1".to_owned())),
        "the item numbered 3 writes 1 back over the first substitute (number 10): {writes:?}"
    );
    task.abort();
    answers.abort();
}

/// an item every document of which a substitute superseded is
/// written empty, and still acknowledged with its position.
#[tokio::test(start_paused = true)]
async fn an_item_emptied_by_a_substitute_still_lands() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    for (ids, position, build) in [(&[1][..], 0, 1), (&[1][..], 1, 2)] {
        rig.progress.published(
            &lane(orders()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(keyed(&orders(), ids, Some(position), build))
            .await
            .unwrap();
    }
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("both items land", || rig.applied(&orders()) == through(1)).await;
    eventually("all acknowledged", || rig.stream.is_idle()).await;
    task.abort();
    answers.abort();
}

/// B1, past the lookahead: the above with every item published up front, in build order, the
/// older build of 1 sitting past the window's lookahead.
#[tokio::test(start_paused = true)]
async fn a_backlog_past_the_lookahead_keeps_what_the_first_substitute_supersedes() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    let mut items: Vec<(Vec<i64>, u64)> = vec![(vec![1], 1), (vec![1, 2], 2)];
    for k in 0..60 {
        items.push((vec![100 + k], 3 + k as u64));
    }
    items.push((vec![1], 70));
    let last = items.len() as u64 - 1;
    for (position, (ids, build)) in items.iter().enumerate() {
        let position = position as u64;
        rig.progress.published(
            &lane(orders()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(keyed(&orders(), ids, Some(position), *build))
            .await
            .unwrap();
    }
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 1000);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    {
        let mut refused = rig.sink.refused_ids.lock().unwrap();
        refused.clear();
        refused.insert("2".to_owned());
    }
    eventually("a fresh build of 2 is asked", || {
        builder.asked().iter().any(|(_, _, ids)| ids == &[2])
    })
    .await;
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("every item lands", || {
        rig.applied(&orders()) == through(last)
    })
    .await;
    let writes = written(&rig, &orders());
    let last_two = writes
        .iter()
        .rposition(|ids| ids == &["2".to_owned()])
        .expect("the substitute of 2 is written");
    assert!(
        writes
            .iter()
            .skip(last_two + 1)
            .all(|ids| !ids.contains(&"1".to_owned())),
        "the item numbered 70 writes 1 back over the first substitute (number 1000): {writes:?}"
    );
    task.abort();
    answers.abort();
}

/// A stream with no substitute lanes.
#[derive(Debug)]
struct NoSubstitutes(Arc<ChannelStream>);

impl Stream for NoSubstitutes {
    fn lane(&self, key: &LaneKey) -> stream::Result<stream::Lane> {
        self.0.lane(key)
    }

    fn lanes(&self, sink: &kernel::SinkName) -> Vec<LaneKey> {
        self.0.lanes(sink)
    }

    fn requests(&self) -> stream::Result<stream::RequestLane> {
        self.0.requests()
    }

    fn is_idle(&self) -> bool {
        self.0.is_idle()
    }
}

/// F2: over a stream with no substitute lanes, a refused lane whose item
/// carries its keys probes the item itself, asking for nothing.
#[tokio::test(start_paused = true)]
async fn a_lane_without_substitutes_probes_the_same_item() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    let start = Instant::now();
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(keyed(&orders(), &[1], None, 1))
        .await
        .unwrap();
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let engine = Arc::new(
        SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&rig.sink) as Arc<dyn Sink>,
            Arc::new(NoSubstitutes(Arc::clone(&rig.stream))) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&rig.progress),
        )
        .with_writes(4)
        .with_failure_policies(FailurePolicies::new(FailurePolicy::Stop)),
    );
    engine.set_blame(Blame::Nobody);
    let task = spawn(Arc::clone(&engine));
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    let probes: Vec<u128> = rig
        .sink
        .writes_of(&orders(), start)
        .iter()
        .map(Duration::as_millis)
        .collect();
    assert_eq!(probes, [0, 1_000, 3_000], "the item, probed at 1 and 3 s");
    assert!(
        builder.asked().is_empty(),
        "nothing asked: no substitute lane"
    );
    task.abort();
    answers.abort();
}

// Review r2 of #458.

/// A write of an older item fails as a whole while a newer write of the same
/// lane is still in flight; the newer one lands after the failure and prunes
/// F9's map, though the older item, dropped unacknowledged, will be
/// redelivered whole to the next run.
#[tokio::test(start_paused = true)]
async fn a_sibling_write_landing_after_a_failed_one_keeps_the_map() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    let publish = |ids: &'static [i64], position: u64, build: u64| {
        let rig = &rig;
        async move {
            rig.progress.published(
                &lane(orders()),
                stream::ItemKey::new(position),
                Position::new(position),
                Position::new(position),
            );
            rig.stream
                .lane(&lane(orders()))
                .unwrap()
                .producer
                .publish(keyed(&orders(), ids, Some(position), build))
                .await
                .unwrap();
        }
    };
    publish(&[1], 0, 1).await;
    publish(&[1, 2], 1, 2).await;
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 1000);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    {
        let mut slow = rig.sink.slow_ids.lock().unwrap();
        slow.insert("2".to_owned(), Duration::from_secs(5));
        slow.insert("3".to_owned(), Duration::from_secs(8));
    }
    *rig.sink.fail_id.lock().unwrap() = Some("2".to_owned());
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("the substitute unparks the lane", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    // A newer change (number 2000), written beside the item numbered 2.
    publish(&[3], 2, 2000).await;
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed write");
    assert!(ended.unwrap().is_err());
    *rig.sink.fail_id.lock().unwrap() = None;
    rig.sink.slow_ids.lock().unwrap().clear();
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let task = spawn(Arc::clone(&engine));
    eventually("every item lands after the restart", || {
        rig.applied(&orders()) == through(2)
    })
    .await;
    let writes = written(&rig, &orders());
    let substitute = writes
        .iter()
        .rposition(|ids| ids == &["1".to_owned()])
        .expect("the substitute is written");
    assert!(
        writes
            .iter()
            .skip(substitute + 1)
            .all(|ids| !ids.contains(&"1".to_owned())),
        "the older build of 1 (number 2) was written over the substitute (number 1000): {writes:?}"
    );
    task.abort();
    answers.abort();
}

/// The last item of a quiet lane waits, unacknowledged, for a caught-up write
/// (its write was searchable only within the refresh interval). A newer item
/// lands meanwhile and prunes F9's map; the caught-up write fails, the run
/// fails, and the older item comes back whole.
#[tokio::test(start_paused = true)]
async fn an_item_waiting_for_a_caught_up_write_keeps_the_map() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .within
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    let publish = |ids: &'static [i64], position: u64, build: u64| {
        let rig = &rig;
        async move {
            rig.progress.published(
                &lane(orders()),
                stream::ItemKey::new(position),
                Position::new(position),
                Position::new(position),
            );
            rig.stream
                .lane(&lane(orders()))
                .unwrap()
                .producer
                .publish(keyed(&orders(), ids, Some(position), build))
                .await
                .unwrap();
        }
    };
    publish(&[1], 0, 1).await;
    publish(&[1, 2], 1, 2).await;
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 1000);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("a fresh build is asked", || builder.asked().len() == 1).await;
    rig.sink
        .refresh_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("the item numbered 2 is written", || {
        written(&rig, &orders()).contains(&vec!["2".to_owned()])
    })
    .await;
    publish(&[3], 2, 2000).await;
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed caught-up write");
    assert!(ended.unwrap().is_err());
    rig.sink
        .refresh_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let task = spawn(Arc::clone(&engine));
    eventually("every item lands after the restart", || {
        rig.applied(&orders()) == through(2)
    })
    .await;
    eventually("all acknowledged", || rig.stream.is_idle()).await;
    let writes = written(&rig, &orders());
    let substitute = writes
        .iter()
        .rposition(|ids| ids == &["1".to_owned()])
        .expect("the substitute is written");
    assert!(
        writes
            .iter()
            .skip(substitute + 1)
            .all(|ids| !ids.contains(&"1".to_owned())),
        "the older build of 1 (number 2) was written over the substitute (number 1000): {writes:?}"
    );
    task.abort();
    answers.abort();
}

/// 14.10's boundary and memory bound: the substitute numbered 10 lands with
/// document 1 while the item numbered 9 (older, written without 1) is still
/// being written: with the oldest unacknowledged item at 9 the entry 10
/// stays. Once 9 is acknowledged nothing numbered below 10 can come: the
/// entry goes, and the lane keeps no state.
#[tokio::test(start_paused = true)]
async fn an_entry_stays_while_an_older_item_is_unacknowledged_and_goes_after() {
    let rig = Rig::new(Gate::default());
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    // 9 shares 1 with the parked item, so it waits behind it; the substitute
    // leaves 1 out of it.
    for (ids, position, build) in [(&[1][..], 0, 1), (&[1, 5][..], 1, 9)] {
        rig.progress.published(
            &lane(orders()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(keyed(&orders(), ids, Some(position), build))
            .await
            .unwrap();
    }
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 10);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("the lane parks", || {
        engine.lane_health(&lane(orders())).kind() == "refused"
    })
    .await;
    // Fixed before the first probe, so its fresh build (10) lands; 9 then
    // takes 5 s to write: in flight when 10 lands.
    rig.sink.refused_ids.lock().unwrap().clear();
    *rig.sink.slow_id.lock().unwrap() = Some("5".to_owned());
    eventually("the substitute lands", || {
        rig.applied(&orders()) == before(1)
    })
    .await;
    // 11 (another document) lands beside 9 and prunes: 9, older, is still
    // unacknowledged.
    rig.progress.published(
        &lane(orders()),
        stream::ItemKey::new(2),
        Position::new(2),
        Position::new(2),
    );
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(keyed(&orders(), &[7], Some(2), 11))
        .await
        .unwrap();
    eventually("11 is written", || {
        written(&rig, &orders()).contains(&vec!["7".to_owned()])
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        engine.superseded_map(&lane(orders())),
        Some(BTreeMap::from([("1".to_owned(), 10)])),
        "9 is unacknowledged: the entry 10 stays"
    );
    eventually("9 lands", || rig.applied(&orders()) == through(2)).await;
    eventually("nothing numbered below 10 can come: the entry goes", || {
        engine.superseded_map(&lane(orders())).is_none()
    })
    .await;
    assert!(engine.lane_health(&lane(orders())).is_open());
    task.abort();
    answers.abort();
}

/// 14.10's seed (review of #458, S1): when its substitute (1000) lands, the
/// parked item's own acknowledgement waits for the quiet lane's caught-up
/// write, so the account starts with it. A newer item landing meanwhile
/// (1001) doesn't drop the entry; the caught-up write fails, the run fails,
/// and the redelivered parked item is written without 1: document 1 is
/// written twice in all, the refused item then its substitute.
#[tokio::test(start_paused = true)]
async fn the_parked_item_is_never_rewritten_whole() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .within
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink.refused_ids.lock().unwrap().insert("1".to_owned());
    let publish = |ids: &'static [i64], position: u64, build: u64| {
        let rig = &rig;
        async move {
            rig.progress.published(
                &lane(orders()),
                stream::ItemKey::new(position),
                Position::new(position),
                Position::new(position),
            );
            rig.stream
                .lane(&lane(orders()))
                .unwrap()
                .producer
                .publish(keyed(&orders(), ids, Some(position), build))
                .await
                .unwrap();
        }
    };
    publish(&[1], 0, 1).await;
    let builder = Arc::new(Builder::default());
    let answers = answering(&rig, Arc::clone(&builder), 1000);
    let engine = Arc::new(rig.engine(FailurePolicy::Stop));
    let task = spawn(Arc::clone(&engine));
    eventually("the lane parks", || {
        engine.lane_health(&lane(orders())).kind() == "refused"
    })
    .await;
    rig.sink
        .refresh_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink.refused_ids.lock().unwrap().clear();
    eventually("the substitute lands", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    publish(&[7], 1, 1001).await;
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed caught-up write");
    assert!(ended.unwrap().is_err());
    rig.sink
        .refresh_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let task = spawn(Arc::clone(&engine));
    eventually("every item lands after the restart", || {
        rig.applied(&orders()) == through(1)
    })
    .await;
    eventually("all acknowledged", || rig.stream.is_idle()).await;
    let writes = written(&rig, &orders());
    let substitute = writes
        .iter()
        .rposition(|ids| ids == &["1".to_owned()])
        .expect("1 is written");
    assert_eq!(
        builder.asked().len(),
        1,
        "the last [1] written is the substitute: {writes:?}"
    );
    assert!(
        writes
            .iter()
            .skip(substitute + 1)
            .all(|ids| !ids.contains(&"1".to_owned())),
        "the parked item's older build of 1 was written over its substitute: {writes:?}"
    );
    let ones = writes
        .iter()
        .filter(|ids| ids.contains(&"1".to_owned()))
        .count();
    assert_eq!(
        ones, 2,
        "1 is written twice: the refused item, then its substitute; never the item again: {writes:?}"
    );
    task.abort();
    answers.abort();
}

// Reindex of a lane not open (stage 9, slice 2, the note's 15.1).

fn spawn_controlled(
    engine: Arc<SinkEngine>,
) -> (
    mpsc::Sender<SinkControl>,
    tokio::task::JoinHandle<crate::Result<()>>,
) {
    let (control, mut receiver) = mpsc::channel::<SinkControl>(4);
    let task = tokio::spawn(async move { engine.run(&mut receiver).await });
    (control, task)
}

/// The next `Backfill` request on the request lane, acknowledged.
async fn next_backfill(rig: &Rig) -> (IndexName, stream::Ticket) {
    let mut requests = rig.stream.requests().unwrap().consumer;
    loop {
        let delivery = tokio::time::timeout(Duration::from_secs(60), requests.recv())
            .await
            .expect("a backfill request comes")
            .unwrap()
            .unwrap();
        let (request, handle) = delivery.into_parts();
        handle.ack().await.unwrap();
        if let Request::Backfill { index, ticket, .. } = request {
            return (index, ticket);
        }
    }
}

/// `of`'s snapshot for `ticket` is complete.
async fn complete(rig: &Rig, of: &IndexName, ticket: stream::Ticket) {
    rig.stream
        .lane(&lane(of.clone()))
        .unwrap()
        .producer
        .publish(LaneItem::SnapshotComplete {
            index: of.clone(),
            ticket,
        })
        .await
        .unwrap();
}

/// How many writes held document `id` of `of`.
fn writes_holding(rig: &Rig, of: &IndexName, id: &str) -> usize {
    written(rig, of)
        .iter()
        .filter(|ids| ids.iter().any(|own| own == id))
        .count()
}

fn slow_backoff() -> crate::Backoff {
    crate::Backoff::new(Duration::from_secs(60), Duration::from_secs(60)).unwrap()
}

/// X1–X4: a reindex of a parked lane (refused, then blocked) opens it; the
/// parked item is not written: it is held until the reindexed index is
/// seeded, then acknowledged and applied; the item behind it is written
/// into the new generation meanwhile, its position applied only after.
#[tokio::test(start_paused = true)]
async fn a_reindex_opens_a_parked_lane_and_holds_its_item_until_seeded() {
    for answer in [Answer::Refuse, Answer::Block] {
        let rig = Rig::new(Gate::default());
        rig.sink.answer(&orders(), Some(answer));
        rig.live(&orders(), 1..2, 0).await;
        let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
        let (control, task) = spawn_controlled(Arc::clone(&engine));
        eventually("the lane parks", || {
            !engine.lane_health(&lane(orders())).is_open()
        })
        .await;
        rig.live(&orders(), 2..3, 1).await;
        rig.sink.answer(&orders(), None);
        control
            .send(SinkControl::Reindex {
                indexes: vec![orders()],
            })
            .await
            .unwrap();
        // Opened by the reindex: the next probe is 60 s away.
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(
            engine.lane_health(&lane(orders())).is_open(),
            "{answer:?}: the reindex opens the lane"
        );
        eventually("the item behind is written", || {
            writes_holding(&rig, &orders(), "2") == 1
        })
        .await;
        assert_eq!(
            rig.applied(&orders()),
            before(0),
            "{answer:?}: nothing counts as landed before the new generation is seeded"
        );
        let (index, ticket) = next_backfill(&rig).await;
        assert_eq!(index, orders());
        complete(&rig, &orders(), ticket).await;
        eventually("seeded: the held item is acknowledged", || {
            rig.applied(&orders()) == through(1)
        })
        .await;
        eventually("everything acknowledged", || rig.stream.is_idle()).await;
        assert_eq!(
            writes_holding(&rig, &orders(), "1"),
            1,
            "{answer:?}: the parked item is never written again"
        );
        task.abort();
    }
}

/// X5: a reindex of a restaging lane opens it, and its items are written
/// into the new generation (they were held only for a missing target).
#[tokio::test(start_paused = true)]
async fn a_reindex_opens_a_restaging_lane_and_writes_its_items() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::MissingForGood));
    rig.live(&orders(), 1..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane is restaging", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    // Opened by the reindex: the next staging is 60 s away.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(engine.lane_health(&lane(orders())).is_open());
    assert_eq!(
        rig.applied(&orders()),
        through(0),
        "its item lands in the new generation"
    );
    task.abort();
}

/// X6, X2: a reindex while the index's backfill is outstanding cuts it and
/// asks again; the refused page the lane parked on is held until the new
/// backfill seeds the index.
#[tokio::test(start_paused = true)]
async fn a_reindex_during_a_backfill_cuts_it_and_asks_again() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .unseeded
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let (index, first) = next_backfill(&rig).await;
    assert_eq!(index, orders());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(match item(&orders(), 1..2, None) {
            LaneItem::Batch(batch) => LaneItem::Batch(Batch {
                ticket: Some(first),
                ..batch
            }),
            other => other,
        })
        .await
        .unwrap();
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks on the page", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    let (index, again) = next_backfill(&rig).await;
    assert_eq!(index, orders());
    assert!(again > first, "a new request: {again} after {first}");
    assert_eq!(
        *rig.sink.forgotten.lock().unwrap(),
        ["orders"],
        "the cut walk's checkpoint is forgotten: a restart can't resume it"
    );
    complete(&rig, &orders(), again).await;
    eventually("seeded, everything acknowledged", || rig.stream.is_idle()).await;
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        1,
        "the page is not written again"
    );
    assert!(engine.lane_health(&lane(orders())).is_open());
    task.abort();
}

/// X7: the run fails before the seed: the held item comes back and is held
/// again, never written; the seed acknowledges it.
#[tokio::test(start_paused = true)]
async fn a_held_item_redelivered_after_a_failed_run_is_held_again() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 1..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    let (_, ticket) = next_backfill(&rig).await;
    rig.sink
        .fail
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.live(&orders(), 2..3, 1).await;
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed write");
    assert!(ended.unwrap().is_err());
    rig.sink
        .fail
        .store(false, std::sync::atomic::Ordering::SeqCst);
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the item behind lands again", || {
        writes_holding(&rig, &orders(), "2") >= 2
    })
    .await;
    assert_eq!(rig.applied(&orders()), before(0));
    complete(&rig, &orders(), ticket).await;
    eventually("seeded: everything acknowledged", || rig.stream.is_idle()).await;
    assert_eq!(rig.applied(&orders()), through(1));
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        1,
        "never written again"
    );
    drop(control);
    task.abort();
}

// Review of #459.

/// X9: a second reindex while the lane already holds an item (the lane
/// parked again before the first reindex's backfill seeded) overwrites
/// `LaneRun.held`: the first held item's handle is dropped unacknowledged,
/// redelivered, and written into the newest generation (X2 broken).
#[tokio::test(start_paused = true)]
async fn a_second_reindex_keeps_the_first_held_item_unwritten() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 1..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    let (_, first) = next_backfill(&rig).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    // Still refused in the new generation: the item behind parks the lane.
    rig.live(&orders(), 2..3, 1).await;
    eventually("the lane parks again", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    let (_, again) = next_backfill(&rig).await;
    assert!(again > first);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        1,
        "the first held item is never written again"
    );
    complete(&rig, &orders(), again).await;
    eventually("seeded, everything acknowledged", || rig.stream.is_idle()).await;
    assert_eq!(rig.applied(&orders()), through(1));
    task.abort();
}

/// X8: the run fails after the seed settled the held item but before its
/// acknowledgement returned (here: its caught-up write fails). `settle_held`
/// already cleared `held_for_seed`, so the redelivered item is written whole
/// into the seeded generation, after the backfill page that holds a newer
/// copy of its document.
#[tokio::test(start_paused = true)]
async fn a_failure_between_the_seed_and_the_held_items_ack_never_writes_it() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 1..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    let (_, ticket) = next_backfill(&rig).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink
        .within
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink
        .refresh_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let producer = rig.stream.lane(&lane(orders())).unwrap().producer;
    // The new walk's page: a newer copy of document 1.
    producer
        .publish(match item(&orders(), 1..2, None) {
            LaneItem::Batch(batch) => LaneItem::Batch(Batch {
                ticket: Some(ticket),
                ..batch
            }),
            other => other,
        })
        .await
        .unwrap();
    rig.progress.published(
        &lane(orders()),
        stream::ItemKey::new(1),
        Position::new(1),
        Position::new(1),
    );
    producer
        .publish(item(&orders(), 2..3, Some(1)))
        .await
        .unwrap();
    producer
        .publish(LaneItem::SnapshotComplete {
            index: orders(),
            ticket,
        })
        .await
        .unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(60), task)
        .await
        .expect("the run ends on the failed caught-up write");
    assert!(ended.unwrap().is_err(), "the run failed");
    rig.sink
        .refresh_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let before_restart = writes_holding(&rig, &orders(), "1");
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let (control2, task) = spawn_controlled(Arc::clone(&engine));
    eventually("everything acknowledged", || rig.stream.is_idle()).await;
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        before_restart,
        "the held item (an older build than the page) is never written after the seed"
    );
    drop((control, control2));
    task.abort();
}

/// X10: a stop while an item is held acknowledges it without applying its
/// position: the lane drains, and its applied mark (so the slot) stays before
/// the held change.
#[tokio::test(start_paused = true)]
async fn a_stop_acknowledges_a_held_item_without_applying_it() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 1..2, 0).await;
    let stop = Stop::new();
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone()),
    );
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    let _ = next_backfill(&rig).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    stop.request();
    eventually("the stop drains the lane", || rig.stream.is_idle()).await;
    assert_eq!(
        rig.applied(&orders()),
        before(0),
        "the held change is never applied: the slot stays before it"
    );
    assert_eq!(writes_holding(&rig, &orders(), "1"), 1, "never written");
    task.abort();
}

/// Every rebuilding, backfill-requested and seeded event, in order.
fn rebuild_events(events: &Arc<Mutex<Vec<String>>>) -> Arc<dyn Observer> {
    Arc::new(super::RebuildEvents(Arc::clone(events))) as Arc<dyn Observer>
}

fn attempts(events: &Arc<Mutex<Vec<String>>>, of: &IndexName) -> usize {
    let rebuilding = format!("rebuilding {of}");
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| **event == rebuilding)
        .count()
}

fn last_event(events: &Arc<Mutex<Vec<String>>>, of: &IndexName) -> Option<String> {
    let suffix = format!(" {of}");
    events
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|event| event.ends_with(&suffix))
        .cloned()
}

/// X15, X16 (review of `main` at `9b70cd6e`, 2): a reindex that cannot stage
/// (a cluster-wide block) is owed: the lane is `restaging` with the error as
/// its reason, the reindex is retried on its backoff, and the sink's run and
/// the other lane go on. Once staging works the retry is the reindex: the
/// parked item is held until the seed, and the fence's `rebuilding` ends
/// there.
#[tokio::test(start_paused = true)]
async fn a_reindex_whose_staging_fails_is_retried_until_it_stages() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Block));
    rig.live(&orders(), 1..2, 0).await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_observer(rebuild_events(&events)),
    );
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink
        .ensure_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(200)).await;
    let health = engine.lane_health(&lane(orders()));
    assert!(
        matches!(&health, LaneHealth::Restaging { reason, .. } if reason.contains("cluster_block_exception")),
        "owed, with its reason: {health:?}"
    );
    assert!(attempts(&events, &orders()) >= 3, "retried: {events:?}");
    assert!(!task.is_finished(), "the sink's run goes on");
    rig.live(&users(), 5..6, 1).await;
    eventually("the other lane keeps writing", || {
        rig.applied(&users()) == through(1)
    })
    .await;
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        1,
        "not written while owed"
    );
    rig.sink
        .ensure_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let (index, ticket) = next_backfill(&rig).await;
    assert_eq!(
        index,
        orders(),
        "the retry staged and asks for its backfill"
    );
    assert!(engine.lane_health(&lane(orders())).is_open());
    assert_eq!(rig.applied(&orders()), before(0), "held until the seed");
    complete(&rig, &orders(), ticket).await;
    eventually("seeded: the held item is acknowledged", || {
        rig.applied(&orders()) == through(0)
    })
    .await;
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        1,
        "the parked item is never written again"
    );
    assert_eq!(
        last_event(&events, &orders()).as_deref(),
        Some("seeded orders"),
        "the fence's rebuilding ends at the seed: {events:?}"
    );
    task.abort();
}

/// X14 (review 2): a reindex of two indexes, one of which cannot stage,
/// stages the other and asks for its backfill; the failing one is owed and
/// staged once it can be.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_two_indexes_stages_one_while_the_other_is_retried() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("users".to_owned());
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders(), users()],
        })
        .await
        .unwrap();
    let (index, orders_ticket) = next_backfill(&rig).await;
    assert_eq!(index, orders(), "the index that staged is backfilled");
    assert!(engine.lane_health(&lane(orders())).is_open());
    assert_eq!(engine.lane_health(&lane(users())).kind(), "restaging");
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    // The retry is due one backoff (60 s) after the failure.
    tokio::time::sleep(Duration::from_secs(30)).await;
    let (index, users_ticket) = next_backfill(&rig).await;
    assert_eq!(index, users(), "the owed one, once it stages");
    complete(&rig, &orders(), orders_ticket).await;
    complete(&rig, &users(), users_ticket).await;
    eventually("both lanes open", || {
        engine.lane_health(&lane(orders())).is_open()
            && engine.lane_health(&lane(users())).is_open()
    })
    .await;
    assert!(!task.is_finished());
    task.abort();
}

/// Review of #470, 1: in one reindex, `users` cannot stage and asking for
/// `orders`' backfill again fails (the same block refuses `forget_walked`,
/// a write). Neither is lost: both are owed, the sink's run goes on, and
/// both are staged and backfilled once the block lifts.
#[tokio::test(start_paused = true)]
async fn a_failed_ask_for_a_backfill_leaves_every_index_of_the_reindex_owed() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .unseeded
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_observer(rebuild_events(&events)),
    );
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let (_, first) = next_backfill(&rig).await;
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(match item(&orders(), 1..2, None) {
            LaneItem::Batch(batch) => LaneItem::Batch(Batch {
                ticket: Some(first),
                ..batch
            }),
            other => other,
        })
        .await
        .unwrap();
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks on the page", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    rig.sink.answer(&orders(), None);
    rig.sink
        .forget_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("users".to_owned());
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders(), users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!task.is_finished(), "the sink's run goes on");
    for of in [orders(), users()] {
        assert_eq!(
            engine.lane_health(&lane(of.clone())).kind(),
            "restaging",
            "{of} is owed its reindex"
        );
    }
    rig.sink
        .forget_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    tokio::time::sleep(Duration::from_secs(30)).await;
    let mut asked = BTreeMap::new();
    for _ in 0..2 {
        let (index, ticket) = next_backfill(&rig).await;
        asked.insert(index.to_string(), ticket);
    }
    assert_eq!(
        asked.keys().map(String::as_str).collect::<Vec<_>>(),
        ["orders", "users"],
        "both retried and backfilled"
    );
    for (index, ticket) in asked {
        complete(&rig, &IndexName::try_new(index).unwrap(), ticket).await;
    }
    eventually("both seeded", || {
        last_event(&events, &orders()).as_deref() == Some("seeded orders")
            && last_event(&events, &users()).as_deref() == Some("seeded users")
    })
    .await;
    assert!(!task.is_finished());
    task.abort();
}

/// A stream whose request lane refuses to publish while `refuse` is set,
/// and takes `slow` to publish while that is set.
#[derive(Debug)]
struct RefusingRequests {
    inner: Arc<ChannelStream>,
    refuse: Arc<std::sync::atomic::AtomicBool>,
    slow: Arc<Mutex<Option<Duration>>>,
}

#[derive(Debug)]
struct RefusingProducer {
    inner: Box<dyn stream::Producer<Request>>,
    refuse: Arc<std::sync::atomic::AtomicBool>,
    slow: Arc<Mutex<Option<Duration>>>,
}

#[async_trait]
impl stream::Producer<Request> for RefusingProducer {
    async fn publish(&self, item: Request) -> stream::Result<()> {
        let slow = *self.slow.lock().unwrap();
        if let Some(slow) = slow {
            tokio::time::sleep(slow).await;
        }
        if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(stream::QueueError::Publish("broker unavailable".to_owned()));
        }
        self.inner.publish(item).await
    }
}

impl Stream for RefusingRequests {
    fn lane(&self, key: &LaneKey) -> stream::Result<stream::Lane> {
        self.inner.lane(key)
    }

    fn substitutes(&self, sink: &kernel::SinkName) -> stream::Result<stream::SubstituteLane> {
        self.inner.substitutes(sink)
    }

    fn lanes(&self, sink: &kernel::SinkName) -> Vec<LaneKey> {
        self.inner.lanes(sink)
    }

    fn requests(&self) -> stream::Result<stream::RequestLane> {
        let lane = self.inner.requests()?;
        Ok(stream::RequestLane {
            producer: Box::new(RefusingProducer {
                inner: lane.producer,
                refuse: Arc::clone(&self.refuse),
                slow: Arc::clone(&self.slow),
            }),
            consumer: lane.consumer,
        })
    }

    fn is_idle(&self) -> bool {
        self.inner.is_idle()
    }
}

/// Review of #470, 1: a reindex staged but whose backfill request cannot be
/// published is owed, not lost: retried once requests go through.
#[tokio::test(start_paused = true)]
async fn a_reindex_whose_backfill_request_fails_is_owed() {
    let rig = Rig::new(Gate::default());
    let refuse = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&rig.sink) as Arc<dyn Sink>,
            Arc::new(RefusingRequests {
                inner: Arc::clone(&rig.stream),
                refuse: Arc::clone(&refuse),
                slow: Arc::default(),
            }) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&rig.progress),
        )
        .with_writes(4)
        .with_failure_policies(FailurePolicies::new(FailurePolicy::Stop))
        .with_backoff(slow_backoff())
        .with_observer(rebuild_events(&events)),
    );
    engine.set_blame(Blame::Nobody);
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!task.is_finished(), "the sink's run goes on");
    let health = engine.lane_health(&lane(orders()));
    assert!(
        matches!(&health, LaneHealth::Restaging { reason, .. } if reason.contains("broker unavailable")),
        "owed, with its reason: {health:?}"
    );
    refuse.store(false, std::sync::atomic::Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(30)).await;
    let (index, ticket) = next_backfill(&rig).await;
    assert_eq!(index, orders());
    complete(&rig, &orders(), ticket).await;
    eventually("seeded", || {
        last_event(&events, &orders()).as_deref() == Some("seeded orders")
    })
    .await;
    assert!(engine.lane_health(&lane(orders())).is_open());
    task.abort();
}

/// Review of #470, 2: a stop while a lane is owed its reindex doesn't wait
/// for the retry and stages nothing: the lane's items are acknowledged
/// unapplied, as held items are at a stop (X10), so the drain ends at once
/// and the slot stays before their changes; the next start replays them.
#[tokio::test(start_paused = true)]
async fn a_stop_while_a_reindex_is_owed_drains_at_once_and_stages_nothing() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone())
            .with_observer(rebuild_events(&events)),
    );
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    eventually("owed", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    rig.live(&orders(), 1..2, 0).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let before_stop = events.lock().unwrap().len();
    stop.request();
    let stopped = Instant::now();
    eventually("the stop drains the lane", || rig.stream.is_idle()).await;
    assert!(
        stopped.elapsed() < Duration::from_secs(2),
        "drained at once, not after the retry's backoff: {:?}",
        stopped.elapsed()
    );
    tokio::time::sleep(Duration::from_secs(120)).await;
    let after: Vec<String> = events.lock().unwrap()[before_stop..].to_vec();
    assert!(
        after.is_empty(),
        "nothing staged or asked for once stopping: {after:?}"
    );
    assert_eq!(
        rig.applied(&orders()),
        before(0),
        "the change is not applied: the slot stays before it"
    );
    assert_eq!(writes_holding(&rig, &orders(), "1"), 0, "never written");
    task.abort();
}

/// Review of #470, 2: a reindex still waiting to be staged when the stop
/// is asked is not staged: nothing new is staged or asked for once stopping.
#[tokio::test(start_paused = true)]
async fn a_reindex_pending_at_a_stop_is_not_staged() {
    let rig = Rig::new(Gate::default());
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone())
            .with_observer(rebuild_events(&events)),
    );
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    tokio::time::sleep(Duration::from_secs(1)).await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    stop.request();
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        events.lock().unwrap().is_empty(),
        "nothing staged: {:?}",
        events.lock().unwrap()
    );
    task.abort();
}

/// Review of #470, 5: an index a reindex never started (another failed
/// first; at most `STAGE_CONCURRENCY` stage at once) is owed and announced
/// `rebuilding` too: its fences say why they wait instead of timing out.
#[tokio::test(start_paused = true)]
async fn an_index_never_started_is_announced_rebuilding_once_owed() {
    let names: Vec<IndexName> = (0..=crate::sink_engine::STAGE_CONCURRENCY)
        .map(|at| index(&format!("i{at}")))
        .collect();
    let stream = Arc::new(ChannelStream::over(
        64,
        names.clone(),
        [sink_name("primary")],
    ));
    let progress = Arc::new(Progress::of(names.clone(), [sink_name("primary")]));
    let sink = Arc::new(Gate::default());
    sink.ensure_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&sink) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            names.iter().cloned().map(mapping).collect(),
            progress,
        )
        .with_backoff(slow_backoff())
        .with_observer(rebuild_events(&events)),
    );
    engine.set_blame(Blame::Nobody);
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    control
        .send(SinkControl::Reindex {
            indexes: names.clone(),
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let last = names.last().unwrap().clone();
    let health = engine.lane_health(&LaneKey::new(last.clone(), sink_name("primary")));
    assert!(
        matches!(&health, LaneHealth::Restaging { reason, .. } if reason.contains("not started")),
        "never started, owed: {health:?}"
    );
    assert_eq!(
        last_event(&events, &last).as_deref(),
        Some(format!("rebuilding {last}").as_str()),
        "announced: {events:?}"
    );
    task.abort();
}

/// How long, from now, until the stream is idle; `None` past `within`.
async fn idle_within(rig: &Rig, within: Duration) -> Option<Duration> {
    let from = Instant::now();
    while from.elapsed() < within {
        if rig.stream.is_idle() {
            return Some(from.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

/// Second review of #470, 1: a lane owed its reindex with its index's
/// `SnapshotComplete` waiting (X6: parked during its backfill, the reindex
/// failed to stage) drains at once at a stop: the completion is acknowledged
/// without seeding, as one a stop cut short. For a sink writing several items
/// at once, one at a time, and an ordered one: releasing takes no write slot.
#[tokio::test(start_paused = true)]
async fn a_stop_releases_the_completion_waiting_on_an_owed_lane() {
    for (ordered, writes) in [(false, 4), (false, 1), (true, 1)] {
        let rig = Rig::new(Gate {
            ordered,
            ..Gate::default()
        });
        rig.sink
            .unseeded
            .lock()
            .unwrap()
            .insert("orders".to_owned());
        let stop = Stop::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let engine = Arc::new(
            rig.engine(FailurePolicy::Stop)
                .with_writes(writes)
                .with_backoff(slow_backoff())
                .with_stop(stop.clone())
                .with_observer(rebuild_events(&events)),
        );
        engine
            .stage(source::cdc::Continuity::Resumed)
            .await
            .unwrap();
        let (_, first) = next_backfill(&rig).await;
        rig.sink.answer(&orders(), Some(Answer::Refuse));
        rig.stream
            .lane(&lane(orders()))
            .unwrap()
            .producer
            .publish(match item(&orders(), 1..2, None) {
                LaneItem::Batch(batch) => LaneItem::Batch(Batch {
                    ticket: Some(first),
                    ..batch
                }),
                other => other,
            })
            .await
            .unwrap();
        let (control, task) = spawn_controlled(Arc::clone(&engine));
        eventually("the lane parks on the page", || {
            !engine.lane_health(&lane(orders())).is_open()
        })
        .await;
        rig.sink.answer(&orders(), None);
        complete(&rig, &orders(), first).await;
        rig.sink
            .ensure_fails_for
            .lock()
            .unwrap()
            .insert("orders".to_owned());
        control
            .send(SinkControl::Reindex {
                indexes: vec![orders()],
            })
            .await
            .unwrap();
        eventually("owed", || {
            engine.lane_health(&lane(orders())).kind() == "restaging"
        })
        .await;
        rig.sink.ensure_fails_for.lock().unwrap().clear();
        let before_stop = events.lock().unwrap().len();
        stop.request();
        let idle = idle_within(&rig, Duration::from_secs(300)).await;
        assert!(
            idle.is_some_and(|after| after < Duration::from_secs(2)),
            "ordered {ordered}, writes {writes}: drained at once: {idle:?}"
        );
        let after: Vec<String> = events.lock().unwrap()[before_stop..].to_vec();
        assert!(
            after.is_empty(),
            "ordered {ordered}, writes {writes}: nothing seeded, staged or asked for: {after:?}"
        );
        task.abort();
    }
}

/// Second review of #470, the gap of the same kind: a lane whose target
/// went missing, its staging on its backoff, drains at once at a stop too:
/// it is not staged again once stopping, its live items and its completion
/// are acknowledged unapplied, and the next start stages it.
#[tokio::test(start_paused = true)]
async fn a_stop_releases_a_lane_restaging_a_missing_target() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::MissingForGood));
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    rig.live(&orders(), 1..2, 0).await;
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone())
            .with_observer(rebuild_events(&events)),
    );
    let task = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move {
            let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
            engine.run(&mut control).await
        }
    });
    eventually("restaging", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    eventually("its staging failed once", || {
        !rig.sink.ensure_fails_for.lock().unwrap().is_empty()
            && engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    // Several items: a lane not open takes one at a time, and the drain must
    // not wait a loop wake-up for each.
    for at in 1..8 {
        rig.live(&orders(), at + 1..at + 2, at as u64).await;
    }
    complete(&rig, &orders(), stream::Ticket::new(99)).await;
    let ensured = rig.sink.ensured.lock().unwrap().len();
    let before_stop = events.lock().unwrap().len();
    stop.request();
    let idle = idle_within(&rig, Duration::from_secs(300)).await;
    assert!(
        idle.is_some_and(|after| after < Duration::from_secs(2)),
        "drained at once: {idle:?}"
    );
    tokio::time::sleep(Duration::from_secs(120)).await;
    assert_eq!(
        rig.sink.ensured.lock().unwrap().len(),
        ensured,
        "not staged again once stopping"
    );
    let after: Vec<String> = events.lock().unwrap()[before_stop..].to_vec();
    assert!(after.is_empty(), "nothing asked for or seeded: {after:?}");
    assert_eq!(rig.applied(&orders()), before(0), "nothing applied");
    task.abort();
}

/// Second review of #470, 2: a stop asked while an owed retry is already
/// staging (its `ensure_index` slow) lets the retry finish, but only as
/// state: the lane stays owed, no backfill is asked for, nothing more is
/// written, and the drain doesn't wait for it.
#[tokio::test(start_paused = true)]
async fn a_stop_during_an_owed_retry_records_it_and_writes_nothing_more() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 0..1, 0).await;
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone())
            .with_observer(rebuild_events(&events)),
    );
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("parked", || !engine.lane_health(&lane(orders())).is_open()).await;
    rig.sink.answer(&orders(), None);
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    eventually("owed", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    *rig.sink.ensure_delay.lock().unwrap() = Some(Duration::from_secs(10));
    rig.live(&orders(), 1..2, 1).await;
    let attempts_before = attempts(&events, &orders());
    eventually("the retry is staging", || {
        attempts(&events, &orders()) > attempts_before
    })
    .await;
    let before_stop = events.lock().unwrap().len();
    stop.request();
    let idle = idle_within(&rig, Duration::from_secs(300)).await;
    assert!(
        idle.is_some_and(|after| after < Duration::from_secs(2)),
        "drained without waiting for the retry: {idle:?}"
    );
    tokio::time::sleep(Duration::from_secs(30)).await;
    let after: Vec<String> = events.lock().unwrap()[before_stop..].to_vec();
    assert!(
        !after.iter().any(|event| event.starts_with("requested")),
        "no backfill asked for once stopping: {after:?}"
    );
    assert_eq!(
        engine.lane_health(&lane(orders())).kind(),
        "restaging",
        "the lane stays owed"
    );
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        0,
        "nothing more written"
    );
    assert_eq!(rig.applied(&orders()), before(0), "nothing applied");
    task.abort();
}

/// Second review of #470, 2, the later moment: the stop comes while an
/// owed retry's backfill request is being published. That ask completes
/// (it is never served), but the retry's outcome is recorded only: the lane
/// stays owed and opens for nothing, its item is released unwritten and
/// unapplied, the drain doesn't wait, and the next start picks the
/// generation up (X20).
#[tokio::test(start_paused = true)]
async fn a_retry_returning_staged_during_a_stop_leaves_the_lane_owed() {
    let rig = Rig::new(Gate::default());
    let slow = Arc::new(Mutex::new(None));
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&rig.sink) as Arc<dyn Sink>,
            Arc::new(RefusingRequests {
                inner: Arc::clone(&rig.stream),
                refuse: Arc::default(),
                slow: Arc::clone(&slow),
            }) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&rig.progress),
        )
        .with_writes(4)
        .with_failure_policies(FailurePolicies::new(FailurePolicy::Stop))
        .with_backoff(slow_backoff())
        .with_stop(stop.clone())
        .with_observer(rebuild_events(&events)),
    );
    engine.set_blame(Blame::Nobody);
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    eventually("owed", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    *slow.lock().unwrap() = Some(Duration::from_secs(10));
    rig.live(&orders(), 1..2, 0).await;
    let attempts_before = attempts(&events, &orders());
    eventually("the retry is asking for its backfill", || {
        attempts(&events, &orders()) > attempts_before
    })
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    stop.request();
    let idle = idle_within(&rig, Duration::from_secs(60)).await;
    assert!(
        idle.is_some_and(|after| after < Duration::from_secs(2)),
        "drained without waiting for the ask: {idle:?}"
    );
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(
        engine.lane_health(&lane(orders())).kind(),
        "restaging",
        "recorded only: the lane stays owed"
    );
    assert!(
        engine.is_outstanding(&orders()),
        "the ask in progress completed: outstanding, never served"
    );
    assert_eq!(writes_holding(&rig, &orders(), "1"), 0, "nothing written");
    assert_eq!(rig.applied(&orders()), before(0), "nothing applied");
    task.abort();
}

/// Third review of #470, 1: a first reindex, staged inline, whose ask for
/// its backfill is in progress when the stop comes. The ask completes
/// (unserved), but once stopping the staged index is owed, not reindexed:
/// the lane doesn't open, nothing after the stop is written, nothing is
/// applied, and the drain waits only for that round trip.
#[tokio::test(start_paused = true)]
async fn a_first_reindex_staging_as_the_stop_comes_opens_nothing() {
    let rig = Rig::new(Gate::default());
    let slow = Arc::new(Mutex::new(None));
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&rig.sink) as Arc<dyn Sink>,
            Arc::new(RefusingRequests {
                inner: Arc::clone(&rig.stream),
                refuse: Arc::default(),
                slow: Arc::clone(&slow),
            }) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&rig.progress),
        )
        .with_writes(4)
        .with_failure_policies(FailurePolicies::new(FailurePolicy::Stop))
        .with_backoff(slow_backoff())
        .with_stop(stop.clone())
        .with_observer(rebuild_events(&events)),
    );
    engine.set_blame(Blame::Nobody);
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 0..1, 0).await;
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("parked", || !engine.lane_health(&lane(orders())).is_open()).await;
    rig.sink.answer(&orders(), None);
    *slow.lock().unwrap() = Some(Duration::from_secs(10));
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    eventually("staging", || attempts(&events, &orders()) > 0).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    stop.request();
    tokio::time::sleep(Duration::from_secs(2)).await;
    rig.live(&orders(), 1..2, 1).await;
    let idle = idle_within(&rig, Duration::from_secs(60)).await;
    assert!(
        idle.is_some_and(|after| after < Duration::from_secs(9)),
        "drained once the staging round trip returned: {idle:?}"
    );
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(
        engine.lane_health(&lane(orders())).kind(),
        "restaging",
        "owed, not opened"
    );
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        0,
        "nothing written after the stop"
    );
    assert_eq!(rig.applied(&orders()), before(0), "nothing applied");
    task.abort();
}

/// A lane that wrote a position just before its target went missing
/// (visible within 1 s, so a caught-up write is still owed), now restaging
/// with its staging failing: at the stop its released item starts that
/// caught-up write, which fails, and the run fails during the stop.
async fn failing_during_the_stop() -> (Rig, Arc<SinkEngine>, Arc<Mutex<Vec<String>>>) {
    let rig = Rig::new(Gate::default());
    rig.sink
        .within
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.sink
        .slow_ids
        .lock()
        .unwrap()
        .insert("1".to_owned(), Duration::from_millis(500));
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_write_documents(1)
            .with_writes(1)
            .with_merge_wait(Duration::ZERO)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone())
            .with_observer(rebuild_events(&events)),
    );
    for at in 0..3 {
        rig.live(&orders(), at..at + 1, at as u64).await;
    }
    let task = spawn(Arc::clone(&engine));
    eventually("0 applied", || rig.applied(&orders()) == before(1)).await;
    rig.sink.answer(&orders(), Some(Answer::MissingForGood));
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    eventually("restaging", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    rig.sink
        .refresh_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    stop.request();
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run fails during the stop");
    assert!(
        ended.unwrap().is_err(),
        "the caught-up write failed the run"
    );
    rig.sink
        .refresh_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    (rig, engine, events)
}

/// Third review of #470, 2: a sink run failing during the stop is
/// restarted, and staging under the stop only ensures. (a) The index still
/// cannot be ensured: staging doesn't fail on it, its lane stays restaging,
/// and the restarted run drains at once. (b) The destination is fine again:
/// staging asks for no backfill and reopens nothing. Neither asks, seeds or
/// applies anything.
#[tokio::test(start_paused = true)]
async fn staging_after_a_run_failed_during_the_stop_only_ensures() {
    for healthy in [false, true] {
        let (rig, engine, events) = failing_during_the_stop().await;
        let before_restart = events.lock().unwrap().len();
        if healthy {
            rig.sink.ensure_fails_for.lock().unwrap().clear();
            rig.sink.answer(&orders(), Some(Answer::Missing));
        }
        tokio::time::timeout(
            Duration::from_secs(5),
            engine.stage(source::cdc::Continuity::Resumed),
        )
        .await
        .expect("staging under the stop returns at once")
        .unwrap();
        assert_eq!(
            engine.lane_health(&lane(orders())).kind(),
            "restaging",
            "healthy {healthy}: left restaging for the stop to release"
        );
        let writes = rig.sink.writes().len();
        let task = spawn(Arc::clone(&engine));
        let idle = idle_within(&rig, Duration::from_secs(60)).await;
        assert!(
            idle.is_some_and(|after| after < Duration::from_secs(2)),
            "healthy {healthy}: the restarted run drains at once: {idle:?}"
        );
        let after: Vec<String> = events.lock().unwrap()[before_restart..].to_vec();
        assert!(
            after.is_empty(),
            "healthy {healthy}: nothing asked for or seeded: {after:?}"
        );
        assert_eq!(
            rig.sink.writes().len(),
            writes,
            "healthy {healthy}: nothing written"
        );
        assert_eq!(
            rig.applied(&orders()),
            before(1),
            "healthy {healthy}: nothing applied"
        );
        task.abort();
    }
}

/// Third review of #470, 4: a restage out (its target gone, `ensure_index`
/// slow) when the stop comes finishes as state only: it asks for no
/// backfill, the lane stays restaging, nothing after the stop is written,
/// and the drain doesn't wait for it.
#[tokio::test(start_paused = true)]
async fn a_restage_out_as_the_stop_comes_asks_for_nothing_and_opens_nothing() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Missing));
    *rig.sink.ensure_delay.lock().unwrap() = Some(Duration::from_secs(10));
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone())
            .with_observer(rebuild_events(&events)),
    );
    rig.live(&orders(), 0..1, 0).await;
    let task = spawn(Arc::clone(&engine));
    eventually("restaging", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    rig.live(&orders(), 1..2, 1).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let before_stop = events.lock().unwrap().len();
    let writes_before = rig.sink.writes().len();
    stop.request();
    let idle = idle_within(&rig, Duration::from_secs(60)).await;
    assert!(
        idle.is_some_and(|after| after < Duration::from_secs(2)),
        "drained without waiting for the restage: {idle:?}"
    );
    tokio::time::sleep(Duration::from_secs(30)).await;
    let after: Vec<String> = events.lock().unwrap()[before_stop..].to_vec();
    assert!(after.is_empty(), "nothing asked for: {after:?}");
    assert_eq!(
        engine.lane_health(&lane(orders())).kind(),
        "restaging",
        "not opened once stopping"
    );
    assert_eq!(rig.sink.writes().len(), writes_before, "nothing written");
    assert_eq!(rig.applied(&orders()), before(0), "nothing applied");
    task.abort();
}

/// Fourth review of #470, 2 (X21's own case): an open lane whose index
/// can't be ensured by staging under the stop (its run failed on a transport
/// error, the cluster still down) is left restaging, so the restarted run
/// releases its items instead of writing, failing and restarting until the
/// deadline.
#[tokio::test(start_paused = true)]
async fn an_open_lane_not_ensured_under_the_stop_is_released() {
    let rig = Rig::new(Gate::default());
    let stop = Stop::new();
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone()),
    );
    rig.live(&orders(), 0..1, 0).await;
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    assert!(engine.lane_health(&lane(orders())).is_open());
    stop.request();
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    assert!(
        matches!(
            engine.lane_health(&lane(orders())),
            LaneHealth::Restaging { reason, .. } if reason.contains("could not be ensured during the stop")
        ),
        "left restaging with why: {:?}",
        engine.lane_health(&lane(orders()))
    );
    let task = spawn(Arc::clone(&engine));
    let idle = idle_within(&rig, Duration::from_secs(60)).await;
    task.abort();
    assert!(
        idle.is_some_and(|after| after < Duration::from_secs(2)),
        "drained at once: {idle:?}"
    );
    assert_eq!(writes_holding(&rig, &orders(), "0"), 0, "nothing written");
    assert_eq!(rig.applied(&orders()), before(0), "nothing applied");
}

/// Fourth review of #470, 3: a stop landing while a restarted run's
/// staging is already ensuring still makes it ensure only: no backfill is
/// asked for and a lane restaging after its target went missing is not
/// reopened, so it is released, not written into the recreated target.
#[tokio::test(start_paused = true)]
async fn a_stop_during_a_staging_makes_it_ensure_only() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::MissingForGood));
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let stop = Stop::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_backoff(slow_backoff())
            .with_stop(stop.clone())
            .with_observer(rebuild_events(&events)),
    );
    rig.live(&orders(), 0..1, 0).await;
    let task = spawn(Arc::clone(&engine));
    eventually("restaging", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    task.abort();
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    rig.sink.answer(&orders(), Some(Answer::Missing));
    *rig.sink.ensure_delay.lock().unwrap() = Some(Duration::from_secs(10));
    let before = events.lock().unwrap().len();
    let staging = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move { engine.stage(source::cdc::Continuity::Resumed).await }
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    stop.request();
    tokio::time::timeout(Duration::from_secs(30), staging)
        .await
        .expect("the staging ends")
        .unwrap()
        .unwrap();
    let after: Vec<String> = events.lock().unwrap()[before..].to_vec();
    assert!(after.is_empty(), "nothing asked for: {after:?}");
    assert_eq!(
        engine.lane_health(&lane(orders())).kind(),
        "restaging",
        "not reopened: the stop releases it"
    );
}

/// #471 (R2, R3), the issue's own: `orders` can't be ensured (its target
/// gone, its staging refused), and a write error on `users` fails the run.
/// The restart's staging goes on without `orders`: `users` writes again,
/// `orders` stays restaging, never reopened unensured, and recovers on its
/// restage path once it can be ensured.
#[tokio::test(start_paused = true)]
async fn a_restart_goes_on_without_the_index_it_cannot_ensure() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Missing));
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    rig.live(&orders(), 1..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let task = spawn(Arc::clone(&engine));
    eventually("orders restaging", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    *rig.sink.fail_id.lock().unwrap() = Some("9".to_owned());
    rig.live(&users(), 9..10, 1).await;
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed write");
    assert!(ended.unwrap().is_err());
    *rig.sink.fail_id.lock().unwrap() = None;

    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .expect("the restart's staging goes on without the index it can't ensure");
    assert_eq!(
        engine.lane_health(&lane(orders())).kind(),
        "restaging",
        "never reopened while it can't be ensured"
    );
    let task = spawn(Arc::clone(&engine));
    rig.live(&users(), 10..11, 2).await;
    eventually("users writes after the restart", || {
        writes_holding(&rig, &users(), "10") >= 1
    })
    .await;
    assert_eq!(engine.lane_health(&lane(orders())).kind(), "restaging");

    rig.sink.ensure_fails_for.lock().unwrap().clear();
    tokio::time::sleep(Duration::from_secs(61)).await;
    eventually("orders recovers on its restage path", || {
        writes_holding(&rig, &orders(), "1") >= 2
    })
    .await;
    task.abort();
}

/// #471 (R2): a start under `Resumed` goes on without an index it can't
/// ensure too, nothing needing a rebuild: that lane is restaging with why,
/// the others open.
#[tokio::test(start_paused = true)]
async fn a_resumed_start_goes_on_without_the_index_it_cannot_ensure() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let engine = rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff());
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .expect("staging goes on without orders");
    assert!(
        matches!(
            engine.lane_health(&lane(orders())),
            LaneHealth::Restaging { reason, .. } if reason.contains("could not be ensured")
        ),
        "{:?}",
        engine.lane_health(&lane(orders()))
    );
    assert!(engine.lane_health(&lane(users())).is_open());
}

/// #471 (R1): when no index could be ensured the destination is down, not
/// an index: the staging fails as a whole and no lane is marked.
#[tokio::test(start_paused = true)]
async fn a_staging_none_of_whose_indexes_is_ensured_fails_as_a_whole() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .ensure_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let engine = rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff());
    assert!(
        engine
            .stage(source::cdc::Continuity::Resumed)
            .await
            .is_err()
    );
    for of in [orders(), users()] {
        assert!(engine.lane_health(&lane(of)).is_open(), "no lane marked");
    }
}

/// #471 (R4): with a `Fresh` source an index that can't be ensured can't be
/// told seeded or not, so whether it needs a rebuild before the source is
/// prepared is unknown: the first staging waits, failing as a whole.
#[tokio::test(start_paused = true)]
async fn a_fresh_staging_waits_for_every_index() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let engine = rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff());
    assert!(engine.stage(source::cdc::Continuity::Fresh).await.is_err());
}

/// X17 (review 2): the owed reindex is on the lane's state, so a run that
/// fails meanwhile keeps it: staging at the next start leaves the lane
/// `restaging`, and the next run's retry stages the reindex.
#[tokio::test(start_paused = true)]
async fn an_owed_reindex_outlives_a_failed_run() {
    let rig = Rig::new(Gate::default());
    rig.sink
        .ensure_fails_for
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    eventually("owed", || {
        engine.lane_health(&lane(orders())).kind() == "restaging"
    })
    .await;
    *rig.sink.fail_id.lock().unwrap() = Some("9".to_owned());
    rig.live(&users(), 9..10, 0).await;
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed write");
    assert!(ended.unwrap().is_err());
    *rig.sink.fail_id.lock().unwrap() = None;
    rig.sink.ensure_fails_for.lock().unwrap().clear();
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    assert_eq!(
        engine.lane_health(&lane(orders())).kind(),
        "restaging",
        "staging the run leaves the owed reindex to its retry"
    );
    let (_control, task) = spawn_controlled(Arc::clone(&engine));
    let (index, ticket) = next_backfill(&rig).await;
    assert_eq!(index, orders(), "the retry stages the reindex");
    complete(&rig, &orders(), ticket).await;
    eventually("open", || engine.lane_health(&lane(orders())).is_open()).await;
    task.abort();
}

/// X12: a reindex staged in the first pass of a run after a failure, before
/// the parked item is redelivered, still holds it when it comes: it is never
/// written, and the seed acknowledges it.
#[tokio::test(start_paused = true)]
async fn a_reindex_staged_before_the_parked_item_is_redelivered_still_holds_it() {
    let rig = Rig::new(Gate::default());
    rig.sink.answer(&orders(), Some(Answer::Refuse));
    rig.live(&orders(), 1..2, 0).await;
    let engine = Arc::new(rig.engine(FailurePolicy::Stop).with_backoff(slow_backoff()));
    let (control, task) = spawn_controlled(Arc::clone(&engine));
    eventually("the lane parks", || {
        !engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    // A users write takes 5 s, then fails the run; the reindex arrives while
    // it is out, so it waits for the next run.
    rig.sink
        .slow_ids
        .lock()
        .unwrap()
        .insert("9".to_owned(), Duration::from_secs(5));
    *rig.sink.fail_id.lock().unwrap() = Some("9".to_owned());
    rig.live(&users(), 9..10, 1).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    rig.sink.answer(&orders(), None);
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .expect("the run ends on the failed write");
    assert!(ended.unwrap().is_err());
    *rig.sink.fail_id.lock().unwrap() = None;
    engine
        .stage(source::cdc::Continuity::Resumed)
        .await
        .unwrap();
    let (control2, task) = spawn_controlled(Arc::clone(&engine));
    let (_, ticket) = next_backfill(&rig).await;
    eventually("the lane is open", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        writes_holding(&rig, &orders(), "1"),
        1,
        "the parked item, redelivered after the reindex, is held"
    );
    complete(&rig, &orders(), ticket).await;
    eventually("seeded: everything acknowledged", || rig.stream.is_idle()).await;
    assert_eq!(rig.applied(&orders()), through(0));
    drop((control, control2));
    task.abort();
}
