use thiserror::Error;

/// Result alias for queue operations.
pub type Result<T> = std::result::Result<T, QueueError>;

/// Why a queue operation failed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum QueueError {
    /// The queue is closed (all producers dropped, or the broker is gone).
    #[error("the queue is closed")]
    Closed,

    /// Publishing an item failed.
    #[error("publish failed: {0}")]
    Publish(String),

    /// Receiving from the queue failed.
    #[error("consume failed: {0}")]
    Consume(String),

    /// Acknowledging or returning a delivery failed.
    #[error("acknowledgement failed: {0}")]
    Ack(String),
}
