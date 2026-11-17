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

    /// The sink rejected one or more documents at the item level and the
    /// failure policy is [`Stop`](crate::FailurePolicy::Stop). Switch to
    /// `skip` to quarantine such documents and keep the pipeline running.
    #[error(
        "sink rejected {0} document(s) (e.g. {1}); stopping. \
         set on-error=skip to quarantine rejected documents and continue"
    )]
    DocumentsRejected(usize, String),

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
    /// its own error; the indexes not listed were staged.
    #[error("{0}")]
    Staging(StagingFailures),
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
