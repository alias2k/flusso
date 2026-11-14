//! Real-service test for document ids OpenSearch refuses before it runs a bulk
//! request (issue #301).
//!
//! OpenSearch validates every action's `_id` before it executes `_bulk`, and
//! one invalid id fails the **whole** request with HTTP 400, taking every
//! other document in it down too. Two ids do that, measured on OpenSearch 2:
//!
//! - an index action whose id is longer than **512 bytes** (UTF-8 bytes, not
//!   characters: 256 × `é` is accepted, 256 × `é` + `a` is refused);
//! - a delete action whose id is empty.
//!
//! The sink must reject such a document on its own, as an item-level
//! rejection the engine's `on_error` decides, and send the rest. A delete with
//! an over-long id is *accepted* by OpenSearch (it reports `not_found`: no
//! document can have that id), so it is sent and is not a rejection.
//!
//! Both write targets are covered: a seeded index (written through its hash
//! alias with `require_alias`) and an unseeded generation (written directly).
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-sink-opensearch --run-ignored all -E 'binary(document_ids)'
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use kernel::{
    ContentHash, Envelope, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    Position, ResolvedField,
};
use serde_json::{Value, json};
use sink::{RejectedDocument, Sink};
use sink_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

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

/// A sink with no bulk retries, so a request-wide failure fails the flush at once.
fn sink(base_url: &str) -> OpensearchSink {
    let config = sink_opensearch::OpensearchConfig {
        url: kernel::Secret::Value(base_url.to_owned()),
        username: None,
        password: None,
        tls_verify: false,
        ca_file: None,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 30,
        max_retries: 0,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 0,
        refresh_interval: "1s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    OpensearchSink::from_config(&kernel::SinkName::try_new("test").unwrap(), &config).unwrap()
}

fn mapping(logical: &str) -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(logical).unwrap(),
        hash: ContentHash::new(1),
        fields: vec![ResolvedField {
            name: FieldName::try_new("name").unwrap(),
            mapping: Mapping {
                mapping_type: MappingType::Keyword,
                extra: BTreeMap::new(),
                map_values: None,
                decimal: false,
                enum_order: None,
                subfields: true,
                indexed: true,
            },
            nullable: true,
            array: false,
            children: Vec::new(),
        }],
    }
}

fn upsert(index: &IndexName, id: &str) -> Envelope {
    let mut doc = BTreeMap::new();
    doc.insert("name".to_owned(), GenericValue::String("x".to_owned()));
    Envelope::upsert(
        index.clone(),
        id,
        GenericValue::Map(doc),
        Some(Position(0)),
        chrono::Utc::now(),
    )
}

fn delete(index: &IndexName, id: &str) -> Envelope {
    Envelope::delete(index.clone(), id, Some(Position(0)), chrono::Utc::now())
}

/// Which of `ids` exist behind `target`: a realtime `_mget`, so no refresh is
/// needed and no id has to be put in a URL.
async fn existing(base: &str, target: &str, ids: &[&str]) -> BTreeSet<String> {
    let body: Value = reqwest::Client::new()
        .post(format!("{base}/{target}/_mget"))
        .json(&json!({ "ids": ids }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["docs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|doc| doc["found"] == json!(true))
        .map(|doc| doc["_id"].as_str().unwrap().to_owned())
        .collect()
}

/// A logical index set up as the seeded (alias) or unseeded (generation) target.
async fn prepare(base: &str, logical: &str, seeded: bool) -> (OpensearchSink, IndexName, String) {
    let index = IndexName::try_new(logical).unwrap();
    let mapping = mapping(logical);
    let hash_alias = format!("{logical}_{}", mapping.hash);
    let setup = sink(base);
    setup.ensure_index(&mapping).await.unwrap();
    if seeded {
        setup.mark_seeded(&index).await.unwrap();
    }
    let run = sink(base);
    run.ensure_index(&mapping).await.unwrap();
    assert_eq!(run.is_seeded(&index).await.unwrap(), seeded);
    (run, index, hash_alias)
}

fn rejected_ids(rejected: &[RejectedDocument]) -> BTreeSet<String> {
    rejected.iter().map(|doc| doc.id.clone()).collect()
}

/// Upserts with ids around the limit, in one flush beside a normal document:
/// only the ones past 512 bytes are rejected, each on its own.
async fn ids_past_512_bytes_are_rejected_alone(base: &str, logical: &str, seeded: bool) {
    let (sink, index, hash_alias) = prepare(base, logical, seeded).await;

    let ascii_513 = "a".repeat(513);
    let ascii_512 = "a".repeat(512);
    let two_byte_512 = "é".repeat(256);
    let two_byte_255_and_two_ascii = format!("{}aa", "é".repeat(255));
    let two_byte_513 = format!("{}a", "é".repeat(256));
    let three_byte_513 = "€".repeat(171);
    let accepted = [
        "normal",
        ascii_512.as_str(),
        two_byte_512.as_str(),
        two_byte_255_and_two_ascii.as_str(),
    ];
    let refused = [
        ascii_513.as_str(),
        two_byte_513.as_str(),
        three_byte_513.as_str(),
    ];

    for id in refused.iter().chain(&accepted) {
        sink.apply(&upsert(&index, id)).await.unwrap();
    }
    let report = sink
        .flush(true)
        .await
        .expect("an over-long id must not fail the whole flush");

    assert_eq!(
        rejected_ids(&report.rejected),
        refused.iter().map(|id| (*id).to_owned()).collect(),
        "exactly the ids past 512 bytes are rejected ({logical})",
    );
    for doc in &report.rejected {
        assert_eq!(doc.index, logical, "a rejection names the logical index");
        assert!(
            doc.reason.contains("513 bytes") && doc.reason.contains("512"),
            "the reason names the id's length and the limit: {}",
            doc.reason,
        );
    }
    assert_eq!(
        existing(base, &hash_alias, &accepted).await,
        accepted.iter().map(|id| (*id).to_owned()).collect(),
        "every id up to 512 bytes is indexed ({logical})",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn an_id_past_512_bytes_is_rejected_alone_and_the_rest_lands() {
    let (_container, base) = start_opensearch().await;
    ids_past_512_bytes_are_rejected_alone(&base, "seeded_ids", true).await;
    ids_past_512_bytes_are_rejected_alone(&base, "unseeded_ids", false).await;
}

/// A delete with an over-long id is accepted by OpenSearch (no document can
/// have that id): it is sent, is not a rejection, and the rest lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_delete_with_an_id_past_512_bytes_is_sent_and_is_not_a_rejection() {
    let (_container, base) = start_opensearch().await;
    for (logical, seeded) in [("seeded_deletes", true), ("unseeded_deletes", false)] {
        let (sink, index, hash_alias) = prepare(&base, logical, seeded).await;
        sink.apply(&upsert(&index, "gone")).await.unwrap();
        sink.flush(true).await.unwrap();

        sink.apply(&delete(&index, &"a".repeat(513))).await.unwrap();
        sink.apply(&delete(&index, "gone")).await.unwrap();
        sink.apply(&upsert(&index, "kept")).await.unwrap();
        let report = sink.flush(true).await.unwrap();

        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        assert_eq!(
            existing(&base, &hash_alias, &["gone", "kept"]).await,
            BTreeSet::from(["kept".to_owned()]),
        );
    }
}

/// An empty id: a delete fails the whole request on OpenSearch, an upsert is
/// refused per item. Both are rejected alone, and the rest lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn an_empty_id_is_rejected_alone_and_the_rest_lands() {
    let (_container, base) = start_opensearch().await;
    for (logical, seeded) in [("seeded_empty", true), ("unseeded_empty", false)] {
        let (sink, index, hash_alias) = prepare(&base, logical, seeded).await;
        sink.apply(&delete(&index, "")).await.unwrap();
        sink.apply(&upsert(&index, "")).await.unwrap();
        sink.apply(&upsert(&index, "normal")).await.unwrap();
        let report = sink
            .flush(true)
            .await
            .expect("an empty id must not fail the whole flush");

        assert_eq!(report.rejected.len(), 2, "{:?}", report.rejected);
        for doc in &report.rejected {
            assert_eq!(doc.id, "");
            assert_eq!(doc.index, logical);
            assert!(doc.reason.contains("empty"), "{}", doc.reason);
        }
        assert_eq!(
            existing(&base, &hash_alias, &["normal"]).await,
            BTreeSet::from(["normal".to_owned()]),
        );
    }
}
