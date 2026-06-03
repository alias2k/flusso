//! `storno` — run the Postgres → sink pipeline from a config file.
//!
//! Loads a `config.toml`, connects to the source's Postgres, and streams
//! changes through the engine to stdout. Logs go to stderr, so stdout stays
//! clean NDJSON you can pipe into `jq`.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, ensure};
use clap::Parser;
use engine::Engine;
use schema::SourceType;
use sinks_stdout::StdoutSink;
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use tracing_subscriber::EnvFilter;
use url::Url;

/// Stream Postgres changes into a sink, driven by a config file.
#[derive(Debug, Parser)]
#[command(name = "storno", version, about)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Logical replication slot to consume. Must already exist.
    #[arg(long, default_value = "storno")]
    slot: String,

    /// Publication to subscribe to. Must already exist and cover the tables.
    #[arg(long, default_value = "storno")]
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
    let source = Box::new(
        WalChangeCapture::new(replication, connection_url.clone()).with_backfill(!cli.skip_backfill),
    );
    let documents = Arc::new(
        PgDocumentBuilder::connect(&connection_url, Arc::clone(&config))
            .await
            .context("connecting to Postgres")?,
    );
    let sink = Arc::new(StdoutSink::new(cli.pretty));

    Engine::new(source, documents, sink)
        .with_queue_capacity(cli.queue_capacity)
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
    let host = url.host_str().context("connection URL has no host")?.to_owned();
    let port = url.port().unwrap_or(5432);
    let user = url.username();
    ensure!(!user.is_empty(), "connection URL has no user");
    let password = url.password().unwrap_or_default().to_owned();
    let database = url.path().trim_start_matches('/');
    // Postgres defaults the database to the user when the URL omits it.
    let database = if database.is_empty() { user } else { database };

    Ok(ReplicationConfig::new(
        host,
        user.to_owned(),
        password,
        database.to_owned(),
        slot.to_owned(),
        publication.to_owned(),
    )
    .with_port(port))
}

/// Log to stderr (stdout is reserved for the document stream). Honors `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
}
