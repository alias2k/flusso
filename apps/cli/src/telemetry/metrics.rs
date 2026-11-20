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

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use daemon::Status;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{ObservableCounter, ObservableGauge};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::{
    Aggregation, MeterProviderBuilder, PeriodicReader, SdkMeterProvider, Stream,
};
use prometheus::Registry;

use crate::telemetry::{OtlpProtocol, OtlpSignal, otlp_protocol};

const OTLP_PUSH_INTERVAL: Duration = Duration::from_secs(10);

/// Histogram buckets for `flusso.flush.duration`, in **seconds** — OTel's
/// defaults assume milliseconds, which would pile every sub-second flush into
/// one bucket and make a p95 meaningless. Spans ~1ms to 10s.
const FLUSH_BUCKETS_SECONDS: &[f64] = &[
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Buckets for `flusso.stage.duration`, in seconds: a resolve or an apply is
/// tens of microseconds, a wait for an item can be minutes. Spans 100 µs to
/// 30 s; the `_sum` is exact whatever the buckets.
const STAGE_BUCKETS_SECONDS: &[f64] = &[
    0.0001, 0.00025, 0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
    5.0, 10.0, 30.0,
];

/// Buckets for `flusso.flush.documents`: a trickle flushes one, a backfill
/// batch up to `max_changes` (default 256), a merged flush more.
const FLUSH_DOCUMENT_BUCKETS: &[f64] = &[
    1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0,
];

/// Buckets for `flusso.flush.size`, in bytes: 1 KiB to 64 MiB, by fours.
const FLUSH_SIZE_BUCKETS_BYTES: &[f64] = &[
    1024.0,
    4096.0,
    16384.0,
    65536.0,
    262_144.0,
    1_048_576.0,
    4_194_304.0,
    16_777_216.0,
    67_108_864.0,
];

/// The explicit buckets of each histogram flusso records, by instrument name.
fn buckets(instrument: &str) -> Option<&'static [f64]> {
    match instrument {
        "flusso.flush.duration" => Some(FLUSH_BUCKETS_SECONDS),
        "flusso.stage.duration" | "flusso.memory.wait" => Some(STAGE_BUCKETS_SECONDS),
        "flusso.flush.documents" => Some(FLUSH_DOCUMENT_BUCKETS),
        "flusso.flush.size" => Some(FLUSH_SIZE_BUCKETS_BYTES),
        _ => None,
    }
}

/// A meter provider builder with flusso's histogram buckets
/// ([`buckets`]) applied.
pub(crate) fn provider_builder() -> MeterProviderBuilder {
    SdkMeterProvider::builder()
        .with_resource(resource())
        .with_view(|instrument: &opentelemetry_sdk::metrics::Instrument| {
            let boundaries = buckets(instrument.name())?;
            Stream::builder()
                .with_aggregation(Aggregation::ExplicitBucketHistogram {
                    boundaries: boundaries.to_vec(),
                    record_min_max: true,
                })
                .build()
                .ok()
        })
}

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
    let mut builder = provider_builder();

    let registry = if prometheus {
        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            // Drop the per-series `otel_scope_name`/`otel_scope_version` labels —
            // there's a single scope ("flusso"), so they only add noise.
            .scope_info_enabled(false)
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

/// Register `flusso.changes.in_flight` as an observable gauge that reads the
/// daemon's [`Status`] at collection time — so it stays current even while the
/// sink is stalled (no commits), which is when a growing backlog matters most.
/// Returns the instrument handle, which the caller keeps alive to keep the
/// callback registered.
pub(crate) fn register_in_flight_gauge(status: Arc<Status>) -> ObservableGauge<u64> {
    global::meter("flusso")
        .u64_observable_gauge("flusso.changes.in_flight")
        .with_description("Captured but not yet committed changes per sink (back-pressure)")
        .with_callback(move |observer| {
            for sink in status.sinks() {
                observer.observe(
                    status.in_flight_for(sink),
                    &[KeyValue::new("sink", sink.as_ref().to_owned())],
                );
            }
        })
        .build()
}

/// Register `flusso.lane.parked` as an observable gauge read from the daemon's
/// [`Status`] at collection: 1 per `(index, sink)` lane flusso doesn't write,
/// with `why` `refused` (a refusal under `on_error = "stop"`), `blocked` (the
/// index refuses writes for now) or `restaging` (its target was gone). An
/// open lane has no series.
pub(crate) fn register_lane_parked_gauge(status: Arc<Status>) -> ObservableGauge<u64> {
    lane_parked_gauge(&global::meter("flusso"), status)
}

fn lane_parked_gauge(
    meter: &opentelemetry::metrics::Meter,
    status: Arc<Status>,
) -> ObservableGauge<u64> {
    meter
        .u64_observable_gauge("flusso.lane.parked")
        .with_description(
            "1 while an (index, sink) lane is not written: refused, blocked or restaging",
        )
        .with_callback(move |observer| {
            for (lane, view) in status.unhealthy_lanes() {
                observer.observe(
                    1,
                    &[
                        KeyValue::new("index", lane.index.as_ref().to_owned()),
                        KeyValue::new("sink", lane.sink.as_ref().to_owned()),
                        KeyValue::new("why", view.why()),
                    ],
                );
            }
        })
        .build()
}

/// Register `flusso.lane.outstanding` as an observable gauge read from the
/// daemon's [`Status`] at collection: per `(index, sink)` lane, the items
/// routed to it and not landed. A lane that stays high is the one holding
/// back confirmation.
pub(crate) fn register_lane_outstanding_gauge(status: Arc<Status>) -> ObservableGauge<u64> {
    global::meter("flusso")
        .u64_observable_gauge("flusso.lane.outstanding")
        .with_description("Items routed to an (index, sink) lane and not landed yet")
        .with_callback(move |observer| {
            for (lane, outstanding) in status.lanes_outstanding() {
                observer.observe(
                    u64::try_from(outstanding).unwrap_or(u64::MAX),
                    &[
                        KeyValue::new("index", lane.index.as_ref().to_owned()),
                        KeyValue::new("sink", lane.sink.as_ref().to_owned()),
                    ],
                );
            }
        })
        .build()
}

/// Stalled lanes, read from the daemon's [`Status`] at collection, per
/// `(index, sink)` lane: `flusso.lane.stall_age` (seconds the lane's oldest
/// outstanding item has stayed its oldest), `flusso.lane.wal_behind` (bytes
/// of WAL the slot holds because of it, while it has something outstanding)
/// and `flusso.slot.pinned_by` (1 on the lane holding the slot back most, or
/// on the index alone, with no `sink`, while its pending documents do).
pub(crate) fn register_stall_gauges(status: Arc<Status>) -> StallGauges {
    stall_gauges(&global::meter("flusso"), status)
}

/// The three gauges [`register_stall_gauges`] keeps registered.
pub(crate) type StallGauges = (
    ObservableGauge<f64>,
    ObservableGauge<u64>,
    ObservableGauge<u64>,
);

fn stall_gauges(meter: &opentelemetry::metrics::Meter, status: Arc<Status>) -> StallGauges {
    let lane = |key: &stream::LaneKey| {
        [
            KeyValue::new("index", key.index.as_ref().to_owned()),
            KeyValue::new("sink", key.sink.as_ref().to_owned()),
        ]
    };
    let ages = Arc::clone(&status);
    let stall_age = meter
        .f64_observable_gauge("flusso.lane.stall_age")
        .with_unit("s")
        .with_description(
            "Seconds an (index, sink) lane's oldest outstanding item has stayed its oldest: the \
             lane has landed nothing for that long; 0 while nothing is outstanding",
        )
        .with_callback(move |observer| {
            for (key, stall) in ages.stalls().0 {
                observer.observe(stall.stalled_for.as_secs_f64(), &lane(&key));
            }
        })
        .build();
    let behind = Arc::clone(&status);
    let wal_behind = meter
        .u64_observable_gauge("flusso.lane.wal_behind")
        .with_unit("By")
        .with_description(
            "Bytes of WAL the replication slot holds because of an (index, sink) lane, while \
             it has something outstanding",
        )
        .with_callback(move |observer| {
            for (key, stall) in behind.stalls().0 {
                if let Some(bytes) = stall.wal_behind {
                    observer.observe(bytes, &lane(&key));
                }
            }
        })
        .build();
    let pinned_by = meter
        .u64_observable_gauge("flusso.slot.pinned_by")
        .with_description(
            "1 on the (index, sink) lane holding the replication slot back the most, or on the \
             index alone while its documents pending and on no lane yet do; absent while \
             nothing is outstanding",
        )
        .with_callback(move |observer| match status.stalls().1 {
            Some(daemon::Pin::Lane(key)) => observer.observe(1, &lane(&key)),
            Some(daemon::Pin::Backlog(index)) => {
                observer.observe(1, &[KeyValue::new("index", index.as_ref().to_owned())])
            }
            None => {}
        })
        .build();
    (stall_age, wal_behind, pinned_by)
}

/// Register `flusso.replication.visibility_wait` as an observable gauge that reads
/// the daemon's [`Status`] at collection time: seconds the source has held a
/// committed change it can't read back yet, `0` while nothing waits.
pub(crate) fn register_visibility_wait_gauge(status: Arc<Status>) -> ObservableGauge<f64> {
    global::meter("flusso")
        .f64_observable_gauge("flusso.replication.visibility_wait")
        .with_unit("s")
        .with_description(
            "Seconds the source has held a committed transaction that is not visible yet \
             (every later change waits behind it); 0 while nothing waits",
        )
        .with_callback(move |observer| {
            observer.observe(
                status
                    .visibility_wait()
                    .map_or(0.0, |waited| waited.as_secs_f64()),
                &[],
            );
        })
        .build()
}

/// The memory budget's holding, read at collection: `flusso.memory.built`
/// (bytes, by `work`) and `flusso.memory.refused` (batches that waited for
/// the budget, by `work`).
pub(crate) fn register_memory_instruments(
    budget: Arc<daemon::Budget>,
) -> (ObservableGauge<u64>, ObservableCounter<u64>) {
    memory_instruments(&global::meter("flusso"), budget)
}

fn memory_instruments(
    meter: &opentelemetry::metrics::Meter,
    budget: Arc<daemon::Budget>,
) -> (ObservableGauge<u64>, ObservableCounter<u64>) {
    let held = Arc::clone(&budget);
    let built = meter
        .u64_observable_gauge("flusso.memory.built")
        .with_unit("By")
        .with_description(
            "Bytes of built documents not yet acknowledged by every sink, reservations of \
             builds in flight included, by work (live, backfill)",
        )
        .with_callback(move |observer| {
            let now = held.held();
            observer.observe(
                (now.total - now.backfill.min(now.total)) as u64,
                &[KeyValue::new("work", "live")],
            );
            observer.observe(now.backfill as u64, &[KeyValue::new("work", "backfill")]);
        })
        .build();
    let refused = meter
        .u64_observable_counter("flusso.memory.refused")
        .with_description(
            "Times a batch waited for the memory budget before it could be built, by work",
        )
        .with_callback(move |observer| {
            for work in [daemon::Work::Live, daemon::Work::Backfill] {
                observer.observe(
                    budget.refused(work),
                    &[KeyValue::new("work", work.as_str())],
                );
            }
        })
        .build();
    (built, refused)
}

/// Whether an OTLP endpoint is configured (general or metrics-specific).
pub(crate) fn otlp_configured() -> bool {
    std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT").is_some()
        || std::env::var_os("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT").is_some()
}

/// A periodic OTLP push reader, reading its endpoint/headers from the standard
/// env vars (as the trace exporter does). The transport follows
/// `OTEL_EXPORTER_OTLP_PROTOCOL` (HTTP/protobuf by default, gRPC when `grpc`).
fn otlp_reader() -> anyhow::Result<PeriodicReader<opentelemetry_otlp::MetricExporter>> {
    let builder = opentelemetry_otlp::MetricExporter::builder();
    let exporter = match otlp_protocol(OtlpSignal::Metrics) {
        OtlpProtocol::HttpProtobuf => builder.with_http().build(),
        OtlpProtocol::Grpc => builder.with_tonic().build(),
    }
    .context("building the OTLP metric exporter")?;
    Ok(PeriodicReader::builder(exporter)
        .with_interval(OTLP_PUSH_INTERVAL)
        .build())
}

fn resource() -> Resource {
    Resource::builder().with_service_name("flusso").build()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use daemon::{Progress, Stalls};
    use kernel::{IndexName, Position, SinkName};
    use opentelemetry::metrics::MeterProvider;
    use prometheus::TextEncoder;
    use source::cdc::{ChangeCapture, Continuity, Landed, LiveChange};
    use stream::LaneKey;

    use super::*;

    #[derive(Debug)]
    struct NoPositions;

    #[async_trait::async_trait]
    impl ChangeCapture for NoPositions {
        async fn continuity(&self) -> source::Result<Continuity> {
            Ok(Continuity::Resumed)
        }
        async fn prepare(&self) -> source::Result<()> {
            Ok(())
        }
        async fn live(
            &self,
        ) -> source::Result<futures::stream::BoxStream<'static, source::Result<LiveChange>>>
        {
            Ok(Box::pin(futures::stream::empty()))
        }
        fn confirm(&self, _: Landed) {}
    }

    #[tokio::test]
    async fn a_lane_not_written_is_exported_with_why() {
        let lane = |index: &str| {
            LaneKey::new(
                IndexName::try_new(index).unwrap(),
                SinkName::try_new("primary").unwrap(),
            )
        };
        let status = Arc::new(Status::new(
            [
                lane("orders").index,
                lane("users").index,
                lane("items").index,
                lane("carts").index,
            ],
            [SinkName::try_new("primary").unwrap()],
            std::time::Instant::now(),
        ));
        let observer = daemon::StatusObserver::new(Arc::clone(&status));
        let now = tokio::time::Instant::now();
        use daemon::Observer as _;
        observer.on_lane_health(
            &lane("orders"),
            &daemon::LaneHealth::Parked {
                reason: daemon::ParkReason::Blocked {
                    reason: "index write (api)".to_owned(),
                },
                since: now,
                next_probe: now + Duration::from_secs(1),
            },
        );
        observer.on_lane_health(
            &lane("users"),
            &daemon::LaneHealth::Parked {
                reason: daemon::ParkReason::Refused {
                    ids: vec!["42".to_owned()],
                    reason: "mapper_parsing_exception".to_owned(),
                },
                since: now,
                next_probe: now + Duration::from_secs(1),
            },
        );
        observer.on_lane_health(
            &lane("carts"),
            &daemon::LaneHealth::Parked {
                reason: daemon::ParkReason::Blocked {
                    reason: "index write (api)".to_owned(),
                },
                since: now,
                next_probe: now + Duration::from_secs(1),
            },
        );
        observer.on_lane_health(&lane("carts"), &daemon::LaneHealth::Open);
        observer.on_lane_health(
            &lane("items"),
            &daemon::LaneHealth::Restaging {
                since: now,
                reason: "no such index".to_owned(),
            },
        );

        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .unwrap();
        let provider = provider_builder().with_reader(exporter).build();
        let _gauge = lane_parked_gauge(&provider.meter("flusso"), status);
        let text = TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        for line in [
            r#"flusso_lane_parked{index="orders",sink="primary",why="blocked"} 1"#,
            r#"flusso_lane_parked{index="users",sink="primary",why="refused"} 1"#,
            r#"flusso_lane_parked{index="items",sink="primary",why="restaging"} 1"#,
        ] {
            assert!(text.contains(line), "{line} missing from:\n{text}");
        }
        assert!(
            !text.contains(r#"index="carts""#),
            "an open lane, parked and opened again, has no series:\n{text}"
        );
    }

    #[test]
    fn a_stalled_lane_is_exported_with_its_age_wal_and_as_the_pinning_lane() {
        let users = LaneKey::new(
            IndexName::try_new("users").unwrap(),
            SinkName::try_new("primary").unwrap(),
        );
        let progress = Progress::new([users.clone()]);
        let stalls = Arc::new(Stalls::new([users.clone()]));
        progress.published(
            &users,
            stream::ItemKey::new(3),
            Position::new(3),
            Position::new(3),
        );
        let start = tokio::time::Instant::now();
        stalls.sample(start, &progress, &NoPositions, Some(2_048));
        stalls.sample(
            start + Duration::from_secs(30),
            &progress,
            &NoPositions,
            Some(2_048),
        );
        let status = Arc::new(
            Status::new(
                [users.index.clone()],
                [users.sink.clone()],
                std::time::Instant::now(),
            )
            .with_stalls(stalls),
        );

        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .unwrap();
        let provider = provider_builder().with_reader(exporter).build();
        let _gauges = stall_gauges(&provider.meter("flusso"), status);
        let text = TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        for line in [
            r#"flusso_lane_stall_age_seconds{index="users",sink="primary"} 30"#,
            r#"flusso_lane_wal_behind_bytes{index="users",sink="primary"} 2048"#,
            r#"flusso_slot_pinned_by{index="users",sink="primary"} 1"#,
        ] {
            assert!(text.contains(line), "no `{line}` in:\n{text}");
        }
    }

    /// An index whose pending documents pin the slot is exported by its
    /// index alone, with no `sink`, and its lane keeps no stall series.
    #[test]
    fn an_index_backlog_pinning_the_slot_is_exported_by_its_index() {
        let users = LaneKey::new(
            IndexName::try_new("users").unwrap(),
            SinkName::try_new("primary").unwrap(),
        );
        let progress = Progress::new([users.clone()]);
        let stalls = Arc::new(Stalls::new([users.clone()]));
        progress.mark([(
            source::document::DocumentId {
                index: users.index.clone(),
                key: source::RowKey(Vec::new()),
            },
            Position::new(3),
        )]);
        let start = tokio::time::Instant::now();
        stalls.sample(start, &progress, &NoPositions, Some(2_048));
        stalls.sample(
            start + Duration::from_secs(30),
            &progress,
            &NoPositions,
            Some(2_048),
        );
        let status = Arc::new(
            Status::new(
                [users.index.clone()],
                [users.sink.clone()],
                std::time::Instant::now(),
            )
            .with_stalls(stalls),
        );

        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .unwrap();
        let provider = provider_builder().with_reader(exporter).build();
        let _gauges = stall_gauges(&provider.meter("flusso"), status);
        let text = TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        for line in [
            r#"flusso_lane_stall_age_seconds{index="users",sink="primary"} 0"#,
            r#"flusso_slot_pinned_by{index="users"} 1"#,
        ] {
            assert!(text.contains(line), "no `{line}` in:\n{text}");
        }
        assert!(
            !text.contains("flusso_lane_wal_behind_bytes"),
            "the lane holds nothing itself:\n{text}"
        );
    }

    #[test]
    fn the_memory_budget_is_exported_by_work() {
        let budget = Arc::new(daemon::Budget::new(1 << 20));
        let _live = budget
            .try_reserve(daemon::Work::Live, 3 * 1024)
            .unwrap()
            .settle(3 * 1024);
        let _backfill = budget
            .try_reserve(daemon::Work::Backfill, 2 * 1024)
            .unwrap()
            .settle(2 * 1024);
        budget.note_refused(daemon::Work::Backfill);

        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .unwrap();
        let provider = provider_builder().with_reader(exporter).build();
        let _instruments = memory_instruments(&provider.meter("flusso"), budget);
        let text = TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        for line in [
            r#"flusso_memory_built_bytes{work="live"} 3072"#,
            r#"flusso_memory_built_bytes{work="backfill"} 2048"#,
            r#"flusso_memory_refused_total{work="backfill"} 1"#,
            r#"flusso_memory_refused_total{work="live"} 0"#,
        ] {
            assert!(text.contains(line), "no `{line}` in:\n{text}");
        }
    }
}
