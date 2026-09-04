//! The adapter-assembler seam.
//!
//! The daemon owns the deployment but not the concrete adapters behind it. A
//! [`Backends`] implementation — supplied by the composition root (the CLI) —
//! turns a [`Config`] into the source capture and its document builder, the
//! stream, and each sink, all as trait objects. This keeps the daemon depending
//! only on the ports (`source`/`stream`/`sink`), never on a Postgres, channel,
//! or OpenSearch crate, and never on an adapter's name: each port entry's
//! `type` is looked up by the implementation, which also owns
//! [`validate`](Backends::validate), the step that turns every entry's options
//! into its adapter's typed config before anything connects.

use std::sync::Arc;

use config::Config;
use kernel::SinkName;
use sink::{Sink, SinkOptions};
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use stream::Stream;

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

/// One configured sink: its name, the adapter instance, and the universal
/// sink keys.
#[derive(Debug)]
pub struct SinkParts {
    /// The sink's name in the config: its lane, its status entry, its metric label.
    pub name: SinkName,
    /// The adapter.
    pub sink: Arc<dyn Sink>,
    /// The universal sink keys (`backfill`).
    pub options: SinkOptions,
}

/// Builds the concrete adapters from a [`Config`].
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

    /// The stream with one lane per sink in `sinks`.
    fn stream(&self, config: &Config, sinks: &[SinkName]) -> anyhow::Result<Arc<dyn Stream>>;

    /// Every configured sink, in name order. An empty config yields the
    /// composition root's default sink.
    async fn sinks(&self, config: &Config) -> anyhow::Result<Vec<SinkParts>>;
}
