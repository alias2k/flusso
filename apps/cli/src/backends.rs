//! The backend assembler — the composition root's wiring of concrete backends.
//!
//! [`FlussoBackends`] is the one place in the codebase that names Postgres and
//! the concrete sinks: it turns a validated [`Config`] into the source capture,
//! its document builder, and the sink the daemon drives. Connection and
//! credentials are resolved here, in the running environment (a compiled
//! `flusso.lock` carries no secrets it wasn't given literally).
//!
//! Adding a backend means a new match arm here (plus its crate); the daemon and
//! engine are untouched.

use std::sync::Arc;

use anyhow::{Context, ensure};
use async_trait::async_trait;
use daemon::{Backends, DaemonOptions, SourceParts};
use schema::{Config, Sink as SinkConfig, SourceType};
use sinks_core::{FanOutSink, Sink};
use sinks_opensearch::OpensearchSink;
use sinks_stdout::StdoutSink;
use sources_core::cdc::ChangeCapture;
use sources_core::document::DocumentBuilder;
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use url::Url;

/// The composition root's backend assembler: a Postgres source plus the
/// configured sinks.
#[derive(Debug, Default)]
pub(crate) struct FlussoBackends;

#[async_trait]
impl Backends for FlussoBackends {
    async fn source(
        &self,
        config: Arc<Config>,
        options: &DaemonOptions,
    ) -> anyhow::Result<SourceParts> {
        ensure!(
            config.source.source_type == SourceType::Postgres,
            "only postgres sources are supported",
        );

        let connection_url = resolve_connection_url(&config)?;
        let replication =
            replication_config(&connection_url, &options.slot, &options.publication)?;

        let capture: Arc<dyn ChangeCapture> =
            Arc::new(WalChangeCapture::new(replication, connection_url.clone()));
        let documents = build_documents(&connection_url, Arc::clone(&config)).await?;

        Ok(SourceParts { capture, documents })
    }

    async fn sink(
        &self,
        config: &Config,
        options: &DaemonOptions,
    ) -> anyhow::Result<Arc<dyn Sink>> {
        build_sink(config, options.pretty)
    }
}

/// Resolve the source connection URL in this environment (applying
/// `DATABASE_URL`).
fn resolve_connection_url(config: &Config) -> anyhow::Result<String> {
    let url = config
        .source
        .resolve_connection_url()
        .context("resolving the source connection URL")?;
    Ok(url.as_ref().to_owned())
}

/// Build the replication client config from the source connection URL plus the
/// slot and publication names.
fn replication_config(
    connection_url: &str,
    slot: &str,
    publication: &str,
) -> anyhow::Result<ReplicationConfig> {
    let url = Url::parse(connection_url).context("parsing connection URL")?;
    let host = url
        .host_str()
        .context("connection URL has no host")?
        .to_owned();
    let port = url.port().unwrap_or(5432);
    let user = url.username();
    ensure!(!user.is_empty(), "connection URL has no user");
    let password = url.password().unwrap_or_default();
    let database = url.path().trim_start_matches('/');
    // Postgres defaults the database to the user when the URL omits it.
    let database = if database.is_empty() { user } else { database };

    Ok(ReplicationConfig::new(host, user, password, database, slot, publication).with_port(port))
}

/// Connect the Postgres document builder.
async fn build_documents(
    connection_url: &str,
    config: Arc<Config>,
) -> anyhow::Result<Arc<dyn DocumentBuilder>> {
    let builder = PgDocumentBuilder::connect(connection_url, config)
        .await
        .context("connecting to Postgres")?;
    Ok(Arc::new(builder))
}

/// Build the sink from config: a single configured sink directly, several as a
/// [`FanOutSink`], or stdout when none are configured. `pretty` only affects the
/// no-sink-configured stdout fallback.
fn build_sink(config: &Config, pretty: bool) -> anyhow::Result<Arc<dyn Sink>> {
    let mut sinks: Vec<Arc<dyn Sink>> = Vec::new();
    for (name, sink_config) in &config.sinks {
        let sink: Arc<dyn Sink> = match sink_config {
            SinkConfig::Opensearch(os) => Arc::new(
                OpensearchSink::from_config(name, os)
                    .with_context(|| format!("building OpenSearch sink '{name}'"))?,
            ),
            SinkConfig::Stdout(s) => Arc::new(StdoutSink::from_config(s)),
        };
        sinks.push(sink);
    }
    Ok(match sinks.len() {
        0 => Arc::new(StdoutSink::new(pretty)),
        1 => sinks
            .into_iter()
            .next()
            .unwrap_or_else(|| Arc::new(StdoutSink::new(pretty))),
        _ => Arc::new(FanOutSink::new(sinks)),
    })
}
