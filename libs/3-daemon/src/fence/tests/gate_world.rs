//! The third re-check, item 3: the O29 gate's property in the real world's
//! shape, through the real sink engines.
//!
//! Two sinks: `audit` (`backfill = false`: a detached lane is reattached)
//! and `primary` (a detached lane is rebuilt to its seed). Two indexes, so
//! four lanes. Events: openings that succeed or fail, one of them past what
//! another process dropped for one lane only (its detach record left in the
//! sink); sink-run restarts, whose staging may fail to read some indexes'
//! records (and the run's own read again with them); changes applied and made
//! searchable per lane; reattaches and rebuilds driven through the engines'
//! controls, the test playing the index's builder (`Attached`, the backfill);
//! `/status` reads; waits at both levels, on one lane or a whole sink.
//!
//! The oracle owes nothing to the engine's counting: the stand-in sink logs
//! every read of a lane's records that got through, with the opening it began
//! under, and keeps the records and gaps. A wait for `p` on a lane passes only
//! if the lane covered `p` while its records had been read under the current
//! stream with no detach record in them, it was neither detached nor
//! rebuilding, and no gap the sink keeps holds `p`. Liveness: once a healthy
//! lane is read, a change applied on it is vouched for on that lane alone,
//! whatever its unread sibling does.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use engine::{SinkControl, SinkEngine};
use futures::FutureExt;
use kernel::{Envelope, IndexMapping};
use proptest::prelude::*;
use sink::{DetachRecord, Detachment, MarkerAt, RecordVersion, Sink, WriteReport};
use stream::{LaneItem, Request, Stream};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::*;

const INDEXES: [&str; 2] = ["orders", "users"];
const SINKS: [&str; 2] = ["audit", "primary"];

fn lanes() -> Vec<LaneKey> {
    SINKS
        .iter()
        .flat_map(|s| INDEXES.iter().map(move |i| LaneKey::new(index(i), sink(s))))
        .collect()
}

fn mapping(of: &str) -> IndexMapping {
    IndexMapping {
        index: index(of),
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// One sink's meta store, and the log of its records' reads.
#[derive(Debug)]
struct Store {
    rereads: Arc<Rereads>,
    live_only: bool,
    kept: Mutex<BTreeMap<String, Detachment>>,
    gaps: Mutex<BTreeMap<String, Vec<sink::Gap>>>,
    unreadable: Mutex<BTreeSet<String>>,
    /// Each read of an index's records that got through: the opening it
    /// began under (the last one held), and whether it found a record.
    reads: Mutex<Vec<(String, u64)>>,
}

impl Store {
    fn new(rereads: Arc<Rereads>, live_only: bool) -> Self {
        Self {
            rereads,
            live_only,
            kept: Mutex::default(),
            gaps: Mutex::default(),
            unreadable: Mutex::default(),
            reads: Mutex::default(),
        }
    }

    /// Another process detached `of` and dropped its changes.
    fn dropped(&self, of: &str) {
        let (record, marker) = if self.live_only {
            (
                DetachRecord::Detached { live_only: true },
                MarkerAt::Serving { seeded: true },
            )
        } else {
            (
                DetachRecord::Detached { live_only: false },
                MarkerAt::Newer { seeded: false },
            )
        };
        self.kept.lock().unwrap().insert(
            of.to_owned(),
            Detachment::recorded(record, Some(marker), Some(RecordVersion::new("1:1"))),
        );
    }

    /// The last opening under which `of`'s records were read.
    fn read_for(&self, of: &str) -> u64 {
        self.reads
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| at == of)
            .map(|(_, epoch)| *epoch)
            .max()
            .unwrap_or(0)
    }
}

#[async_trait]
impl Sink for Store {
    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    async fn detachment(&self, mapping: &IndexMapping) -> sink::Result<Detachment> {
        let began = self.rereads.read_begins();
        let of = mapping.index.as_ref();
        if self.unreadable.lock().unwrap().contains(of) {
            return Err(sink::SinkError::Write(
                "the records can't be read".to_owned(),
            ));
        }
        let mut kept = self
            .kept
            .lock()
            .unwrap()
            .get(of)
            .cloned()
            .unwrap_or_default();
        kept.gaps = self
            .gaps
            .lock()
            .unwrap()
            .get(of)
            .cloned()
            .unwrap_or_default();
        self.reads.lock().unwrap().push((of.to_owned(), began));
        Ok(kept)
    }

    async fn detach(
        &self,
        mapping: &IndexMapping,
        _: &sink::Detach,
    ) -> sink::Result<sink::Recorded> {
        self.dropped(mapping.index.as_ref());
        Ok(sink::Recorded::Kept)
    }

    async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        _: &RecordVersion,
    ) -> sink::Result<()> {
        self.kept.lock().unwrap().remove(mapping.index.as_ref());
        Ok(())
    }

    async fn record_gaps(
        &self,
        mapping: &IndexMapping,
        decide: &sink::DecideGaps<'_>,
    ) -> sink::Result<Vec<sink::Gap>> {
        let mut gaps = self.gaps.lock().unwrap();
        let kept = gaps
            .get(mapping.index.as_ref())
            .cloned()
            .unwrap_or_default();
        let Some(new) = decide(&kept) else {
            return Ok(kept);
        };
        gaps.insert(mapping.index.to_string(), new.clone());
        Ok(new)
    }

    async fn begin_rebuild(&self, mapping: &IndexMapping) -> sink::Result<u64> {
        let mut kept = self.kept.lock().unwrap();
        let record = kept.entry(mapping.index.to_string()).or_default();
        let attempts = record.attempts.unwrap_or(0) + 1;
        *record = Detachment::recorded(
            DetachRecord::Rebuilding { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            Some(RecordVersion::new("2:1")),
        );
        record.attempts = Some(attempts);
        Ok(attempts)
    }

    async fn demote(&self, mapping: &IndexMapping, live_only: bool) -> sink::Result<()> {
        if let Some(record) = self.kept.lock().unwrap().get_mut(mapping.index.as_ref()) {
            record.record = Some(DetachRecord::Detached { live_only });
        }
        Ok(())
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        // The seed writes the marker whole: the record and the gaps go. A
        // seed this process made holds every change its walk read after the
        // attach, so for the oracle it stands for a read of the records under
        // the stream it was made in.
        self.kept.lock().unwrap().remove(index.as_ref());
        self.gaps.lock().unwrap().remove(index.as_ref());
        let began = self.rereads.read_begins();
        self.reads.lock().unwrap().push((index.to_string(), began));
        Ok(())
    }
}

/// What the engines report of each lane: detached, rebuilding.
#[derive(Debug, Default)]
struct Tap {
    detached: Mutex<BTreeSet<LaneKey>>,
    rebuilding: Mutex<BTreeSet<LaneKey>>,
}

impl engine::Observer for Tap {
    fn on_lane_health(&self, lane: &LaneKey, health: &engine::LaneHealth) {
        let mut detached = self.detached.lock().unwrap();
        if health.is_detached() {
            detached.insert(lane.clone());
        } else {
            detached.remove(lane);
        }
    }

    fn on_lane_rebuild(&self, lane: &LaneKey, rebuilding: Option<u64>, _: Option<u64>) {
        let mut set = self.rebuilding.lock().unwrap();
        if rebuilding.is_some() {
            set.insert(lane.clone());
        } else {
            set.remove(lane);
        }
    }
}

#[derive(Debug, Clone)]
enum Event {
    /// The stream opens, starting `jump` past where the source stood; when
    /// `drop` names a lane, another process dropped its changes up to there
    /// (and left a detach record); any other lane it applied.
    Open {
        ok: bool,
        jump: u64,
        drop: Option<usize>,
    },
    /// A restart of `sink`'s run: staging, the indexes in `unreadable`
    /// failing their read (and keep failing until `Readable`).
    Restart {
        sink: usize,
        unreadable: u8,
    },
    Readable {
        sink: usize,
    },
    /// A change of `index`, landed on the lanes in `on` that may write.
    Apply {
        index: usize,
        on: u8,
    },
    Searchable {
        lane: usize,
    },
    Horizon {
        by: u64,
    },
    Status,
    /// A reindex of `index` on `primary`: a rebuild when detached.
    Reindex {
        index: usize,
    },
    /// A reattach of `index` on `audit`.
    Reattach {
        index: usize,
    },
    Wait {
        lane: usize,
        whole_sink: bool,
        searchable: bool,
        at: u64,
    },
}

fn event() -> impl Strategy<Value = Event> {
    prop_oneof![
        3 => (any::<bool>(), 0u64..3, prop::option::of(0usize..4))
            .prop_map(|(ok, jump, drop)| Event::Open { ok, jump: jump * 10, drop }),
        3 => (0usize..2, 0u8..4).prop_map(|(sink, unreadable)| Event::Restart { sink, unreadable }),
        2 => (0usize..2).prop_map(|sink| Event::Readable { sink }),
        3 => (0usize..2, 0u8..4).prop_map(|(index, on)| Event::Apply { index, on }),
        1 => (0usize..4).prop_map(|lane| Event::Searchable { lane }),
        1 => (1u64..10).prop_map(|by| Event::Horizon { by }),
        1 => Just(Event::Status),
        1 => (0usize..2).prop_map(|index| Event::Reindex { index }),
        1 => (0usize..2).prop_map(|index| Event::Reattach { index }),
        4 => (0usize..4, any::<bool>(), any::<bool>(), 0u64..120).prop_map(
            |(lane, whole_sink, searchable, at)| Event::Wait { lane, whole_sink, searchable, at }
        ),
    ]
}

type Run = (JoinHandle<engine::Result<()>>, mpsc::Sender<SinkControl>);

struct World {
    source: Arc<NumberedSource>,
    progress: Arc<Progress>,
    rereads: Arc<Rereads>,
    fence: Fence,
    observer: Arc<engine::FanOut>,
    tap: Arc<Tap>,
    stream: Arc<ChannelStream>,
    stores: Vec<Arc<Store>>,
    engines: Vec<Arc<SinkEngine>>,
    runs: Vec<Option<Run>>,
    lanes: Vec<LaneKey>,
    /// Per lane and level, the highest position covered as the oracle allows.
    covered: Vec<[Option<u64>; 2]>,
    last: Vec<Option<Position>>,
    horizon: u64,
    /// Openings begun, as the oracle counts them.
    opening: u64,
}

impl World {
    fn new() -> Self {
        let source = Arc::new(NumberedSource::default());
        source.close();
        let lanes = lanes();
        let progress = Arc::new(Progress::new(lanes.iter().cloned()));
        let rereads = Arc::new(Rereads::default());
        let fence = Fence::new(
            Arc::clone(&source) as Arc<dyn ChangeCapture>,
            Arc::clone(&progress),
            lanes.iter().cloned(),
        )
        .with_rereads(Arc::clone(&rereads));
        let tap = Arc::new(Tap::default());
        let observer = Arc::new(engine::FanOut::new(vec![
            Arc::new(FenceObserver::new(fence.clone())) as Arc<dyn engine::Observer>,
            Arc::clone(&tap) as Arc<dyn engine::Observer>,
        ]));
        let stream = Arc::new(ChannelStream::over(
            256,
            INDEXES.map(index),
            SINKS.map(sink),
        ));
        let stores: Vec<Arc<Store>> = SINKS
            .iter()
            .map(|name| Arc::new(Store::new(Arc::clone(&rereads), *name == "audit")))
            .collect();
        let engines = SINKS
            .iter()
            .zip(&stores)
            .map(|(name, store)| {
                Arc::new(
                    SinkEngine::new(
                        sink(name),
                        Arc::clone(store) as Arc<dyn Sink>,
                        Arc::clone(&stream) as Arc<dyn Stream>,
                        INDEXES.iter().map(|i| mapping(i)).collect(),
                        Arc::clone(&progress),
                    )
                    .with_options(sink::SinkOptions {
                        backfill: *name != "audit",
                    })
                    .with_observer(Arc::clone(&observer) as Arc<dyn engine::Observer>)
                    .with_rereads(Arc::clone(&rereads))
                    .with_positions(Arc::new(engine::CapturePositions(
                        Arc::clone(&source) as Arc<dyn ChangeCapture>
                    ))),
                )
            })
            .collect();
        Self {
            source,
            progress,
            rereads,
            fence,
            observer,
            tap,
            stream,
            stores,
            engines,
            runs: vec![None, None],
            covered: vec![[None; 2]; 4],
            last: vec![None; 4],
            lanes,
            horizon: 0,
            opening: 0,
        }
    }

    fn sink_of(lane: usize) -> usize {
        lane / 2
    }

    fn index_of(lane: usize) -> &'static str {
        INDEXES[lane % 2]
    }

    async fn stop(&mut self, at: usize) {
        if let Some((task, _)) = self.runs[at].take() {
            task.abort();
            let _ = task.await;
        }
    }

    fn spawn(&mut self, at: usize) {
        let (send, mut control) = mpsc::channel(8);
        let engine = Arc::clone(&self.engines[at]);
        let task = tokio::spawn(async move { engine.run(&mut control).await });
        self.runs[at] = Some((task, send));
    }

    async fn restart(&mut self, at: usize) {
        self.stop(at).await;
        let _ = self.engines[at].stage(Continuity::Resumed).await;
        self.spawn(at);
    }

    async fn control(&mut self, at: usize, message: SinkControl) {
        if let Some((_, send)) = &self.runs[at] {
            let _ = send.send(message).await;
        }
    }

    /// Let the runs take what is ready, playing each index's builder for the
    /// requests they send: `Attached` for an attach, an empty backfill.
    async fn settle(&mut self) {
        let mut requests = self.stream.requests().unwrap().consumer;
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            while let Some(Ok(Some(delivery))) = requests.recv().now_or_never() {
                let (request, handle) = delivery.into_parts();
                let (key, item) = match request {
                    Request::Attach {
                        index,
                        sink,
                        attempt,
                    } => {
                        let key = LaneKey::new(index, sink);
                        let _ = self.progress.attach(&key, attempt);
                        (key, LaneItem::Attached { attempt })
                    }
                    Request::Backfill {
                        index,
                        sink,
                        ticket,
                        ..
                    } => {
                        let key = LaneKey::new(index.clone(), sink);
                        (key, LaneItem::SnapshotComplete { index, ticket })
                    }
                    _ => {
                        let _ = handle.ack().await;
                        continue;
                    }
                };
                if let Ok(lane) = self.stream.lane(&key) {
                    let _ = lane.producer.publish(item).now_or_never();
                }
                let _ = handle.ack().await;
            }
            // A run that ended on a failed read is restarted by the daemon.
            for at in 0..2 {
                if self.runs[at]
                    .as_ref()
                    .is_some_and(|(task, _)| task.is_finished())
                {
                    self.restart(at).await;
                }
            }
        }
    }

    /// The oracle: the lane's records read under the current stream with no
    /// record in them, the lane neither detached nor rebuilding.
    fn open(&self, lane: usize) -> bool {
        let key = &self.lanes[lane];
        let store = &self.stores[Self::sink_of(lane)];
        let of = Self::index_of(lane);
        let opened = self.opened();
        opened > 0
            && self.rereads.read_begins() == opened
            && store.read_for(of) >= opened
            && !store.kept.lock().unwrap().contains_key(of)
            && !self.tap.detached.lock().unwrap().contains(key)
            && !self.tap.rebuilding.lock().unwrap().contains(key)
    }

    fn opened(&self) -> u64 {
        self.opening
    }

    fn in_gap(&self, lane: usize, at: u64) -> bool {
        let store = &self.stores[Self::sink_of(lane)];
        let gaps = store.gaps.lock().unwrap();
        let parse = |text: &Option<String>| text.as_deref().and_then(|t| t.parse::<u64>().ok());
        gaps.get(Self::index_of(lane)).is_some_and(|gaps| {
            gaps.iter().any(|gap| {
                parse(&gap.from).is_none_or(|from| at > from)
                    && parse(&gap.to).is_none_or(|to| at <= to)
            })
        })
    }

    fn cover(&mut self) {
        for lane in 0..4 {
            if !self.open(lane) {
                continue;
            }
            for (level, which) in [(0, Level::Applied), (1, Level::Searchable)] {
                if let Some(SourcePosition(at)) = self
                    .progress
                    .lane(&self.lanes[lane], which)
                    .and_then(|landed| self.source.reached(landed))
                {
                    let covered = &mut self.covered[lane][level];
                    *covered = Some(covered.map_or(at, |was| was.max(at)));
                }
            }
        }
    }

    fn apply(&mut self, at: usize, on: u8) {
        self.horizon += 5;
        let change = self.source.emit(self.horizon);
        for sink_at in 0..2 {
            self.progress.published(
                &self.lanes[sink_at * 2 + at],
                stream::ItemKey::new(change.seq),
                change,
                change,
            );
        }
        self.progress.routed(change);
        for sink_at in 0..2 {
            let lane = sink_at * 2 + at;
            let key = self.lanes[lane].clone();
            // What the engine would write: a lane its own count says is read,
            // not detached.
            if on & (1 << sink_at) != 0
                && self.rereads.current(&key)
                && !self.tap.detached.lock().unwrap().contains(&key)
            {
                self.progress
                    .applied(&key, stream::ItemKey::new(change.seq));
                self.observer.on_position_applied(&key, change);
                self.last[lane] = Some(change);
            }
        }
    }
}

impl World {
    // Kept beside the struct for the oracle: the openings begun.
    fn begin_opening(&mut self) {
        self.opening += 1;
    }
}

async fn run(events: Vec<Event>) -> std::result::Result<(), TestCaseError> {
    let mut world = World::new();
    for at in 0..2 {
        world.restart(at).await;
    }
    for event in events {
        match event.clone() {
            Event::Open { ok, jump, drop } => {
                let start = world.horizon + jump;
                if let Some(lane) = drop.filter(|_| jump > 0) {
                    world.stores[World::sink_of(lane)].dropped(World::index_of(lane));
                }
                world.begin_opening();
                let source = Arc::clone(&world.source);
                let opened = world
                    .rereads
                    .open(async move {
                        source.closed.store(false, Ordering::SeqCst);
                        source.idle_to(start);
                        if ok { Ok(()) } else { Err(()) }
                    })
                    .await;
                prop_assert_eq!(opened.is_ok(), ok);
                world.horizon = start;
            }
            Event::Restart {
                sink: at,
                unreadable,
            } => {
                *world.stores[at].unreadable.lock().unwrap() = INDEXES
                    .iter()
                    .enumerate()
                    .filter(|(bit, _)| unreadable & (1 << bit) != 0)
                    .map(|(_, name)| (*name).to_owned())
                    .collect();
                world.restart(at).await;
            }
            Event::Readable { sink: at } => world.stores[at].unreadable.lock().unwrap().clear(),
            Event::Apply { index: at, on } => world.apply(at, on),
            Event::Searchable { lane } => {
                if let Some(last) = world.last[lane] {
                    let key = world.lanes[lane].clone();
                    world
                        .progress
                        .searchable(&key, stream::ItemKey::new(last.seq));
                    world.observer.on_position_searchable(&key, last);
                }
            }
            Event::Horizon { by } => {
                world.horizon += by;
                world.source.idle_to(world.horizon);
            }
            Event::Status => {
                let _ = world.fence.positions();
            }
            Event::Reindex { index: at } => {
                world
                    .control(
                        1,
                        SinkControl::Reindex {
                            indexes: vec![index(INDEXES[at])],
                        },
                    )
                    .await;
            }
            Event::Reattach { index: at } => {
                world
                    .control(
                        0,
                        SinkControl::Reattach {
                            index: index(INDEXES[at]),
                            reply: engine::Reply::ignored(),
                        },
                    )
                    .await;
            }
            Event::Wait {
                lane,
                whole_sink,
                searchable,
                at,
            } => {
                let key = world.lanes[lane].clone();
                let selected: Vec<usize> = if whole_sink {
                    vec![World::sink_of(lane) * 2, World::sink_of(lane) * 2 + 1]
                } else {
                    vec![lane]
                };
                let request = FenceRequest {
                    position: at.to_string(),
                    level: if searchable {
                        FenceLevel::Searchable
                    } else {
                        FenceLevel::Applied
                    },
                    sink: Some(key.sink.clone()),
                    indexes: if whole_sink {
                        Vec::new()
                    } else {
                        vec![key.index.clone()]
                    },
                    timeout: Duration::from_millis(10),
                };
                if let Ok(FenceOutcome::Reached { .. }) = world.fence.wait(&request).await {
                    let level = usize::from(searchable);
                    for lane in selected {
                        prop_assert!(
                            world.covered[lane][level].is_some_and(|covered| at <= covered)
                                && !world.in_gap(lane, at),
                            "the fence passed a wait for {at} ({}) on {}; covered {:?}, open {}; \
                             opened {} held {} read {} kept {:?} detached {:?} rebuilding {:?} \
                             engine-current {} health {:?}",
                            if searchable { "searchable" } else { "applied" },
                            world.lanes[lane],
                            world.covered[lane],
                            world.open(lane),
                            world.opening,
                            world.rereads.read_begins(),
                            world.stores[World::sink_of(lane)].read_for(World::index_of(lane)),
                            world.stores[World::sink_of(lane)]
                                .kept
                                .lock()
                                .unwrap()
                                .keys()
                                .collect::<Vec<_>>(),
                            world.tap.detached.lock().unwrap(),
                            world.tap.rebuilding.lock().unwrap(),
                            world.rereads.current(&world.lanes[lane]),
                            world.engines[World::sink_of(lane)].lane_health(&world.lanes[lane]),
                        );
                    }
                }
            }
        }
        world.settle().await;
        world.cover();
    }

    // Liveness: the slot held, every record readable but `orders` on
    // `primary`, `primary` restarted: a change of `users` applied on its
    // healthy lane is vouched for on that lane alone.
    let users = 3;
    world.stores[1].kept.lock().unwrap().remove("users");
    world.stores[1].gaps.lock().unwrap().remove("users");
    if world
        .tap
        .detached
        .lock()
        .unwrap()
        .contains(&world.lanes[users])
        || world
            .tap
            .rebuilding
            .lock()
            .unwrap()
            .contains(&world.lanes[users])
    {
        return Ok(());
    }
    world.begin_opening();
    let source = Arc::clone(&world.source);
    world
        .rereads
        .open(async move {
            source.closed.store(false, Ordering::SeqCst);
            Ok::<(), ()>(())
        })
        .await
        .unwrap();
    *world.stores[1].unreadable.lock().unwrap() = ["orders".to_owned()].into();
    world.restart(1).await;
    // Long enough for a reindex owed or a rebuild under way to finish: the
    // staging backoff doubles to a minute.
    for _ in 0..40 {
        world.settle().await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    world.horizon += 5;
    let change = world.source.emit(world.horizon);
    let key = world.lanes[users].clone();
    world
        .progress
        .published(&key, stream::ItemKey::new(change.seq), change, change);
    world
        .progress
        .applied(&key, stream::ItemKey::new(change.seq));
    world.observer.on_position_applied(&key, change);
    let at = world.horizon.to_string();
    prop_assert_eq!(
        world
            .fence
            .wait(&FenceRequest {
                indexes: vec![index("users")],
                ..request(&at, FenceLevel::Applied, Some("primary"))
            })
            .await,
        reached(&at),
        "read and healthy, users on primary is vouched for beside an unread orders"
    );
    for at in 0..2 {
        world.stop(at).await;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn every_lane_vouches_only_for_what_it_covered_while_read(
        events in prop::collection::vec(event(), 1..40)
    ) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
            .block_on(run(events))?;
    }
}
