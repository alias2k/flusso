//! #479 seen from `/status`: a sink engine whose seed is refused because the
//! old generation is blocked parks the lane `blocked` with the reason, while
//! the sink stays `live` and the deployment ready.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use engine::{Observer, SinkEngine};
use kernel::{ContentHash, Envelope, GenericValue, IndexMapping};
use sink::{Sink, SinkError, WriteReport};
use source::cdc::Continuity;
use stream::{Batch, LaneItem, LaneKey, Stream};
use stream_channel::ChannelStream;

use super::{primary, users};
use crate::observer::StatusObserver;
use crate::status::Status;

/// A sink whose seed is always refused by a read-only old generation.
#[derive(Debug)]
struct Refusing;

#[async_trait]
impl Sink for Refusing {
    async fn write(&self, _: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        Ok(WriteReport::clean())
    }

    async fn is_seeded(&self, _: &kernel::IndexName) -> sink::Result<bool> {
        Ok(true)
    }

    async fn mark_seeded(&self, _: &kernel::IndexName) -> sink::Result<()> {
        Err(SinkError::RetireBlocked {
            new: "users_00000001_2".to_owned(),
            old: "users_00000001_1".to_owned(),
            block: "index [users_00000001_1] blocked by: [FORBIDDEN/5/index read-only (api)];"
                .to_owned(),
            deletable: false,
        })
    }
}

#[tokio::test(start_paused = true)]
async fn a_lane_parked_on_a_refused_seed_shows_blocked_and_the_deployment_stays_ready() {
    let stream = Arc::new(ChannelStream::over(64, [users()], [primary()]));
    let progress = Arc::new(engine::Progress::of([users()], [primary()]));
    let fence = crate::Fence::new(
        Arc::new(super::Holding(None)),
        Arc::clone(&progress),
        [LaneKey::new(users(), primary())],
    );
    let status =
        Arc::new(Status::new([users()], [primary()], std::time::Instant::now()).with_fence(fence));
    let observer = Arc::new(StatusObserver::new(Arc::clone(&status)));
    let engine = Arc::new(
        SinkEngine::new(
            primary(),
            Arc::new(Refusing) as Arc<dyn Sink>,
            Arc::clone(&stream) as Arc<dyn Stream>,
            vec![IndexMapping {
                index: users(),
                hash: ContentHash::new(1),
                fields: Vec::new(),
            }],
            progress,
        )
        .with_observer(Arc::clone(&observer) as Arc<dyn Observer>),
    );
    engine.stage(Continuity::Resumed).await.unwrap();
    observer.on_live_started();
    let producer = stream
        .lane(&LaneKey::new(users(), primary()))
        .unwrap()
        .producer;
    producer
        .publish(LaneItem::Batch(Batch {
            envelopes: vec![Envelope::upsert(
                users(),
                "1",
                GenericValue::Map(BTreeMap::new()),
                None,
                chrono::Utc::now(),
            )]
            .into(),
            ..Batch::default()
        }))
        .await
        .unwrap();
    producer
        .publish(LaneItem::SnapshotComplete {
            index: users(),
            ticket: stream::Ticket::default(),
        })
        .await
        .unwrap();
    let running = Arc::clone(&engine);
    let (_control, mut receiver) = tokio::sync::mpsc::channel(1);
    let run = tokio::spawn(async move { running.run(&mut receiver).await });

    let lane = |status: &Status| {
        serde_json::to_value(status.snapshot()).unwrap()["sinks"]["primary"].clone()
    };
    for _ in 0..10_000 {
        if lane(&status)["lanes"]["users"]["health"] == "blocked" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    let sink = lane(&status);
    assert_eq!(sink["lanes"]["users"]["health"], "blocked", "{sink}");
    let reason = sink["lanes"]["users"]["reason"].as_str().unwrap();
    assert!(
        reason.starts_with("blocked: the new generation users_00000001_2 is built"),
        "{reason}"
    );
    assert!(
        reason.contains("FORBIDDEN/5/index read-only (api)"),
        "{reason}"
    );
    assert_eq!(sink["phase"], "live", "{sink}");
    assert!(
        status.is_ready(),
        "a parked seed leaves the deployment ready"
    );
    assert!(!run.is_finished(), "the run goes on");
    run.abort();
}
