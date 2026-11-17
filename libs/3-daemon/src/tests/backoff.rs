//! A failed engine restarts after a backoff that starts at one second and
//! doubles while it keeps failing without progress. A run that made progress
//! (a sink engine acknowledged a lane item; the ingest engine's source reached
//! further than ever, or confirmed further where the source reports no such
//! position, or a backfill finished) resets it, so a short outage never lengthens every later recovery.
//! On paused time: each test reads the restart waits off the virtual clock.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use kernel::{Envelope, IndexName, Position};
use sink::{Sink, SinkError, SinkOptions, WriteReport};
use source::cdc::{ChangeCapture, Continuity, LiveChange};
use tokio::time::Instant;

use super::{
    FedSource, ScriptedSource, backendless_config, backends_for, primary, row_event, wait_until,
};
use crate::Daemon;

/// A sink whose flushes follow `script` (`false` fails the flush), then
/// succeed. Records when each flush was called.
#[derive(Debug)]
struct ScriptedFlushes {
    script: Mutex<Vec<bool>>,
    flushed_at: Mutex<Vec<Instant>>,
    seeded: bool,
}

impl ScriptedFlushes {
    fn new(script: &[bool]) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script.iter().rev().copied().collect()),
            flushed_at: Mutex::new(Vec::new()),
            seeded: true,
        })
    }

    /// The same sink with no index seeded, so it asks for a backfill.
    fn unseeded(self: Arc<Self>) -> Self {
        Self {
            script: Mutex::new(self.script.lock().unwrap().clone()),
            flushed_at: Mutex::new(Vec::new()),
            seeded: false,
        }
    }

    fn flushes(&self) -> Vec<Instant> {
        self.flushed_at.lock().unwrap().clone()
    }
}

#[async_trait]
impl Sink for ScriptedFlushes {
    async fn write(&self, _envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        self.flushed_at.lock().unwrap().push(Instant::now());
        if self.script.lock().unwrap().pop() == Some(false) {
            return Err(SinkError::Write("simulated outage".to_owned()));
        }
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(self.seeded)
    }

    async fn mark_seeded(&self, _: &IndexName) -> sink::Result<()> {
        Ok(())
    }
}

fn gaps(times: &[Instant]) -> Vec<Duration> {
    times.windows(2).map(|w| w[1] - w[0]).collect()
}

/// Whole seconds, so the few milliseconds of batching around a restart don't
/// matter.
fn seconds(gaps: &[Duration]) -> Vec<u64> {
    gaps.iter().map(Duration::as_secs).collect()
}

#[tokio::test(start_paused = true)]
async fn a_sink_engine_that_made_progress_restarts_after_one_second_again() {
    let sink = ScriptedFlushes::new(&[false, true, false, true]);
    let inner = ScriptedSource::new(Vec::new());
    let confirmed = Arc::clone(&inner.confirmed);
    let (source, feed) = FedSource::new(inner);
    let backends = backends_for(
        Arc::new(source),
        vec![(
            primary(),
            Arc::clone(&sink) as Arc<dyn Sink>,
            SinkOptions::default(),
        )],
    );
    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let flushes = Arc::clone(&sink);
    let run = tokio::spawn(running.run(async move {
        wait_until_count(&flushes, 4).await;
    }));

    feed.send(row_event(false, 1)).await.unwrap();
    wait_until("the first batch to be confirmed after a restart", || {
        confirmed.lock().unwrap().contains(&Position(0))
    })
    .await;
    feed.send(row_event(false, 2)).await.unwrap();
    run.await.unwrap().unwrap();

    let flushes = sink.flushes();
    assert_eq!(flushes.len(), 4, "{flushes:?}");
    let waits = gaps(&flushes);
    assert_eq!(seconds(&[waits[0], waits[2]]), [1, 1], "{waits:?}");
}

#[tokio::test(start_paused = true)]
async fn a_sink_engine_failing_without_progress_doubles_its_backoff() {
    let sink = ScriptedFlushes::new(&[false, false, false, true]);
    let (source, feed) = FedSource::new(ScriptedSource::new(Vec::new()));
    let backends = backends_for(
        Arc::new(source),
        vec![(
            primary(),
            Arc::clone(&sink) as Arc<dyn Sink>,
            SinkOptions::default(),
        )],
    );
    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let flushes = Arc::clone(&sink);
    let run = tokio::spawn(running.run(async move {
        wait_until_count(&flushes, 4).await;
    }));

    feed.send(row_event(false, 1)).await.unwrap();
    run.await.unwrap().unwrap();

    assert_eq!(seconds(&gaps(&sink.flushes())), [1, 2, 4]);
}

async fn wait_until_count(sink: &ScriptedFlushes, count: usize) {
    while sink.flushed_at.lock().unwrap().len() < count {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// What one live stream of [`ScriptedStreams`] does.
#[derive(Debug, Clone, Copy)]
enum Run {
    /// Fails before yielding anything.
    FailAtOnce,
    /// Yields one change at `position` (a new position when `None`), then
    /// fails `after` it.
    ChangeThenFail {
        position: Option<u64>,
        after: Duration,
    },
}

/// A source whose live streams follow `script`, then stay open. Records when
/// each stream opened and when each failed.
#[derive(Debug)]
struct ScriptedStreams {
    script: Mutex<Vec<Run>>,
    times: Arc<RunTimes>,
    next: Mutex<u64>,
}

#[derive(Debug, Default)]
struct RunTimes {
    opened: Mutex<Vec<Instant>>,
    failed: Mutex<Vec<Instant>>,
}

impl RunTimes {
    /// How long each restart waited: from a failure to the next open.
    fn waits(&self) -> Vec<u64> {
        let opened = self.opened.lock().unwrap();
        let failed = self.failed.lock().unwrap();
        let waits: Vec<Duration> = failed
            .iter()
            .zip(opened.iter().skip(1))
            .map(|(failed, opened)| *opened - *failed)
            .collect();
        seconds(&waits)
    }

    fn opened(&self) -> usize {
        self.opened.lock().unwrap().len()
    }

    fn failure(
        self: &Arc<Self>,
    ) -> impl futures::Stream<Item = source::Result<LiveChange>> + use<> {
        let times = Arc::clone(self);
        stream::once(async move {
            times.failed.lock().unwrap().push(Instant::now());
            Err(source::SourceError::Connection("simulated drop".to_owned()))
        })
    }
}

impl ScriptedStreams {
    fn new(script: &[Run]) -> Self {
        Self {
            script: Mutex::new(script.iter().rev().copied().collect()),
            times: Arc::default(),
            next: Mutex::new(0),
        }
    }
}

#[async_trait]
impl ChangeCapture for ScriptedStreams {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        self.times.opened.lock().unwrap().push(Instant::now());
        Ok(match self.script.lock().unwrap().pop() {
            None => Box::pin(stream::pending()),
            Some(Run::FailAtOnce) => Box::pin(self.times.failure()),
            Some(Run::ChangeThenFail { position, after }) => {
                let position = position.unwrap_or_else(|| {
                    let mut next = self.next.lock().unwrap();
                    *next += 1;
                    *next - 1
                });
                let change = stream::iter([Ok((Position(position), row_event(false, 1)))]);
                let pause = stream::once(tokio::time::sleep(after)).filter_map(|()| async { None });
                Box::pin(change.chain(pause).chain(self.times.failure()))
            }
        })
    }

    fn confirm(&self, _: Position) {}
}

/// A sink whose flush never returns: nothing it is handed is acknowledged.
#[derive(Debug)]
struct NeverFlushes;

#[async_trait]
impl Sink for NeverFlushes {
    async fn write(&self, _envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        std::future::pending().await
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// Run `capture` with `sink` until `capture` was opened `opened` times, then
/// abort the deployment: a sink that never acknowledges would hold a graceful
/// stop's drain for ever.
async fn run_until_opened(
    capture: Arc<dyn ChangeCapture>,
    times: &Arc<RunTimes>,
    opened: usize,
    sink: Arc<dyn Sink>,
    config: config::Config,
) {
    let backends = backends_for(capture, vec![(primary(), sink, SinkOptions::default())]);
    let running = Daemon::new(config, backends).start().await.unwrap();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));
    while times.opened() < opened {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    run.abort();
}

async fn ingest_restart_waits(script: &[Run], sink: Arc<dyn Sink>) -> Vec<u64> {
    let source = Arc::new(ScriptedStreams::new(script));
    let times = Arc::clone(&source.times);
    run_until_opened(source, &times, script.len() + 1, sink, backendless_config()).await;
    times.waits()
}

const AT_ONCE: Run = Run::FailAtOnce;

#[tokio::test(start_paused = true)]
async fn an_ingest_engine_that_confirmed_restarts_after_one_second_again() {
    let confirming = Run::ChangeThenFail {
        position: None,
        after: Duration::from_millis(1500),
    };
    let waits = ingest_restart_waits(
        &[AT_ONCE, AT_ONCE, confirming, AT_ONCE],
        ScriptedFlushes::new(&[]),
    )
    .await;
    assert_eq!(waits, [1, 2, 1, 2]);
}

#[tokio::test(start_paused = true)]
async fn an_ingest_engine_failing_without_progress_doubles_its_backoff() {
    let waits = ingest_restart_waits(&[AT_ONCE, AT_ONCE, AT_ONCE], ScriptedFlushes::new(&[])).await;
    assert_eq!(waits, [1, 2, 4]);
}

/// A transaction that fails partway is replayed from its first change on
/// every restart: publishing it again confirms nothing, so it is no progress.
#[tokio::test(start_paused = true)]
async fn an_ingest_engine_replaying_an_unconfirmed_change_doubles_its_backoff() {
    let replay = Run::ChangeThenFail {
        position: Some(0),
        after: Duration::from_millis(200),
    };
    let waits = ingest_restart_waits(&[replay; 4], Arc::new(NeverFlushes)).await;
    assert_eq!(waits, [1, 2, 4, 8]);
}

/// A live stream that stays open, and a backfill that publishes its first
/// page then fails at the same row every time it is served.
#[derive(Debug, Default)]
struct BackfillFailingAtOneRow {
    times: Arc<RunTimes>,
}

#[async_trait]
impl ChangeCapture for BackfillFailingAtOneRow {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }

    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        Ok(Box::pin(stream::pending()))
    }

    fn confirm(&self, _: Position) {}

    async fn snapshot(
        &self,
        _: &[source::SnapshotTable],
    ) -> source::Result<BoxStream<'static, source::Result<source::cdc::ChangeEvent>>> {
        self.times.opened.lock().unwrap().push(Instant::now());
        let first_page = stream::iter([Ok(row_event(false, 10))]);
        let times = Arc::clone(&self.times);
        let failure = stream::once(async move {
            times.failed.lock().unwrap().push(Instant::now());
            Err(source::SourceError::Connection(
                "simulated failure at row 11".to_owned(),
            ))
        });
        Ok(Box::pin(first_page.chain(failure)))
    }
}

#[tokio::test(start_paused = true)]
async fn a_backfill_failing_at_the_same_row_doubles_its_backoff() {
    let source = Arc::new(BackfillFailingAtOneRow::default());
    let times = Arc::clone(&source.times);
    let mut config = backendless_config();
    config.batch.max_changes = std::num::NonZeroUsize::new(1);
    let sink = Arc::new(ScriptedFlushes::new(&[]).unseeded());

    run_until_opened(source, &times, 5, sink, config).await;

    assert_eq!(times.waits(), [1, 2, 4, 8]);
}

/// A source that, like Postgres, numbers a replayed change afresh on every
/// reopened stream, and reports having reached no further than just below the
/// commit that keeps failing.
#[derive(Debug)]
struct ReplaysUnderNewPositions(ScriptedStreams);

#[async_trait]
impl ChangeCapture for ReplaysUnderNewPositions {
    async fn continuity(&self) -> source::Result<Continuity> {
        Ok(Continuity::Resumed)
    }
    async fn prepare(&self) -> source::Result<()> {
        Ok(())
    }
    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        self.0.live().await
    }
    fn confirm(&self, _: Position) {}
    fn reached(&self, _: Option<Position>) -> Option<source::cdc::SourcePosition> {
        Some(source::cdc::SourcePosition(41))
    }
}

#[tokio::test(start_paused = true)]
async fn a_split_transaction_replayed_under_new_positions_doubles_its_backoff() {
    let replay = Run::ChangeThenFail {
        position: None,
        after: Duration::from_millis(200),
    };
    let source = Arc::new(ReplaysUnderNewPositions(ScriptedStreams::new(&[replay; 4])));
    let times = Arc::clone(&source.0.times);
    run_until_opened(
        source,
        &times,
        5,
        ScriptedFlushes::new(&[]),
        backendless_config(),
    )
    .await;

    assert_eq!(times.waits(), [1, 2, 4, 8]);
}
