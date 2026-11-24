//! A fence on a healthy sink is not held back by a sibling sink whose lane is
//! full (ADR 0009, per sink).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::BoxStream;
use kernel::{Envelope, IndexName};
use sink::{Sink, SinkOptions, WriteReport};
use source::cdc::{ChangeCapture, Continuity, Landed, LiveChange, SourcePosition};

use super::{RecordingSink, ScriptedSource, backendless_config, row_event, wait_until};
use crate::{Daemon, FenceLevel, FenceOutcome, FenceRequest};

/// A sink whose writes never return.
#[derive(Debug)]
struct Stalled;

#[async_trait]
impl Sink for Stalled {
    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        std::future::pending().await
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

/// [`ScriptedSource`] whose position `n` covers source position `n + 1`.
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

    fn confirm(&self, landed: Landed) {
        self.0.confirm(landed);
    }

    fn parse_position(&self, text: &str) -> source::Result<SourcePosition> {
        text.parse()
            .map(SourcePosition)
            .map_err(|_| source::SourceError::Unsupported(text.to_owned()))
    }

    fn reached(&self, landed: Landed) -> Option<SourcePosition> {
        Some(SourcePosition(match landed {
            Landed::Before(oldest) => oldest.seq,
            Landed::Through(last) => last.seq + 1,
            Landed::Idle => 0,
            Landed::Free => u64::MAX,
        }))
    }
}

/// One change per batch; `stalled` never writes, so its lane fills and the
/// ingest engine waits on it, while `healthy` writes everything it took. A
/// fence on `healthy` for what it wrote passes.
#[tokio::test]
async fn a_fence_on_a_healthy_sink_passes_while_a_sibling_lane_is_full() {
    let (source, _end) =
        ScriptedSource::new((1..=400).map(|id| row_event(false, id)).collect()).held_open();
    let healthy = Arc::new(RecordingSink::seeded());
    let ops = Arc::clone(&healthy.ops);
    let documents = Arc::new(super::ScriptedDocuments {
        events: Arc::clone(&source.events),
    });
    let backends = Arc::new(super::MockBackends {
        capture: Arc::new(Placed(source)),
        documents,
        sinks: vec![
            (
                super::primary(),
                healthy as Arc<dyn Sink>,
                SinkOptions::default(),
            ),
            (
                kernel::SinkName::try_new("stalled").unwrap(),
                Arc::new(Stalled),
                SinkOptions::default(),
            ),
        ],
        built: Arc::new(std::sync::Mutex::new(Vec::new())),
    });
    let mut config = backendless_config();
    config.batch.max_changes = std::num::NonZeroUsize::new(1);
    let running = Daemon::new(config, backends).start().await.unwrap();
    let fence = running.fence();
    let run = tokio::spawn(running.run(std::future::pending::<()>()));

    // The stalled lane takes at most capacity + the writes in flight; past
    // that the ingest engine waits on it.
    wait_until("the healthy sink to stop writing", || {
        ops.lock().unwrap().len() >= 20
    })
    .await;
    let mut written = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let now = ops.lock().unwrap().len();
        if now == written {
            break;
        }
        written = now;
    }
    assert!(
        written < 400,
        "the stalled lane holds the ingest engine back"
    );
    let outcome = fence
        .wait(&FenceRequest {
            position: written.to_string(),
            level: FenceLevel::Applied,
            sink: Some(super::primary()),
            indexes: Vec::new(),
            timeout: Duration::from_secs(1),
        })
        .await;
    assert!(
        matches!(outcome, Ok(FenceOutcome::Reached { .. })),
        "{written} changes written to the healthy sink: {outcome:?}"
    );
    run.abort();
}
