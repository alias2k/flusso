use thiserror::Error;

/// Result alias for source operations.
pub type Result<T> = std::result::Result<T, SourceError>;

/// Why a [`ChangeCapture`](crate::cdc::ChangeCapture) failed to start or to
/// produce the next change.
///
/// The split that matters to the engine is transient vs. fatal:
/// [`Connection`](Self::Connection) is worth retrying (resume picks up from the
/// last confirmed point), while [`Setup`](Self::Setup) needs operator
/// intervention.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SourceError {
    /// The connection to Postgres dropped or could not be established.
    /// Transient: the engine may retry, and the mechanism resumes from its last
    /// confirmed point.
    #[error("connection failed: {0}")]
    Connection(String),

    /// Setup the mechanism depends on is missing or invalid — a dropped
    /// replication slot, a missing publication, `wal_level` too low, missing
    /// privileges. Not recoverable by retrying.
    #[error("setup error: {0}")]
    Setup(String),

    /// A raw change could not be decoded into a [`Change`](crate::cdc::Change).
    #[error("decode error: {0}")]
    Decode(String),

    /// A query against the source failed (assembling or resolving a document).
    #[error("query error: {0}")]
    Query(String),

    /// The configuration uses a feature the active source implementation does
    /// not support yet.
    #[error("unsupported: {0}")]
    Unsupported(String),
}
