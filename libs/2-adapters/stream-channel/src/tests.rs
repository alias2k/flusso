use super::*;
use stream::Batch;

fn sink(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

fn batch(position: u64) -> LaneItem {
    LaneItem::Batch(Batch {
        position: Some(Position(position)),
        changes: 1,
        envelopes: Vec::new().into(),
    })
}

fn snapshot_batch() -> LaneItem {
    LaneItem::Batch(Batch {
        position: None,
        changes: 0,
        envelopes: Vec::new().into(),
    })
}

fn envelopes_of(item: &LaneItem) -> &Arc<[kernel::Envelope]> {
    match item {
        LaneItem::Batch(batch) => &batch.envelopes,
        LaneItem::SnapshotComplete { .. } => panic!("expected a batch"),
    }
}

#[tokio::test]
async fn publishes_and_consumes_in_order_and_acks_move_the_watermark() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    lane.producer.publish(batch(1)).await.unwrap();
    lane.producer.publish(batch(2)).await.unwrap();
    let mut consumer = lane.consumer;

    assert_eq!(stream.watermark(), None);
    let (first, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(first.position(), Some(Position(1)));
    handle.ack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(1)));

    let (second, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(second.position(), Some(Position(2)));
    assert!(consumer.is_empty());
    handle.ack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(2)));
}

#[tokio::test]
async fn watermark_is_the_minimum_over_lanes() {
    let stream = ChannelStream::new(8, [sink("fast"), sink("slow")]);
    for name in ["fast", "slow"] {
        let lane = stream.lane(&sink(name)).unwrap();
        lane.producer.publish(batch(5)).await.unwrap();
        lane.producer.publish(batch(6)).await.unwrap();
    }
    let mut fast = stream.lane(&sink("fast")).unwrap().consumer;
    let mut slow = stream.lane(&sink("slow")).unwrap().consumer;
    for _ in 0..2 {
        let (_, handle) = fast.recv().await.unwrap().unwrap().into_parts();
        handle.ack().await.unwrap();
    }
    assert_eq!(stream.watermark(), None, "slow has acked nothing yet");
    let (_, handle) = slow.recv().await.unwrap().unwrap().into_parts();
    handle.ack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(5)));
}

#[tokio::test]
async fn snapshot_batches_do_not_move_the_watermark() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    lane.producer.publish(snapshot_batch()).await.unwrap();
    lane.producer.publish(batch(3)).await.unwrap();
    let mut consumer = lane.consumer;
    let (_, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    handle.ack().await.unwrap();
    assert_eq!(stream.watermark(), None);
    let (_, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    handle.ack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(3)));
}

/// The lane and its redelivery ledger hold the ingest engine's one build, not
/// copies of it: the documents are never duplicated on the way to a sink.
#[tokio::test]
async fn a_delivery_and_its_redelivery_share_the_published_envelopes() {
    let published = batch(1);
    let envelopes = Arc::clone(envelopes_of(&published));
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    lane.producer.publish(published).await.unwrap();

    let mut consumer = lane.consumer;
    let delivery = consumer.recv().await.unwrap().unwrap();
    assert!(Arc::ptr_eq(&envelopes, envelopes_of(delivery.item())));
    delivery.nack().await.unwrap();
    let (redelivered, _) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert!(Arc::ptr_eq(&envelopes, envelopes_of(&redelivered)));
}

#[tokio::test]
async fn an_unacked_item_is_redelivered_to_the_next_consumer() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    lane.producer.publish(batch(1)).await.unwrap();
    lane.producer.publish(batch(2)).await.unwrap();

    let mut first = lane.consumer;
    let delivery = first.recv().await.unwrap().unwrap();
    assert_eq!(delivery.item().position(), Some(Position(1)));
    delivery.nack().await.unwrap();
    drop(first);

    let mut second = stream.lane(&sink("a")).unwrap().consumer;
    let (redelivered, handle) = second.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position(1)));
    handle.ack().await.unwrap();
    let (next, _) = second.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(next.position(), Some(Position(2)));
}

#[tokio::test]
async fn requests_travel_up_and_are_redelivered_until_acked() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let requests = stream.requests().unwrap();
    let request = Request::Backfill {
        sink: sink("a"),
        indexes: vec![kernel::IndexName::try_new("users").unwrap()],
    };
    requests.producer.publish(request.clone()).await.unwrap();
    let mut consumer = requests.consumer;
    let delivery = consumer.recv().await.unwrap().unwrap();
    assert_eq!(delivery.item(), &request);
    drop(delivery);
    let mut again = stream.requests().unwrap().consumer;
    let (item, handle) = again.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(item, request);
    handle.ack().await.unwrap();
    assert!(again.is_empty());
}

/// Several deliveries may be outstanding at once (the ingest engine holds every
/// coalesced request until the snapshot is published); each acks on its own,
/// and whatever is nacked or dropped unacked comes back in its original order.
#[tokio::test]
async fn several_deliveries_can_be_outstanding_and_ack_independently() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    for position in 1..=3 {
        lane.producer.publish(batch(position)).await.unwrap();
    }
    let mut consumer = lane.consumer;
    let (first, first_handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (second, second_handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (third, third_handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(
        [first.position(), second.position(), third.position()],
        [Some(Position(1)), Some(Position(2)), Some(Position(3))],
        "each recv hands out the next item, not the one still unacked"
    );
    assert!(consumer.is_empty(), "nothing waits behind the deliveries");
    assert!(!stream.is_idle(), "but three are unacknowledged");

    second_handle.ack().await.unwrap();
    assert_eq!(
        stream.watermark(),
        None,
        "2 is acknowledged, but 1 before it is not"
    );
    first_handle.nack().await.unwrap();
    drop(third_handle);

    let (redelivered, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position(1)));
    handle.ack().await.unwrap();
    let (redelivered, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position(3)));
    handle.ack().await.unwrap();
    assert!(stream.is_idle());
    assert_eq!(stream.watermark(), Some(Position(3)));
}

/// The caught-up hint is about what waits *behind* the current delivery: true
/// while the batch being committed is the only thing in flight, false while a
/// backlog is queued.
#[tokio::test]
async fn caught_up_ignores_the_delivery_being_processed() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    lane.producer.publish(batch(1)).await.unwrap();
    lane.producer.publish(batch(2)).await.unwrap();
    let mut consumer = lane.consumer;
    let (_, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert!(!consumer.is_empty(), "a second batch is queued");
    handle.ack().await.unwrap();
    let (_, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert!(
        consumer.is_empty(),
        "only the current delivery is in flight"
    );
    assert!(!stream.is_idle());
    handle.ack().await.unwrap();
    assert!(stream.is_idle());
}

#[test]
fn unknown_lane_is_an_error() {
    let stream = ChannelStream::new(1, [sink("a")]);
    assert!(matches!(
        stream.lane(&sink("zzz")),
        Err(QueueError::UnknownLane(_))
    ));
    assert_eq!(stream.sinks().count(), 1);
}

/// Releasing one of several deliveries redelivers that one alone: the others
/// are still being processed and are neither served again nor disturbed.
#[tokio::test]
async fn releasing_one_delivery_redelivers_only_that_one() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    for position in 1..=4 {
        lane.producer.publish(batch(position)).await.unwrap();
    }
    let mut consumer = lane.consumer;
    let (_, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, second) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, third) = consumer.recv().await.unwrap().unwrap().into_parts();

    first.nack().await.unwrap();

    let (redelivered, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position(1)));
    let (next, fourth) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(
        next.position(),
        Some(Position(4)),
        "the deliveries still in flight are not served again"
    );
    assert!(consumer.is_empty());

    for handle in [second, third, first, fourth] {
        handle.ack().await.unwrap();
    }
    assert!(stream.is_idle());
    assert_eq!(stream.watermark(), Some(Position(4)));
}

/// Released deliveries come back in lane order, whatever order they were
/// released in, and the ones left in flight stay out of it.
#[tokio::test]
async fn released_deliveries_come_back_in_lane_order() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    for position in 1..=3 {
        lane.producer.publish(batch(position)).await.unwrap();
    }
    let mut consumer = lane.consumer;
    let (_, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, second) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, third) = consumer.recv().await.unwrap().unwrap().into_parts();

    drop(third);
    first.nack().await.unwrap();
    assert!(!consumer.is_empty(), "two deliveries are due again");

    let (again, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(again.position(), Some(Position(1)));
    let (again, third) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(again.position(), Some(Position(3)));
    assert!(consumer.is_empty(), "the second is still in flight");

    second.ack().await.unwrap();
    first.ack().await.unwrap();
    third.ack().await.unwrap();
    assert!(stream.is_idle());
    assert_eq!(stream.watermark(), Some(Position(3)));
}

/// Deliveries acknowledged out of order (several writes in flight) never move
/// the watermark past an earlier one still unacknowledged: it stays at the
/// last positioned item before the oldest unacknowledged one.
#[tokio::test]
async fn a_later_ack_never_moves_the_watermark_past_an_earlier_unacked_item() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    for position in 1..=4 {
        lane.producer.publish(batch(position)).await.unwrap();
    }
    let mut consumer = lane.consumer;
    let (_, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, second) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, third) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, fourth) = consumer.recv().await.unwrap().unwrap().into_parts();

    fourth.ack().await.unwrap();
    third.ack().await.unwrap();
    assert_eq!(stream.watermark(), None, "1 and 2 are unacknowledged");

    first.ack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(1)), "2 is unacknowledged");

    second.ack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(4)));
}

/// A released delivery holds the watermark back until it is acknowledged on
/// redelivery, even when later ones were acknowledged meanwhile; snapshot
/// batches between them hold nothing back.
#[tokio::test]
async fn a_released_delivery_holds_the_watermark_until_it_is_acked() {
    let stream = ChannelStream::new(8, [sink("a")]);
    let lane = stream.lane(&sink("a")).unwrap();
    lane.producer.publish(batch(1)).await.unwrap();
    lane.producer.publish(snapshot_batch()).await.unwrap();
    lane.producer.publish(batch(2)).await.unwrap();
    lane.producer.publish(batch(3)).await.unwrap();
    let mut consumer = lane.consumer;
    let (_, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, page) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, second) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, third) = consumer.recv().await.unwrap().unwrap().into_parts();
    first.ack().await.unwrap();
    page.ack().await.unwrap();
    third.ack().await.unwrap();
    second.nack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(1)));

    let (again, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(again.position(), Some(Position(2)));
    handle.ack().await.unwrap();
    assert_eq!(stream.watermark(), Some(Position(3)));
}
