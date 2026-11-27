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

    /// A live lane item's index had no pending build to register it under:
    /// published, it would land without the progress knowing, and the lane
    /// would answer past it. Every live document comes from a take, so this
    /// is a bug in the engine, never in the data.
    #[error(
        "a live item for lane `{0}` has no pending build to register under; refusing to publish \
         it unregistered"
    )]
    UnregisteredLiveItem(String),

    /// Every builder returned with documents read and not built, named per
    /// index: a run never ends `Ok` with a read change unbuilt (ADR 0012
    /// stage 8a, §3.9 §8). The restart replays them.
    #[error("the run ended with documents read and not built ({0}); the restart reads them again")]
    DocumentsLeft(String),

    /// A live lane item (it carries a position) arrived with no
    /// [`ItemKey`](stream::ItemKey): it could be neither marked landed nor
    /// fenced, so the sink engine refuses it. Every live item the ingest engine
    /// publishes carries its key.
    #[error(
        "a live item on lane `{0}` carries no item key; every live item must be numbered by \
         the engine's progress"
    )]
    UnkeyedLiveItem(String),

    /// Another process changed an index's detach records while this one
    /// didn't hold the slot (ADR 0012 stage 10, the cut of 2026-11-27):
    /// flusso doesn't reconcile with it, the deployment stops. Nothing past
    /// the opening under which it was read was confirmed, so the next start,
    /// which decides from the records, replays every change from there.
    #[error(
        "sink `{sink}`, index `{index}`: another flusso process changed the index's detach \
         records while this one didn't hold the replication slot ({found}). Stopping: flusso \
         doesn't carry on over another process's record changes. Nothing past this stream's \
         start was confirmed; start flusso again (one process per slot) and its start decides \
         from the records"
    )]
    ForeignChange {
        sink: String,
        index: String,
        found: String,
    },
}

impl EngineError {
    /// Whether this stops the deployment rather than one engine: another
    /// process changed the records ([`EngineError::ForeignChange`]), which
    /// no restart of an engine in this process may carry on over.
    pub fn stops_the_deployment(&self) -> bool {
        matches!(self, EngineError::ForeignChange { .. })
    }
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
