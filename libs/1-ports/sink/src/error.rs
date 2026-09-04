use thiserror::Error;

/// Result alias for sink operations.
pub type Result<T> = std::result::Result<T, SinkError>;

/// Why a sink write failed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SinkError {
    /// Writing to the destination failed (I/O, network, the remote rejecting it).
    #[error("write failed: {0}")]
    Write(String),

    /// A document could not be serialized for the destination.
    #[error("serialization failed: {0}")]
    Serialize(String),
}
