//! Shared helpers for the live e2e binaries: an OpenSearch container, the real
//! sink pointed at it, and single-field mappings/documents to seed it with.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::time::Duration;

use kernel::{
    ContentHash, Envelope, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    Position, ResolvedField,
};
use sink::Sink;
use sink_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// Drive the sink through a full seed of one index: announce the mapping
/// (creating generation 1 + its hash alias), write one document, flush, and mark
/// it seeded so the alias serves it.
pub(crate) async fn seed(
    sink: &OpensearchSink,
    mapping: IndexMapping,
    id: &str,
    doc: GenericValue,
) {
    let index = mapping.index.clone();
    sink.ensure_index(&mapping).await.unwrap();
    sink.apply(&Envelope::upsert(
        index.clone(),
        id,
        doc,
        Some(Position(0)),
        chrono::Utc::now(),
    ))
    .await
    .unwrap();
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();
}

/// A single-field mapping with a fixed hash, so the generation index is
/// `{logical}_{hash:08x}_1` — the suffixed name the union must dispatch.
pub(crate) fn mapping(logical: &str, hash: u32, field: &str, ty: MappingType) -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(logical).unwrap(),
        hash: ContentHash::new(hash),
        fields: vec![ResolvedField {
            name: FieldName::try_new(field).unwrap(),
            mapping: Mapping {
                mapping_type: ty,
                extra: BTreeMap::new(),
                map_values: None,
                decimal: false,
                enum_order: None,
            },
            nullable: true,
            array: false,
            children: Vec::new(),
        }],
    }
}

pub(crate) fn document(field: &str, value: &str) -> GenericValue {
    let mut map = BTreeMap::new();
    map.insert(field.to_owned(), GenericValue::String(value.to_owned()));
    GenericValue::Map(map)
}

pub(crate) fn sink(base_url: &str) -> OpensearchSink {
    let config = sink_opensearch::OpensearchConfig {
        url: kernel::Secret::Value(base_url.to_owned()),
        username: None,
        password: None,
        tls_verify: false,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 30,
        max_retries: 3,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 0,
        refresh_interval: "1s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    let name = kernel::SinkName::try_new("e2e").unwrap();
    OpensearchSink::from_config(&name, &config).unwrap()
}

pub(crate) async fn start_opensearch() -> (ContainerAsync<GenericImage>, String) {
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
    (container, format!("http://127.0.0.1:{port}"))
}

/// Seed several documents into one index: generation 1 + its hash alias, every
/// document written and flushed, then marked seeded.
pub(crate) async fn seed_many(
    sink: &OpensearchSink,
    mapping: &IndexMapping,
    docs: &[(&str, GenericValue)],
) {
    sink.ensure_index(mapping).await.unwrap();
    for (id, doc) in docs {
        write(sink, &mapping.index, id, doc.clone()).await;
    }
    sink.mark_seeded(&mapping.index).await.unwrap();
}

/// Upsert one document through the sink and flush it.
pub(crate) async fn write(sink: &OpensearchSink, index: &IndexName, id: &str, doc: GenericValue) {
    sink.apply(&Envelope::upsert(
        index.clone(),
        id,
        doc,
        Some(Position(0)),
        chrono::Utc::now(),
    ))
    .await
    .unwrap();
    sink.flush(true).await.unwrap();
}

/// Make every write so far searchable on `path` (an index, alias, or list).
pub(crate) async fn refresh(base: &str, path: &str) {
    reqwest::Client::new()
        .post(format!("{base}/{path}/_refresh"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}
