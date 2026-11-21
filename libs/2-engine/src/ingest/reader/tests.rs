//! The reader's rules (R1, R2, R7–R9 of ADR 0012 stage 8a's slice 6a), on
//! paused time, each test bounded so a broken rule fails instead of hanging.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{ColumnName, GenericValue, IndexName, SinkName, TableName};
use source::RowKey;
use source::cdc::Landed;
use source::document::Document;
use stream::LaneKey;

use super::*;
use crate::ingest::together::together;
use crate::observer::NoopObserver;

const LIMIT: Duration = Duration::from_secs(5);

fn users() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn orders() -> IndexName {
    IndexName::try_new("orders").unwrap()
}

fn key(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

fn id_of(key: &RowKey) -> i64 {
    match key.0.first().map(|(_, value)| value) {
        Some(GenericValue::BigInt(id)) => *id,
        _ => -1,
    }
}

fn change(id: i64) -> source::Result<LiveChange> {
    Ok((
        Position::new(id as u64),
        ChangeEvent::Upsert {
            table: TableName::try_new("users").unwrap(),
            key: key(id),
            before: None,
            after: None,
        },
    ))
}

/// Progress over `users` and `orders`, one sink.
fn progress() -> Arc<Progress> {
    let sink = SinkName::try_new("primary").unwrap();
    Arc::new(Progress::new([
        LaneKey::new(users(), sink.clone()),
        LaneKey::new(orders(), sink),
    ]))
}

/// Resolves change `id` to `id`'s document: `users` for even ids, `orders`
/// for odd ones, an index with no lane for ids of 1000 and up. Requests
/// `stop` when resolving `stop_at`, fails on `fail_at`, panics on
/// `panic_at`.
#[derive(Debug, Default)]
struct Resolver {
    stop_at: Option<(i64, Stop)>,
    fail_at: Option<i64>,
    panic_at: Option<i64>,
}

#[async_trait]
impl DocumentBuilder for Resolver {
    async fn resolve(
        &self,
        _: &TableName,
        key: &RowKey,
        _: Option<&source::RowImage>,
        _: Option<&source::RowImage>,
    ) -> source::Result<Vec<DocumentId>> {
        let id = id_of(key);
        if let Some((at, stop)) = &self.stop_at
            && *at == id
        {
            stop.request();
        }
        if self.fail_at == Some(id) {
            return Err(source::SourceError::Connection("resolve failed".to_owned()));
        }
        assert!(self.panic_at != Some(id), "resolve panicked");
        let index = if id >= 1000 {
            IndexName::try_new("elsewhere").unwrap()
        } else if id % 2 == 0 {
            users()
        } else {
            orders()
        };
        Ok(vec![DocumentId {
            index,
            key: key.clone(),
        }])
    }

    async fn build(&self, id: &DocumentId) -> source::Result<Document> {
        Ok(Document::Delete { id: id.clone() })
    }

    fn backfill_indexes(&self) -> Vec<IndexName> {
        Vec::new()
    }

    async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
        Ok(None)
    }

    async fn keys(
        &self,
        _: &IndexName,
        _: Option<&RowKey>,
        _: &RowKey,
        _: usize,
    ) -> source::Result<Vec<RowKey>> {
        Ok(Vec::new())
    }
}

/// A stream of `ids`, every change ready at once, then open (pending) or
/// ended.
fn stream_of(ids: &[i64], open: bool) -> LiveStream {
    let ready = futures::stream::iter(ids.iter().map(|id| change(*id)).collect::<Vec<_>>());
    if open {
        Box::pin(ready.chain(futures::stream::pending()))
    } else {
        Box::pin(ready)
    }
}

/// The reader run as the engine runs it (§3.8, review round 3): its future
/// beside a builder that only waits for its end, the stop watched once
/// ([`together`]), all on a task of its own so a test drives it from outside.
/// Dropped, the task is aborted.
struct ReaderTask(tokio::task::JoinHandle<Result<Ended>>);

impl ReaderTask {
    fn spawn(reader: Reader, live: LiveStream) -> Self {
        let stop = reader.stop.clone();
        let (reading, mut end) = reading(reader, live);
        Self(tokio::spawn(async move {
            together(
                reading,
                async move { end.join().await },
                Vec::<std::future::Pending<()>>::new(),
                async move {
                    stop.requested().await;
                },
            )
            .await
        }))
    }

    /// The reader's end, or its panic resumed here.
    async fn join(&mut self) -> Result<Ended> {
        match (&mut self.0).await {
            Ok(ended) => ended,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }
}

impl Drop for ReaderTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Started {
    task: ReaderTask,
    runs: mpsc::UnboundedReceiver<Run>,
    progress: Arc<Progress>,
    lead: Arc<Lead>,
    stop: Stop,
}

fn start(
    live: LiveStream,
    resolver: Resolver,
    max_changes: usize,
    max: MaxWaiting,
    stop: Stop,
    handing: Option<Box<dyn Fn(&Run) + Send + Sync>>,
    progress: Arc<Progress>,
) -> Started {
    let (reader, runs, lead) = reader(resolver, max_changes, max, &stop, handing, &progress);
    let task = ReaderTask::spawn(reader, live);
    Started {
        task,
        runs,
        progress,
        lead,
        stop,
    }
}

/// A reader of `max_changes` per run under `max`, and the receiving end of
/// its runs.
fn reader(
    resolver: Resolver,
    max_changes: usize,
    max: MaxWaiting,
    stop: &Stop,
    handing: Option<Box<dyn Fn(&Run) + Send + Sync>>,
    progress: &Arc<Progress>,
) -> (Reader, mpsc::UnboundedReceiver<Run>, Arc<Lead>) {
    let (sender, runs) = mpsc::unbounded_channel();
    let lead = Lead::new(max, progress.changes_read());
    let reader = Reader {
        resolver: Arc::new(resolver),
        progress: Arc::clone(progress),
        observer: Arc::new(NoopObserver),
        stop: stop.clone(),
        batch: BatchPolicy {
            max_changes,
            max_delay: Duration::from_secs(60),
        },
        lead: Arc::clone(&lead),
        runs: sender,
        handing,
        checked: None,
    };
    (reader, runs, lead)
}

fn plenty() -> MaxWaiting {
    MaxWaiting {
        per_index: 1000,
        total: 1000,
    }
}

/// Let the reader run until it waits.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(10)).await;
}

fn last(run: &Run) -> Position {
    run.changes.last().unwrap().0
}

fn received(runs: &mut mpsc::UnboundedReceiver<Run>) -> Vec<Run> {
    let mut received = Vec::new();
    while let Ok(run) = runs.try_recv() {
        received.push(run);
    }
    received
}

/// R1: whatever the builder receives of a run is already marked and
/// counted: at the hand-over every document of the run waits and the read
/// count includes it.
#[tokio::test(start_paused = true)]
async fn a_run_is_marked_and_counted_before_it_is_handed_over() {
    let progress = progress();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let handing = {
        let progress = Arc::clone(&progress);
        let seen = Arc::clone(&seen);
        Box::new(move |run: &Run| {
            seen.lock()
                .unwrap()
                .push((progress.waiting(), progress.changes_read(), run.read));
        }) as Box<dyn Fn(&Run) + Send + Sync>
    };
    let mut started = start(
        stream_of(&[0, 1, 2], false),
        Resolver::default(),
        10,
        plenty(),
        Stop::new(),
        Some(handing),
        progress,
    );
    let ended = tokio::time::timeout(LIMIT, started.task.join())
        .await
        .unwrap()
        .unwrap();
    assert!(ended.stream_ended);
    assert_eq!(*seen.lock().unwrap(), [(3, 3, 3)]);
    let runs = received(&mut started.runs);
    assert_eq!(runs.len(), 1, "three ready changes are one run");
    assert_eq!(
        (last(&runs[0]), runs[0].changes.len()),
        (Position::new(2), 3)
    );
}

/// R2: one index's waiting documents stop the reading at the per-index
/// bound; a take (the builder's grant) lets it read on, up to the bound
/// again.
#[tokio::test(start_paused = true)]
async fn the_reader_waits_at_the_lead_per_index_and_resumes_after_a_take() {
    let mut started = start(
        stream_of(&[0, 2, 4, 6, 8], true),
        Resolver::default(),
        1,
        MaxWaiting {
            per_index: 2,
            total: 100,
        },
        Stop::new(),
        None,
        progress(),
    );
    settle().await;
    assert_eq!(
        started.progress.changes_read(),
        2,
        "users has two documents waiting"
    );
    let first = received(&mut started.runs);
    started
        .progress
        .take_through(&users(), last(&first[0]), &|_| false)
        .unwrap();
    started.lead.granted(first[0].read);
    settle().await;
    assert_eq!(
        started.progress.changes_read(),
        3,
        "one taken, one more read"
    );
    drop(started.task);
}

/// R2: every index's waiting documents together stop the reading at the
/// total bound, also once the changes read are granted (as a grant that the
/// budget left deferring everything would).
#[tokio::test(start_paused = true)]
async fn the_reader_waits_at_the_lead_over_every_index() {
    let mut started = start(
        stream_of(&[0, 1, 2, 3, 4, 5], true),
        Resolver::default(),
        1,
        MaxWaiting {
            per_index: 100,
            total: 3,
        },
        Stop::new(),
        None,
        progress(),
    );
    settle().await;
    assert_eq!(started.progress.changes_read(), 3);
    assert_eq!(started.progress.waiting(), 3);
    let runs = received(&mut started.runs);
    started.lead.granted(runs[2].read);
    settle().await;
    assert_eq!(
        started.progress.changes_read(),
        3,
        "three documents still wait: the reading stays stopped"
    );
}

/// R2 for changes that build nothing: documents of an index with no lane
/// never wait and pin nothing, but the changes read ahead of the grants
/// still stop the reading at the total bound, until a grant moves past them.
/// Routed, they need nothing more to be confirmed (stage 8a slice 6b-1a).
#[tokio::test(start_paused = true)]
async fn changes_that_build_nothing_still_count_toward_the_lead() {
    let mut started = start(
        stream_of(&[1000, 1001, 1002, 1003], true),
        Resolver::default(),
        1,
        MaxWaiting {
            per_index: 100,
            total: 2,
        },
        Stop::new(),
        None,
        progress(),
    );
    settle().await;
    assert_eq!(started.progress.changes_read(), 2);
    assert_eq!(started.progress.waiting(), 0, "no lane, nothing waits");
    assert_eq!(
        started.progress.landed(),
        source::cdc::Landed::Through(Position::new(1001)),
        "routed, and nothing to build"
    );
    let runs = received(&mut started.runs);
    started.lead.granted(runs[1].read);
    settle().await;
    assert_eq!(started.progress.changes_read(), 4);
}

/// R7: a stop while the reader waits at its lead ends it at once, with the
/// stream handed back.
#[tokio::test(start_paused = true)]
async fn a_stop_while_the_reader_waits_at_its_lead_ends_it() {
    let mut started = start(
        stream_of(&[0, 2, 4], true),
        Resolver::default(),
        1,
        MaxWaiting {
            per_index: 1,
            total: 100,
        },
        Stop::new(),
        None,
        progress(),
    );
    settle().await;
    started.stop.request();
    let ended = tokio::time::timeout(LIMIT, started.task.join())
        .await
        .expect("the stop ends the wait")
        .unwrap();
    assert!(!ended.stream_ended);
    assert_eq!(started.progress.changes_read(), 1);
}

/// R7: a stop in the middle of a run cuts it there; what was read is marked,
/// counted and handed over before the reader ends, and nothing more is read.
#[tokio::test(start_paused = true)]
async fn a_stop_mid_run_hands_over_what_the_reader_read() {
    let stop = Stop::new();
    let mut started = start(
        stream_of(&[0, 2, 4, 6, 8], true),
        Resolver {
            stop_at: Some((4, stop.clone())),
            ..Resolver::default()
        },
        100,
        plenty(),
        stop,
        None,
        progress(),
    );
    let ended = tokio::time::timeout(LIMIT, started.task.join())
        .await
        .unwrap()
        .unwrap();
    assert!(!ended.stream_ended);
    let runs = received(&mut started.runs);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        (last(&runs[0]), runs[0].changes.len()),
        (Position::new(4), 3)
    );
    assert_eq!(started.progress.waiting(), 3);
    assert!(
        started.runs.recv().await.is_none(),
        "the channel closed with the reader"
    );
}

/// R9: a resolve error ends the reader with it; the run it was reading is
/// not handed over (the replay reads it again).
#[tokio::test(start_paused = true)]
async fn a_resolve_error_ends_the_reader_with_it() {
    let mut started = start(
        stream_of(&[0, 2, 4], true),
        Resolver {
            fail_at: Some(2),
            ..Resolver::default()
        },
        100,
        plenty(),
        Stop::new(),
        None,
        progress(),
    );
    let ended = tokio::time::timeout(LIMIT, started.task.join())
        .await
        .unwrap();
    assert!(matches!(ended, Err(EngineError::Source(_))));
    assert!(received(&mut started.runs).is_empty());
}

/// R9: a panic in the reader unwinds the task polling it (the engine run's,
/// in production).
#[tokio::test(start_paused = true)]
#[should_panic(expected = "resolve panicked")]
async fn a_reader_panic_unwinds_the_task_that_polls_it() {
    let mut started = start(
        stream_of(&[0, 2], true),
        Resolver {
            panic_at: Some(2),
            ..Resolver::default()
        },
        100,
        plenty(),
        Stop::new(),
        None,
        progress(),
    );
    let _ = tokio::time::timeout(LIMIT, started.task.join()).await;
}

/// A stream that records when it is dropped.
struct Watched(Arc<AtomicBool>);

impl Drop for Watched {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// R9: the reader is a future of the engine run, nothing spawned for it:
/// dropping the run's future drops the reader and its stream at once, with
/// nothing left to wait for.
#[tokio::test(start_paused = true)]
async fn dropping_the_run_drops_the_reader_and_its_stream_at_once() {
    let dropped = Arc::new(AtomicBool::new(false));
    let polled = Arc::new(AtomicUsize::new(0));
    let watched = Watched(Arc::clone(&dropped));
    let counter = Arc::clone(&polled);
    let live: BoxStream<'static, source::Result<LiveChange>> =
        Box::pin(futures::stream::poll_fn(move |_| {
            let _ = &watched;
            counter.fetch_add(1, Ordering::SeqCst);
            std::task::Poll::Pending
        }));
    let stop = Stop::new();
    let (reader, _runs, _lead) = reader(Resolver::default(), 1, plenty(), &stop, None, &progress());
    let (reading, mut end) = reading(reader, live);
    let mut run = Box::pin(together(
        reading,
        async move {
            let _ = end.join().await;
        },
        Vec::<std::future::Pending<()>>::new(),
        std::future::pending::<()>(),
    ));
    assert!(futures::poll!(run.as_mut()).is_pending());
    assert!(polled.load(Ordering::SeqCst) > 0, "the reader read");
    assert!(!dropped.load(Ordering::SeqCst));
    drop(run);
    assert!(
        dropped.load(Ordering::SeqCst),
        "the stream went with the run's future"
    );
}

/// R2 without a lost wake-up: a take landing between the reader finding its
/// lead reached and its wait still wakes it.
#[tokio::test(start_paused = true)]
async fn a_take_between_the_check_and_the_wait_still_wakes_the_reader() {
    a_take_at(Check::BeforeWaiting).await;
}

/// R2 without a lost wake-up on the fast path: a take landing after the
/// first check found the lead reached, before the wake-up is registered, is
/// seen by the check made once it is.
#[tokio::test(start_paused = true)]
async fn a_take_before_the_wake_up_is_registered_is_seen_by_the_check_after_it() {
    a_take_at(Check::BeforeRegistering).await;
}

/// One take, at `point`, then the stream's end: the reader must read on.
async fn a_take_at(point: Check) {
    let progress = progress();
    let (sender, mut runs) = mpsc::unbounded_channel();
    let lead = Lead::new(
        MaxWaiting {
            per_index: 1,
            total: 100,
        },
        0,
    );
    let checked = {
        let progress = Arc::clone(&progress);
        let lead = Arc::clone(&lead);
        let once = AtomicBool::new(false);
        Box::new(move |at: Check| {
            if at == point && !once.swap(true, Ordering::SeqCst) {
                let taken = progress
                    .take_through(&users(), Position::new(0), &|_| false)
                    .unwrap();
                assert_eq!(taken.ids.len(), 1);
                lead.granted(1);
            }
        }) as Box<dyn Fn(Check) + Send + Sync>
    };
    let mut task = ReaderTask::spawn(
        Reader {
            resolver: Arc::new(Resolver::default()),
            progress: Arc::clone(&progress),
            observer: Arc::new(NoopObserver),
            stop: Stop::new(),
            batch: BatchPolicy {
                max_changes: 1,
                max_delay: Duration::from_secs(60),
            },
            lead,
            runs: sender,
            handing: None,
            checked: Some(checked),
        },
        stream_of(&[0], false),
    );
    let ended = tokio::time::timeout(LIMIT, task.join())
        .await
        .expect("the take woke the reader")
        .unwrap();
    assert!(ended.stream_ended, "it read on to the stream's end");
    assert_eq!(received(&mut runs).len(), 1);
}

/// R9 and `changes_captured`: a run an error cuts is not handed over, but
/// its changes still count as read, the failing one included, as `main`
/// counted each change as it read it.
#[tokio::test(start_paused = true)]
async fn a_run_cut_by_an_error_still_counts_its_changes_read() {
    let mut started = start(
        stream_of(&[0, 2, 4], true),
        Resolver {
            fail_at: Some(4),
            ..Resolver::default()
        },
        100,
        plenty(),
        Stop::new(),
        None,
        progress(),
    );
    let ended = tokio::time::timeout(LIMIT, started.task.join())
        .await
        .unwrap();
    assert!(ended.is_err());
    assert_eq!(started.progress.changes_read(), 3);
    assert_eq!(started.progress.waiting(), 0, "nothing marked");
}

/// D1 (stage 8a slice 6b-1a): the reader raises what it routed only after
/// the run's marks and its count, so a lane that answers through a routed
/// change never misses one of its documents. A listener runs as `routed`
/// moves; each time it sees the run's three documents waiting and counted.
#[tokio::test(start_paused = true)]
async fn routed_moves_only_after_the_runs_marks() {
    let progress = progress();
    let seen = Arc::new(Mutex::new(Vec::new()));
    {
        let watched = Arc::clone(&progress);
        let seen = Arc::clone(&seen);
        progress.on_registered(Arc::new(move || {
            let orders = LaneKey::new(orders(), SinkName::try_new("primary").unwrap());
            seen.lock().unwrap().push((
                watched.waiting(),
                watched.changes_read(),
                watched.lane(&orders, crate::Level::Applied),
            ));
        }));
    }
    let mut started = start(
        stream_of(&[0, 2, 4], false),
        Resolver::default(),
        10,
        plenty(),
        Stop::new(),
        None,
        Arc::clone(&progress),
    );
    let ended = tokio::time::timeout(LIMIT, started.task.join())
        .await
        .unwrap()
        .unwrap();
    assert!(ended.stream_ended);
    let seen = seen.lock().unwrap();
    assert!(!seen.is_empty(), "routed moved");
    for (waiting, read, orders) in seen.iter() {
        assert_eq!((*waiting, *read), (3, 3), "{seen:?}");
        assert_eq!(*orders, Some(Landed::Through(Position::new(4))));
    }
}

/// D2 (stage 8a slice 6b-1a): a run cut by an error before its marks is
/// counted as read and never marked, so no sink counts it before the next
/// run's first live grant: not while the engine waits to restart, nor after.
#[tokio::test(start_paused = true)]
async fn a_run_cut_before_its_marks_counts_no_sooner_than_the_first_grant() {
    let progress = progress();
    let mut started = start(
        stream_of(&[0, 2, 4], true),
        Resolver {
            fail_at: Some(4),
            ..Resolver::default()
        },
        100,
        plenty(),
        Stop::new(),
        None,
        Arc::clone(&progress),
    );
    let ended = tokio::time::timeout(LIMIT, started.task.join())
        .await
        .unwrap();
    assert!(ended.is_err());
    assert_eq!(progress.changes_read(), 3);
    let sink = SinkName::try_new("primary").unwrap();
    assert_eq!(
        progress.committed(&sink),
        0,
        "the engine restarts after a backoff: none of the three counts meanwhile"
    );
    progress.forget_all();
    assert_eq!(progress.committed(&sink), 0, "none of the three was built");
}
