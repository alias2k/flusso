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
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<()> {
        let name = mapping.index.to_string();
        self.ensured.lock().unwrap().push(name.clone());
        self.ensured_at.lock().unwrap().push(Instant::now());
        let mut answers = self.answers.lock().unwrap();
        if answers.get(&name) == Some(&Answer::Missing) {
            answers.remove(&name);
            self.unseeded.lock().unwrap().insert(name);
        }
        Ok(())
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
            return Ok(WriteReport::clean());
        }
        let started = Instant::now();
        let slow = self.slow_id.lock().unwrap().clone();
        if let Some(slow) = slow
            && parts.iter().any(|part| part.iter().any(|e| e.id == slow))
        {
            tokio::time::sleep(Duration::from_secs(5)).await;
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
        Ok(report)
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(!self.unseeded.lock().unwrap().contains(index.as_ref()))
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
        position: position.map(Position::new),
        changes: 1,
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
    assert_eq!(rig.progress.pinned_by(), Some(lane(orders())));
    match engine.lane_health(&lane(orders())) {
        LaneHealth::Parked {
            reason: ParkReason::Refused { ids, .. },
            ..
        } => assert_eq!(ids, ["0", "1"]),
        other => panic!("orders should be parked on its refusal, is {other:?}"),
    }
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(rig.observer.kinds(&lane(orders())), ["parked"]);
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
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_max_probe_backoff(Duration::from_secs(4)),
    );
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
    let engine = Arc::new(
        rig.engine(FailurePolicy::Stop)
            .with_max_probe_backoff(Duration::from_secs(30)),
    );
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
    assert_eq!(engine.lane_health(&lane(orders())).kind(), "parked");
    assert_eq!(rig.observer.kinds(&lane(orders())).first(), Some(&"parked"));
    assert!(
        !rig.stream.is_idle(),
        "the refused page stays unacknowledged"
    );
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
