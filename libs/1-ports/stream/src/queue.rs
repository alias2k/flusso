use async_trait::async_trait;

use crate::Result;

/// Publishes items onto a lane. An adapter's producer is cheap to obtain, so
/// the ingest engine holds one per lane.
#[async_trait]
pub trait Producer<T: Send>: std::fmt::Debug + Send + Sync {
    /// Publish an item. May wait (backpressure) until the lane has room.
    async fn publish(&self, item: T) -> Result<()>;
}

/// Pulls items from a lane for processing. Exactly one consumer is active per
/// lane at a time; a restarted engine obtains a fresh one over the same lane
/// and is redelivered whatever the previous one left unacknowledged.
#[async_trait]
pub trait Consumer<T: Send>: std::fmt::Debug + Send {
    /// The next delivery, or `None` once the lane is closed and drained.
    async fn recv(&mut self) -> Result<Option<Delivery<T>>>;

    /// Whether nothing is waiting behind the last delivery — the sink engine's
    /// *caught-up* hint, read at a batch boundary and forwarded to the sink's
    /// flush so a destination with a cost to making writes visible can pay it
    /// only when idle. A point-in-time snapshot; an adapter that cannot tell
    /// returns `false`.
    fn is_empty(&self) -> bool;
}

/// A received item paired with the handle that confirms or returns it.
///
/// Processing is complete only when the [`AckHandle`] taken from
/// [`into_parts`](Self::into_parts) is acked; until then the adapter may
/// redeliver after a restart. Dropping a delivery without acking leaves it
/// unconfirmed (it is redelivered to the next consumer of the lane).
#[derive(Debug)]
pub struct Delivery<T> {
    item: T,
    handle: Box<dyn AckHandle>,
}

impl<T> Delivery<T> {
    /// Build a delivery from an item and its adapter ack handle.
    pub fn new(item: T, handle: Box<dyn AckHandle>) -> Self {
        Self { item, handle }
    }

    /// Peek at the item without consuming the delivery.
    pub fn item(&self) -> &T {
        &self.item
    }

    /// Split into the item and its ack handle, so the item can be processed and
    /// the handle acked once the work is durably done.
    pub fn into_parts(self) -> (T, Box<dyn AckHandle>) {
        (self.item, self.handle)
    }

    /// Return the item to the lane for redelivery (processing failed).
    pub async fn nack(self) -> Result<()> {
        self.handle.nack().await
    }
}

/// Confirms or returns a single delivery. Adapter-specific: an in-memory
/// bookkeeping step for the channel, a server acknowledgement for a broker.
/// Acking a lane batch is also what moves that lane's watermark.
#[async_trait]
pub trait AckHandle: std::fmt::Debug + Send {
    /// Confirm the delivery; it will not be redelivered.
    async fn ack(self: Box<Self>) -> Result<()>;

    /// Return the delivery for redelivery.
    async fn nack(self: Box<Self>) -> Result<()>;
}
