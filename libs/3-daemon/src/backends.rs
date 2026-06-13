//! The backend-assembler seam.
//!
//! The daemon owns the pipeline but not the concrete backends behind it. A
//! [`Backends`] implementation — supplied by the composition root (the CLI) —
//! turns a validated [`Config`] into the source capture, its document builder,
//! and the sink, all as trait objects. This keeps the daemon depending only on
//! the source/sink *abstractions* (`sources-core`/`sinks-core`), never on a
//! concrete Postgres/OpenSearch crate.

use std::sync::Arc;

use schema::Config;
use sinks_core::Sink;
use sources_core::cdc::ChangeCapture;
use sources_core::document::DocumentBuilder;

use crate::DaemonOptions;

/// A source capture paired with the document builder over the same source.
///
/// They are built together because both read from one source and share its
/// connection/config — the capture tails changes and reports lag; the builder
/// resolves changed rows to documents.
#[derive(Debug)]
pub struct SourceParts {
    /// Streams row changes (live tail + backfill snapshot) and reports lag.
    pub capture: Arc<dyn ChangeCapture>,
    /// Resolves changed rows to [`DocumentId`](sources_core::document)s and
    /// assembles the documents.
    pub documents: Arc<dyn DocumentBuilder>,
}

/// Builds the concrete pipeline backends from a validated [`Config`].
///
/// The daemon depends only on this trait; the composition root (the CLI) is the
/// single place that names concrete backends. Connection and credentials are
/// resolved by the implementation, in the environment that *runs* the pipeline —
/// so a compiled `flusso.lock` carries no secret it wasn't given literally.
#[async_trait::async_trait]
pub trait Backends: std::fmt::Debug + Send + Sync {
    /// Build the source capture and its document builder.
    async fn source(
        &self,
        config: Arc<Config>,
        options: &DaemonOptions,
    ) -> anyhow::Result<SourceParts>;

    /// Build the sink the engine writes to — a single configured sink, a
    /// fan-out over several, or a stdout fallback when none are configured.
    async fn sink(&self, config: &Config, options: &DaemonOptions)
    -> anyhow::Result<Arc<dyn Sink>>;
}
