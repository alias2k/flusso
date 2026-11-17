//! Stage timings (issue #399): each stage is reported once per batch or lane
//! item, at the point it ends, with the time it took. The mocks sleep on a
//! paused clock, so every timing is exact.

use super::*;
use crate::{Stage, StageTiming, StartupStep, Work};

/// One stage timing as recorded: stage, work, sink, how long.
type Timing = (Stage, Option<Work>, Option<String>, Duration);

/// Records every timing, start-up step and commit.
#[derive(Debug, Default)]
struct Timings {
    stages: Mutex<Vec<Timing>>,
    startup: Mutex<Vec<(StartupStep, Option<String>, Duration)>>,
    indexed: Mutex<Vec<(Stage, Option<String>, Option<String>, Duration)>>,
    commits: Mutex<Vec<(Work, usize, Option<u64>)>>,
}

impl Timings {
    fn of(&self, stage: Stage) -> Vec<Timing> {
        self.stages
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.0 == stage)
            .cloned()
            .collect()
    }

    fn total(&self, stage: Stage) -> Duration {
        self.of(stage).iter().map(|t| t.3).sum()
    }
}

impl Observer for Timings {
    fn on_stage(&self, timing: StageTiming<'_>) {
        if let Some(index) = timing.index {
            self.indexed.lock().unwrap().push((
                timing.stage,
                timing.sink.map(ToString::to_string),
                Some(index.to_string()),
                timing.elapsed,
            ));
        }
        self.stages.lock().unwrap().push((
            timing.stage,
            timing.work,
            timing.sink.map(ToString::to_string),
            timing.elapsed,
        ));
    }

    fn on_startup_step(&self, step: StartupStep, sink: Option<&SinkName>, elapsed: Duration) {
        self.startup
            .lock()
            .unwrap()
            .push((step, sink.map(ToString::to_string), elapsed));
    }

    fn on_batch_committed(&self, _: &SinkName, stats: CommitStats) {
        self.commits
            .lock()
            .unwrap()
            .push((stats.work, stats.envelopes, stats.bytes));
    }
}

const RESOLVE: Duration = Duration::from_millis(2);
const BUILD: Duration = Duration::from_millis(30);
const APPLY: Duration = Duration::from_millis(1);
const FLUSH: Duration = Duration::from_millis(50);
const ROW_FETCH: Duration = Duration::from_millis(10);
const SEED: Duration = Duration::from_millis(20);
const REINDEX: Duration = Duration::from_millis(40);

/// [`MockDocuments`] that takes [`RESOLVE`] per change and [`BUILD`] per
/// batch.
#[derive(Debug, Default)]
struct SlowDocuments(MockDocuments);

#[async_trait]
impl DocumentBuilder for SlowDocuments {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        tokio::time::sleep(RESOLVE).await;
        self.0.resolve(table, key, before, after).await
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        self.0.build(id).await
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        tokio::time::sleep(BUILD).await;
        self.0.build_many(ids).await
    }

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        self.0.backfill_scopes()
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        self.0.index_mappings().await
    }
}

/// A sink taking [`APPLY`] per envelope, [`FLUSH`] per flush, [`SEED`] per
/// index seeded and [`REINDEX`] per reindex, reporting 100 bytes per document.
#[derive(Debug)]
struct SlowSink {
    seeded: bool,
    pending: AtomicUsize,
}

impl SlowSink {
    fn new(seeded: bool) -> Arc<Self> {
        Arc::new(Self {
            seeded,
            pending: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl Sink for SlowSink {
    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        for _ in envelopes {
            tokio::time::sleep(APPLY).await;
            self.pending.fetch_add(1, Ordering::SeqCst);
        }
        tokio::time::sleep(FLUSH).await;
        let documents = self.pending.swap(0, Ordering::SeqCst);
        Ok(WriteReport::clean().sent(documents as u64 * 100))
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded)
    }

    async fn mark_seeded(&self, _: &IndexName) -> sink::Result<()> {
        tokio::time::sleep(SEED).await;
        Ok(())
    }

    async fn reindex(&self, _: &IndexMapping) -> sink::Result<()> {
        tokio::time::sleep(REINDEX).await;
        Ok(())
    }
}

/// A source whose snapshot fetches each row for [`ROW_FETCH`].
#[derive(Debug)]
struct SlowSnapshot(MockSource);

#[async_trait]
impl ChangeCapture for SlowSnapshot {
    async fn continuity(&self) -> source::Result<Continuity> {
        self.0.continuity().await
    }

    async fn prepare(&self) -> source::Result<()> {
        self.0.prepare().await
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        self.0.live().await
    }

    fn confirm(&self, position: Position) {
        self.0.confirm(position);
    }

    async fn snapshot(
        &self,
        _: &[SnapshotTable],
    ) -> source::Result<BoxStream<'static, source::Result<ChangeEvent>>> {
        use futures::StreamExt;
        let rows = self.0.snapshot_rows.clone();
        Ok(Box::pin(futures::stream::iter(rows).then(|row| async {
            tokio::time::sleep(ROW_FETCH).await;
            Ok(row)
        })))
    }
}

/// Run one ingest engine over `source` and one sink engine over `sink`, on a
/// lane holding `capacity` items, until the live stream ends and the lane is
/// drained.
async fn run(
    source: Arc<dyn ChangeCapture>,
    sink: Arc<dyn Sink>,
    capacity: usize,
    batch: BatchPolicy,
) -> Arc<Timings> {
    let timings = Arc::new(Timings::default());
    let observer: Arc<dyn Observer> = Arc::clone(&timings) as Arc<dyn Observer>;
    let primary = sink_name("primary");
    let stream = Arc::new(ChannelStream::new(capacity, [primary.clone()]));
    let engine = SinkEngine::new(
        primary.clone(),
        sink,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
    )
    .with_observer(Arc::clone(&observer));
    engine.stage(Continuity::Resumed).await.unwrap();
    let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    let task = tokio::spawn(async move { engine.run(&mut rx).await });
    IngestEngine::new(
        source,
        Arc::new(SlowDocuments::default()),
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary],
    )
    .with_batch(batch)
    .with_observer(observer)
    .run()
    .await
    .unwrap();
    while !stream.is_idle() {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    task.abort();
    timings
}

fn batch(max_changes: usize) -> BatchPolicy {
    BatchPolicy {
        max_changes,
        max_delay: Duration::from_secs(5),
    }
}

#[tokio::test(start_paused = true)]
async fn a_live_batch_reports_each_stage_once_with_the_time_it_took() {
    let source = Arc::new(MockSource::new(vec![upsert(1), upsert(2), upsert(3)]));
    let timings = run(source, SlowSink::new(true), 64, batch(256)).await;

    let live = Some(Work::Live);
    let primary = Some("primary".to_owned());
    assert_eq!(
        timings.of(Stage::Resolve),
        vec![(Stage::Resolve, live, None, RESOLVE * 3)],
        "three changes ready at once are one batch, resolved one after another"
    );
    assert_eq!(
        timings.of(Stage::Build),
        vec![(Stage::Build, live, None, BUILD)]
    );
    let publish = timings.of(Stage::Publish);
    assert_eq!(publish.len(), 1);
    assert_eq!(publish[0].1, live);
    assert!(publish[0].3 < Duration::from_millis(1), "{publish:?}");
    let lane_wait = timings.of(Stage::LaneWait);
    assert_eq!(lane_wait.len(), 1);
    assert_eq!(lane_wait[0].2, primary);
    assert!(lane_wait[0].3 <= publish[0].3);
    assert_eq!(
        timings.of(Stage::Write),
        vec![(Stage::Write, live, primary.clone(), APPLY * 3 + FLUSH)],
        "a write is the item's apply and flush together"
    );
    let sink_idle: Vec<Timing> = timings
        .of(Stage::Idle)
        .into_iter()
        .filter(|t| t.2.is_some())
        .collect();
    assert_eq!(
        sink_idle.len(),
        1,
        "one lane item, one idle stretch before it"
    );
    assert_eq!((sink_idle[0].1, sink_idle[0].2.clone()), (None, primary));
    assert!(
        sink_idle[0].3 >= RESOLVE * 3 + BUILD,
        "the sink sat idle while the batch was resolved and built: {sink_idle:?}"
    );
    assert!(timings.of(Stage::Page).is_empty(), "no backfill ran");
    assert_eq!(
        *timings.commits.lock().unwrap(),
        vec![(Work::Live, 3, Some(300))],
        "the flush's size comes from the sink's report"
    );
}

#[tokio::test(start_paused = true)]
async fn a_full_lane_shows_as_time_waiting_to_publish() {
    let changes: Vec<ChangeEvent> = (1..=6).map(upsert).collect();
    let source = Arc::new(MockSource::new(changes));
    let timings = run(source, SlowSink::new(true), 1, batch(1)).await;

    let lane_wait = timings.of(Stage::LaneWait);
    let publish = timings.of(Stage::Publish);
    assert_eq!(lane_wait.len(), 6);
    assert_eq!(publish.len(), 6);
    assert!(
        publish.iter().map(|t| t.3).max().unwrap() >= FLUSH - BUILD - RESOLVE,
        "a batch built while the lane is full waits for the sink's flush: {publish:?}"
    );
    for (wait, whole) in lane_wait.iter().zip(&publish) {
        assert!(
            wait.3 <= whole.3,
            "a lane never waits longer than the publish"
        );
    }
    assert_eq!(timings.total(Stage::Write), (APPLY + FLUSH) * 6);
    assert!(
        timings.total(Stage::Publish) > Duration::ZERO,
        "the ingest side was blocked on the lane"
    );
}

#[tokio::test(start_paused = true)]
async fn a_backfill_reports_its_pages_snapshot_and_seed_and_tags_its_work() {
    let rows: Vec<ChangeEvent> = (1..=5).map(upsert).collect();
    let source = Arc::new(SlowSnapshot(
        MockSource::new(Vec::new()).with_snapshot(rows),
    ));
    let timings = run(source, SlowSink::new(false), 64, batch(2)).await;

    let backfill = Some(Work::Backfill);
    let page = timings.of(Stage::Page);
    assert!(page.iter().all(|t| t.1 == backfill && t.2.is_none()));
    assert_eq!(
        timings.total(Stage::Page),
        ROW_FETCH * 5,
        "every row's fetch is counted once: {page:?}"
    );
    assert!(
        timings
            .of(Stage::Idle)
            .iter()
            .filter(|t| t.2.is_none())
            .all(|t| t.3 < ROW_FETCH),
        "a wait for the snapshot is a page, not ingest idle time"
    );
    let builds = timings.of(Stage::Build);
    assert_eq!(builds.len(), 3, "five rows in batches of two");
    assert!(builds.iter().all(|t| t.1 == backfill && t.3 == BUILD));
    assert!(
        timings
            .of(Stage::Write)
            .iter()
            .all(|t| t.1 == backfill && t.2.as_deref() == Some("primary"))
    );
    let snapshot = timings.of(Stage::Snapshot);
    assert_eq!(
        snapshot.len(),
        2,
        "opened once and closed once: {snapshot:?}"
    );
    assert!(snapshot.iter().all(|t| t.1 == backfill && t.2.is_none()));
    assert_eq!(
        *timings.indexed.lock().unwrap(),
        vec![(
            Stage::Seed,
            Some("primary".to_owned()),
            Some("users".to_owned()),
            SEED
        )],
        "recording the index as seeded is the sink's own stage, per index"
    );
    assert_eq!(
        *timings.commits.lock().unwrap(),
        vec![
            (Work::Backfill, 2, Some(200)),
            (Work::Backfill, 2, Some(200)),
            (Work::Backfill, 1, Some(100)),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_quiet_pipeline_reports_idle_every_second_and_a_reindex_is_not_idle() {
    let timings = Arc::new(Timings::default());
    let observer: Arc<dyn Observer> = Arc::clone(&timings) as Arc<dyn Observer>;
    let primary = sink_name("primary");
    let stream = Arc::new(ChannelStream::new(64, [primary.clone()]));
    let engine = SinkEngine::new(
        primary.clone(),
        SlowSink::new(true),
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![mapping()],
    )
    .with_observer(Arc::clone(&observer));
    engine.stage(Continuity::Resumed).await.unwrap();
    let (control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    let sink_task = tokio::spawn(async move { engine.run(&mut rx).await });
    let (source, feed) = MockSource::fed();
    let ingest = IngestEngine::new(
        Arc::new(source),
        Arc::new(SlowDocuments::default()),
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![primary],
    )
    .with_observer(observer);
    let ingest_task = tokio::spawn(async move { ingest.run().await });

    let started = tokio::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(3_500)).await;
    let quiet = |side: fn(&Timing) -> bool| -> Vec<Duration> {
        timings
            .of(Stage::Idle)
            .into_iter()
            .filter(|t| side(t))
            .map(|t| t.3)
            .collect()
    };
    for (task, idle) in [
        ("ingest", quiet(|t| t.2.is_none())),
        ("sink", quiet(|t| t.2.is_some())),
    ] {
        assert!(
            idle.len() >= 3,
            "{task}: a quiet task reports idle every second: {idle:?}"
        );
        assert!(
            idle.iter().all(|d| *d <= Duration::from_secs(1)),
            "{task}: no idle stretch is held back past the tick: {idle:?}"
        );
    }

    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let reindex = timings.of(Stage::Reindex);
    assert_eq!(
        reindex,
        vec![(Stage::Reindex, None, Some("primary".to_owned()), REINDEX)],
        "a reindex is timed as itself"
    );
    let sink_idle: Duration = quiet(|t| t.2.is_some()).into_iter().sum();
    let elapsed = started.elapsed();
    assert!(
        sink_idle + REINDEX <= elapsed,
        "the reindex is not idle time: idle {sink_idle:?} + {REINDEX:?} > {elapsed:?}"
    );

    drop(feed);
    ingest_task.await.unwrap().unwrap();
    sink_task.abort();
}

#[tokio::test(start_paused = true)]
async fn start_up_reports_staging_per_sink_then_prepare_and_the_live_stream() {
    let source = Arc::new(MockSource::new(vec![upsert(1)]));
    let timings = run(source, SlowSink::new(true), 64, batch(256)).await;

    let steps: Vec<(StartupStep, Option<String>)> = timings
        .startup
        .lock()
        .unwrap()
        .iter()
        .map(|(step, sink, _)| (*step, sink.clone()))
        .collect();
    assert_eq!(
        steps,
        vec![
            (StartupStep::Stage, Some("primary".to_owned())),
            (StartupStep::Prepare, None),
            (StartupStep::OpenLive, None),
        ]
    );
}
