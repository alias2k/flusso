//! A shutdown is a graceful stop: `stopping` (not ready, no new operation),
//! then the in-flight batch lands and the run ends only once every sink
//! flushed it and its last writes are searchable.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{Envelope, IndexMapping, IndexName, Position};
use sink::{Sink, SinkOptions, Visibility, WriteReport};
use source::cdc::{ChangeCapture, Continuity, LiveChange, SourcePosition};
use tokio::sync::{Notify, oneshot};

use super::{ScriptedSource, backendless_config, backends, primary, row_event, users, wait_until};
use crate::status::Phase;
use crate::{ControlError, Daemon, FenceLevel, FenceOutcome, FenceRequest};

/// A sink whose flushes wait for `release` while `held`, then report
/// `visibility`. Counts flushes.
#[derive(Debug)]
struct HeldSink {
    held: AtomicBool,
    release: Notify,
    flushing: Notify,
    flushed: Mutex<usize>,
    visibility: Visibility,
}

impl HeldSink {
    fn new(visibility: Visibility) -> Self {
        Self {
            held: AtomicBool::new(false),
            release: Notify::new(),
            flushing: Notify::new(),
            flushed: Mutex::new(0),
            visibility,
        }
    }
}

#[async_trait]
impl Sink for HeldSink {
    async fn write(&self, envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        // An empty write is the engine asking an idle sink to refresh: held
        // and counted are the writes of documents.
        if !envelopes.is_empty() {
            if self.held.load(Ordering::SeqCst) {
                self.flushing.notify_one();
                self.release.notified().await;
            }
            *self.flushed.lock().unwrap() += 1;
        }
        Ok(WriteReport::clean().visible(self.visibility))
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// A sink whose staging never succeeds.
#[derive(Debug)]
struct UnstageableSink;

#[async_trait]
impl Sink for UnstageableSink {
    async fn ensure_index(&self, _: &IndexMapping) -> sink::Result<sink::Ensured> {
        Err(sink::SinkError::Write("destination unreachable".to_owned()))
    }

    async fn write(&self, _envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }
}

/// [`ScriptedSource`] that places a position on the source's own order
/// (position `n` covers source position `n + 1`), so the fence can answer.
#[derive(Debug)]
struct Placed(ScriptedSource);

#[async_trait]
impl ChangeCapture for Placed {
    async fn continuity(&self) -> source::Result<Continuity> {
        self.0.continuity().await
    }

    async fn prepare(&self) -> source::Result<()> {
        self.0.prepare().await
    }

    async fn live(&self) -> source::Result<BoxStream<'static, source::Result<LiveChange>>> {
        self.0.live().await
    }

    fn confirm(&self, landed: source::cdc::Landed) {
        self.0.confirm(landed);
    }

    fn parse_position(&self, text: &str) -> source::Result<SourcePosition> {
        text.parse()
            .map(SourcePosition)
            .map_err(|_| source::SourceError::Unsupported(text.to_owned()))
    }

    fn reached(&self, landed: source::cdc::Landed) -> Option<SourcePosition> {
        Some(SourcePosition(
            landed.covered_seq().map_or(0, |seq| seq + 1),
        ))
    }
}

#[tokio::test]
async fn a_stop_while_staging_ends_the_run_before_anything_is_captured() {
    let source = ScriptedSource::new(vec![row_event(false, 1)]);
    let events = Arc::clone(&source.events);
    let backends = backends(
        source,
        vec![(primary(), Arc::new(UnstageableSink), SinkOptions::default())],
    );
    let running = Daemon::new(super::fast_restarts(), backends)
        .start()
        .await
        .unwrap();
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let run = tokio::spawn(running.run(async move {
        let _ = shutdown_rx.await;
    }));

    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("a stop during staging ends the run")
        .unwrap()
        .unwrap();

    assert!(
        !events.lock().unwrap().contains(&"prepare".to_owned()),
        "the source was never prepared: {:?}",
        events.lock().unwrap()
    );
}

#[tokio::test]
async fn a_stop_is_not_ready_refuses_a_reindex_and_ends_once_the_in_flight_batch_is_flushed() {
    let sink = Arc::new(HeldSink::new(Visibility::Immediate));
    let (source, _end) = ScriptedSource::new(Vec::new()).held_open();
    let confirmed = Arc::clone(&source.confirmed);
    let (feed_source, feed) = super::FedSource::new(source);
    let backends = super::backends_for(
        Arc::new(feed_source),
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
    let status = running.status();
    let control = running.control();
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let mut run = tokio::spawn(running.run(async move {
        let _ = shutdown_rx.await;
    }));
    wait_until("the deployment to be live", || status.is_ready()).await;

    sink.held.store(true, Ordering::SeqCst);
    feed.send(row_event(false, 1)).await.unwrap();
    sink.flushing.notified().await;
    shutdown.send(()).unwrap();
    wait_until("the stopping phase", || {
        status.snapshot().phase == Phase::Stopping
    })
    .await;

    assert!(!status.is_ready(), "a stopping deployment is not ready");
    assert!(matches!(
        control.reindex(users(), None),
        Err(ControlError::Stopping)
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut run)
            .await
            .is_err(),
        "the run waits for the batch being flushed"
    );
    sink.release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("the run ends once the batch landed")
        .unwrap()
        .unwrap();
    assert_eq!(*sink.flushed.lock().unwrap(), 1);
    assert_eq!(confirmed.lock().unwrap().last(), Some(&Position::new(0)));
    assert_eq!(status.snapshot().phase, Phase::Stopped);
}

#[tokio::test(start_paused = true)]
async fn a_stop_ends_once_the_last_flush_is_searchable_and_answers_a_searchable_waiter() {
    let refresh = Duration::from_secs(1);
    let sink = Arc::new(HeldSink::new(Visibility::Within(refresh)));
    let (source, _end) = ScriptedSource::new(Vec::new()).held_open();
    let (feed_source, feed) = super::FedSource::new(Placed(source));
    let backends = super::backends_for(
        Arc::new(feed_source),
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
    let status = running.status();
    let fence = running.fence();
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let run = tokio::spawn(running.run(async move {
        let _ = shutdown_rx.await;
    }));
    wait_until("the deployment to be live", || status.is_ready()).await;

    sink.held.store(true, Ordering::SeqCst);
    feed.send(row_event(false, 1)).await.unwrap();
    sink.flushing.notified().await;
    let waiter = tokio::spawn(async move {
        fence
            .wait(&FenceRequest {
                position: "1".to_owned(),
                level: FenceLevel::Searchable,
                sink: None,
                indexes: Vec::new(),
                timeout: Duration::from_secs(20),
            })
            .await
    });
    shutdown.send(()).unwrap();
    sink.release.notify_one();
    let started = tokio::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert!(
        started.elapsed() >= refresh,
        "the run outlasts the refresh interval: {:?}",
        started.elapsed()
    );
    assert!(matches!(
        waiter.await.unwrap(),
        Ok(FenceOutcome::Reached { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn a_stop_nobody_waits_on_does_not_sit_out_the_refresh_interval() {
    let refresh = Duration::from_secs(10);
    let sink = Arc::new(HeldSink::new(Visibility::Within(refresh)));
    let (source, _end) = ScriptedSource::new(Vec::new()).held_open();
    let (feed_source, feed) = super::FedSource::new(Placed(source));
    let backends = super::backends_for(
        Arc::new(feed_source),
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
    let status = running.status();
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let run = tokio::spawn(running.run(async move {
        let _ = shutdown_rx.await;
    }));
    wait_until("the deployment to be live", || status.is_ready()).await;
    feed.send(row_event(false, 1)).await.unwrap();
    wait_until("the flush", || *sink.flushed.lock().unwrap() == 1).await;

    let started = tokio::time::Instant::now();
    shutdown.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(20), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert!(
        started.elapsed() < refresh,
        "nobody waits on searchable: {:?}",
        started.elapsed()
    );
}
