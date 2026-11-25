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

    /// The index's target is not the one staging named: it was deleted, or
    /// recreated by anyone else, so it does not hold what was written to it.
    #[error("the index's target changed: {0}")]
    TargetChanged(String),

    /// A seed found its new target built but couldn't retire the target it
    /// replaces: the destination blocks that one (issue #479). Nothing was
    /// swapped; the seed can be retried as is once the block is lifted or the
    /// old target is gone.
    #[error("{new} is built, but {old} can't be retired: {block}")]
    RetireBlocked {
        /// The target the seed was making the one readers see.
        new: String,
        /// The target it replaces, which the destination blocks.
        old: String,
        /// What the destination said of the block.
        block: String,
    },
}
