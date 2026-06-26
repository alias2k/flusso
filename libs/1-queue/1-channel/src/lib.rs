#![doc = include_str!("../README.md")]

use async_trait::async_trait;
use queue_core::{AckHandle, Consumer, Delivery, Producer, QueueError, Result};
use tokio::sync::mpsc;

/// Create a bounded in-process queue with room for `capacity` pending items.
pub fn channel<T: Send + 'static>(capacity: usize) -> (ChannelProducer<T>, ChannelConsumer<T>) {
    let (tx, rx) = mpsc::channel(capacity);
    (ChannelProducer { tx }, ChannelConsumer { rx })
}

/// The publishing half. Clone it to publish from many tasks.
pub struct ChannelProducer<T> {
    tx: mpsc::Sender<T>,
}

impl<T> Clone for ChannelProducer<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<T> std::fmt::Debug for ChannelProducer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelProducer").finish_non_exhaustive()
    }
}

#[async_trait]
impl<T: Send + 'static> Producer<T> for ChannelProducer<T> {
    async fn publish(&self, item: T) -> Result<()> {
        self.tx.send(item).await.map_err(|_| QueueError::Closed)
    }
}

/// The consuming half — a single consumer (clone the producer, not this).
pub struct ChannelConsumer<T> {
    rx: mpsc::Receiver<T>,
}

impl<T> ChannelConsumer<T> {
    /// Whether no items are currently waiting in the queue — a point-in-time
    /// snapshot of whether the pipeline has drained everything captured so far.
    /// The engine reads this at a batch boundary to decide whether a flush has
    /// *caught up* (see `Sink::flush`'s `caught_up`).
    pub fn is_empty(&self) -> bool {
        self.rx.is_empty()
    }
}

impl<T> std::fmt::Debug for ChannelConsumer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelConsumer").finish_non_exhaustive()
    }
}

#[async_trait]
impl<T: Send + 'static> Consumer<T> for ChannelConsumer<T> {
    async fn recv(&mut self) -> Result<Option<Delivery<T>>> {
        Ok(self
            .rx
            .recv()
            .await
            .map(|item| Delivery::new(item, Box::new(ChannelAck))))
    }
}

/// In-memory deliveries need no acknowledgement.
#[derive(Debug)]
struct ChannelAck;

#[async_trait]
impl AckHandle for ChannelAck {
    async fn ack(self: Box<Self>) -> Result<()> {
        Ok(())
    }

    async fn nack(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
