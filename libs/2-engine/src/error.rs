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
    Source(#[from] sources_core::SourceError),

    /// From a sink write.
    #[error(transparent)]
    Sink(#[from] sinks_core::SinkError),

    /// From the work queue.
    #[error(transparent)]
    Queue(#[from] queue_core::QueueError),

    /// A spawned task failed to join (panicked).
    #[error("task failed: {0}")]
    Task(String),

    /// The sink rejected one or more documents at the item level and the
    /// failure policy is [`Stop`](crate::FailurePolicy::Stop). Switch to
    /// `skip` to quarantine such documents and keep the pipeline running.
    #[error(
        "sink rejected {0} document(s) (e.g. {1}); stopping. \
         set on-error=skip to quarantine rejected documents and continue"
    )]
    DocumentsRejected(usize, String),
}
