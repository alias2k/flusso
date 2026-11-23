//! A shutdown is a graceful stop: `stopping` (not ready, no new operation),
//! then the in-flight batch lands and the run ends only once every sink
//! flushed it and its last writes are searchable.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
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
use engine::{EngineId, Observer, Stop};

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

/// A sink whose staging asks for the stop: the staging then ends inside the
/// poll that already looked at the stop.
#[derive(Debug, Default)]
struct StoppingSink {
    stop: OnceLock<Stop>,
}

#[async_trait]
impl Sink for StoppingSink {
    async fn ensure_index(&self, _: &IndexMapping) -> sink::Result<sink::Ensured> {
        if let Some(stop) = self.stop.get() {
            stop.request();
        }
        Ok(sink::Ensured::default())
    }

    async fn write(&self, _envelopes: &[Envelope], _caught_up: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }
}

/// Records whether the ingest engine ran to its end.
#[derive(Debug, Default)]
struct IngestRan(AtomicBool);

impl Observer for IngestRan {
    fn on_engine_stopped(&self, engine: &EngineId) {
        if matches!(engine, EngineId::Ingest) {
            self.0.store(true, Ordering::SeqCst);
        }
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

/// Fourth review of #470, 1: a stop requested before the first staging never
/// lets the source prepare: the supervisor doesn't go on to the engines, so
/// a `Fresh` source's rebuild can't be skipped and then prepared over (#120).
/// Here the stop is the daemon's own, requested before the run starts, with
/// a sink whose staging is ready at once.
#[tokio::test]
async fn a_stop_before_the_first_staging_never_prepares() {
    for _ in 0..20 {
        let source = ScriptedSource::new(Vec::new()).with_continuity(Continuity::Fresh);
        let events = Arc::clone(&source.events);
        let sink = Arc::new(super::RecordingSink::seeded().with_events(&events));
        let ensured = Arc::clone(&sink.ensured);
        let backends = backends(
            source,
            vec![(primary(), sink as Arc<dyn Sink>, SinkOptions::default())],
        );
        let running = Daemon::new(backendless_config(), backends)
            .start()
            .await
            .unwrap();
        running.stop.request();
        tokio::time::timeout(Duration::from_secs(5), running.run(std::future::pending()))
            .await
            .expect("the run ends on the stop")
            .unwrap();
        let events = events.lock().unwrap().clone();
        assert!(
            !events.contains(&"prepare".to_owned()),
            "never prepared: {events:?}"
        );
        assert_eq!(
            ensured.load(Ordering::SeqCst),
            0,
            "the supervisor saw the stop first: nothing staged"
        );
    }
}

/// Fourth review of #470, 1, as the reviewer reproduced it: a shutdown that
/// is ready at once races the first staging; whichever wins, the source is
/// never prepared without the `Fresh` rebuild staged first.
#[tokio::test]
async fn a_shutdown_racing_the_first_staging_never_prepares_without_the_rebuild() {
    for _ in 0..60 {
        let source = ScriptedSource::new(Vec::new()).with_continuity(Continuity::Fresh);
        let events = Arc::clone(&source.events);
        let sink = Arc::new(super::RecordingSink::seeded().with_events(&events));
        let backends = backends(
            source,
            vec![(primary(), sink as Arc<dyn Sink>, SinkOptions::default())],
        );
        let running = Daemon::new(backendless_config(), backends)
            .start()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), running.run(std::future::ready(())))
            .await
            .expect("the run ends on the stop")
            .unwrap();
        let events = events.lock().unwrap().clone();
        if events.contains(&"prepare".to_owned()) {
            assert!(
                events.contains(&"reindex users".to_owned()),
                "prepared without the rebuild: {events:?}"
            );
        }
    }
}

/// Fifth review of #470, 1: a stop requested from inside the first staging
/// ends that staging in the poll that already passed the stop. The check
/// after staging is then the only thing keeping the engines from starting
/// behind a staging that only ensured.
#[tokio::test]
async fn a_stop_requested_inside_the_first_staging_starts_no_engine() {
    let source = ScriptedSource::new(Vec::new()).with_continuity(Continuity::Fresh);
    let events = Arc::clone(&source.events);
    let sink = Arc::new(StoppingSink::default());
    let ingest_ran = Arc::new(IngestRan::default());
    let backends = backends(
        source,
        vec![(
            primary(),
            Arc::clone(&sink) as Arc<dyn Sink>,
            SinkOptions::default(),
        )],
    );
    let running = Daemon::new(backendless_config(), backends)
        .with_observer(Arc::clone(&ingest_ran) as Arc<dyn Observer>)
        .start()
        .await
        .unwrap();
    sink.stop.set(running.stop.clone()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), running.run(std::future::pending()))
        .await
        .expect("the run ends on the stop")
        .unwrap();
    let events = events.lock().unwrap().clone();
    assert!(
        !events.contains(&"prepare".to_owned()),
        "never prepared: {events:?}"
    );
    assert!(
        !ingest_ran.0.load(Ordering::SeqCst),
        "the ingest engine never ran"
    );
}
