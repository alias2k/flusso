//! `flusso` — keep a search index in sync with Postgres from a config file.
//!
//! Three subcommands:
//!
//! - `compile` reads a `config.toml`, parses and validates every schema it
//!   references, and writes the whole validated configuration to a single
//!   portable binary artifact (`flusso.bin`). No database is needed: the schema
//!   is self-describing, and secrets are kept as references, not baked in.
//! - `run` streams Postgres changes through the engine to the configured
//!   sink(s). With no `--config` it loads the compiled artifact; with `--config`
//!   it compiles the source afresh and runs that. Connection and credentials are
//!   resolved here, in the running environment. The replication slot is created
//!   automatically if it does not exist. Logs go to stderr.
//! - `check` validates the config and every schema, prints the fully-typed
//!   mapping (database-free), and — unless `--offline` — also confirms the
//!   declared types and nullability agree with the live database.

mod check;

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use engine::Engine;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use schema::{Config, Sink as SinkConfig, SourceType};
use sinks_core::{FanOutSink, Sink};
use sinks_opensearch::OpensearchSink;
use sinks_stdout::StdoutSink;
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};
use url::Url;

/// The default compiled-artifact path, written by `compile` and loaded by a
/// bare `run`.
const DEFAULT_ARTIFACT: &str = "flusso.bin";

/// Keep a search index in sync with Postgres, driven by a config file.
#[derive(Debug, Parser)]
#[command(name = "flusso", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile a config and its schemas into a single portable artifact.
    Compile(CompileArgs),
    /// Stream Postgres changes into the configured sink(s).
    Run(RunArgs),
    /// Validate the config and schemas without running the pipeline.
    Check(CheckArgs),
}

#[derive(Debug, Args)]
struct CompileArgs {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Where to write the compiled artifact.
    #[arg(short, long, default_value = DEFAULT_ARTIFACT)]
    out: PathBuf,
}

#[derive(Debug, Args)]
struct RunArgs {
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

#[derive(Debug, Args)]
struct CheckArgs {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Validate the files only; do not connect to the database. The mapping is
    /// shown either way; `--offline` skips confirming it against the database.
    #[arg(long)]
    offline: bool,

    /// Output format: a human-readable report, or JSON for piping.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    /// A colored, human-readable report (default).
    Human,
    /// A single JSON document: `{ "config": …, "mappings": … }`.
    Json,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Compile(args) => compile(args),
        Command::Run(args) => run(args).await,
        Command::Check(args) => check(args).await,
    }
}

/// Compile a config and its schemas into a single portable artifact. Needs no
/// database and no secret to be set.
fn compile(args: CompileArgs) -> anyhow::Result<()> {
    let compiled = schema::compile(&args.config)
        .with_context(|| format!("compiling config from {}", args.config.display()))?;
    schema::write(&compiled, &args.out)
        .with_context(|| format!("writing compiled artifact to {}", args.out.display()))?;

    let mut out = std::io::stdout().lock();
    let pen = check::Pen::detect();
    check::success(
        &mut out,
        pen,
        &format!(
            "compiled {} index(es) → {}",
            compiled.config.indexes.len(),
            args.out.display()
        ),
    )?;
    Ok(())
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

/// Load the config and stream changes through the engine until the live stream
/// ends or an error stops the pipeline.
async fn run(args: RunArgs) -> anyhow::Result<()> {
    let tracer_provider = init_tracing();

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

/// Validate the config and schemas. The mapping is now projected from the
/// self-describing schema with no database — so the fully-typed shape is shown
/// either way. Unless `--offline`, the command also connects and confirms the
/// declared types and nullability agree with the live database, reporting any
/// disagreement; an `Error`-level disagreement fails the check.
async fn check(args: CheckArgs) -> anyhow::Result<()> {
    // File-format validation: everything `schema::load` enforces (identifier
    // shapes, join/aggregate arity, declared types, filter value shapes).
    let config = Arc::new(
        schema::load(&args.config)
            .with_context(|| format!("loading config from {}", args.config.display()))?,
    );

    // The mapping is derived from the schema alone — no database needed.
    let mappings = config.resolve_mappings();

    // Source validation (skipped by `--offline`): connect and confirm the
    // declared schema matches the live database, collecting disagreements.
    let diagnostics = if args.offline {
        None
    } else {
        ensure!(
            config.source.source_type == SourceType::Postgres,
            "only postgres sources are supported",
        );
        let connection_url = config
            .source
            .resolve_connection_url()
            .context("resolving the source connection URL")?;
        let documents = PgDocumentBuilder::connect(connection_url.as_ref(), Arc::clone(&config))
            .await
            .context("connecting to the database")?;
        Some(
            sources_core::validate_indexes(&config, &documents)
                .await
                .context("validating schemas against the database")?,
        )
    };

    let has_errors = diagnostics.as_ref().is_some_and(|ds| {
        ds.iter()
            .any(|d| d.severity == sources_core::Severity::Error)
    });

    let mut out = std::io::stdout().lock();
    match args.format {
        OutputFormat::Json => {
            let doc = serde_json::json!({
                "config": &*config,
                "mappings": mappings,
                "diagnostics": diagnostics.as_ref().map(|ds| ds
                    .iter()
                    .map(|d| serde_json::json!({
                        "index": d.index.as_ref(),
                        "field": d.field.as_ref(),
                        "severity": format!("{:?}", d.severity).to_lowercase(),
                        "message": d.message,
                    }))
                    .collect::<Vec<_>>()),
            });
            writeln!(out, "{}", serde_json::to_string_pretty(&doc)?)?;
        }
        OutputFormat::Human => {
            let pen = check::Pen::detect();
            check::success(
                &mut out,
                pen,
                &format!("config valid: {}", args.config.display()),
            )?;
            check::config(&mut out, pen, &config)?;
            check::resolved(&mut out, pen, &mappings)?;
            match &diagnostics {
                None => {
                    check::warning(
                        &mut out,
                        pen,
                        "offline",
                        "skipped database validation — types and nullability not checked",
                    )?;
                }
                Some(diagnostics) => {
                    check::diagnostics(&mut out, pen, diagnostics)?;
                    writeln!(out)?;
                    if has_errors {
                        check::warning(&mut out, pen, "failed", "schema disagrees with database")?;
                    } else {
                        check::success(&mut out, pen, "check passed")?;
                    }
                }
            }
        }
    }

    ensure!(!has_errors, "schema validation failed against the database");
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
