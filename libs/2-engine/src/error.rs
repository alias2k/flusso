use std::fmt;

use kernel::IndexName;
use thiserror::Error;

/// Result alias for the engine.
pub type Result<T> = std::result::Result<T, EngineError>;

/// A fatal error that stops the sync run. Because the engine confirms a
/// change's source ack only after the change is durably applied, stopping on an
/// error is safe: unconfirmed changes are redelivered when the run restarts.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EngineError {
    /// From the source — capturing changes or resolving/assembling documents.
    #[error(transparent)]
    Source(source::SourceError),

    /// From a sink write.
    #[error(transparent)]
    Sink(#[from] sink::SinkError),

    /// From the stream: a lane or the request lane.
    #[error(transparent)]
    Stream(#[from] stream::QueueError),

    /// A document could not be built (a script failed on it, or an `array:
    /// true` column held an object) and the failure
    /// policy for its index is [`Stop`](crate::FailurePolicy::Stop).
    #[error(
        "document `{id}` in index `{index}` could not be built: {failure}\n\
         stopping; the change is retried after a restart. \
         set on_error = \"skip\" for this index to leave such documents at their indexed version"
    )]
    DocumentFailed {
        index: String,
        id: String,
        failure: Box<source::document::BuildFailure>,
    },

    /// Staging failed for one or more of a sink's indexes. Each is named with
    /// its own error; the indexes not listed were staged or never started.
    #[error("{0}")]
    Staging(StagingFailures),

    /// A lane of the stream has no place in the engine's [`Progress`](crate::Progress):
    /// what is routed to it could never hold confirmation back, so the engine
    /// refuses to run. Build the stream and the progress from the same lanes.
    #[error(
        "the stream's lane `{0}` is not in the engine's progress; build the stream and the \
         progress from the same lanes"
    )]
    LaneNotInProgress(String),
}

/// The indexes a sink failed to stage, in the sink's index order, each with
/// its error.
#[derive(Debug)]
pub struct StagingFailures(pub(crate) Vec<(IndexName, sink::SinkError)>);

impl StagingFailures {
    /// Each failed index with its error.
    pub fn iter(&self) -> impl Iterator<Item = &(IndexName, sink::SinkError)> {
        self.0.iter()
    }
}

impl fmt::Display for StagingFailures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "staging failed for")?;
        for (n, (index, error)) in self.0.iter().enumerate() {
            let separator = if n == 0 { "" } else { ";" };
            write!(f, "{separator} index `{index}`: {error}")?;
        }
        Ok(())
    }
}
