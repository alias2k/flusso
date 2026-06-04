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
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use schema::{Sink as SinkConfig, SourceType};
use sinks_core::{FanOutSink, Sink};
use sinks_opensearch::OpensearchSink;
use sinks_stdout::StdoutSink;
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};
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
    let tracer_provider = init_tracing();

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
        1 => sinks
            .into_iter()
            .next()
            .unwrap_or_else(|| Arc::new(StdoutSink::new(false))),
        _ => Arc::new(FanOutSink::new(sinks)),
    };

    let result = Engine::new(source, documents, sink)
        .with_queue_capacity(cli.queue_capacity)
        .skip_backfill(cli.skip_backfill)
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

/// Initialize logging and tracing.
///
/// Always logs to stderr (stdout is reserved for the document stream), honoring
/// `RUST_LOG` (default `info`). Set `FLUSSO_LOG_FORMAT=json` for structured JSON
/// lines instead of the human-readable format.
///
/// When an OTLP endpoint is configured via the standard OpenTelemetry env vars
/// (`OTEL_EXPORTER_OTLP_ENDPOINT` or `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`),
/// spans are *also* exported to that collector over OTLP/HTTP. With no endpoint
/// configured — or if the exporter can't be built — it falls back to
/// stderr-only logging rather than failing startup.
///
/// Returns the tracer provider (if OTLP was enabled) so the caller can flush it
/// on shutdown.
fn init_tracing() -> Option<SdkTracerProvider> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let json = std::env::var("FLUSSO_LOG_FORMAT")
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    let fmt_layer: Box<dyn Layer<Registry> + Send + Sync> = if json {
        Box::new(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::io::stderr),
        )
    } else {
        Box::new(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
    };
    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![fmt_layer];

    // Add the OTLP layer only when a collector is configured. Capture any setup
    // error to log *after* the subscriber is installed (we have no logging yet).
    let mut otlp_error: Option<String> = None;
    let provider = match otlp_provider() {
        Ok(Some(provider)) => {
            let tracer = provider.tracer("flusso");
            layers.push(Box::new(tracing_opentelemetry::layer().with_tracer(tracer)));
            Some(provider)
        }
        Ok(None) => None,
        Err(error) => {
            otlp_error = Some(format!("{error:#}"));
            None
        }
    };

    Registry::default().with(layers).with(filter).init();

    if let Some(error) = otlp_error {
        tracing::warn!(error, "OTLP trace export disabled; logging to stderr only");
    } else if provider.is_some() {
        tracing::info!("OTLP trace export enabled");
    }
    provider
}

/// Build an OTLP tracer provider when an OTLP endpoint is configured via the
/// standard env vars; otherwise `Ok(None)`. The exporter reads its endpoint,
/// headers, and timeout from those same env vars and ships spans over
/// OTLP/HTTP (protobuf) on a background batch processor.
fn otlp_provider() -> anyhow::Result<Option<SdkTracerProvider>> {
    let configured = std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT").is_some()
        || std::env::var_os("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").is_some();
    if !configured {
        return Ok(None);
    }

    let exporter = SpanExporter::builder()
        .with_http()
        .build()
        .context("building OTLP span exporter")?;

    let resource = Resource::builder()
        .with_service_name(env!("CARGO_PKG_NAME"))
        .build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    Ok(Some(provider))
}
