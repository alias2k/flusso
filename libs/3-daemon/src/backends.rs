//! The backend-assembler seam.
//!
//! The daemon owns the pipeline but not the concrete adapters behind it. A
//! [`Backends`] implementation — supplied by the composition root (the CLI) —
//! turns a [`Config`] into the source capture, its document builder, and the
//! sink, all as trait objects. This keeps the daemon depending only on the
//! ports (`source`/`sink`), never on a concrete Postgres/OpenSearch crate, and
//! never on an adapter's name: each port entry's `type` is looked up by the
//! implementation, which also owns [`validate`](Backends::validate), the step
//! that turns every entry's options into its adapter's typed config before
//! anything connects.

use std::sync::Arc;

use config::Config;
use sink::Sink;
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;

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
    /// Resolves changed rows to [`DocumentId`](source::document)s and
    /// assembles the documents.
    pub documents: Arc<dyn DocumentBuilder>,
}

/// Builds the concrete pipeline adapters from a [`Config`].
///
/// The daemon depends only on this trait; the composition root (the CLI) is the
/// single place that names concrete adapters. Connection and credentials are
/// resolved by the implementation, in the environment that *runs* the pipeline —
/// so a compiled `flusso.lock` carries no secret it wasn't given literally.
#[async_trait::async_trait]
pub trait Backends: std::fmt::Debug + Send + Sync {
    /// Instantiate every port entry's adapter config without connecting to
    /// anything: an unknown `type`, an unknown option, or a wrong value fails
    /// here, before any network call or lock write. `flusso build`, `check`
    /// (offline included), and `run` all call this right after loading.
    fn validate(&self, config: &Config) -> anyhow::Result<()>;

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
