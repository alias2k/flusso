//! Each lane's progress is its own: a sibling lane that is full holds back
//! neither what a healthy lane is told it landed nor a fence on it.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use kernel::{Envelope, IndexName, Position};
use sink::{Sink, WriteReport};
use source::cdc::{ChangeCapture, Landed};
use source::document::DocumentBuilder;
use stream::Stream;
use stream_channel::ChannelStream;
use tokio::sync::mpsc;

use super::{MockDocuments, MockSource, RecordingSink, mapping, sink_name, upsert, wait_for};
use crate::{BatchPolicy, IngestEngine, Level, SinkControl, SinkEngine};

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

/// The seq just past what `landed` says is landed.
fn bound(landed: Landed) -> u64 {
    match landed {
        Landed::Before(oldest) => oldest.seq,
        Landed::Through(last) => last.seq + 1,
        Landed::Idle => 0,
    }
}

/// Lanes hold one item, `stalled` never writes: the ingest engine blocks
/// publishing to it while `healthy` goes on writing what it already took.
/// Everything `healthy` wrote must be what its lane reaches.
#[tokio::test]
async fn a_healthy_lane_reaches_what_it_landed_while_a_sibling_lane_is_full() {
    let (healthy, stalled) = (sink_name("healthy"), sink_name("stalled"));
    let stream = Arc::new(ChannelStream::new(1, [healthy.clone(), stalled.clone()]));
    let source = Arc::new(MockSource::new((0..60).map(upsert).collect()));
    let ingest = IngestEngine::new(
        Arc::clone(&source) as Arc<dyn ChangeCapture>,
        Arc::new(MockDocuments::default()) as Arc<dyn DocumentBuilder>,
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![healthy.clone(), stalled.clone()],
        Arc::new(crate::Progress::new([healthy.clone(), stalled.clone()])),
    )
    .with_batch(BatchPolicy {
        max_changes: 1,
        max_delay: Duration::from_millis(1),
    });
    let progress = ingest.progress();
    let recording = Arc::new(RecordingSink::seeded(true));
    let ops = Arc::clone(&recording.ops);
    let mut tasks = Vec::new();
    for (name, sink) in [
        (healthy.clone(), recording as Arc<dyn Sink>),
        (stalled.clone(), Arc::new(Stalled) as Arc<dyn Sink>),
    ] {
        let engine = SinkEngine::new(
            name,
            sink,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![mapping()],
            Arc::clone(&progress),
        );
        engine
            .stage(source::cdc::Continuity::Resumed)
            .await
            .unwrap();
        tasks.push(tokio::spawn(async move {
            let (_keep, mut control) = mpsc::channel::<SinkControl>(1);
            engine.run(&mut control).await
        }));
    }
    let run = tokio::spawn(async move { ingest.run().await });

    wait_for(|| ops.lock().unwrap().len() >= 3).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let written = ops.lock().unwrap().len() as u64;
    let reached = progress.lane(&healthy, Level::Applied).unwrap();
    assert!(
        bound(reached) >= written,
        "the healthy sink wrote positions 0..{written}, its lane says {reached:?}"
    );
    assert_eq!(
        progress.lane(&stalled, Level::Applied),
        Some(Landed::Before(Position::new(0))),
        "the stalled lane still pins at its first item"
    );
    assert_eq!(progress.landed(), Landed::Before(Position::new(0)));

    run.abort();
    for task in tasks {
        task.abort();
    }
}
