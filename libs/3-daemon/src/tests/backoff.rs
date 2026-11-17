//! A failed engine restarts after a backoff that starts at one second and
//! doubles while it keeps failing without progress. A run that made progress
//! (a sink engine acknowledged a lane item, the ingest engine published a
//! batch) resets it, so a short outage never lengthens every later recovery.
//! On paused time: each test reads the restart waits off the virtual clock.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};
use kernel::{Envelope, IndexName, Position};
use sink::{FlushReport, Sink, SinkError, SinkOptions};
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
}

impl ScriptedFlushes {
    fn new(script: &[bool]) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script.iter().rev().copied().collect()),
            flushed_at: Mutex::new(Vec::new()),
        })
    }

    fn flushes(&self) -> Vec<Instant> {
        self.flushed_at.lock().unwrap().clone()
    }
}

#[async_trait]
impl Sink for ScriptedFlushes {
    async fn apply(&self, _: &Envelope) -> sink::Result<()> {
        Ok(())
    }

    async fn flush(&self, _: bool) -> sink::Result<FlushReport> {
        self.flushed_at.lock().unwrap().push(Instant::now());
        if self.script.lock().unwrap().pop() == Some(false) {
            return Err(SinkError::Write("simulated outage".to_owned()));
        }
        Ok(FlushReport::clean())
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
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

/// A source whose live streams follow `script`: `true` yields one change,
/// then fails once it has been published (200 ms later); `false` fails at
/// once. Past the script the stream stays open. Records when each stream was
/// opened.
#[derive(Debug)]
struct ScriptedStreams {
    script: Mutex<Vec<bool>>,
    opened_at: Arc<Mutex<Vec<Instant>>>,
    next: Mutex<u64>,
}

impl ScriptedStreams {
    fn new(script: &[bool]) -> Self {
        Self {
            script: Mutex::new(script.iter().rev().copied().collect()),
            opened_at: Arc::new(Mutex::new(Vec::new())),
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
        self.opened_at.lock().unwrap().push(Instant::now());
        let failure = || Err(source::SourceError::Connection("simulated drop".to_owned()));
        Ok(match self.script.lock().unwrap().pop() {
            None => Box::pin(stream::pending()),
            Some(false) => Box::pin(stream::iter([failure()])),
            Some(true) => {
                let position = {
                    let mut next = self.next.lock().unwrap();
                    *next += 1;
                    Position(*next - 1)
                };
                let change = stream::iter([Ok((position, row_event(false, 1)))]);
                let then_fail = stream::once(async move {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    failure()
                });
                Box::pin(change.chain(then_fail))
            }
        })
    }

    fn confirm(&self, _: Position) {}
}

async fn ingest_restart_waits(script: &[bool]) -> Vec<u64> {
    let source = ScriptedStreams::new(script);
    let opened = Arc::clone(&source.opened_at);
    let sink = ScriptedFlushes::new(&[]);
    let backends = backends_for(
        Arc::new(source),
        vec![(primary(), sink as Arc<dyn Sink>, SinkOptions::default())],
    );
    let running = Daemon::new(backendless_config(), backends)
        .start()
        .await
        .unwrap();
    let streams = script.len() + 1;
    let watched = Arc::clone(&opened);
    running
        .run(async move {
            while watched.lock().unwrap().len() < streams {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    let opened = opened.lock().unwrap().clone();
    seconds(&gaps(&opened))
}

#[tokio::test(start_paused = true)]
async fn an_ingest_engine_that_published_restarts_after_one_second_again() {
    assert_eq!(ingest_restart_waits(&[true, true]).await, [1, 1]);
}

#[tokio::test(start_paused = true)]
async fn an_ingest_engine_failing_without_progress_doubles_its_backoff() {
    assert_eq!(
        ingest_restart_waits(&[false, false, false]).await,
        [1, 2, 4]
    );
}
