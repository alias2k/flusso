//! Real-service integration test for the alias-over-generations reindex.
//!
//! Starts a genuine OpenSearch 2 cluster in a container and drives the sink
//! through a full reindex lifecycle across **two sink instances** (mirroring the
//! pipeline restart a reindex triggers — each run builds a fresh sink): an
//! initial seed into generation 1, then a staged reindex that rebuilds into
//! generation 2 from scratch. It asserts the behavior the unit tests can't reach:
//! the stable `{logical}_{hash}` alias swaps atomically onto the new generation,
//! the old generation is dropped, reads stay on the old generation until the
//! swap, and documents only present in the old generation are gone after.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p sinks-opensearch --run-ignored all
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::StatusCode;
use schema_core::{
    ContentHash, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    ResolvedField,
};
use serde_json::Value;
use sinks_core::Sink;
use sinks_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// Logical index the test writes into.
const LOGICAL: &str = "users";

/// Start a single-node OpenSearch 2 cluster with security disabled (mirrors the
/// dev `docker-compose.yml` and the bench). Returns the container (kept alive by
/// the caller) and its `http://host:port` base URL.
async fn start_opensearch() -> (ContainerAsync<GenericImage>, String) {
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

/// A fresh sink for `base_url` — one per "run", as the pipeline builds a new sink
/// on each (re)start.
fn sink(base_url: &str) -> OpensearchSink {
    let config = schema_core::OpensearchSink {
        url: schema_core::Secret::Value(base_url.to_owned()),
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
        refresh_interval: "10s".to_owned(),
        text_analysis: schema_core::TextAnalysis::Builtin,
        auto_subfields: true,
    };
    let name = schema_core::SinkName::try_new("test").unwrap();
    OpensearchSink::from_config(&name, &config).unwrap()
}

/// The explicit mapping the documents conform to. The hash is fixed, so the two
/// runs share the same `{logical}_{hash}` alias — exactly the reindex case (same
/// schema, new generation).
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
        index: IndexName::try_new(LOGICAL).unwrap(),
        hash: ContentHash::new(1),
        fields: vec![
            field("id", MappingType::Long),
            field("email", MappingType::Keyword),
        ],
    }
}

fn document(i: usize) -> GenericValue {
    let mut map = BTreeMap::new();
    map.insert("id".to_owned(), GenericValue::Int(i as i64));
    map.insert(
        "email".to_owned(),
        GenericValue::String(format!("user{i}@example.com")),
    );
    GenericValue::Map(map)
}

/// The concrete indexes the alias `alias` currently points at (sorted).
async fn alias_targets(client: &reqwest::Client, base: &str, alias: &str) -> Vec<String> {
    let resp = client
        .get(format!("{base}/_alias/{alias}"))
        .send()
        .await
        .unwrap();
    if resp.status() == StatusCode::NOT_FOUND {
        return Vec::new();
    }
    let body: Value = resp.json().await.unwrap();
    let mut targets: Vec<String> = body.as_object().unwrap().keys().cloned().collect();
    targets.sort();
    targets
}

/// Whether a concrete index (not alias) exists, via `HEAD`.
async fn index_exists(client: &reqwest::Client, base: &str, index: &str) -> bool {
    client
        .head(format!("{base}/{index}"))
        .send()
        .await
        .unwrap()
        .status()
        .is_success()
}

/// Whether a document `id` is retrievable through `target` (an index or alias) —
/// a realtime GET, so no refresh is needed.
async fn doc_exists(client: &reqwest::Client, base: &str, target: &str, id: &str) -> bool {
    client
        .get(format!("{base}/{target}/_doc/{id}"))
        .send()
        .await
        .unwrap()
        .status()
        .is_success()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn reindex_swaps_generations_and_drops_the_old_one() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");
    let gen2 = format!("{hash_alias}_2");

    // ── Run 1: initial seed into generation 1 (ids 1, 2, 3) ──────────────────
    let run1 = sink(&base);
    run1.ensure_index(&mapping).await.unwrap();
    assert!(
        !run1.is_seeded(&index).await.unwrap(),
        "a fresh index is unseeded before its backfill"
    );
    for i in [1, 2, 3] {
        run1.upsert(&index, &i.to_string(), &document(i))
            .await
            .unwrap();
    }
    run1.flush(true).await.unwrap();
    run1.mark_seeded(&index).await.unwrap();

    assert!(
        run1.is_seeded(&index).await.unwrap(),
        "seeded after mark_seeded"
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()],
        "the hash alias points at generation 1",
    );
    assert!(
        doc_exists(&client, &base, &hash_alias, "3").await,
        "id 3 is visible through the alias",
    );

    // ── Reindex staged on a throwaway sink (as the run loop does) ────────────
    sink(&base).reindex(&mapping).await.unwrap();

    // ── Run 2: a fresh sink (restart) rebuilds into generation 2, from empty,
    //    with only ids 1 and 2 — so id 3 becomes an orphan the rebuild drops. ─
    let run2 = sink(&base);
    run2.ensure_index(&mapping).await.unwrap();
    assert!(
        !run2.is_seeded(&index).await.unwrap(),
        "a staged reindex reports the index unseeded again"
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()],
        "reads stay on generation 1 while generation 2 is being built",
    );
    for i in [1, 2] {
        run2.upsert(&index, &i.to_string(), &document(i))
            .await
            .unwrap();
    }
    run2.flush(true).await.unwrap();
    run2.mark_seeded(&index).await.unwrap();

    // ── The swap: alias now on generation 2, generation 1 dropped, orphan gone.
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen2.clone()],
        "the hash alias swapped onto generation 2",
    );
    assert!(
        !index_exists(&client, &base, &gen1).await,
        "the superseded generation 1 was dropped",
    );
    assert!(
        doc_exists(&client, &base, &hash_alias, "1").await,
        "rebuilt documents are visible through the alias",
    );
    assert!(
        !doc_exists(&client, &base, &hash_alias, "3").await,
        "rebuilding from empty dropped id 3, which the new generation never received",
    );
    assert!(run2.is_seeded(&index).await.unwrap());
}
