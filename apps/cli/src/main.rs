//! `flusso` — run the Postgres → sink pipeline from a config file.
//!
//! Loads a `config.toml`, connects to the source's Postgres, and streams
//! changes through the engine to the configured sink(s). The replication slot
//! is created automatically if it does not exist. Logs go to stderr.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, ensure};
use clap::Parser;
use engine::Engine;
use schema::{Sink as SinkConfig, SourceType};
use sinks_core::{FanOutSink, Sink};
use sinks_opensearch::OpensearchSink;
use sinks_stdout::StdoutSink;
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use tracing_subscriber::EnvFilter;
use url::Url;

/// Stream Postgres changes into a sink, driven by a config file.
#[derive(Debug, Parser)]
#[command(name = "flusso", version, about)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing();

    let config = schema::load(&cli.config)
        .with_context(|| format!("loading config from {}", cli.config.display()))?;
    ensure!(
        config.source.source_type == SourceType::Postgres,
        "only postgres sources are supported",
    );

    let connection_url = config.source.connection_url.to_string();
    let replication = replication_config(&connection_url, &cli.slot, &cli.publication)?;

    tracing::info!(
        slot = %cli.slot,
        publication = %cli.publication,
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
    for (name, config) in &config.sinks {
        let sink: Arc<dyn Sink> = match config {
            SinkConfig::Opensearch(os) => Arc::new(
                OpensearchSink::from_config(os)
                    .with_context(|| format!("building OpenSearch sink '{name}'"))?,
            ),
            SinkConfig::Stdout(s) => Arc::new(StdoutSink::from_config(s)),
        };
        sinks.push(sink);
    }
    let sink: Arc<dyn Sink> = match sinks.len() {
        0 => Arc::new(StdoutSink::new(cli.pretty)),
        1 => sinks.into_iter().next().unwrap_or_else(|| Arc::new(StdoutSink::new(false))),
        _ => Arc::new(FanOutSink::new(sinks)),
    };

    Engine::new(source, documents, sink)
        .with_queue_capacity(cli.queue_capacity)
        .skip_backfill(cli.skip_backfill)
        .run()
        .await
        .context("sync engine stopped")?;

    Ok(())
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

/// Log to stderr (stdout is reserved for the document stream). Honors `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}
