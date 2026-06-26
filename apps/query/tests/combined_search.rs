//! Real-service e2e for combined search over a [`FlussoMultiDocument`] union.
//!
//! This is the suite that proves the one thing the unit tests can't: that a
//! **live** OpenSearch cluster, queried through the stable `{logical}_{hash}`
//! hash alias the sink owns, returns the *generation-suffixed concrete* index
//! (`{logical}_{hash}_{n}`) in each hit's `_index` — and that this crate
//! dispatches those hits into the right union variant anyway.
//!
//! It drives the **real OpenSearch sink** (not a mock) to build two
//! generation-named indexes and their aliases, then runs `Union::query().send`
//! through this crate against the same cluster. So it guards the sink↔query
//! naming contract end to end: a drift on either side (the sink's generation
//! scheme, the query side's suffix normalization) breaks it. Before the fix in
//! [`flusso_query::MultiSearch`], every hit missed dispatch and the search
//! failed with `UnexpectedIndex`.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query --run-ignored all
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use flusso_query::{Client, FlussoDocument, FlussoIndex, FlussoMultiDocument, Result, Segment};
use schema_core::{
    ContentHash, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    ResolvedField,
};
use sinks_core::Sink;
use sinks_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

// ───────────────────────── the union under test ──────────────────────────

/// One storefront user. `SCHEMA_HASH` is the eight-hex form of the mapping's
/// `ContentHash::new(1)` — what the sink suffixes the index with.
#[derive(Debug, serde::Deserialize)]
struct User {
    email: String,
}
impl FlussoDocument for User {
    const PATH: &'static [Segment] = &[];
}
impl FlussoIndex for User {
    const INDEX: &'static str = "users";
    const SCHEMA_HASH: &'static str = "00000001";
}

/// One storefront order. `SCHEMA_HASH` matches `ContentHash::new(2)`.
#[derive(Debug, serde::Deserialize)]
struct Order {
    status: String,
}
impl FlussoDocument for Order {
    const PATH: &'static [Segment] = &[];
}
impl FlussoIndex for Order {
    const INDEX: &'static str = "orders";
    const SCHEMA_HASH: &'static str = "00000002";
}

/// The blended search surface: one query across both indexes.
#[derive(Debug)]
enum StoreItem {
    User(User),
    Order(Order),
}

impl FlussoMultiDocument for StoreItem {
    const TARGETS: &'static [(&'static str, &'static str)] = &[
        (User::INDEX, User::SCHEMA_HASH),
        (Order::INDEX, Order::SCHEMA_HASH),
    ];

    fn decode(physical_index: &str, source: serde_json::Value) -> Result<Self> {
        if physical_index == User::physical_index() {
            return Ok(Self::User(serde_json::from_value(source)?));
        }
        if physical_index == Order::physical_index() {
            return Ok(Self::Order(serde_json::from_value(source)?));
        }
        Err(flusso_query::Error::UnexpectedIndex {
            index: physical_index.to_owned(),
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn combined_search_decodes_generation_named_hits() {
    let (_container, base) = start_opensearch().await;

    // ── Seed both indexes through the real sink (generation 1 + hash alias) ──
    let sink = sink(&base);
    seed(
        &sink,
        mapping("users", 1, "email", MappingType::Keyword),
        "1",
        document("email", "ada@example.com"),
    )
    .await;
    seed(
        &sink,
        mapping("orders", 2, "status", MappingType::Keyword),
        "9",
        document("status", "open"),
    )
    .await;

    // ── Query through this crate, over the hash aliases the union addresses ──
    let client = Client::connect(&base).unwrap();
    let search = StoreItem::query();
    assert_eq!(
        search.physical_path(),
        "users_00000001,orders_00000002",
        "the union addresses the un-suffixed hash aliases",
    );

    let page = await_hits(&search, &client, &base, 2).await;
    assert_eq!(page.total, 2, "both seeded documents match");

    // Each hit's `_index` came back as `{logical}_{hash}_1` from the cluster;
    // dispatch must still land it in the right variant.
    let mut saw_user = false;
    let mut saw_order = false;
    for hit in &page.hits {
        match &hit.source {
            StoreItem::User(user) => {
                assert_eq!(user.email, "ada@example.com");
                saw_user = true;
            }
            StoreItem::Order(order) => {
                assert_eq!(order.status, "open");
                saw_order = true;
            }
        }
    }
    assert!(
        saw_user && saw_order,
        "both variants decoded from the blend"
    );

    assert_eq!(
        search.count(&client).await.unwrap(),
        2,
        "combined count agrees with the blended page total",
    );
}

// ─────────────────────────────── helpers ────────────────────────────────

/// Drive the sink through a full seed of one index: announce the mapping
/// (creating generation 1 + its hash alias), write one document, flush, and mark
/// it seeded so the alias serves it.
async fn seed(sink: &OpensearchSink, mapping: IndexMapping, id: &str, doc: GenericValue) {
    let index = mapping.index.clone();
    sink.ensure_index(&mapping).await.unwrap();
    sink.upsert(&index, id, &doc).await.unwrap();
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();
}

/// Refresh both target aliases and run the search until it returns `want` hits
/// or a deadline trips (the sink's refresh is asynchronous).
async fn await_hits(
    search: &flusso_query::MultiSearch<StoreItem>,
    client: &Client,
    base: &str,
    want: usize,
) -> flusso_query::SearchResponse<StoreItem> {
    let http = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let _ = http
            .post(format!("{base}/{}/_refresh", search.physical_path()))
            .send()
            .await;
        let page = search.send(client).await.unwrap();
        if page.hits.len() >= want {
            return page;
        }
        assert!(
            Instant::now() < deadline,
            "combined search did not return {want} hits in time (got {})",
            page.hits.len(),
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A single-field mapping with a fixed hash, so the generation index is
/// `{logical}_{hash:08x}_1` — the suffixed name the union must dispatch.
fn mapping(logical: &str, hash: u32, field: &str, ty: MappingType) -> IndexMapping {
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
            },
            nullable: true,
            array: false,
            children: Vec::new(),
        }],
    }
}

fn document(field: &str, value: &str) -> GenericValue {
    let mut map = BTreeMap::new();
    map.insert(field.to_owned(), GenericValue::String(value.to_owned()));
    GenericValue::Map(map)
}

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
        refresh_interval: "1s".to_owned(),
        text_analysis: schema_core::TextAnalysis::Builtin,
        auto_subfields: true,
    };
    let name = schema_core::SinkName::try_new("e2e").unwrap();
    OpensearchSink::from_config(&name, &config).unwrap()
}

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
