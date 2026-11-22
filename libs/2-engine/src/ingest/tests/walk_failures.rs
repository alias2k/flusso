//! Walks that fail, over two sinks (adopted from the review of #456, round
//! 2; ADR 0012 stage 8a, slice 8-1 fix rounds 1 and 2): a failed walk
//! resumes each lane after the last walk mark published to that lane, it
//! fails alone (the index's live documents keep flowing), and its backoff
//! starts over once it published past a failure. Two sinks (`primary` and
//! `audit`), walks that can be resumed (`Behaviour::resumable`), each sink's
//! documents compared with the rows at the end. Every wait is bounded.

use std::collections::BTreeSet;

use super::*;
use crate::observer::Observer;
use sink::Seeding;

fn audit() -> SinkName {
    SinkName::try_new("audit").unwrap()
}

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

/// A sink over a [`Recorder`] that records its walk marks as a real sink
/// does, and answers staging with the mark it holds (8b).
#[derive(Debug)]
struct Marking {
    inner: Arc<Recorder>,
    walked: Mutex<BTreeMap<String, String>>,
    /// Every mark recorded, per index, in order.
    recorded: Mutex<BTreeMap<String, Vec<i64>>>,
}

impl Marking {
    fn new(inner: Arc<Recorder>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            walked: Mutex::default(),
            recorded: Mutex::default(),
        })
    }
}

#[async_trait]
impl Sink for Marking {
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        self.inner.write(envelopes, caught_up).await
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        self.inner.is_seeded(index).await
    }

    async fn seeding(&self, index: &IndexName) -> sink::Result<Seeding> {
        Ok(if self.inner.is_seeded(index).await? {
            Seeding::Seeded
        } else {
            Seeding::Unseeded {
                walked: self.walked.lock().unwrap().get(index.as_ref()).cloned(),
            }
        })
    }

    async fn record_walked(&self, index: &IndexName, mark: &str) -> sink::Result<()> {
        self.walked
            .lock()
            .unwrap()
            .insert(index.as_ref().to_owned(), mark.to_owned());
        self.recorded
            .lock()
            .unwrap()
            .entry(index.as_ref().to_owned())
            .or_default()
            .push(mark.parse().unwrap());
        Ok(())
    }

    async fn mark_seeded(&self, index: &IndexName) -> sink::Result<()> {
        self.walked.lock().unwrap().remove(index.as_ref());
        self.inner.mark_seeded(index).await
    }
}

/// The test builder, whose `orders` key reads fail at the given read
/// numbers (0 is the bound, 1 the first page), and which counts every
/// resumed point it is asked to read back.
#[derive(Debug)]
struct Reads {
    inner: Arc<Builder>,
    fail_at: Mutex<BTreeSet<usize>>,
    reads: AtomicUsize,
    resumed: Mutex<Vec<String>>,
}

impl Reads {
    fn over(inner: Arc<Builder>, fail_at: impl IntoIterator<Item = usize>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            fail_at: Mutex::new(fail_at.into_iter().collect()),
            reads: AtomicUsize::new(0),
            resumed: Mutex::default(),
        })
    }

    fn read(&self, index: &IndexName) -> source::Result<()> {
        if *index != orders() {
            return Ok(());
        }
        let read = self.reads.fetch_add(1, Ordering::SeqCst);
        if self.fail_at.lock().unwrap().remove(&read) {
            return Err(SourceError::Query(format!(
                "orders' key read {read} failed"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl DocumentBuilder for Reads {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        self.inner.resolve(table, key, before, after).await
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        self.inner.build(id).await
    }

    async fn build_many(&self, ids: &[DocumentId]) -> source::Result<Vec<Document>> {
        self.inner.build_many(ids).await
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        self.inner.backfill_indexes()
    }

    async fn last_key(&self, index: &IndexName) -> source::Result<Option<RowKey>> {
        self.read(index)?;
        self.inner.last_key(index).await
    }

    async fn keys(
        &self,
        index: &IndexName,
        after: Option<&RowKey>,
        through: &RowKey,
        limit: usize,
    ) -> source::Result<Vec<RowKey>> {
        self.read(index)?;
        self.inner.keys(index, after, through, limit).await
    }

    async fn index_mappings(&self) -> source::Result<Vec<IndexMapping>> {
        self.inner.index_mappings().await
    }

    async fn walk_mark(&self, index: &IndexName, key: &RowKey) -> source::Result<Option<String>> {
        self.inner.walk_mark(index, key).await
    }

    async fn resume_after(&self, index: &IndexName, mark: &str) -> source::Result<Option<RowKey>> {
        self.resumed.lock().unwrap().push(mark.to_owned());
        self.inner.resume_after(index, mark).await
    }

    async fn compare_keys(
        &self,
        index: &IndexName,
        a: &RowKey,
        b: &RowKey,
    ) -> source::Result<Option<std::cmp::Ordering>> {
        self.inner.compare_keys(index, a, b).await
    }
}

/// Counts index failures (with when each was, and its wait), restarts, and
/// the moments snapshots started.
#[derive(Debug, Default)]
struct Counts {
    failed: AtomicUsize,
    restarted: AtomicUsize,
    waits: Mutex<Vec<Duration>>,
    failed_at: Mutex<Vec<tokio::time::Instant>>,
    snapshots: Mutex<Vec<tokio::time::Instant>>,
}

impl Observer for Counts {
    fn on_index_failed(&self, _: &IndexName, _: &str, wait: Duration) {
        self.failed.fetch_add(1, Ordering::SeqCst);
        self.waits.lock().unwrap().push(wait);
        self.failed_at
            .lock()
            .unwrap()
            .push(tokio::time::Instant::now());
    }

    fn on_index_restarted(&self, _: &IndexName) {
        self.restarted.fetch_add(1, Ordering::SeqCst);
    }

    fn on_snapshot_started(&self, _: &[IndexName]) {
        self.snapshots
            .lock()
            .unwrap()
            .push(tokio::time::Instant::now());
    }
}

impl Counts {
    fn failed_at(&self) -> Vec<tokio::time::Instant> {
        self.failed_at.lock().unwrap().clone()
    }

    fn snapshots(&self) -> Vec<tokio::time::Instant> {
        self.snapshots.lock().unwrap().clone()
    }
}

/// Two indexes (`users` even ids, `orders` odd) into two sinks.
struct TwoSinks {
    stream: Arc<ChannelStream>,
    feed: Arc<Feed>,
    ingest: IngestEngine,
    primary: Arc<Marking>,
    audit: Arc<Marking>,
    engines: Vec<SinkEngine>,
    counts: Arc<Counts>,
}

impl TwoSinks {
    /// `audit` holds `orders` through `audit_walked` already (its rows at
    /// version 0, as of the database's start) and asks to resume after it.
    fn new(
        db: &Arc<Database>,
        documents: Arc<dyn DocumentBuilder>,
        feed: Feed,
        audit_walked: Option<i64>,
        configure: impl FnOnce(IngestEngine) -> IngestEngine,
    ) -> Self {
        let indexes = [users(), orders()];
        let sinks = [sink_name(), audit()];
        let stream = Arc::new(ChannelStream::over(64, indexes.clone(), sinks.clone()));
        let feed = Arc::new(feed);
        let progress = Arc::new(crate::Progress::of(indexes.clone(), sinks.clone()));
        let counts = Arc::new(Counts::default());
        let observer: Arc<dyn Observer> = Arc::clone(&counts) as Arc<dyn Observer>;
        let ingest = configure(
            IngestEngine::new(
                Arc::clone(&feed) as Arc<dyn ChangeCapture>,
                documents,
                Arc::clone(&stream) as Arc<dyn Stream>,
                sinks.to_vec(),
                Arc::clone(&progress),
            )
            .with_observer(observer),
        );
        let seeded = db.rows == 0;
        let primary = Marking::new(Recorder::seeded(seeded));
        let audit_sink = Marking::new(Recorder::seeded(seeded));
        if let Some(walked) = audit_walked {
            audit_sink
                .walked
                .lock()
                .unwrap()
                .insert("orders".to_owned(), walked.to_string());
            let mut last = audit_sink.inner.last.lock().unwrap();
            for id in (0..db.rows.min(walked + 1)).filter(|id| id % 2 == 1) {
                last.insert(id, Some(0));
            }
        }
        let engines = [
            (sink_name(), Arc::clone(&primary)),
            (audit(), Arc::clone(&audit_sink)),
        ]
        .into_iter()
        .map(|(name, sink)| {
            SinkEngine::new(
                name,
                sink as Arc<dyn Sink>,
                Arc::clone(&stream) as Arc<dyn Stream>,
                indexes
                    .iter()
                    .map(|index| IndexMapping {
                        index: index.clone(),
                        ..mapping()
                    })
                    .collect(),
                Arc::clone(&progress),
            )
        })
        .collect();
        Self {
            stream,
            feed,
            ingest,
            primary,
            audit: audit_sink,
            engines,
            counts,
        }
    }

    async fn drained(&self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        while !self.stream.is_idle() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the lanes never drained"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    /// Run until the live stream ends (or, open, until `db` landed on both
    /// sinks and a stop), restarting a failed run as the daemon does.
    /// Returns how many runs failed.
    async fn run(
        &mut self,
        db: &Arc<Database>,
        stop: Option<&Stop>,
        most_restarts: usize,
    ) -> usize {
        let mut tasks = Vec::new();
        for engine in std::mem::take(&mut self.engines) {
            engine.stage(Continuity::Resumed).await.unwrap();
            tasks.push(tokio::spawn(async move {
                let (_control, mut rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
                loop {
                    match engine.run(&mut rx).await {
                        Ok(()) => return,
                        Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                    }
                }
            }));
        }
        let watcher = stop.map(|stop| {
            let db = Arc::clone(db);
            let stop = stop.clone();
            let primary = Arc::clone(&self.primary.inner);
            let audit = Arc::clone(&self.audit.inner);
            let stream = Arc::clone(&self.stream);
            tokio::spawn(async move {
                loop {
                    if db.visible.load(Ordering::SeqCst) == db.log.len()
                        && stream.is_idle()
                        && primary.state() == db.expected()
                        && audit.state() == db.expected()
                    {
                        stop.request();
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
        });
        let mut failed = 0;
        loop {
            match self.ingest.run().await {
                Ok(()) => break,
                Err(error) => {
                    failed += 1;
                    assert!(
                        failed <= most_restarts,
                        "the ingest engine failed {failed} times; the last: {error}"
                    );
                    self.drained().await;
                    self.feed.resume_after_confirmed();
                }
            }
        }
        if let Some(watcher) = watcher {
            watcher.await.unwrap();
        }
        self.drained().await;
        for task in tasks {
            task.abort();
        }
        failed
    }

    /// Both sinks hold exactly the rows; versions never went down; each
    /// sink's walk marks went up between seeds.
    fn assert_rows(&self, db: &Arc<Database>, context: &str) {
        for (name, sink) in [("primary", &self.primary), ("audit", &self.audit)] {
            let regressed = sink.inner.regressed.lock().unwrap().clone();
            assert!(
                regressed.is_empty(),
                "{name}: versions went down {regressed:?} ({context})"
            );
            let state = sink.inner.state();
            let expected = db.expected();
            let missing: Vec<i64> = expected
                .keys()
                .filter(|id| !state.contains_key(id))
                .copied()
                .collect();
            assert_eq!(
                state, expected,
                "{name}: documents differ from the rows, missing {missing:?} ({context})"
            );
        }
    }

    fn assert_seeded(&self, context: &str) {
        for (name, sink) in [("primary", &self.primary), ("audit", &self.audit)] {
            let seeded = sink.inner.seeded_indexes.lock().unwrap().clone();
            assert!(
                sink.inner.seeded.load(Ordering::SeqCst)
                    || (seeded.contains("users") && seeded.contains("orders")),
                "{name}: not every index seeded: {seeded:?} ({context})"
            );
        }
    }
}

/// `orders`' backfill fails at row 41 twice and at row 81 once, with the
/// builds of later batches running; `audit` resumes `orders` after 21 by its
/// own mark. Each lane must get every row at least once.
#[tokio::test(start_paused = true)]
async fn two_sinks_resume_each_lane_after_its_own_mark() {
    let db = Database::new(120, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(41, 2), (81, 1), (13, 1)])),
            resumable: true,
            delay: Some(Duration::from_millis(3)),
            ..Behaviour::default()
        },
    );
    let mut two = TwoSinks::new(
        &db,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        Feed::new(&db),
        Some(21),
        |ingest| ingest.with_batch(batch(4)).with_builds(4),
    );
    let failed = tokio::time::timeout(Duration::from_secs(600), two.run(&db, None, 0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0);
    // Two failing rows can share one build: three or four walk failures,
    // and no restart: only the walk failed.
    assert!(two.counts.failed.load(Ordering::SeqCst) >= 3);
    assert_eq!(two.counts.restarted.load(Ordering::SeqCst), 0);
    two.assert_rows(&db, "directed");
    two.assert_seeded("directed");
}

/// `orders`' key reads fail mid-walk (pages 2 and 3 of a walk of three
/// pages, each once), two sinks, `audit` from its own mark at 1501.
#[tokio::test(start_paused = true)]
async fn a_key_read_failing_mid_walk_resumes_each_lane() {
    let db = Database::new(6200, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            resumable: true,
            ..Behaviour::default()
        },
    );
    let reads = Reads::over(Arc::clone(&builder), [2, 5]);
    let mut two = TwoSinks::new(
        &db,
        Arc::clone(&reads) as Arc<dyn DocumentBuilder>,
        Feed::new(&db),
        Some(1501),
        |ingest| ingest.with_batch(batch(64)).with_builds(4),
    );
    let failed = tokio::time::timeout(Duration::from_secs(600), two.run(&db, None, 0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0);
    assert_eq!(two.counts.failed.load(Ordering::SeqCst), 2);
    assert_eq!(two.counts.restarted.load(Ordering::SeqCst), 0);
    two.assert_rows(&db, "key reads");
    two.assert_seeded("key reads");
    let resumed = reads.resumed.lock().unwrap().clone();
    assert!(
        resumed.len() >= 3,
        "the restarts resumed from marks: {resumed:?}"
    );
}

/// After `orders` restarted alone (its walk resumed after marks), a whole
/// run fails (a resolve), the stream resumes from what was confirmed and
/// the requests come back from the request lane as the sinks sent them.
#[tokio::test(start_paused = true)]
async fn a_run_failure_after_a_walk_failure_lands_every_row() {
    let log: Vec<Change> = (0..200).map(|n| Change::upsert(n % 150)).collect();
    let arrivals: Vec<Duration> = (0..200u64).map(|n| Duration::from_millis(20 * n)).collect();
    let db = Database::new(120, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(41, 1), (61, 1)])),
            resolve_fails_once: Mutex::new(HashSet::from([140])),
            resumable: true,
            delay: Some(Duration::from_millis(20)),
            ..Behaviour::default()
        },
    );
    let mut two = TwoSinks::new(
        &db,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        Feed::new(&db).with_arrivals(arrivals),
        Some(31),
        |ingest| ingest.with_batch(batch(4)).with_builds(2),
    );
    let failed = tokio::time::timeout(Duration::from_secs(600), two.run(&db, None, 1))
        .await
        .expect("the run ends");
    assert_eq!(failed, 1, "the resolve failed the run once");
    assert!(two.counts.failed.load(Ordering::SeqCst) >= 1);
    two.assert_rows(&db, "run failure");
    two.assert_seeded("run failure");
}

/// One random schedule over two indexes and two sinks with walks that
/// resume: random build delays and failures (index restarts mid-walk), at
/// times a whole run failing, at times `audit` resuming `orders` from its
/// own mark. Returns (index restarts, run failures, resumed points read).
async fn schedule_two_sinks(seed: u64, builds: usize, open: bool) -> (usize, usize, usize) {
    let mut random = Rng::new(seed ^ 0x5eed_0456);
    let rows = 1 + random.below(80) as i64;
    let ids = rows + random.below(4) as i64;
    let changes = random.below(100) as usize;
    let log: Vec<Change> = (0..changes)
        .map(|_| Change {
            id: random.below(ids as u64) as i64,
            delete: random.below(6) == 0,
        })
        .collect();
    let max_changes = 1 + random.below(8) as usize;
    let db = Database::new(rows, log);
    let run_failure = (random.below(3) == 0 && ids > 0).then(|| random.below(ids as u64) as i64);
    let audit_walked = (random.below(3) == 0).then(|| (random.below(rows as u64) as i64) | 1);
    let builder = Builder::two(
        &db,
        Behaviour {
            random: Some(Mutex::new(Rng::new(seed.wrapping_mul(31)))),
            failures_left: AtomicUsize::new(random.below(7) as usize),
            resolve_fails_once: Mutex::new(run_failure.into_iter().collect()),
            resumable: true,
            ..Behaviour::default()
        },
    );
    let fail_reads: Vec<usize> = (0..random.below(3))
        .map(|_| random.below(6) as usize)
        .collect();
    let reads = Reads::over(Arc::clone(&builder), fail_reads);
    let feed = Feed::new(&db).with_gaps(seed.wrapping_mul(17));
    let feed = if open { feed.open() } else { feed };
    let stop = Stop::new();
    let mut two = TwoSinks::new(
        &db,
        Arc::clone(&reads) as Arc<dyn DocumentBuilder>,
        feed,
        audit_walked,
        |ingest| {
            ingest
                .with_batch(batch(max_changes))
                .with_builds(builds)
                .with_stop(stop.clone())
        },
    );
    let failed = tokio::time::timeout(
        Duration::from_secs(3_600),
        two.run(&db, open.then_some(&stop), 8),
    )
    .await
    .unwrap_or_else(|_| panic!("seed {seed} builds {builds} open {open}: the run hung"));
    let context = format!(
        "seed {seed}, builds {builds}, open {open}, {changes} changes over {ids} ids, {rows} rows, max_changes {max_changes}, audit from {audit_walked:?}, run failure {run_failure:?}"
    );
    assert!(
        !builder.overlapped.load(Ordering::SeqCst),
        "two builds of one document at once ({context})"
    );
    two.assert_rows(&db, &context);
    if !open {
        two.assert_seeded(&context);
    }
    let resumed = reads
        .resumed
        .lock()
        .unwrap()
        .iter()
        .filter(|mark| audit_walked.is_none_or(|walked| **mark != walked.to_string()))
        .count();
    (two.counts.restarted.load(Ordering::SeqCst), failed, resumed)
}

fn seeds() -> std::ops::RangeInclusive<u64> {
    let from: u64 = std::env::var("FLUSSO_WALK_SCHEDULE_SEED_FROM")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(1);
    let count: u64 = std::env::var("FLUSSO_WALK_SCHEDULE_SEEDS")
        .ok()
        .and_then(|seeds| seeds.parse().ok())
        .unwrap_or(100);
    from..=from + count - 1
}

#[test]
fn random_two_sink_schedules_with_resumable_walks() {
    let (mut restarts, mut runs, mut resumed, mut schedules) = (0, 0, 0, 0);
    for seed in seeds() {
        for (builds, open) in [(2, false), (4, false), (4, true)] {
            let (r, f, m) =
                crate::tests::paused_runtime().block_on(schedule_two_sinks(seed, builds, open));
            restarts += r;
            runs += f;
            resumed += m;
            schedules += 1;
        }
    }
    assert!(
        schedules == 0 || (restarts > 0 && runs > 0 && resumed > 0),
        "{schedules} schedules: {restarts} index failures, {runs} run failures, {resumed} resumed reads"
    );
}

/// Claim 2's cost (design question C): `orders`' backfill row 41 fails
/// every time, while `orders` gets a live change every 100 ms. Its live
/// documents wait as long as its walk's backoff, which grows to `max_ms`:
/// the whole builder is down during a walk's wait.
#[tokio::test(start_paused = true)]
async fn a_failing_walk_leaves_the_index_live_documents_flowing() {
    let changes = 600u64;
    let log: Vec<Change> = (0..changes as i64)
        .map(|n| Change::upsert(2 * (n % 10) + 1))
        .collect();
    let arrivals: Vec<Duration> = (0..changes)
        .map(|n| Duration::from_millis(100 * n))
        .collect();
    let db = Database::new(60, log.clone());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(41, usize::MAX)])),
            resumable: true,
            ..Behaviour::default()
        },
    );
    let recorder = Recorder::seeded(false);
    recorder
        .seeded_indexes
        .lock()
        .unwrap()
        .insert("users".to_owned());
    let mut deployment = Deployment::new(
        Feed::new(&db).open().with_arrivals(arrivals),
        Arc::clone(&builder),
        Arc::clone(&recorder),
    )
    .ingest(|ingest| ingest.with_batch(batch(4)).with_builds(2));
    let sink = deployment.start_sink().await;
    let ingest = deployment.ingest;
    let run = tokio::spawn(async move { ingest.run().await });
    let opened = tokio::time::Instant::now();
    tokio::time::sleep(Duration::from_secs(58)).await;
    run.abort();
    sink.abort();
    let landings = recorder.landings.lock().unwrap().clone();
    let mut worst = Duration::ZERO;
    let mut unlanded = 0;
    for (n, change) in log.iter().enumerate().take(300) {
        let version = n as u64 + 1;
        let arrived = opened + Duration::from_millis(100 * n as u64);
        let landed = landings
            .iter()
            .filter(|(id, v, _)| *id == change.id && *v >= version)
            .map(|(_, _, at)| *at)
            .min();
        match landed {
            Some(at) => worst = worst.max(at - arrived),
            None => unlanded += 1,
        }
    }
    assert!(
        worst < Duration::from_secs(5) && unlanded == 0,
        "of the live changes of orders in the first 30 s, the slowest waited {worst:?} to land beside a failing walk, {unlanded} not landed by 58 s"
    );
}

/// Claim 2 (design question C): distinct transient failures along one walk
/// (`orders` rows 41, 81, ..., 281, each failing once) double the wait every
/// time, though each restart resumed past the previous failure and published
/// batches: only a completed walk starts the waits over. The index's live
/// documents wait with it.
#[tokio::test(start_paused = true)]
async fn distinct_one_off_walk_failures_each_wait_the_first_wait() {
    let db = Database::new(400, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new((1..=7).map(|n| (40 * n + 1, 1)).collect()),
            resumable: true,
            ..Behaviour::default()
        },
    );
    let mut two = TwoSinks::new(
        &db,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        Feed::new(&db),
        None,
        |ingest| ingest.with_batch(batch(4)).with_builds(2),
    );
    let started = tokio::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(600), two.run(&db, None, 0))
        .await
        .expect("the run ends");
    two.assert_rows(&db, "distinct failures");
    let waits = two.counts.waits.lock().unwrap().clone();
    assert!(
        waits.iter().all(|wait| *wait <= Duration::from_secs(2)),
        "seven different rows failing once each, the walk past each: waits {waits:?}, the backfill took {:?}",
        started.elapsed()
    );
}

/// R6 where it matters (guard gap): `orders` 1 is taken at change 0 and its
/// build held; the reader marks 1 again at change 1 (waiting behind its
/// build). The build fails and is given back: 1 must wait at change 0, the
/// older, so while `orders` is down its lane and the slot stay before change
/// 0. Waiting at change 1 would let a fence or the slot pass change 0 with
/// its document unbuilt.
#[tokio::test(start_paused = true)]
async fn a_change_marked_while_its_build_runs_keeps_the_given_back_position() {
    let mut log = vec![Change::upsert(1), Change::upsert(1)];
    log.extend((0..20).map(|n| Change::upsert(2 * n)));
    let db = Database::new(0, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([1])),
            fail_once: Mutex::new(HashSet::from([1])),
            delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
    );
    let counts = Arc::new(Counts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&counts) as Arc<dyn Observer>;
    let mut deployment =
        Deployment::new(Feed::new(&db), Arc::clone(&builder), Recorder::seeded(true)).ingest(
            |ingest| {
                ingest
                    .with_batch(batch(1))
                    .with_builds(2)
                    .with_observer(observer)
            },
        );
    let progress = deployment.ingest.progress();
    let watcher = {
        let builder = Arc::clone(&builder);
        let counts = Arc::clone(&counts);
        tokio::spawn(async move {
            let (mut released, mut samples, mut bad) = (false, 0, Vec::new());
            for _ in 0..5_000 {
                tokio::time::sleep(Duration::from_millis(1)).await;
                if !released && progress.changes_read() >= 2 {
                    builder.release(1);
                    released = true;
                }
                if counts.restarted.load(Ordering::SeqCst) >= 1 {
                    break;
                }
                if counts.failed.load(Ordering::SeqCst) >= 1 {
                    samples += 1;
                    let orders = progress.lane(
                        &stream::LaneKey::new(orders(), sink_name()),
                        crate::Level::Applied,
                    );
                    let landed = progress.landed();
                    let past = |answer: &source::cdc::Landed| {
                        !matches!(answer, source::cdc::Landed::Before(p) if p.seq == 0)
                            && !matches!(answer, source::cdc::Landed::Idle)
                    };
                    if orders.as_ref().is_some_and(past) || past(&landed) {
                        bad.push((orders, landed));
                    }
                }
            }
            (samples, bad)
        })
    };
    tokio::time::timeout(Duration::from_secs(300), deployment.run_to_end(0))
        .await
        .expect("the run ends");
    let (samples, bad) = watcher.await.unwrap();
    assert!(samples > 100, "sampled while orders was down: {samples}");
    assert!(
        bad.is_empty(),
        "past change 0 while its document waits given back: {:?}",
        &bad[..bad.len().min(3)]
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// Round 3 (claim 3): `audit` is stalled, so every batch of `orders`' walk
/// published to its lane is still unapplied when `orders` fails (rows 41
/// and 81) and resumes after the mark published there. Then `audit`'s next
/// write fails: its sink engine restarts and the lane redelivers. Every row
/// must land on both sinks.
#[tokio::test(start_paused = true)]
async fn a_resumed_walk_beside_a_sink_that_has_applied_nothing_and_restarts() {
    let db = Database::new(160, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(41, 1), (81, 1)])),
            resumable: true,
            delay: Some(Duration::from_millis(3)),
            ..Behaviour::default()
        },
    );
    let mut two = TwoSinks::new(
        &db,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        Feed::new(&db),
        Some(21),
        |ingest| ingest.with_batch(batch(4)).with_builds(4),
    );
    two.audit.inner.stall();
    let releaser = {
        let audit = Arc::clone(&two.audit.inner);
        let counts = Arc::clone(&two.counts);
        tokio::spawn(async move {
            while counts.failed.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            audit.fail_next.store(true, Ordering::SeqCst);
            audit.resume();
        })
    };
    let failed = tokio::time::timeout(Duration::from_secs(600), two.run(&db, None, 0))
        .await
        .expect("the run ends");
    releaser.await.unwrap();
    assert_eq!(failed, 0);
    assert!(two.counts.failed.load(Ordering::SeqCst) >= 2);
    assert!(
        !two.audit.inner.fail_next.load(Ordering::SeqCst),
        "audit's write failed once"
    );
    two.assert_rows(&db, "stalled audit");
    two.assert_seeded("stalled audit");
}

/// Round 3 (claim 3, the bound): rows inserted behind and past the resume
/// point while `orders` is down (live changes) land on both sinks.
#[tokio::test(start_paused = true)]
async fn rows_changed_behind_the_resume_point_while_a_walk_waits_land() {
    let log: Vec<Change> = (0..60)
        .map(|n| Change::upsert(if n % 2 == 0 { 2 * n + 1 } else { 199 + 2 * n }))
        .collect();
    let arrivals: Vec<Duration> = (0..60u64).map(|n| Duration::from_millis(50 * n)).collect();
    let db = Database::new(120, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(61, 2)])),
            resumable: true,
            delay: Some(Duration::from_millis(5)),
            ..Behaviour::default()
        },
    );
    let mut two = TwoSinks::new(
        &db,
        Arc::clone(&builder) as Arc<dyn DocumentBuilder>,
        Feed::new(&db).with_arrivals(arrivals),
        Some(11),
        |ingest| ingest.with_batch(batch(4)).with_builds(2),
    );
    let failed = tokio::time::timeout(Duration::from_secs(600), two.run(&db, None, 0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0);
    assert!(two.counts.failed.load(Ordering::SeqCst) >= 1);
    two.assert_rows(&db, "changed while down");
    two.assert_seeded("changed while down");
}

/// W2 (fix round 2): `orders`' backfill batch holding row 41 is held, then
/// fails, while later batches of its walk and live batches of `orders` were
/// granted behind it. The walk's later batches are discarded (published,
/// they would move each lane's resume point past 41..47, losing those
/// rows); the live ones publish. Nothing of `orders`' live work failed, so
/// its builder never restarts, and every row and change lands.
#[tokio::test(start_paused = true)]
async fn a_backfill_batch_failing_beside_live_slots_discards_only_the_walks() {
    let log: Vec<Change> = (0..40).map(|n| Change::upsert(101 + 2 * n)).collect();
    let arrivals: Vec<Duration> = (0..40u64).map(|n| Duration::from_millis(10 * n)).collect();
    let db = Database::new(80, log);
    let builder = Builder::two(
        &db,
        Behaviour {
            held: Mutex::new(HashSet::from([41])),
            fail_once: Mutex::new(HashSet::from([41])),
            resumable: true,
            delay: Some(Duration::from_millis(2)),
            ..Behaviour::default()
        },
    );
    let counts = Arc::new(Counts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&counts) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(
        Feed::new(&db).with_arrivals(arrivals),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(4)
            .with_observer(observer)
    });
    let releaser = {
        let builder = Arc::clone(&builder);
        tokio::spawn(async move {
            loop {
                let grants = builder.grants();
                let at = grants.iter().position(|ids| ids.contains(&41));
                let later = at.is_some_and(|at| {
                    let after = grants.get(at + 1..).unwrap_or_default();
                    after
                        .iter()
                        .any(|ids| ids.iter().any(|id| (43..80).contains(id)))
                        && after.iter().any(|ids| ids.iter().any(|id| *id >= 101))
                });
                if later {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            builder.release(41);
        })
    };
    let failed = tokio::time::timeout(Duration::from_secs(300), deployment.run_to_end(0))
        .await
        .expect("the run ends");
    releaser.await.unwrap();
    assert_eq!(failed, 0);
    assert_eq!(
        counts.failed.load(Ordering::SeqCst),
        1,
        "the walk failed once"
    );
    assert_eq!(
        counts.restarted.load(Ordering::SeqCst),
        0,
        "the builder never went down: only its walk failed"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// W3 (fix round 2): a request for `orders` arriving while its walk waits
/// to retry (a 10 s wait) joins that retry: no walk starts before it, and
/// every row lands.
#[tokio::test(start_paused = true)]
async fn a_request_during_a_walks_wait_joins_its_retry() {
    let db = Database::new(40, Vec::new());
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([21])),
            resumable: true,
            ..Behaviour::default()
        },
    );
    let counts = Arc::new(Counts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&counts) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(
        Feed::new(&db),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(2)
            .with_observer(observer)
            .with_backoff(
                crate::Backoff::new(Duration::from_secs(10), Duration::from_secs(60)).unwrap(),
            )
    });
    let requester = {
        let counts = Arc::clone(&counts);
        let requests = deployment.stream.requests().unwrap().producer;
        tokio::spawn(async move {
            while counts.failed.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            requests
                .publish(stream::Request::Backfill {
                    index: orders(),
                    sink: sink_name(),
                    after: None,
                    ticket: stream::Ticket::new(1_000),
                })
                .await
                .unwrap();
        })
    };
    let failed = tokio::time::timeout(Duration::from_secs(300), deployment.run_to_end(0))
        .await
        .expect("the run ends");
    requester.await.unwrap();
    assert_eq!(failed, 0);
    let failed_at = counts.failed_at()[0];
    let early: Vec<Duration> = counts
        .snapshots()
        .iter()
        .filter(|at| **at > failed_at && **at < failed_at + Duration::from_secs(10))
        .map(|at| *at - failed_at)
        .collect();
    assert!(
        early.is_empty(),
        "a walk started before the retry time: {early:?}"
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}

/// W5 (fix round 2): a stop while `orders`' walk waits to retry (30 s) ends
/// the run cleanly at once; the walk's request goes back to the request
/// lane, unacknowledged, for the next start (O3: a stop doesn't finish a
/// backfill).
#[tokio::test(start_paused = true)]
async fn a_stop_while_a_walk_waits_to_retry_ends_cleanly() {
    let db = Database::new(40, vec![Change::upsert(100)]);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_once: Mutex::new(HashSet::from([21])),
            resumable: true,
            ..Behaviour::default()
        },
    );
    let stop = Stop::new();
    let counts = Arc::new(Counts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&counts) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(
        Feed::new(&db).open(),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(2)
            .with_stop(stop.clone())
            .with_observer(observer)
            .with_backoff(
                crate::Backoff::new(Duration::from_secs(30), Duration::from_secs(60)).unwrap(),
            )
    });
    let sink = deployment.start_sink().await;
    let stopper = {
        let counts = Arc::clone(&counts);
        tokio::spawn(async move {
            while counts.failed.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            stop.request();
            tokio::time::Instant::now()
        })
    };
    let ended = tokio::time::timeout(Duration::from_secs(300), deployment.ingest.run())
        .await
        .expect("the run ends");
    let took = tokio::time::Instant::now() - stopper.await.unwrap();
    sink.abort();
    ended.expect("the stop ends the run cleanly");
    assert!(
        took < Duration::from_secs(5),
        "the stop waited for the walk: {took:?}"
    );
    assert!(
        !deployment.stream.requests().unwrap().consumer.is_empty(),
        "the walk's request is back on the request lane for the next start"
    );
}

/// W6 (fix round 2): `orders`' walk fails twice (waits 2 s, then 4 s); a
/// live build of `orders` fails during the second wait (live wait 2 s).
/// The builder goes down and restarts, and its walk still retries at its
/// own time, not at the builder's restart, resuming where its lane is.
#[tokio::test(start_paused = true)]
async fn a_live_failure_while_a_walk_waits_keeps_its_retry() {
    let db = Database::new(40, vec![Change::upsert(101)]);
    let builder = Builder::two(
        &db,
        Behaviour {
            fail_times: Mutex::new(BTreeMap::from([(21, 2)])),
            fail_once: Mutex::new(HashSet::from([101])),
            resumable: true,
            ..Behaviour::default()
        },
    );
    let counts = Arc::new(Counts::default());
    let observer: Arc<dyn Observer> = Arc::clone(&counts) as Arc<dyn Observer>;
    let mut deployment = Deployment::new(
        Feed::new(&db).with_arrivals(vec![Duration::from_millis(3_000)]),
        Arc::clone(&builder),
        Recorder::seeded(false),
    )
    .ingest(|ingest| {
        ingest
            .with_batch(batch(4))
            .with_builds(2)
            .with_observer(observer)
            .with_backoff(
                crate::Backoff::new(Duration::from_secs(2), Duration::from_secs(60)).unwrap(),
            )
    });
    let failed = tokio::time::timeout(Duration::from_secs(300), deployment.run_to_end(0))
        .await
        .expect("the run ends");
    assert_eq!(failed, 0);
    let failures = counts.failed_at();
    assert_eq!(failures.len(), 3, "two walk failures and a live one");
    assert_eq!(
        counts.restarted.load(Ordering::SeqCst),
        1,
        "the live failure"
    );
    let second_walk = failures[1];
    let retried: Vec<tokio::time::Instant> = counts
        .snapshots()
        .into_iter()
        .filter(|at| *at > second_walk)
        .collect();
    assert!(
        retried
            .first()
            .is_some_and(|at| *at >= second_walk + Duration::from_secs(4)),
        "the walk retried at its own time, 4 s after its second failure: {:?}",
        retried.first().map(|at| *at - second_walk)
    );
    assert_eq!(deployment.recorder.state(), db.expected());
}
