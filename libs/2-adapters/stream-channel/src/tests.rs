use super::*;
use kernel::{IndexName, Position};
use stream::Batch;

fn sink(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

/// The `users` lane of `name`.
fn key(name: &str) -> LaneKey {
    LaneKey::new(index("users"), sink(name))
}

fn batch(position: u64) -> LaneItem {
    LaneItem::Batch(Batch {
        position: Some(Position::new(position)),
        changes: 1,
        envelopes: Vec::new().into(),
        hold: Default::default(),
        walked: None,
        ticket: None,
    })
}

fn envelopes_of(item: &LaneItem) -> &Arc<[kernel::Envelope]> {
    match item {
        LaneItem::Batch(batch) => &batch.envelopes,
        LaneItem::SnapshotComplete { .. } => panic!("expected a batch"),
    }
}

#[tokio::test]
async fn publishes_and_consumes_in_order() {
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
    lane.producer.publish(batch(1)).await.unwrap();
    lane.producer.publish(batch(2)).await.unwrap();
    let mut consumer = lane.consumer;

    let (first, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(first.position(), Some(Position::new(1)));
    handle.ack().await.unwrap();

    let (second, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(second.position(), Some(Position::new(2)));
    assert!(consumer.is_empty());
    handle.ack().await.unwrap();
}

/// The lane and its redelivery ledger hold the ingest engine's one build, not
/// copies of it: the documents are never duplicated on the way to a sink.
#[tokio::test]
async fn a_delivery_and_its_redelivery_share_the_published_envelopes() {
    let published = batch(1);
    let envelopes = Arc::clone(envelopes_of(&published));
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
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
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
    lane.producer.publish(batch(1)).await.unwrap();
    lane.producer.publish(batch(2)).await.unwrap();

    let mut first = lane.consumer;
    let delivery = first.recv().await.unwrap().unwrap();
    assert_eq!(delivery.item().position(), Some(Position::new(1)));
    delivery.nack().await.unwrap();
    drop(first);

    let mut second = stream.lane(&key("a")).unwrap().consumer;
    let (redelivered, handle) = second.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position::new(1)));
    handle.ack().await.unwrap();
    let (next, _) = second.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(next.position(), Some(Position::new(2)));
}

#[tokio::test]
async fn requests_travel_up_and_are_redelivered_until_acked() {
    let stream = ChannelStream::new(8, [key("a")]);
    let requests = stream.requests().unwrap();
    let request = Request::Backfill {
        index: index("users"),
        sink: sink("a"),
        after: None,
        ticket: stream::Ticket::new(1),
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
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
    for position in 1..=3 {
        lane.producer.publish(batch(position)).await.unwrap();
    }
    let mut consumer = lane.consumer;
    let (first, first_handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (second, second_handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (third, third_handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(
        [first.position(), second.position(), third.position()],
        [
            Some(Position::new(1)),
            Some(Position::new(2)),
            Some(Position::new(3))
        ],
        "each recv hands out the next item, not the one still unacked"
    );
    assert!(consumer.is_empty(), "nothing waits behind the deliveries");
    assert!(!stream.is_idle(), "but three are unacknowledged");

    second_handle.ack().await.unwrap();
    assert!(
        !stream.is_idle(),
        "2 is acknowledged, but 1 before it is not"
    );
    first_handle.nack().await.unwrap();
    drop(third_handle);

    let (redelivered, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position::new(1)));
    handle.ack().await.unwrap();
    let (redelivered, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position::new(3)));
    handle.ack().await.unwrap();
    assert!(stream.is_idle());
}

/// A handle dropped without an ack makes its delivery due: the lane is not
/// empty, and the next `recv` hands it back before anything newer. An acked
/// handle dropped afterwards leaves nothing behind: no redelivery, and the
/// lane reads empty and idle.
#[tokio::test]
async fn only_a_handle_dropped_unacked_is_redelivered() {
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
    lane.producer.publish(batch(1)).await.unwrap();
    lane.producer.publish(batch(2)).await.unwrap();
    let mut consumer = lane.consumer;

    let (_, acked) = consumer.recv().await.unwrap().unwrap().into_parts();
    acked.ack().await.unwrap();
    let (_, dropped) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert!(consumer.is_empty(), "nothing queued, nothing due");
    drop(dropped);
    assert!(!consumer.is_empty(), "the dropped delivery is due");

    let (again, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(again.position(), Some(Position::new(2)), "it comes back");
    handle.ack().await.unwrap();
    assert!(consumer.is_empty());
    assert!(stream.is_idle());
    let mut next = consumer.recv();
    let polled = next
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()));
    assert!(
        polled.is_pending(),
        "nothing is redelivered for the acked handles"
    );
}

/// The caught-up hint is about what waits *behind* the current delivery: true
/// while the batch being committed is the only thing in flight, false while a
/// backlog is queued.
#[tokio::test]
async fn caught_up_ignores_the_delivery_being_processed() {
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
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
    let stream = ChannelStream::new(1, [key("a")]);
    assert!(matches!(
        stream.lane(&key("zzz")),
        Err(QueueError::UnknownLane(_))
    ));
    assert_eq!(stream.sinks(), [sink("a")]);
}

/// Releasing one of several deliveries redelivers that one alone: the others
/// are still being processed and are neither served again nor disturbed.
#[tokio::test]
async fn releasing_one_delivery_redelivers_only_that_one() {
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
    for position in 1..=4 {
        lane.producer.publish(batch(position)).await.unwrap();
    }
    let mut consumer = lane.consumer;
    let (_, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, second) = consumer.recv().await.unwrap().unwrap().into_parts();
    let (_, third) = consumer.recv().await.unwrap().unwrap().into_parts();

    first.nack().await.unwrap();

    let (redelivered, first) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(redelivered.position(), Some(Position::new(1)));
    let (next, fourth) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(
        next.position(),
        Some(Position::new(4)),
        "the deliveries still in flight are not served again"
    );
    assert!(consumer.is_empty());

    for handle in [second, third, first, fourth] {
        handle.ack().await.unwrap();
    }
    assert!(stream.is_idle());
}

/// Released deliveries come back in lane order, whatever order they were
/// released in, and the ones left in flight stay out of it.
#[tokio::test]
async fn released_deliveries_come_back_in_lane_order() {
    let stream = ChannelStream::new(8, [key("a")]);
    let lane = stream.lane(&key("a")).unwrap();
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
    assert_eq!(again.position(), Some(Position::new(1)));
    let (again, third) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(again.position(), Some(Position::new(3)));
    assert!(consumer.is_empty(), "the second is still in flight");

    second.ack().await.unwrap();
    first.ack().await.unwrap();
    third.ack().await.unwrap();
    assert!(stream.is_idle());
}

/// A sink's lanes share the configured capacity: each holds its part, so a
/// sink holds about `capacity` items whatever the number of its indexes.
#[tokio::test]
async fn a_sinks_lanes_share_its_capacity() {
    let users = LaneKey::new(index("users"), sink("a"));
    let orders = LaneKey::new(index("orders"), sink("a"));
    let audit = LaneKey::new(index("users"), sink("audit"));
    let stream = ChannelStream::new(4, [users.clone(), orders.clone(), audit.clone()]);
    for (key, room) in [(&users, 2), (&orders, 2), (&audit, 4)] {
        let producer = stream.lane(key).unwrap().producer;
        for position in 0..room {
            producer.publish(batch(position)).await.unwrap();
        }
        let mut full = producer.publish(batch(room));
        let polled = full
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()));
        assert!(polled.is_pending(), "{key} holds {room} items");
    }
    assert_eq!(stream.lanes(&sink("a")), [orders, users]);
}

/// Lanes are independent: an item released on one lane is redelivered there
/// alone, and another lane's deliveries go on.
#[tokio::test]
async fn a_release_on_one_lane_leaves_the_others_alone() {
    let users = LaneKey::new(index("users"), sink("a"));
    let orders = LaneKey::new(index("orders"), sink("a"));
    let stream = ChannelStream::new(8, [users.clone(), orders.clone()]);
    let users_lane = stream.lane(&users).unwrap();
    let orders_lane = stream.lane(&orders).unwrap();
    users_lane.producer.publish(batch(1)).await.unwrap();
    orders_lane.producer.publish(batch(2)).await.unwrap();
    orders_lane.producer.publish(batch(3)).await.unwrap();

    let mut users_consumer = users_lane.consumer;
    let mut orders_consumer = orders_lane.consumer;
    let delivery = users_consumer.recv().await.unwrap().unwrap();
    let (first, handle) = orders_consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(first.position(), Some(Position::new(2)));
    delivery.nack().await.unwrap();
    handle.ack().await.unwrap();

    let (again, handle) = users_consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(again.position(), Some(Position::new(1)));
    handle.ack().await.unwrap();
    let (next, handle) = orders_consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(next.position(), Some(Position::new(3)));
    handle.ack().await.unwrap();
    assert!(stream.is_idle());
}

/// Each lane built from configuration holds the backstop, whatever the
/// number of sinks and indexes (the request lane too).
#[test]
fn every_configured_lane_holds_the_backstop() {
    let sinks = ["audit", "primary"].map(|name| kernel::SinkName::try_new(name).unwrap());
    let indexes = ["orders", "users"].map(|name| kernel::IndexName::try_new(name).unwrap());
    let lanes: Vec<LaneKey> = indexes
        .iter()
        .flat_map(|index| {
            sinks
                .iter()
                .map(move |sink| LaneKey::new(index.clone(), sink.clone()))
        })
        .collect();
    let stream = ChannelStream::from_config(&ChannelConfig::default(), lanes);
    for channel in stream.lanes.values() {
        assert_eq!(channel.tx.max_capacity(), LANE_BACKSTOP);
    }
    assert_eq!(stream.requests.tx.max_capacity(), LANE_BACKSTOP);
}

/// Counts the warnings logged while it is the default subscriber.
#[derive(Clone, Default)]
struct Warnings(Arc<std::sync::atomic::AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Warnings {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if *event.metadata().level() == tracing::Level::WARN {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

/// A `capacity` the config still names warns once and changes nothing: a
/// lane of capacity one takes several items unread.
#[tokio::test]
async fn a_deprecated_capacity_warns_once_and_is_ignored() {
    use tracing_subscriber::layer::SubscriberExt;

    let warnings = Warnings::default();
    let subscriber = tracing_subscriber::registry().with(warnings.clone());
    let config: ChannelConfig = toml::from_str("capacity = 1").unwrap();
    let stream = tracing::subscriber::with_default(subscriber, || {
        ChannelStream::from_config(&config, [key("a"), key("b")])
    });
    assert_eq!(warnings.0.load(std::sync::atomic::Ordering::SeqCst), 1);

    let lane = stream.lane(&key("a")).unwrap();
    for position in 0..4 {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            lane.producer.publish(batch(position)),
        )
        .await
        .expect("a lane takes more than the deprecated capacity")
        .unwrap();
    }

    let quiet = Warnings::default();
    let subscriber = tracing_subscriber::registry().with(quiet.clone());
    tracing::subscriber::with_default(subscriber, || {
        ChannelStream::from_config(&ChannelConfig::default(), [key("a")])
    });
    assert_eq!(quiet.0.load(std::sync::atomic::Ordering::SeqCst), 0);
}
