//! The composition root's [`Backends`]: assembles the Postgres source, the
//! configured sinks, and the source-side helpers from a [`Config`].
//!
//! This is where each port entry's options become a running adapter. The
//! adapter *kinds* are resolved in [`crate::adapters`]; this module builds the
//! instances, resolving connection URLs and credentials in the running
//! environment. The daemon and the engine never see an adapter's name.

use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use config::Config;
use daemon::{Backends, DaemonOptions, SourceParts};
use sink::{FanOutSink, Sink};
use sink_opensearch::OpensearchSink;
use sink_stdout::{StdoutConfig, StdoutSink};
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source::{CaptureProvisioning, SourceSpec};
use source_postgres::{
    PgDocumentBuilder, PostgresConfig, WalChangeCapture, replication_config, sql_connection_url,
};

use crate::adapters::{self, SinkConfig};

/// The one [`Backends`] implementation: the composition root's choice of
/// concrete adapters, looked up by each port entry's `type`.
#[derive(Debug, Default)]
pub(crate) struct FlussoBackends;

#[async_trait]
impl Backends for FlussoBackends {
    fn validate(&self, config: &Config) -> anyhow::Result<()> {
        adapters::validate(config)
    }

    async fn source(
        &self,
        config: Arc<Config>,
        _options: &DaemonOptions,
    ) -> anyhow::Result<SourceParts> {
        let postgres = adapters::source_config(&config)?;
        let connection_url = resolve_connection_url(&postgres)?;
        let tls = postgres.tls();
        let replication =
            replication_config(&connection_url, &tls, &postgres.slot, &postgres.publication)
                .context("building the replication connection config")?;
        let sql_url = sql_connection_url(&connection_url, &tls)
            .context("applying the source TLS settings to the connection URL")?;

        let capture: Arc<dyn ChangeCapture> = Arc::new(
            WalChangeCapture::new(replication, sql_url.clone()).with_publication_management(
                source_spec(&config).all_tables(),
                postgres.manage_publication,
            ),
        );
        let documents = build_documents(&sql_url, &config).await?;

        Ok(SourceParts { capture, documents })
    }

    async fn sink(
        &self,
        config: &Config,
        _options: &DaemonOptions,
    ) -> anyhow::Result<Arc<dyn Sink>> {
        build_sink(config)
    }
}

/// A source capture used only for its read-only provisioning surface
/// (`flusso check`): the slot name is irrelevant there.
pub(crate) fn build_provisioning(
    config: &Config,
    publication: &str,
) -> anyhow::Result<Arc<dyn CaptureProvisioning>> {
    let postgres = adapters::source_config(config)?;
    let connection_url = resolve_connection_url(&postgres)?;
    let tls = postgres.tls();
    let replication = replication_config(&connection_url, &tls, "flusso", publication)
        .context("building the replication connection config")?;
    let sql_url = sql_connection_url(&connection_url, &tls)
        .context("applying the source TLS settings to the connection URL")?;
    let capture = WalChangeCapture::new(replication, sql_url);
    Ok(Arc::new(capture))
}

/// The resolved SQL connection URL (TLS settings applied) for the source, as
/// `check` and the document builder need it.
pub(crate) fn source_sql_url(config: &Config) -> anyhow::Result<(PostgresConfig, String)> {
    let postgres = adapters::source_config(config)?;
    let connection_url = resolve_connection_url(&postgres)?;
    let sql_url = sql_connection_url(&connection_url, &postgres.tls())
        .context("applying the source TLS settings to the connection URL")?;
    Ok((postgres, sql_url))
}

fn resolve_connection_url(postgres: &PostgresConfig) -> anyhow::Result<String> {
    let url = postgres
        .resolve_connection_url()
        .context("resolving the source connection URL")?;
    Ok(url.as_ref().to_owned())
}

async fn build_documents(
    connection_url: &str,
    config: &Config,
) -> anyhow::Result<Arc<dyn DocumentBuilder>> {
    let spec = Arc::new(source_spec(config));
    let builder = PgDocumentBuilder::connect(connection_url, spec)
        .await
        .context("connecting to Postgres")?;
    Ok(Arc::new(builder))
}

/// The enabled indexes and their schemas: the source-facing subset of a
/// [`Config`].
pub(crate) fn source_spec(config: &Config) -> SourceSpec {
    let indexes = config
        .indexes
        .iter()
        .filter(|(_, index)| index.enabled)
        .map(|(name, index)| (name.clone(), index.schema.clone()))
        .collect();
    SourceSpec::new(indexes)
}

/// Every configured sink, fanned out; with no sink configured, a plain stdout
/// sink so a first run shows its documents.
fn build_sink(config: &Config) -> anyhow::Result<Arc<dyn Sink>> {
    let mut sinks: Vec<Arc<dyn Sink>> = Vec::new();
    for (name, entry) in &config.sinks {
        let built: Arc<dyn Sink> = match adapters::sink_config(name, entry)? {
            SinkConfig::Opensearch(os) => Arc::new(
                OpensearchSink::from_config(name, &os)
                    .with_context(|| format!("building OpenSearch sink '{name}'"))?
                    .with_index_prefix(&config.prefix),
            ),
            SinkConfig::Stdout(s) => Arc::new(StdoutSink::from_config(&s)),
        };
        sinks.push(built);
    }
    Ok(match sinks.len() {
        0 => Arc::new(StdoutSink::from_config(&StdoutConfig::default())),
        1 => sinks
            .into_iter()
            .next()
            .unwrap_or_else(|| Arc::new(StdoutSink::from_config(&StdoutConfig::default()))),
        _ => Arc::new(FanOutSink::new(sinks)),
    })
}
