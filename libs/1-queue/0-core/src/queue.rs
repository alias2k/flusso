use async_trait::async_trait;

use crate::Result;

/// Publishes work items onto a queue. Cloneable backends let many producers
/// share one queue.
#[async_trait]
pub trait Producer<T: Send>: std::fmt::Debug + Send + Sync {
    /// Publish an item. May wait (backpressure) until the queue has room.
    async fn publish(&self, item: T) -> Result<()>;
}

/// Pulls work items from a queue for processing.
#[async_trait]
pub trait Consumer<T: Send>: std::fmt::Debug + Send {
    /// The next delivery, or `None` once the queue is closed and drained.
    async fn recv(&mut self) -> Result<Option<Delivery<T>>>;
}

/// A received item paired with the handle that confirms or returns it.
///
/// Processing is complete only when [`ack`](Self::ack) is called; until then a
/// durable backend may redeliver after a crash. Dropping a delivery without
/// acking leaves it unconfirmed (a durable backend redelivers it later).
#[derive(Debug)]
pub struct Delivery<T> {
    item: T,
    handle: Box<dyn AckHandle>,
}

impl<T> Delivery<T> {
    /// Build a delivery from an item and its backend ack handle.
    pub fn new(item: T, handle: Box<dyn AckHandle>) -> Self {
        Self { item, handle }
    }

    /// The work item.
    pub fn item(&self) -> &T {
        &self.item
    }

    /// Split into the item and its ack handle, so the item can be processed and
    /// the handle acked once the work is durably done.
    pub fn into_parts(self) -> (T, Box<dyn AckHandle>) {
        (self.item, self.handle)
    }

    /// Confirm the item was processed; a durable backend won't redeliver it.
    pub async fn ack(self) -> Result<()> {
        self.handle.ack().await
    }

    /// Return the item to the queue for redelivery (processing failed).
    pub async fn nack(self) -> Result<()> {
        self.handle.nack().await
    }
}

/// Confirms or returns a single delivery. Backend-specific: a no-op for
/// in-memory channels, a server acknowledgement for a durable broker.
#[async_trait]
pub trait AckHandle: std::fmt::Debug + Send {
    /// Confirm the delivery; it will not be redelivered.
    async fn ack(self: Box<Self>) -> Result<()>;

    /// Return the delivery for redelivery.
    async fn nack(self: Box<Self>) -> Result<()>;
}
