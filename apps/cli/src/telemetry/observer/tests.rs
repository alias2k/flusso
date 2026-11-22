#![allow(clippy::unwrap_used)]

use std::time::Duration;

use daemon::{Stage, Work};
use opentelemetry::metrics::MeterProvider;
use prometheus::{Registry, TextEncoder};

use super::*;
use crate::telemetry::metrics::provider_builder;

/// An observer recording into its own Prometheus registry, as `/metrics`
/// renders it.
fn recorder() -> (
    OtelObserver,
    Registry,
    opentelemetry_sdk::metrics::SdkMeterProvider,
) {
    let registry = Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .scope_info_enabled(false)
        .build()
        .unwrap();
    let provider = provider_builder().with_reader(exporter).build();
    let observer = OtelObserver::with_meter(&provider.meter("flusso"), Instant::now());
    (observer, registry, provider)
}

fn scrape(registry: &Registry) -> String {
    TextEncoder::new()
        .encode_to_string(&registry.gather())
        .unwrap()
}

/// The value of the one series whose name and labels are exactly `series`.
fn value(text: &str, series: &str) -> f64 {
    text.lines()
        .find_map(|line| line.strip_prefix(series)?.strip_prefix(' '))
        .unwrap_or_else(|| panic!("no series {series} in:\n{text}"))
        .parse()
        .unwrap()
}

fn sink(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

#[test]
fn stage_timings_are_exposed_as_one_histogram_by_stage_work_and_sink() {
    let (observer, registry, _provider) = recorder();
    let primary = sink("primary");
    let users = IndexName::try_new("users").unwrap();
    observer.on_stage(StageTiming::new(Stage::Resolve, Duration::from_millis(3)).work(Work::Live));
    observer.on_stage(StageTiming::new(Stage::Resolve, Duration::from_millis(5)).work(Work::Live));
    observer.on_stage(
        StageTiming::new(Stage::Write, Duration::from_millis(40))
            .work(Work::Backfill)
            .sink(&primary),
    );
    observer.on_stage(StageTiming::new(Stage::Idle, Duration::from_secs(2)).sink(&primary));
    observer.on_stage(
        StageTiming::new(Stage::Build, Duration::from_millis(7))
            .work(Work::Live)
            .index(&users),
    );

    let text = scrape(&registry);
    let resolve = r#"flusso_stage_duration_seconds_sum{stage="resolve",work="live"}"#;
    assert!((value(&text, resolve) - 0.008).abs() < 1e-9);
    assert_eq!(
        value(
            &text,
            r#"flusso_stage_duration_seconds_count{stage="resolve",work="live"}"#
        ),
        2.0
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_stage_duration_seconds_bucket{sink="primary",stage="write",work="backfill",le="0.05"}"#
        ),
        1.0,
        "the stage buckets apply, not OTel's millisecond defaults"
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_stage_duration_seconds_sum{sink="primary",stage="idle"}"#
        ),
        2.0
    );
    assert!(
        (value(
            &text,
            r#"flusso_stage_duration_seconds_sum{index="users",stage="build",work="live"}"#
        ) - 0.007)
            .abs()
            < 1e-9
    );
}

#[test]
fn a_commit_records_the_flush_documents_and_size_by_sink_and_work() {
    let (observer, registry, _provider) = recorder();
    observer.on_batch_committed(
        &sink("primary"),
        CommitStats::new(128, Duration::from_millis(60), Work::Backfill).sent(Some(50_000)),
    );
    observer.on_batch_committed(
        &sink("audit"),
        CommitStats::new(1, Duration::from_millis(1), Work::Live),
    );
    observer.on_batch_committed(
        &sink("audit"),
        CommitStats::new(0, Duration::from_millis(1), Work::Live).sent(Some(0)),
    );

    let text = scrape(&registry);
    assert_eq!(
        value(
            &text,
            r#"flusso_flush_documents_sum{sink="primary",work="backfill"}"#
        ),
        128.0
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_flush_documents_bucket{sink="primary",work="backfill",le="250"}"#
        ),
        1.0
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_flush_size_bytes_sum{sink="primary",work="backfill"}"#
        ),
        50_000.0
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_flush_documents_count{sink="audit",work="live"}"#
        ),
        1.0
    );
    assert!(
        !text.contains(r#"flusso_flush_size_bytes_count{sink="audit""#),
        "a sink that counts no bytes records no size, and an empty flush neither"
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_flush_duration_seconds_count{sink="primary"}"#
        ),
        1.0
    );
}

#[test]
fn start_up_steps_are_a_gauge_per_step_and_sink() {
    let (observer, registry, _provider) = recorder();
    observer.on_startup_step(StartupStep::Source, None, Duration::from_millis(120));
    observer.on_startup_step(
        StartupStep::Stage,
        Some(&sink("primary")),
        Duration::from_millis(300),
    );
    observer.on_startup_step(StartupStep::Prepare, None, Duration::from_millis(80));
    observer.on_startup_step(StartupStep::OpenLive, None, Duration::from_millis(5));
    observer.on_live_started();
    let first = value(
        &scrape(&registry),
        r#"flusso_startup_duration_seconds{step="live"}"#,
    );
    std::thread::sleep(Duration::from_millis(20));
    observer.on_live_started();

    let text = scrape(&registry);
    assert_eq!(
        value(&text, r#"flusso_startup_duration_seconds{step="source"}"#),
        0.12
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_startup_duration_seconds{sink="primary",step="stage"}"#
        ),
        0.3
    );
    assert_eq!(
        value(&text, r#"flusso_startup_duration_seconds{step="prepare"}"#),
        0.08
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_startup_duration_seconds{step="open_live"}"#
        ),
        0.005
    );
    assert_eq!(
        value(&text, r#"flusso_startup_duration_seconds{step="live"}"#),
        first,
        "a restarted ingest engine opening the stream again doesn't move the start-up time"
    );
}

#[test]
fn a_parked_lanes_probes_are_counted_by_outcome() {
    let (observer, registry, _provider) = recorder();
    let lane = stream::LaneKey::new(IndexName::try_new("orders").unwrap(), sink("primary"));
    observer.on_lane_probe(&lane, daemon::ProbeOutcome::Blocked);
    observer.on_lane_probe(&lane, daemon::ProbeOutcome::Blocked);
    observer.on_lane_probe(&lane, daemon::ProbeOutcome::Landed);
    let text = scrape(&registry);
    assert_eq!(
        value(
            &text,
            r#"flusso_lane_probes_total{index="orders",outcome="blocked",sink="primary"}"#
        ),
        2.0
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_lane_probes_total{index="orders",outcome="landed",sink="primary"}"#
        ),
        1.0
    );
}

/// Slice 7: each wait of the reader at its lead is one observation of
/// `flusso.reader.waits`, by the bound that stopped it, and for a per-index
/// bound by the index at it (the other bounds carry no `index` label).
#[test]
fn the_readers_waits_are_a_histogram_by_bound() {
    let (observer, registry, _provider) = recorder();
    let orders = IndexName::try_new("orders").unwrap();
    observer.on_reader_wait(
        daemon::ReaderBound::PerIndex,
        Some(&orders),
        Duration::from_millis(30),
    );
    observer.on_reader_wait(
        daemon::ReaderBound::PerIndex,
        Some(&orders),
        Duration::from_millis(70),
    );
    observer.on_reader_wait(daemon::ReaderBound::Pending, None, Duration::from_secs(2));
    let text = scrape(&registry);
    assert_eq!(
        value(
            &text,
            r#"flusso_reader_waits_seconds_count{bound="per_index",index="orders"}"#
        ),
        2.0
    );
    assert!(
        (value(
            &text,
            r#"flusso_reader_waits_seconds_sum{bound="per_index",index="orders"}"#
        ) - 0.1)
            .abs()
            < 1e-9
    );
    assert_eq!(
        value(
            &text,
            r#"flusso_reader_waits_seconds_count{bound="pending"}"#
        ),
        1.0
    );
    for le in ["60", "120", "300", "600"] {
        let bucket = format!(r#"flusso_reader_waits_seconds_bucket{{bound="pending",le="{le}"}}"#);
        assert!(text.contains(&bucket), "{bucket} missing:\n{text}");
    }
    assert!(
        !text.contains(r#"bound="total""#),
        "no series for a bound never hit:\n{text}"
    );
}
