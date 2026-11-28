//! ADR 0012 stage 10's spikes S1, S3 and S4's OpenSearch half, against a live
//! OpenSearch: what the cluster answers to the requests slice 4's probe will
//! send, before any of it is built. The stage-10 note's §10 records what each
//! one found and what it changes in slice 4.
//!
//! - **S1, the write check** (§6): a one-item `_bulk` index op through the
//!   hash alias, `require_alias`, `if_seq_no: 0`, `if_primary_term: 1`, an
//!   `_id` holding U+0000. It writes no document under any condition. It
//!   answers `version_conflict_engine_exception` only on a healthy
//!   generation; every block, a closed or deleted generation, a pipeline, and
//!   an alias that names no single write index answer something else, and a
//!   block's refusal names it, `read_only_allow_delete` apart from the rest.
//! - **S3, `write_serving`** (§6): a document written alone through the
//!   alias with `require_alias` is refused by a deleted, closed or blocked
//!   serving generation, writes nothing there, and creates no index.
//! - **S4, flood stage and the meta index** (§7): on a node past the flood
//!   stage, the meta index whose shard sits there is blocked like any index:
//!   a record write and a document delete are refused with the flood-stage
//!   block, and with every node past it no generation can be created.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-sink-opensearch --test detach_spikes --run-ignored all
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::time::{Duration, Instant};

use serde_json::{Value, json};
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

/// The write check's `_id`: no Postgres `text` value holds U+0000.
const PROBE_ID: &str = "\u{0}flusso-probe";

/// What a one-item `_bulk` answered.
#[derive(Debug, Clone)]
struct Answer {
    /// The HTTP status of the whole request.
    http: u16,
    /// The item's status, or the request's when it was refused whole.
    status: u64,
    /// The error's `type`, if any.
    kind: Option<String>,
    /// The error's `reason`, if any.
    reason: Option<String>,
    /// The item's `result` (`created`, `noop`, …), if it has one.
    result: Option<String>,
}

impl Answer {
    fn read(http: u16, body: &Value) -> Self {
        let item = body["items"][0]["index"].clone();
        let (status, error) = if item.is_object() {
            (item["status"].as_u64().unwrap(), item["error"].clone())
        } else {
            (u64::from(http), body["error"].clone())
        };
        Answer {
            http,
            status,
            kind: error["type"].as_str().map(str::to_owned),
            reason: error["reason"].as_str().map(str::to_owned),
            result: item["result"].as_str().map(str::to_owned),
        }
    }

    fn kind(&self) -> &str {
        self.kind.as_deref().unwrap_or("")
    }

    fn reason(&self) -> &str {
        self.reason.as_deref().unwrap_or("")
    }
}

/// How slice 4's probe has to read the write check's answer, from what
/// these spikes found: it lands only on the version conflict; a flood-stage
/// block is told apart by its block (id 12), never by the status (429, which
/// the sink's bulk reader takes for busy).
#[derive(Debug, PartialEq, Eq)]
enum Check {
    Lands,
    FloodStage,
    Refused,
}

fn classify(answer: &Answer) -> Check {
    if answer.kind() == "version_conflict_engine_exception" {
        Check::Lands
    } else if answer.kind() == "cluster_block_exception"
        && answer.reason().contains("TOO_MANY_REQUESTS/12/")
    {
        Check::FloodStage
    } else {
        Check::Refused
    }
}

async fn bulk(http: &reqwest::Client, base: &str, action: Value, doc: Value) -> Answer {
    let response = http
        .post(format!("{base}/_bulk"))
        .header("content-type", "application/x-ndjson")
        .body(format!("{action}\n{doc}\n"))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap();
    Answer::read(status, &body)
}

/// The write check of §6, addressed to `target`.
async fn write_check(http: &reqwest::Client, base: &str, target: &str) -> Answer {
    bulk(
        http,
        base,
        json!({"index": {
            "_index": target,
            "_id": PROBE_ID,
            "if_seq_no": 0,
            "if_primary_term": 1,
            "require_alias": true,
        }}),
        json!({}),
    )
    .await
}

/// `write_serving` of §6: one document written alone through the alias.
async fn write_serving(http: &reqwest::Client, base: &str, target: &str) -> Answer {
    bulk(
        http,
        base,
        json!({"index": {"_index": target, "_id": "7", "require_alias": true}}),
        json!({"id": 7}),
    )
    .await
}

async fn put(http: &reqwest::Client, url: String, body: Value) -> Value {
    http.put(url)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

async fn post(http: &reqwest::Client, url: String) -> Value {
    http.post(url).send().await.unwrap().json().await.unwrap()
}

/// A generation `{alias}_1` behind the hash alias `alias`, as the sink
/// creates one: a strict mapping, the alias in the create request.
async fn generation(http: &reqwest::Client, base: &str, alias: &str, settings: Value) {
    let mut settings = settings;
    settings["number_of_replicas"] = json!(0);
    let body = json!({
        "settings": settings,
        "mappings": {"dynamic": "strict", "properties": {"id": {"type": "long"}}},
        "aliases": {alias: {}},
    });
    let answer = put(http, format!("{base}/{alias}_1"), body).await;
    assert_eq!(answer["acknowledged"], true, "{answer}");
}

async fn set(http: &reqwest::Client, base: &str, index: &str, settings: Value) {
    let answer = put(http, format!("{base}/{index}/_settings"), settings).await;
    assert_eq!(answer["acknowledged"], true, "{answer}");
}

/// Lift `block` from `index`: alone, since a `read_only` index refuses any
/// other settings change.
async fn unblock(http: &reqwest::Client, base: &str, index: &str, block: &str) {
    set(http, base, index, json!({ block: false })).await;
}

async fn cluster(http: &reqwest::Client, base: &str, settings: Value) {
    let answer = put(
        http,
        format!("{base}/_cluster/settings"),
        json!({ "persistent": settings }),
    )
    .await;
    assert_eq!(answer["acknowledged"], true, "{answer}");
}

/// The documents `index` holds, refreshed.
async fn count(http: &reqwest::Client, base: &str, index: &str) -> u64 {
    post(http, format!("{base}/{index}/_refresh")).await;
    let body: Value = http
        .get(format!("{base}/{index}/_count"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["count"].as_u64().unwrap()
}

async fn exists(http: &reqwest::Client, base: &str, index: &str) -> bool {
    http.head(format!("{base}/{index}"))
        .send()
        .await
        .unwrap()
        .status()
        .is_success()
}

async fn version(http: &reqwest::Client, base: &str) -> String {
    let body: Value = http.get(base).send().await.unwrap().json().await.unwrap();
    body["version"]["number"].as_str().unwrap().to_owned()
}

/// An index-level block, and the reason its refusal carries.
const INDEX_BLOCKS: [(&str, u64, &str); 3] = [
    ("index.blocks.write", 403, "FORBIDDEN/8/index write (api)"),
    (
        "index.blocks.read_only",
        403,
        "FORBIDDEN/5/index read-only (api)",
    ),
    (
        "index.blocks.read_only_allow_delete",
        429,
        "TOO_MANY_REQUESTS/12/disk usage exceeded flood-stage watermark, index has \
         read-only-allow-delete block",
    ),
];

/// S1: the write check lands only on a healthy generation, writes no
/// document under any condition, and a block's refusal names the block.
#[tokio::test]
#[ignore = "requires Docker"]
async fn the_write_check_lands_only_on_a_healthy_generation_and_writes_nothing() {
    let (_os, base) = start_opensearch().await;
    let http = reqwest::Client::new();
    assert!(version(&http, &base).await.starts_with("2."));

    // Healthy: the version check runs, and nothing is written, whatever the
    // U+0000 in the id.
    generation(&http, &base, "healthy", json!({})).await;
    let answer = write_check(&http, &base, "healthy").await;
    assert_eq!(classify(&answer), Check::Lands, "{answer:?}");
    assert_eq!(answer.status, 409, "{answer:?}");
    assert_eq!(count(&http, &base, "healthy_1").await, 0);
    let found: Value = http
        .get(format!("{base}/healthy_1/_doc/%00flusso-probe"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(found["found"], false, "{found}");

    // Each index block is met before the version check, and named.
    for (block, status, reason) in INDEX_BLOCKS {
        let alias = block.replace('.', "_");
        generation(&http, &base, &alias, json!({})).await;
        set(&http, &base, &format!("{alias}_1"), json!({ block: true })).await;
        let answer = write_check(&http, &base, &alias).await;
        assert_eq!(
            answer.kind(),
            "cluster_block_exception",
            "{block}: {answer:?}"
        );
        assert_eq!(answer.status, status, "{block}: {answer:?}");
        assert!(answer.reason().contains(reason), "{block}: {answer:?}");
        let expected = if block.ends_with("read_only_allow_delete") {
            Check::FloodStage
        } else {
            Check::Refused
        };
        assert_eq!(classify(&answer), expected, "{block}: {answer:?}");
        unblock(&http, &base, &format!("{alias}_1"), block).await;
        assert_eq!(
            count(&http, &base, &format!("{alias}_1")).await,
            0,
            "{block}"
        );
    }

    // Closed.
    generation(&http, &base, "closed", json!({})).await;
    post(&http, format!("{base}/closed_1/_close")).await;
    let answer = write_check(&http, &base, "closed").await;
    assert_eq!(answer.kind(), "index_closed_exception", "{answer:?}");
    assert_eq!(classify(&answer), Check::Refused);
    post(
        &http,
        format!("{base}/closed_1/_open?wait_for_active_shards=1"),
    )
    .await;
    assert_eq!(count(&http, &base, "closed_1").await, 0);

    // A deleted generation: its alias goes with it, and `require_alias`
    // creates nothing in its place.
    generation(&http, &base, "deleted", json!({})).await;
    http.delete(format!("{base}/deleted_1"))
        .send()
        .await
        .unwrap();
    let answer = write_check(&http, &base, "deleted").await;
    assert_eq!(answer.kind(), "index_not_found_exception", "{answer:?}");
    assert!(answer.reason().contains("is not an alias"), "{answer:?}");
    assert_eq!(classify(&answer), Check::Refused);
    assert!(!exists(&http, &base, "deleted").await);

    // The alias form: a concrete index named like the alias is not written,
    // nor an alias over two generations with no write index.
    let created = put(
        &http,
        format!("{base}/concrete"),
        json!({"settings": {"number_of_replicas": 0}}),
    )
    .await;
    assert_eq!(created["acknowledged"], true, "{created}");
    let answer = write_check(&http, &base, "concrete").await;
    assert_eq!(answer.kind(), "index_not_found_exception", "{answer:?}");
    assert_eq!(classify(&answer), Check::Refused);
    assert_eq!(count(&http, &base, "concrete").await, 0);
    generation(&http, &base, "two", json!({})).await;
    let second = put(
        &http,
        format!("{base}/two_2"),
        json!({"settings": {"number_of_replicas": 0}, "aliases": {"two": {}}}),
    )
    .await;
    assert_eq!(second["acknowledged"], true, "{second}");
    let answer = write_check(&http, &base, "two").await;
    assert_eq!(answer.kind(), "illegal_argument_exception", "{answer:?}");
    assert!(answer.reason().contains("no write index"), "{answer:?}");
    assert_eq!(classify(&answer), Check::Refused);

    // Pipelines run before the shard's version check: a `fail` refuses, a
    // `drop` answers `noop` with `errors: false`, which is no landing.
    for (name, processor) in [
        (
            "fail",
            json!({"fail": {"message": "refused by the pipeline"}}),
        ),
        ("drop", json!({"drop": {}})),
    ] {
        let answer = put(
            &http,
            format!("{base}/_ingest/pipeline/{name}"),
            json!({ "processors": [processor] }),
        )
        .await;
        assert_eq!(answer["acknowledged"], true, "{answer}");
        for setting in ["index.default_pipeline", "index.final_pipeline"] {
            let alias = format!("{name}_{}", setting.replace('.', "_"));
            generation(&http, &base, &alias, json!({ setting: name })).await;
            let answer = write_check(&http, &base, &alias).await;
            assert_eq!(classify(&answer), Check::Refused, "{alias}: {answer:?}");
            if name == "fail" {
                assert_eq!(answer.kind(), "fail_processor_exception", "{answer:?}");
            } else {
                assert_eq!(answer.result.as_deref(), Some("noop"), "{answer:?}");
                assert_eq!(answer.kind, None, "{answer:?}");
            }
            assert_eq!(
                count(&http, &base, &format!("{alias}_1")).await,
                0,
                "{alias}"
            );
        }
    }

    // A cluster block refuses the whole request, and names itself.
    generation(&http, &base, "cluster", json!({})).await;
    for (block, reason) in [
        (
            "cluster.blocks.read_only",
            "FORBIDDEN/6/cluster read-only (api)",
        ),
        (
            "cluster.blocks.read_only_allow_delete",
            "FORBIDDEN/13/cluster read-only / allow delete (api)",
        ),
    ] {
        cluster(&http, &base, json!({ block: true })).await;
        let answer = write_check(&http, &base, "cluster").await;
        assert_eq!(answer.http, 403, "{block}: {answer:?}");
        assert_eq!(
            answer.kind(),
            "cluster_block_exception",
            "{block}: {answer:?}"
        );
        assert!(answer.reason().contains(reason), "{block}: {answer:?}");
        assert_eq!(classify(&answer), Check::Refused, "{block}");
        cluster(&http, &base, json!({ block: null })).await;
    }
    assert_eq!(count(&http, &base, "cluster_1").await, 0);
}

/// S3: a document written alone through the alias is refused by a deleted,
/// closed or blocked serving generation, writes nothing, and creates no
/// index; a `drop` pipeline answers `noop`, which reads as written.
#[tokio::test]
#[ignore = "requires Docker"]
async fn write_serving_is_refused_by_a_deleted_closed_or_blocked_serving_generation() {
    let (_os, base) = start_opensearch().await;
    let http = reqwest::Client::new();

    generation(&http, &base, "healthy", json!({})).await;
    let answer = write_serving(&http, &base, "healthy").await;
    assert_eq!(answer.result.as_deref(), Some("created"), "{answer:?}");
    assert_eq!(count(&http, &base, "healthy_1").await, 1);

    generation(&http, &base, "deleted", json!({})).await;
    http.delete(format!("{base}/deleted_1"))
        .send()
        .await
        .unwrap();
    let answer = write_serving(&http, &base, "deleted").await;
    assert_eq!(answer.status, 404, "{answer:?}");
    assert_eq!(answer.kind(), "index_not_found_exception", "{answer:?}");
    assert!(
        !exists(&http, &base, "deleted").await,
        "nothing is auto-created"
    );

    generation(&http, &base, "closed", json!({})).await;
    post(&http, format!("{base}/closed_1/_close")).await;
    let answer = write_serving(&http, &base, "closed").await;
    assert_eq!(answer.status, 400, "{answer:?}");
    assert_eq!(answer.kind(), "index_closed_exception", "{answer:?}");
    post(
        &http,
        format!("{base}/closed_1/_open?wait_for_active_shards=1"),
    )
    .await;
    assert_eq!(count(&http, &base, "closed_1").await, 0);

    for (block, status, reason) in INDEX_BLOCKS {
        let alias = block.replace('.', "_");
        generation(&http, &base, &alias, json!({})).await;
        set(&http, &base, &format!("{alias}_1"), json!({ block: true })).await;
        let answer = write_serving(&http, &base, &alias).await;
        assert_eq!(answer.status, status, "{block}: {answer:?}");
        assert_eq!(
            answer.kind(),
            "cluster_block_exception",
            "{block}: {answer:?}"
        );
        assert!(answer.reason().contains(reason), "{block}: {answer:?}");
        unblock(&http, &base, &format!("{alias}_1"), block).await;
        assert_eq!(
            count(&http, &base, &format!("{alias}_1")).await,
            0,
            "{block}"
        );
    }

    let answer = put(
        &http,
        format!("{base}/_ingest/pipeline/drop"),
        json!({"processors": [{"drop": {}}]}),
    )
    .await;
    assert_eq!(answer["acknowledged"], true, "{answer}");
    generation(
        &http,
        &base,
        "dropped",
        json!({"index.default_pipeline": "drop"}),
    )
    .await;
    let answer = write_serving(&http, &base, "dropped").await;
    assert_eq!(answer.http, 200, "{answer:?}");
    assert_eq!(answer.status, 200, "{answer:?}");
    assert_eq!(answer.kind, None, "{answer:?}");
    assert_eq!(answer.result.as_deref(), Some("noop"), "{answer:?}");
    assert_eq!(count(&http, &base, "dropped_1").await, 0);
}

async fn blocks_of(http: &reqwest::Client, base: &str, index: &str) -> Value {
    let body: Value = http
        .get(format!("{base}/{index}/_settings/index.blocks.*"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body[index]["settings"]["index"]["blocks"].clone()
}

async fn wait_for_blocks(
    http: &reqwest::Client,
    base: &str,
    index: &str,
    blocked: bool,
) -> Duration {
    let started = Instant::now();
    loop {
        let blocks = blocks_of(http, base, index).await;
        if (blocks["read_only_allow_delete"] == "true") == blocked {
            return started.elapsed();
        }
        assert!(
            started.elapsed() < Duration::from_secs(90),
            "{index}: blocks {blocks} after {:?}",
            started.elapsed()
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Wait until the cluster's `cluster.blocks.create_index` is `on`.
async fn wait_for_create_block(http: &reqwest::Client, base: &str, on: bool) {
    let started = Instant::now();
    loop {
        let settings: Value = http
            .get(format!("{base}/_cluster/settings?flat_settings=true"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let set = ["persistent", "transient"]
            .iter()
            .any(|scope| settings[scope]["cluster.blocks.create_index"] == "true");
        if set == on {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(90),
            "create-index block not {on}: {settings}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// S4, OpenSearch's half: past the flood stage, the meta index, its shard on
/// the full node, gets the flood-stage block like the generations. A record
/// write and a document delete (a record's delete, a tombstone) are refused
/// with the block's reason; only deleting a whole index goes through. With
/// every node past it, no index can be created either. All of it lifts once
/// the node is under the high watermark again.
#[tokio::test]
#[ignore = "requires Docker"]
async fn past_the_flood_stage_the_meta_index_on_the_full_node_refuses_record_writes() {
    let (_os, base) = start_opensearch().await;
    let http = reqwest::Client::new();
    generation(&http, &base, "users", json!({})).await;
    let meta = put(
        &http,
        format!("{base}/flusso_meta"),
        json!({"settings": {"number_of_replicas": 0}}),
    )
    .await;
    assert_eq!(meta["acknowledged"], true, "{meta}");
    let written = bulk(
        &http,
        &base,
        json!({"index": {"_index": "flusso_meta", "_id": "detach:users"}}),
        json!({"state": "detached"}),
    )
    .await;
    assert_eq!(written.result.as_deref(), Some("created"), "{written:?}");

    let shards: Value = http
        .get(format!(
            "{base}/_cat/shards/flusso_meta,users_1?format=json"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let nodes: Vec<&str> = shards
        .as_array()
        .unwrap()
        .iter()
        .map(|shard| shard["node"].as_str().unwrap())
        .collect();
    assert_eq!(nodes.len(), 2, "{shards}");
    assert_eq!(nodes[0], nodes[1], "one node holds both: {shards}");

    let scratch = put(
        &http,
        format!("{base}/scratch"),
        json!({"settings": {"number_of_replicas": 0}}),
    )
    .await;
    assert_eq!(scratch["acknowledged"], true, "{scratch}");

    // Free space below every watermark, read as often as OpenSearch allows.
    cluster(
        &http,
        &base,
        json!({
            "cluster.info.update.interval": "10s",
            "cluster.routing.allocation.disk.watermark.low": "1000tb",
            "cluster.routing.allocation.disk.watermark.high": "999tb",
            "cluster.routing.allocation.disk.watermark.flood_stage": "998tb",
        }),
    )
    .await;
    wait_for_blocks(&http, &base, "users_1", true).await;
    wait_for_blocks(&http, &base, "flusso_meta", true).await;

    let refused = bulk(
        &http,
        &base,
        json!({"index": {"_index": "flusso_meta", "_id": "detach:users"}}),
        json!({"state": "rebuilding"}),
    )
    .await;
    assert_eq!(refused.status, 429, "{refused:?}");
    assert_eq!(refused.kind(), "cluster_block_exception", "{refused:?}");
    assert!(
        refused.reason().contains(
            "TOO_MANY_REQUESTS/12/disk usage exceeded flood-stage watermark, index has \
             read-only-allow-delete block"
        ),
        "{refused:?}"
    );
    let probe = write_check(&http, &base, "users").await;
    assert_eq!(classify(&probe), Check::FloodStage, "{probe:?}");
    // `read_only_allow_delete` allows deleting an index, not a document: a
    // record's delete and a tombstone are refused with the same block.
    let deleted = http
        .delete(format!("{base}/flusso_meta/_doc/detach:users"))
        .send()
        .await
        .unwrap();
    let status = deleted.status().as_u16();
    let body: Value = deleted.json().await.unwrap();
    let answer = Answer::read(status, &body);
    assert_eq!(answer.status, 429, "{body}");
    assert_eq!(classify(&answer), Check::FloodStage, "{body}");
    let response = http
        .post(format!("{base}/_bulk"))
        .header("content-type", "application/x-ndjson")
        .body(format!(
            "{}\n",
            json!({"delete": {"_index": "users", "_id": "7"}})
        ))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap();
    let tombstone = Answer::read(
        status,
        &json!({"items": [{"index": body["items"][0]["delete"].clone()}]}),
    );
    assert_eq!(tombstone.status, 429, "{body}");
    assert_eq!(classify(&tombstone), Check::FloodStage, "{body}");
    // Every node past the flood stage, OpenSearch also blocks creating any
    // index (a cluster setting it sets on a later round of its disk monitor):
    // a rebuild's generation included.
    wait_for_create_block(&http, &base, true).await;
    let created: Value = http
        .put(format!("{base}/users_2"))
        .json(&json!({"settings": {"number_of_replicas": 0}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        created["error"]["type"], "index_create_block_exception",
        "{created}"
    );
    assert!(
        created["error"]["reason"]
            .as_str()
            .unwrap()
            .contains("FORBIDDEN/10/cluster create-index blocked"),
        "{created}"
    );
    wait_for_blocks(&http, &base, "scratch", true).await;
    let dropped = http.delete(format!("{base}/scratch")).send().await.unwrap();
    assert!(
        dropped.status().is_success(),
        "an index delete goes through: {dropped:?}"
    );

    cluster(
        &http,
        &base,
        json!({
            "cluster.routing.allocation.disk.watermark.low": null,
            "cluster.routing.allocation.disk.watermark.high": null,
            "cluster.routing.allocation.disk.watermark.flood_stage": null,
        }),
    )
    .await;
    wait_for_blocks(&http, &base, "flusso_meta", false).await;
    wait_for_blocks(&http, &base, "users_1", false).await;
    wait_for_create_block(&http, &base, false).await;
    let started = Instant::now();
    loop {
        let created = put(
            &http,
            format!("{base}/users_2"),
            json!({"settings": {"number_of_replicas": 0}}),
        )
        .await;
        if created["acknowledged"] == true {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(90), "{created}");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let written = bulk(
        &http,
        &base,
        json!({"index": {"_index": "flusso_meta", "_id": "detach:users"}}),
        json!({"state": "detached"}),
    )
    .await;
    // The record its delete could not remove is there still.
    assert_eq!(written.result.as_deref(), Some("updated"), "{written:?}");
}
