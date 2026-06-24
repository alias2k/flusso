//! Logging and OpenTelemetry tracing setup for the `run` command — plus, as
//! submodules, the binary's other telemetry concerns: [`metrics`] (the meter
//! provider and instruments) and [`observer`] (the engine observer that records
//! into them).

pub(crate) mod metrics;
pub(crate) mod observer;

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

    // Capture any setup error to log *after* the subscriber is installed (we have
    // no logging yet).
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

/// Which OTLP transport an exporter uses, selected by the standard
/// `OTEL_EXPORTER_OTLP_PROTOCOL` env vars. The endpoint/port is the user's
/// responsibility per protocol (4317 for gRPC, 4318 for HTTP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OtlpProtocol {
    /// OTLP over HTTP with protobuf payloads — the default (conventionally `:4318`).
    HttpProtobuf,
    /// OTLP over gRPC (conventionally `:4317`).
    Grpc,
}

/// The OTLP signal whose protocol is being resolved — picks which per-signal env
/// var overrides the general one.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OtlpSignal {
    Traces,
    Metrics,
}

impl OtlpSignal {
    fn per_signal_var(self) -> &'static str {
        match self {
            OtlpSignal::Traces => "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
            OtlpSignal::Metrics => "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
        }
    }
}

/// Resolve the OTLP transport for `signal` from the standard env vars: the
/// per-signal `OTEL_EXPORTER_OTLP_{TRACES,METRICS}_PROTOCOL` wins over the
/// general `OTEL_EXPORTER_OTLP_PROTOCOL`; unset defaults to `http/protobuf`; an
/// unrecognized value warns and falls back to `http/protobuf`.
pub(crate) fn otlp_protocol(signal: OtlpSignal) -> OtlpProtocol {
    let per_signal = std::env::var(signal.per_signal_var()).ok();
    let general = std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL").ok();
    resolve_protocol(per_signal.as_deref(), general.as_deref())
}

/// Pure resolution: per-signal wins over general; unset → http/protobuf;
/// unrecognized warns and falls back. Split out so it's testable without
/// mutating process-wide env.
fn resolve_protocol(per_signal: Option<&str>, general: Option<&str>) -> OtlpProtocol {
    match per_signal.or(general).map(str::trim) {
        None | Some("http/protobuf") => OtlpProtocol::HttpProtobuf,
        Some("grpc") => OtlpProtocol::Grpc,
        Some(other) => {
            tracing::warn!(
                protocol = other,
                "unrecognized OTEL_EXPORTER_OTLP_PROTOCOL; falling back to http/protobuf"
            );
            OtlpProtocol::HttpProtobuf
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OtlpProtocol, resolve_protocol};

    #[test]
    fn unset_defaults_to_http_protobuf() {
        assert_eq!(resolve_protocol(None, None), OtlpProtocol::HttpProtobuf);
    }

    #[test]
    fn general_protocol_is_honored() {
        assert_eq!(resolve_protocol(None, Some("grpc")), OtlpProtocol::Grpc);
        assert_eq!(
            resolve_protocol(None, Some(" http/protobuf ")),
            OtlpProtocol::HttpProtobuf
        );
    }

    #[test]
    fn per_signal_overrides_general() {
        assert_eq!(
            resolve_protocol(Some("grpc"), Some("http/protobuf")),
            OtlpProtocol::Grpc
        );
        assert_eq!(
            resolve_protocol(Some("http/protobuf"), Some("grpc")),
            OtlpProtocol::HttpProtobuf
        );
    }

    #[test]
    fn unrecognized_falls_back_to_http_protobuf() {
        assert_eq!(
            resolve_protocol(Some("thrift"), None),
            OtlpProtocol::HttpProtobuf
        );
    }
}

/// Build an OTLP tracer provider when an OTLP endpoint is configured via the
/// standard env vars; otherwise `Ok(None)`. The exporter reads its endpoint,
/// headers, and timeout from those same env vars and ships spans on a background
/// batch processor over the transport selected by `OTEL_EXPORTER_OTLP_PROTOCOL`
/// (HTTP/protobuf by default, gRPC when set to `grpc`).
fn otlp_provider() -> anyhow::Result<Option<SdkTracerProvider>> {
    let configured = std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT").is_some()
        || std::env::var_os("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").is_some();
    if !configured {
        return Ok(None);
    }

    let builder = SpanExporter::builder();
    let exporter = match otlp_protocol(OtlpSignal::Traces) {
        OtlpProtocol::HttpProtobuf => builder.with_http().build(),
        OtlpProtocol::Grpc => builder.with_tonic().build(),
    }
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
