//! Metrics wiring: one OpenTelemetry [`SdkMeterProvider`] feeding up to two
//! readers from the *same* instruments —
//!
//! - a **Prometheus** reader, scraped at `/metrics` (this is what the dev-stack
//!   Grafana reads), enabled whenever the HTTP surface is served;
//! - an **OTLP** periodic push reader, enabled when an OTLP endpoint is
//!   configured via the standard `OTEL_EXPORTER_OTLP_*` env vars — the same
//!   convention the CLI's trace export already uses.
//!
//! The provider is installed as the **global** meter provider *before* the
//! daemon is started, so the daemon's observer builds its instruments from
//! `global::meter` and records into whichever readers are configured (and
//! harmlessly into none, as a no-op, when metrics are off).

use std::time::Duration;

use anyhow::Context;
use opentelemetry::global;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::{Aggregation, PeriodicReader, SdkMeterProvider, Stream};
use prometheus::Registry;

/// How often the OTLP reader pushes, when enabled.
const OTLP_PUSH_INTERVAL: Duration = Duration::from_secs(10);

/// Histogram buckets for `flusso.flush.duration`, in **seconds** — OTel's
/// defaults assume milliseconds, which would pile every sub-second flush into
/// one bucket and make a p95 meaningless. Spans ~1ms to 10s.
const FLUSH_BUCKETS_SECONDS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Owns the installed meter provider (for shutdown) and, when Prometheus is on,
/// the registry the `/metrics` endpoint renders.
pub(crate) struct Metrics {
    provider: SdkMeterProvider,
    pub registry: Option<Registry>,
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metrics")
            .field("prometheus", &self.registry.is_some())
            .finish_non_exhaustive()
    }
}

impl Metrics {
    /// Flush and stop the meter provider — pushes the final OTLP batch so the
    /// last metrics aren't lost on shutdown.
    pub(crate) fn shutdown(&self) {
        if let Err(error) = self.provider.shutdown() {
            tracing::warn!(%error, "failed to shut down meter provider");
        }
    }
}

/// Build and install the global meter provider. `prometheus` adds the scrape
/// reader (and returns its registry); an OTLP reader is added when the env
/// configures an endpoint. With neither, the provider has no readers and every
/// instrument is a cheap no-op.
pub(crate) fn init(prometheus: bool) -> anyhow::Result<Metrics> {
    // Override the flush histogram's buckets with seconds-appropriate ones; the
    // view matches by instrument name and leaves every other instrument on its
    // default aggregation.
    let flush_view = |instrument: &opentelemetry_sdk::metrics::Instrument| {
        (instrument.name() == "flusso.flush.duration")
            .then(|| {
                Stream::builder()
                    .with_aggregation(Aggregation::ExplicitBucketHistogram {
                        boundaries: FLUSH_BUCKETS_SECONDS.to_vec(),
                        record_min_max: true,
                    })
                    .build()
                    .ok()
            })
            .flatten()
    };

    let mut builder = SdkMeterProvider::builder()
        .with_resource(resource())
        .with_view(flush_view);

    let registry = if prometheus {
        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            // Drop the per-series `otel_scope_name`/`otel_scope_version` labels —
            // there's a single scope ("flusso"), so they only add noise.
            .without_scope_info()
            .build()
            .context("building the Prometheus metrics exporter")?;
        builder = builder.with_reader(exporter);
        Some(registry)
    } else {
        None
    };

    if otlp_configured() {
        match otlp_reader() {
            Ok(reader) => {
                builder = builder.with_reader(reader);
                tracing::info!("OTLP metric export enabled");
            }
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "OTLP metric export disabled");
            }
        }
    }

    let provider = builder.build();
    global::set_meter_provider(provider.clone());
    Ok(Metrics { provider, registry })
}

/// Whether an OTLP endpoint is configured (general or metrics-specific).
pub(crate) fn otlp_configured() -> bool {
    std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT").is_some()
        || std::env::var_os("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT").is_some()
}

/// A periodic OTLP push reader over OTLP/HTTP, reading its endpoint/headers from
/// the standard env vars (as the trace exporter does).
fn otlp_reader() -> anyhow::Result<PeriodicReader<opentelemetry_otlp::MetricExporter>> {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .build()
        .context("building the OTLP metric exporter")?;
    Ok(PeriodicReader::builder(exporter)
        .with_interval(OTLP_PUSH_INTERVAL)
        .build())
}

fn resource() -> Resource {
    Resource::builder().with_service_name("flusso").build()
}
