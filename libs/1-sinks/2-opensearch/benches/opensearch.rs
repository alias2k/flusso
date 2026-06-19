//! Real-service benchmarks for the OpenSearch sink.
//!
//! These start a genuine OpenSearch 2 cluster in a container and measure the
//! bulk-indexing path end to end — JSON serialization, the in-memory buffer,
//! and the HTTP bulk round-trip — against a live server. Nothing is mocked.
//!
//! Two dimensions are measured:
//!
//! - `bulk_index`: throughput of flushing N documents with a fixed (default)
//!   batch size, so larger N amortizes per-request overhead across more docs.
//! - `batch_size`: a fixed document count flushed with different `batch_size`
//!   settings, to show how bulk chunking affects throughput.
//!
//! Requires Docker. Run with:
//!
//! ```text
//! cargo bench -p sinks-opensearch
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use schema_core::{
    ContentHash, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    ResolvedField,
};
use sinks_core::Sink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{GenericImage, ImageExt};
use tokio::runtime::Runtime;

/// Logical index every benchmark writes into.
const INDEX: &str = "bench";

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap()
}

/// Start a single-node OpenSearch 2 cluster with security disabled, mirroring
/// the dev `docker-compose.yml`. Returns the running container (kept alive by
/// the caller) and its `http://host:port` base URL.
async fn start_opensearch() -> (
    testcontainers_modules::testcontainers::ContainerAsync<GenericImage>,
    String,
) {
    let container = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/_cluster/health")
                .with_port(9200.tcp())
                .with_expected_status_code(200u16)
                .with_poll_interval(Duration::from_secs(1)),
        ))
        .with_env_var("discovery.type", "single-node")
        .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
        .with_env_var("DISABLE_INSTALL_DEMO_CONFIG", "true")
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");

    let port = container.get_host_port_ipv4(9200).await.unwrap();
    let base_url = format!("http://127.0.0.1:{port}");
    (container, base_url)
}

/// A sink configured for `base_url` with the given bulk `batch_size`.
fn sink(base_url: &str, batch_size: u32) -> sinks_opensearch::OpensearchSink {
    let config = schema_core::OpensearchSink {
        url: schema_core::Secret::Value(base_url.to_owned()),
        username: None,
        password: None,
        tls_verify: false,
        batch_size,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 30,
        max_retries: 3,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 1,
        refresh_interval: "10s".to_owned(),
        text_analysis: schema_core::TextAnalysis::Builtin,
        auto_subfields: true,
    };
    let name = schema_core::SinkName::try_new("bench").unwrap();
    sinks_opensearch::OpensearchSink::from_config(&name, &config).unwrap()
}

/// The explicit, `dynamic: strict` mapping the documents below conform to.
fn mapping() -> IndexMapping {
    let field = |name: &str, ty: MappingType| ResolvedField {
        name: FieldName::try_new(name).unwrap(),
        mapping: Mapping {
            mapping_type: ty,
            extra: BTreeMap::new(),
        },
        nullable: true,
        array: false,
        children: Vec::new(),
    };
    IndexMapping {
        index: IndexName::try_new(INDEX).unwrap(),
        hash: ContentHash::new(1),
        fields: vec![
            field("id", MappingType::Long),
            field("email", MappingType::Keyword),
            field("name", MappingType::Text),
            field("active", MappingType::Boolean),
            field("score", MappingType::Double),
        ],
    }
}

/// A realistic flat document for id `i`, matching the mapping above.
fn document(i: usize) -> GenericValue {
    let mut map = BTreeMap::new();
    map.insert("id".to_owned(), GenericValue::BigInt(i as i64));
    map.insert(
        "email".to_owned(),
        GenericValue::String(format!("user{i}@example.com")),
    );
    map.insert(
        "name".to_owned(),
        GenericValue::String(format!("Customer Number {i}")),
    );
    map.insert("active".to_owned(), GenericValue::Bool(i.is_multiple_of(2)));
    map.insert("score".to_owned(), GenericValue::Decimal((i as i64).into()));
    GenericValue::Map(map)
}

/// Buffer `docs` as upserts and flush them — the operation under measurement.
/// Reusing the same ids across iterations keeps the index size stable (each
/// flush overwrites rather than grows), so we measure indexing, not growth.
async fn index_batch(
    sink: &sinks_opensearch::OpensearchSink,
    index: &IndexName,
    docs: &[(String, GenericValue)],
) {
    for (id, doc) in docs {
        sink.upsert(index, id, doc).await.unwrap();
    }
    sink.flush(true).await.unwrap();
}

fn bench(c: &mut Criterion) {
    let rt = runtime();
    // Keep the runtime entered for the whole scope so the container's async
    // teardown (testcontainers drops it via the reactor) has one to run on.
    let _guard = rt.enter();
    let (_container, base_url) = rt.block_on(start_opensearch());
    let index = IndexName::try_new(INDEX).unwrap();

    rt.block_on(async {
        sink(&base_url, 1000)
            .ensure_index(&mapping())
            .await
            .unwrap();
    });

    let mut group = c.benchmark_group("bulk_index");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(5));
    group.measurement_time(Duration::from_secs(15));
    for &n in &[1usize, 100, 1_000, 5_000] {
        let docs: Vec<(String, GenericValue)> =
            (0..n).map(|i| (i.to_string(), document(i))).collect();
        let sink = sink(&base_url, 1000);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &docs, |b, docs| {
            b.to_async(&rt).iter(|| index_batch(&sink, &index, docs));
        });
    }
    group.finish();

    let mut group = c.benchmark_group("batch_size");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(5));
    group.measurement_time(Duration::from_secs(20));
    let docs: Vec<(String, GenericValue)> = (0..5_000usize)
        .map(|i| (i.to_string(), document(i)))
        .collect();
    for &batch in &[100u32, 500, 1_000, 5_000] {
        let sink = sink(&base_url, batch);
        group.throughput(Throughput::Elements(5_000));
        group.bench_with_input(BenchmarkId::from_parameter(batch), &docs, |b, docs| {
            b.to_async(&rt).iter(|| index_batch(&sink, &index, docs));
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
