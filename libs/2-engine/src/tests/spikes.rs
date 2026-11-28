//! Spike S6 of ADR 0012 stage 10: the two failures the attach boundary must
//! survive, driven over the real channel stream and the engine's `Progress`.
//!
//! The attach itself (`Progress::attach`, `LaneItem::Attached`) is not built
//! yet, so these pin the stream's and the progress's behaviour the rules rest
//! on, not the rules: a `SnapshotComplete` stands in for `Attached` on a lane,
//! and a backfill request for `Request::Attach` on the request lane.
//!
//! - **A sink run killed with a discard's acknowledgement pending (R6, H3).**
//!   An earlier delivery left unacknowledged comes back to the next run ahead
//!   of everything not yet delivered, even when the boundary after it was
//!   acknowledged: acting on the boundary while an earlier delivery is
//!   unacknowledged is not safe. Earlier deliveries acknowledged in line never
//!   come back.
//! - **An ingest run dying between the attach and `Attached` (R8, H2).** The
//!   request the dying run took and never acknowledged comes back to the next
//!   run, and what the dying run recorded in `Progress` is still there after
//!   the next run starts (`forget_all`): the next run can see the lane
//!   attached and must publish the boundary again.

use std::time::Duration;

use kernel::Position;
use stream::{Batch, Consumer, LaneItem, LaneKey, Request, Stream, Ticket};
use stream_channel::ChannelStream;

use super::{sink_name, users};

fn lane() -> LaneKey {
    LaneKey::new(users(), sink_name("primary"))
}

fn item(key: u64) -> LaneItem {
    LaneItem::Batch(Batch {
        key: Some(stream::ItemKey::new(key)),
        position: Some(Position::new(key)),
        envelopes: Vec::new().into(),
        ..Batch::default()
    })
}

fn boundary() -> LaneItem {
    LaneItem::SnapshotComplete {
        index: users(),
        ticket: Ticket::new(1),
    }
}

/// What a lane item is, for comparing deliveries.
fn kind(item: &LaneItem) -> String {
    match item {
        LaneItem::Batch(batch) => format!("item {}", batch.key.map_or(0, |key| key.number())),
        LaneItem::SnapshotComplete { .. } => "boundary".to_owned(),
        LaneItem::Attached { attempt } => format!("attached {attempt}"),
    }
}

/// Every delivery waiting on `consumer`, acknowledged as it is read.
async fn drain(consumer: &mut Box<dyn Consumer<LaneItem>>) -> Vec<String> {
    let mut seen = Vec::new();
    while let Ok(Ok(Some(delivery))) =
        tokio::time::timeout(Duration::from_millis(10), consumer.recv()).await
    {
        let (item, handle) = delivery.into_parts();
        handle.ack().await.unwrap();
        seen.push(kind(&item));
    }
    seen
}

/// A lane holding an item from before the boundary, the boundary, and one
/// after it, as a detached lane receives them.
async fn published() -> ChannelStream {
    let stream = ChannelStream::new(8, [lane()]);
    let producer = stream.lane(&lane()).unwrap().producer;
    producer.publish(item(1)).await.unwrap();
    producer.publish(boundary()).await.unwrap();
    producer.publish(item(2)).await.unwrap();
    stream
}

/// H3: a run takes the item from before the boundary, defers its
/// acknowledgement (as a held item's is deferred to the run's next step),
/// acts on the boundary and acknowledges it, then dies. The next run gets the
/// item from before the boundary first, on a lane it would now believe open.
#[tokio::test(start_paused = true)]
async fn spike_s6_an_acknowledged_boundary_does_not_shield_an_earlier_unacknowledged_item() {
    let stream = published().await;
    {
        let mut run = stream.lane(&lane()).unwrap().consumer;
        let (_, before) = run.recv().await.unwrap().unwrap().into_parts();
        let (taken, at_boundary) = run.recv().await.unwrap().unwrap().into_parts();
        assert_eq!(kind(&taken), "boundary");
        at_boundary.ack().await.unwrap();
        drop(before);
    }
    let mut next = stream.lane(&lane()).unwrap().consumer;
    assert_eq!(drain(&mut next).await, ["item 1", "item 2"]);
}

/// H3, the rule's side: every delivery before the boundary acknowledged in
/// line, before the boundary is taken. A run dying after it leaves nothing
/// from before the boundary to come back.
#[tokio::test(start_paused = true)]
async fn spike_s6_discards_acknowledged_in_line_never_come_back() {
    let stream = published().await;
    {
        let mut run = stream.lane(&lane()).unwrap().consumer;
        let (_, before) = run.recv().await.unwrap().unwrap().into_parts();
        before.ack().await.unwrap();
        let (_, at_boundary) = run.recv().await.unwrap().unwrap().into_parts();
        at_boundary.ack().await.unwrap();
    }
    let mut next = stream.lane(&lane()).unwrap().consumer;
    assert_eq!(drain(&mut next).await, ["item 2"]);
}

/// H3: a run dying with the boundary itself unacknowledged gets it back
/// first, after its acknowledged predecessors, so a boundary is acted on
/// again rather than lost; the rule makes a repeat of the current attempt a
/// no-op.
#[tokio::test(start_paused = true)]
async fn spike_s6_an_unacknowledged_boundary_comes_back_before_what_follows_it() {
    let stream = published().await;
    {
        let mut run = stream.lane(&lane()).unwrap().consumer;
        let (_, before) = run.recv().await.unwrap().unwrap().into_parts();
        before.ack().await.unwrap();
        let (_, at_boundary) = run.recv().await.unwrap().unwrap().into_parts();
        drop(at_boundary);
    }
    let mut next = stream.lane(&lane()).unwrap().consumer;
    assert_eq!(drain(&mut next).await, ["boundary", "item 2"]);
}

/// H2: an ingest run takes the attach request, changes what `Progress` holds
/// for the lane, and dies before publishing the boundary or acknowledging
/// the request. The next run's request consumer gets the request again, and
/// the progress still holds the dying run's change after the next run's
/// `forget_all`: the next run finds the lane as the dying run left it.
#[tokio::test(start_paused = true)]
async fn spike_s6_a_dying_runs_request_and_progress_reach_the_next_run() {
    let stream = ChannelStream::over(8, [users()], [sink_name("primary")]);
    let progress = crate::Progress::of([users()], [sink_name("primary")]);
    stream
        .requests()
        .unwrap()
        .producer
        .publish(Request::Backfill {
            index: users(),
            sink: sink_name("primary"),
            after: None,
            ticket: Ticket::new(7),
        })
        .await
        .unwrap();
    {
        let mut dying = stream.requests().unwrap().consumer;
        let (request, handle) = dying.recv().await.unwrap().unwrap().into_parts();
        assert!(matches!(request, Request::Backfill { ticket, .. } if ticket == Ticket::new(7)));
        progress.published(
            &lane(),
            stream::ItemKey::new(0),
            Position::new(1),
            Position::new(1),
        );
        progress.applied(&lane(), stream::ItemKey::new(0));
        drop(handle);
    }
    let landed_before = progress.lane(&lane(), crate::Level::Applied);
    progress.forget_all();
    assert_eq!(
        progress.lane(&lane(), crate::Level::Applied),
        landed_before,
        "a lane's own state outlives the run that set it"
    );
    let mut next = stream.requests().unwrap().consumer;
    let (request, handle) = tokio::time::timeout(Duration::from_millis(10), next.recv())
        .await
        .expect("the request comes back")
        .unwrap()
        .unwrap()
        .into_parts();
    assert!(matches!(request, Request::Backfill { ticket, .. } if ticket == Ticket::new(7)));
    handle.ack().await.unwrap();
}
