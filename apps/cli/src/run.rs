//! `flusso run` — load the configuration and stream Postgres changes through the
//! engine into the configured sink(s) until the live stream ends or an error
//! stops the pipeline.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, ensure};
use clap::Args;
use engine::Engine;
use schema::{Config, Sink as SinkConfig, SourceType};
use sinks_core::{FanOutSink, Sink};
use sinks_opensearch::OpensearchSink;
use sinks_stdout::StdoutSink;
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use url::Url;

use crate::DEFAULT_ARTIFACT;
use crate::telemetry;

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    /// Source config to compile and run. When omitted, the compiled artifact at
    /// `--artifact` is loaded instead.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Compiled artifact to run when `--config` is not given.
    #[arg(long, default_value = DEFAULT_ARTIFACT)]
    artifact: PathBuf,

    /// Logical replication slot to consume. Must already exist.
    #[arg(long, default_value = "flusso")]
    slot: String,

    /// Publication to subscribe to. Must already exist and cover the tables.
    #[arg(long, default_value = "flusso")]
    publication: String,

    /// Skip the initial backfill and resume live capture only. Use after the
    /// index has already been seeded, to avoid re-reading every existing row.
    #[arg(long)]
    skip_backfill: bool,

    /// Pretty-print documents instead of compact one-per-line JSON.
    #[arg(long)]
    pretty: bool,

    /// Maximum changes buffered between capture and processing.
    #[arg(long, default_value_t = 1024)]
    queue_capacity: usize,
}

pub(crate) async fn execute(args: RunArgs) -> anyhow::Result<()> {
    let tracer_provider = telemetry::init_tracing();

    let config = load_run_config(&args)?;
    ensure!(
        config.source.source_type == SourceType::Postgres,
        "only postgres sources are supported",
    );

    // Resolve the connection in *this* environment (applying DATABASE_URL).
    let connection_url = config
        .source
        .resolve_connection_url()
        .context("resolving the source connection URL")?;
    let connection_url = connection_url.as_ref().to_owned();
    let replication = replication_config(&connection_url, &args.slot, &args.publication)?;

    tracing::info!(
        slot = %args.slot,
        publication = %args.publication,
        indexes = config.indexes.len(),
        "starting sync",
    );

    let config = Arc::new(config);
    let source = Box::new(WalChangeCapture::new(replication, connection_url.clone()));
    let documents = Arc::new(
        PgDocumentBuilder::connect(&connection_url, Arc::clone(&config))
            .await
            .context("connecting to Postgres")?,
    );
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
    let sink: Arc<dyn Sink> = match sinks.len() {
        0 => Arc::new(StdoutSink::new(args.pretty)),
        1 => sinks
            .into_iter()
            .next()
            .unwrap_or_else(|| Arc::new(StdoutSink::new(false))),
        _ => Arc::new(FanOutSink::new(sinks)),
    };

    let result = Engine::new(source, documents, sink)
        .with_queue_capacity(args.queue_capacity)
        .skip_backfill(args.skip_backfill)
        .run()
        .await
        .context("sync engine stopped");

    // Flush any buffered spans to the collector before exiting, on success or
    // error alike — otherwise the last batch of traces is lost.
    if let Some(provider) = tracer_provider
        && let Err(error) = provider.shutdown()
    {
        tracing::warn!(%error, "failed to flush OTLP tracer on shutdown");
    }

    result
}

/// Load the configuration a `run` should use: compiled fresh from `--config`, or
/// read back from the compiled artifact.
fn load_run_config(args: &RunArgs) -> anyhow::Result<Config> {
    match &args.config {
        Some(path) => {
            schema::load(path).with_context(|| format!("loading config from {}", path.display()))
        }
        None => schema::load_compiled(&args.artifact)
            .with_context(|| format!("loading compiled config from {}", args.artifact.display())),
    }
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
