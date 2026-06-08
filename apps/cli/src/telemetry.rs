//! Logging and OpenTelemetry tracing setup for the `run` command.

use anyhow::Context;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

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
pub(crate) fn init_tracing() -> Option<SdkTracerProvider> {
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
