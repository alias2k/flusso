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

/// `flusso.reader.waits`: the stage buckets, then minutes, since a reader
/// stopped behind a stuck index waits that long.
const READER_WAIT_BUCKETS_SECONDS: &[f64] = &[
    0.0001, 0.00025, 0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
    5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
];

/// The explicit buckets of each histogram flusso records, by instrument name.
fn buckets(instrument: &str) -> Option<&'static [f64]> {
    match instrument {
        "flusso.flush.duration" => Some(FLUSH_BUCKETS_SECONDS),
        "flusso.stage.duration" | "flusso.memory.wait" => Some(STAGE_BUCKETS_SECONDS),
        "flusso.reader.waits" => Some(READER_WAIT_BUCKETS_SECONDS),
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

/// Register `flusso.changes.committed` as an observable counter read from
/// the daemon's [`Status`] at collection: per sink, the live changes every
/// lane of that sink has landed or had nothing for, with every change read
/// before them. Read, not added up from commits, so a redelivered item that
/// had landed counts once.
pub(crate) fn register_changes_committed_counter(status: Arc<Status>) -> ObservableCounter<u64> {
    global::meter("flusso")
        .u64_observable_counter("flusso.changes.committed")
        .with_description(
            "Live changes a sink has landed on every lane (or had nothing for), in read order",
        )
        .with_callback(move |observer| {
            for sink in status.sinks() {
                observer.observe(
                    status.committed(sink),
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

/// Each index's ingest state (ADR 0012 stage 8a, slice 7), read from the
/// daemon's [`Status`] at collection, one series per enabled index (one
/// index too): `flusso.index.documents_waiting`, `flusso.index.backlog_age`
/// (s), and 0/1 for `flusso.index.exempt` (exempt or frozen),
/// `flusso.index.frozen`, `flusso.index.working`, `flusso.index.held` (by
/// `budget` and by `lane`) and `flusso.index.backfilling` (its walk runs).
pub(crate) fn register_index_gauges(status: Arc<Status>) -> IndexGauges {
    index_gauges(&global::meter("flusso"), status)
}

/// The gauges [`register_index_gauges`] keeps registered.
pub(crate) type IndexGauges = (
    ObservableGauge<f64>,
    Vec<ObservableGauge<u64>>,
    opentelemetry::metrics::ObservableCounter<u64>,
);

fn index_gauges(meter: &opentelemetry::metrics::Meter, status: Arc<Status>) -> IndexGauges {
    use daemon::{BackfillState, Exemption, IndexHold, IndexIngestSnapshot, IngestState};
    let label = |index: &daemon::IndexName| KeyValue::new("index", index.as_ref().to_owned());
    let ages = Arc::clone(&status);
    let backlog_age = meter
        .f64_observable_gauge("flusso.index.backlog_age")
        .with_unit("s")
        .with_description(
            "Seconds an index's oldest unbuilt change (waiting, or in a build not yet sent to a \
             lane) has stayed its oldest; 0 while nothing waits to be built",
        )
        .with_callback(move |observer| {
            for (index, entry) in ages.ingest_indexes() {
                observer.observe(entry.backlog_seconds, &[label(&index)]);
            }
        })
        .build();
    let flag =
        |name: &'static str, description: &'static str, read: fn(&IndexIngestSnapshot) -> bool| {
            let status = Arc::clone(&status);
            meter
                .u64_observable_gauge(name)
                .with_description(description)
                .with_callback(move |observer| {
                    for (index, entry) in status.ingest_indexes() {
                        observer.observe(u64::from(read(&entry)), &[label(&index)]);
                    }
                })
                .build()
        };
    let waiting = Arc::clone(&status);
    let held = Arc::clone(&status);
    let gauges = vec![
        meter
            .u64_observable_gauge("flusso.index.documents_waiting")
            .with_description("Documents of an index read and not yet taken by a build")
            .with_callback(move |observer| {
                for (index, entry) in waiting.ingest_indexes() {
                    observer.observe(entry.documents_waiting, &[label(&index)]);
                }
            })
            .build(),
        flag(
            "flusso.index.exempt",
            "1 while an index is exempt (held at its cap while another index works, growing \
             or frozen): its documents count only toward max_pending_documents",
            |entry| entry.exemption != Exemption::None,
        ),
        flag(
            "flusso.index.frozen",
            "1 while an exempt index is frozen: its hold ended or no other index works, so it \
             may drain but not grow",
            |entry| entry.exemption == Exemption::Frozen,
        ),
        flag(
            "flusso.index.working",
            "1 while an index works: nothing of it has waited at a sink 5 s with none \
             acknowledged, and it isn't down",
            |entry| entry.working,
        ),
        flag(
            "flusso.index.backfilling",
            "1 while an index's backfill walk runs (not while a failed walk waits to retry)",
            |entry| {
                entry
                    .backfill
                    .as_ref()
                    .is_some_and(|backfill| backfill.state == BackfillState::Walking)
            },
        ),
        flag(
            "flusso.index.backfill_retrying",
            "1 while an index's backfill walk failed and waits to retry, its live changes \
             flowing",
            |entry| {
                entry
                    .backfill
                    .as_ref()
                    .is_some_and(|backfill| backfill.state == BackfillState::Retrying)
            },
        ),
        flag(
            "flusso.index.down",
            "1 while an index is down: its live work failed and its builder waits to restart \
             (always 0 with one index, whose failure restarts the whole run)",
            |entry| entry.state == IngestState::Down,
        ),
        meter
            .u64_observable_gauge("flusso.index.held")
            .with_description(
                "1 on the series of what holds an index's builder: by budget (its live batch \
                 refused by the memory budget) or by lane (a publish blocked on a full lane)",
            )
            .with_callback(move |observer| {
                for (index, entry) in held.ingest_indexes() {
                    for (by, hold) in [("budget", IndexHold::Budget), ("lane", IndexHold::Lane)] {
                        observer.observe(
                            u64::from(entry.held_by == Some(hold)),
                            &[label(&index), KeyValue::new("by", by)],
                        );
                    }
                }
            })
            .build(),
    ];
    let restarted = Arc::clone(&status);
    let restarts = meter
        .u64_observable_counter("flusso.index.restarts")
        .with_description(
            "How often an index's work started again after a failure, by kind: live (its \
             builder restarted) or backfill (its walk started again)",
        )
        .with_callback(move |observer| {
            for (index, entry) in restarted.ingest_indexes() {
                for (kind, count) in [
                    ("live", entry.restarts.live),
                    ("backfill", entry.restarts.backfill),
                ] {
                    observer.observe(count, &[label(&index), KeyValue::new("kind", kind)]);
                }
            }
        })
        .build();
    (backlog_age, gauges, restarts)
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

    /// Slice 7: each index's gauges, one series per index: `orders` exempt
    /// and frozen, held by its lane (information: still working), walking;
    /// `users` neither, but its lane has landed nothing for 5 s, so it is not
    /// working.
    #[tokio::test(start_paused = true)]
    async fn each_index_is_exported_with_its_ingest_state() {
        let orders = IndexName::try_new("orders").unwrap();
        let users = IndexName::try_new("users").unwrap();
        let primary = SinkName::try_new("primary").unwrap();
        let states = Arc::new(daemon::IndexStates::new([orders.clone(), users.clone()]));
        // `users`' lane has landed nothing for 5 s: not working (one index's
        // rule, the states keep no record here).
        let users_lane = LaneKey::new(users.clone(), primary.clone());
        let lanes = [
            users_lane.clone(),
            LaneKey::new(orders.clone(), primary.clone()),
        ];
        let progress = Arc::new(Progress::new(lanes.clone()));
        let stalls = Arc::new(Stalls::new(lanes));
        progress.published(
            &users_lane,
            stream::ItemKey::new(1),
            Position::new(1),
            Position::new(1),
        );
        let start = tokio::time::Instant::now();
        stalls.sample(start, &progress, &NoPositions, None);
        stalls.sample(
            start + Duration::from_secs(5),
            &progress,
            &NoPositions,
            None,
        );
        let status = Arc::new(
            Status::new(
                [
                    orders.clone(),
                    users.clone(),
                    IndexName::try_new("archive").unwrap(),
                ],
                [primary.clone()],
                std::time::Instant::now(),
            )
            .with_ingest_indexes([orders.clone(), users.clone()])
            .with_progress(progress)
            .with_stalls(stalls),
        );
        status.attach_index_states(Arc::clone(&states));
        states.set_exemption(&orders, daemon::ExemptionState::Frozen);
        let _full = states.lane_full(&orders, &primary);
        let _walking =
            states.walking([(orders.clone(), std::collections::BTreeSet::from([primary]))]);
        // Slice 8-2: `users` went down (a live failure) and restarted once
        // before, and its walk waits to retry; `orders` restarted its walk
        // once.
        {
            use daemon::Observer;
            let observer = daemon::StatusObserver::new(Arc::clone(&status));
            observer.on_index_failed(&users, daemon::Work::Live, "live", Duration::from_secs(1));
            observer.on_index_restarted(&users, daemon::Work::Live);
            observer.on_index_failed(&users, daemon::Work::Live, "live", Duration::from_secs(2));
            observer.on_index_failed(
                &users,
                daemon::Work::Backfill,
                "walk",
                Duration::from_secs(1),
            );
            observer.on_index_restarted(&orders, daemon::Work::Backfill);
        }

        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .unwrap();
        let provider = provider_builder().with_reader(exporter).build();
        let _gauges = index_gauges(&provider.meter("flusso"), status);
        let text = TextEncoder::new()
            .encode_to_string(&registry.gather())
            .unwrap();
        for line in [
            r#"flusso_index_documents_waiting{index="orders"} 0"#,
            r#"flusso_index_backlog_age_seconds{index="orders"} 0"#,
            r#"flusso_index_exempt{index="orders"} 1"#,
            r#"flusso_index_frozen{index="orders"} 1"#,
            r#"flusso_index_working{index="orders"} 1"#,
            r#"flusso_index_held{by="budget",index="orders"} 0"#,
            r#"flusso_index_held{by="lane",index="orders"} 1"#,
            r#"flusso_index_backfilling{index="orders"} 1"#,
            r#"flusso_index_documents_waiting{index="users"} 0"#,
            r#"flusso_index_exempt{index="users"} 0"#,
            r#"flusso_index_frozen{index="users"} 0"#,
            r#"flusso_index_working{index="users"} 0"#,
            r#"flusso_index_held{by="lane",index="users"} 0"#,
            r#"flusso_index_backfilling{index="users"} 0"#,
            r#"flusso_index_down{index="orders"} 0"#,
            r#"flusso_index_down{index="users"} 1"#,
            r#"flusso_index_backfill_retrying{index="orders"} 0"#,
            r#"flusso_index_backfill_retrying{index="users"} 1"#,
            r#"flusso_index_restarts_total{index="orders",kind="backfill"} 1"#,
            r#"flusso_index_restarts_total{index="orders",kind="live"} 0"#,
            r#"flusso_index_restarts_total{index="users",kind="backfill"} 0"#,
            r#"flusso_index_restarts_total{index="users",kind="live"} 1"#,
        ] {
            assert!(text.contains(line), "{line} missing from:\n{text}");
        }
        assert!(
            !text.contains(r#"index="archive""#),
            "a disabled index has no series:\n{text}"
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
