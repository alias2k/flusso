//! Real-service e2e for `map` keys (issue #177): every runtime key is mapped
//! like a plain field of the map's `values` type.
//!
//! Against a live OpenSearch, through the real sink, it proves:
//!
//! - **Typed keys under the strict root**: a `text` map at the root, a `keyword`
//!   map inside a `nested` array and a `double` map inside an `object` all get
//!   their declared type, analyzer and subfields from the sink's
//!   `dynamic_templates`, while an unknown root field is still refused.
//! - **The declared type wins over the first value**: a `double` key whose first
//!   value is integral stays `double`, so a later fraction keeps its precision.
//! - **Mismatched values behave like a plain field**: a number in a `text` map is
//!   coerced; a string in a numeric map, an object in a `text` map and a dotted
//!   key (an object path to OpenSearch) are item-level rejections in the flush report (so `on_error` applies), and the
//!   rest of the batch lands.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-sink-opensearch --run-ignored all -E 'binary(map_keys)'
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::time::Duration;

use kernel::{
    ContentHash, Envelope, FieldName, FlussoType, GenericValue, IndexMapping, IndexName, Mapping,
    MappingType, Position, ResolvedField,
};
use serde_json::{Value, json};
use sink::Sink;
use sink_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

const LOGICAL: &str = "products";
/// The hash alias the sink serves a seeded index through: `{logical}_{hash}`.
const ALIAS: &str = "products_00000177";

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

fn sink(base_url: &str) -> OpensearchSink {
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
    OpensearchSink::from_config(&kernel::SinkName::try_new("test").unwrap(), &config).unwrap()
}

fn field(name: &str, mapping_type: MappingType, children: Vec<ResolvedField>) -> ResolvedField {
    ResolvedField {
        name: FieldName::try_new(name).unwrap(),
        mapping: Mapping {
            mapping_type,
            extra: BTreeMap::new(),
            map_values: None,
            decimal: false,
            enum_order: None,
            subfields: true,
            indexed: true,
        },
        nullable: true,
        array: false,
        children,
    }
}

fn map_field(name: &str, values: &FlussoType) -> ResolvedField {
    let mut map = field(name, MappingType::Object, vec![]);
    map.mapping
        .extra
        .insert("dynamic".to_owned(), GenericValue::Bool(true));
    map.mapping.map_values = Some(Box::new(Mapping::of_type(values)));
    map
}

fn mapping() -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(LOGICAL).unwrap(),
        hash: ContentHash::new(0x177),
        fields: vec![
            map_field("title", &FlussoType::Text),
            field(
                "orders",
                MappingType::Nested,
                vec![
                    field("id", MappingType::Long, vec![]),
                    map_field("labels", &FlussoType::Keyword),
                ],
            ),
            field(
                "meta",
                MappingType::Object,
                vec![map_field("prices", &FlussoType::Double)],
            ),
        ],
    }
}

fn generic(value: Value) -> GenericValue {
    match value {
        Value::String(string) => GenericValue::String(string),
        Value::Number(number) => match number.as_i64() {
            Some(int) => GenericValue::BigInt(int),
            None => GenericValue::Double(number.as_f64().unwrap()),
        },
        Value::Object(object) => GenericValue::Map(
            object
                .into_iter()
                .map(|(key, value)| (key, generic(value)))
                .collect(),
        ),
        Value::Array(items) => GenericValue::Array(items.into_iter().map(generic).collect()),
        Value::Bool(flag) => GenericValue::Bool(flag),
        Value::Null => GenericValue::Null,
    }
}

async fn upsert(sink: &OpensearchSink, id: &str, doc: Value) {
    sink.apply(&Envelope::upsert(
        IndexName::try_new(LOGICAL).unwrap(),
        id,
        generic(doc),
        Some(Position(0)),
        chrono::Utc::now(),
    ))
    .await
    .unwrap();
}

async fn search(base: &str, query: Value) -> Vec<String> {
    let response: Value = reqwest::Client::new()
        .post(format!("{base}/{ALIAS}/_search"))
        .json(&json!({ "query": query }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut ids: Vec<String> = response["hits"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["_id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn map_keys_are_typed_like_plain_fields_of_their_values() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    let mapping = mapping();
    sink.ensure_index(&mapping).await.unwrap();

    upsert(
        &sink,
        "1",
        json!({
            "title": { "en": "Café Excavator" },
            "orders": [{ "id": 1, "labels": { "kind": "Foo Bar" } }],
            "meta": { "prices": { "usd": 3 } },
        }),
    )
    .await;
    upsert(
        &sink,
        "2",
        json!({ "title": { "en": 42 }, "meta": { "prices": { "usd": 3.5 } } }),
    )
    .await;
    upsert(
        &sink,
        "3",
        json!({ "meta": { "prices": { "usd": "abc" } } }),
    )
    .await;
    upsert(
        &sink,
        "4",
        json!({ "title": { "en": { "nested": "object" } } }),
    )
    .await;
    upsert(&sink, "6", json!({ "title": { "en.US": "dotted key" } })).await;
    let report = sink.flush(true).await.unwrap();
    let mut rejected: Vec<&str> = report.rejected.iter().map(|doc| doc.id.as_str()).collect();
    rejected.sort_unstable();
    assert_eq!(rejected, ["3", "4", "6"], "{:?}", report.rejected);
    sink.mark_seeded(&mapping.index).await.unwrap();
    reqwest::Client::new()
        .post(format!("{base}/{ALIAS}/_refresh"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let body: Value = reqwest::Client::new()
        .get(format!("{base}/{ALIAS}/_mapping"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let (_, index) = body.as_object().unwrap().iter().next().unwrap();
    let properties = &index["mappings"]["properties"];
    let title_en = &properties["title"]["properties"]["en"];
    assert_eq!(title_en["type"], "text");
    assert_eq!(title_en["analyzer"], "flusso_text");
    assert_eq!(
        title_en["fields"]["keyword_lowercase"]["normalizer"],
        "flusso_lowercase"
    );
    let label = &properties["orders"]["properties"]["labels"]["properties"]["kind"];
    assert_eq!(label["type"], "keyword");
    assert_eq!(label["fields"]["text"]["analyzer"], "flusso_code");
    let usd = &properties["meta"]["properties"]["prices"]["properties"]["usd"];
    assert_eq!(usd["type"], "double");

    assert_eq!(
        search(&base, json!({ "match": { "title.en": "cafe" } })).await,
        ["1"]
    );
    assert_eq!(
        search(&base, json!({ "match": { "title.en": "42" } })).await,
        ["2"]
    );
    assert_eq!(
        search(&base, json!({ "term": { "meta.prices.usd": 3.5 } })).await,
        ["2"]
    );
    assert_eq!(
        search(
            &base,
            json!({ "nested": { "path": "orders", "query": { "term": { "orders.labels.kind": "Foo Bar" } } } }),
        )
        .await,
        ["1"]
    );

    upsert(&sink, "5", json!({ "unknown": "field" })).await;
    let report = sink.flush(true).await.unwrap();
    assert_eq!(report.rejected.len(), 1);
    assert!(
        report.rejected[0].reason.contains("strict"),
        "{:?}",
        report.rejected
    );
}
