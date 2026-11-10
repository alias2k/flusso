//! Real-service integration test for the alias-over-generations reindex.
//!
//! Starts a genuine OpenSearch 2 cluster in a container and drives the sink
//! through a full reindex lifecycle across **two sink instances** (mirroring the
//! pipeline restart a reindex triggers — each run builds a fresh sink): an
//! initial seed into generation 1, then a staged reindex that rebuilds into
//! generation 2 from scratch. It asserts the behavior the unit tests can't reach:
//! the stable `{logical}_{hash}` alias swaps atomically onto the new generation,
//! the old generation is dropped, reads stay on the old generation until the
//! swap, and documents only present in the old generation are gone after. A
//! second test covers the seed marker outliving its generation index (issue
//! #120): `ensure_index` must retract the marker and report the recreated,
//! empty generation unseeded rather than serve it. The rest cover the generation
//! lifecycle while running: a generation deleted mid-run fails the flush rather
//! than being auto-created, and is rebuilt by the next `ensure_index` (#123); a
//! second reindex during a rebuild stages nothing, and `mark_seeded` sweeps
//! orphan generations (#121). The last covers `index_settings` (#168): the
//! sink-wide and per-index settings land on the created generation, the
//! per-index one winning, and a raised `total_fields.limit` admits a mapping
//! past OpenSearch's 1000-field default.
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

use kernel::{
    ContentHash, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    ResolvedField,
};
use kernel::{Envelope, Position};
use reqwest::StatusCode;
use serde_json::Value;
use sink::Sink;
use sink_opensearch::OpensearchSink;
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
    let name = kernel::SinkName::try_new("test").unwrap();
    OpensearchSink::from_config(&name, &sink_config(base_url)).unwrap()
}

/// The test sink's configuration: replicas off for a single-node cluster.
fn sink_config(base_url: &str) -> sink_opensearch::OpensearchConfig {
    sink_opensearch::OpensearchConfig {
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
        refresh_interval: "10s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    }
}

/// A fresh sink that prepends `prefix` to every name it owns — the deployment
/// index prefix, as a `flusso run --index-prefix` would.
fn sink_with_prefix(base_url: &str, prefix: &str) -> OpensearchSink {
    sink(base_url).with_index_prefix(prefix)
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
            map_values: None,
            decimal: false,
            enum_order: None,
            subfields: true,
            indexed: true,
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
    map.insert("id".to_owned(), GenericValue::BigInt(i as i64));
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
        run1.apply(&upsert(&index, &i.to_string(), &document(i)))
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
        run2.apply(&upsert(&index, &i.to_string(), &document(i)))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn index_prefix_isolates_two_deployments_and_scopes_reindex() {
    // Two deployments (`dev_`, `staging_`) with the *same* schema (so the same
    // hash, the colliding case) share one cluster. The prefix must keep their
    // indexes, convenience aliases, meta state, and reindex blast radius fully
    // independent.
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();

    let names = |prefix: &str| {
        let hash_alias = format!("{prefix}{LOGICAL}_{}", mapping.hash);
        let gen1 = format!("{hash_alias}_1");
        let gen2 = format!("{hash_alias}_2");
        let convenience = format!("{prefix}{LOGICAL}");
        let meta = format!("{prefix}flusso_meta");
        (hash_alias, gen1, gen2, convenience, meta)
    };
    let (dev_alias, dev_gen1, dev_gen2, dev_conv, dev_meta) = names("dev_");
    let (stg_alias, stg_gen1, _stg_gen2, stg_conv, stg_meta) = names("staging_");

    // ── Seed dev_ (ids 1,2,3) and staging_ (ids 1,2) on the same cluster ─────
    let dev = sink_with_prefix(&base, "dev_");
    dev.ensure_index(&mapping).await.unwrap();
    for i in [1, 2, 3] {
        dev.apply(&upsert(&index, &i.to_string(), &document(i)))
            .await
            .unwrap();
    }
    dev.flush(true).await.unwrap();
    dev.mark_seeded(&index).await.unwrap();

    let staging = sink_with_prefix(&base, "staging_");
    staging.ensure_index(&mapping).await.unwrap();
    for i in [1, 2] {
        staging
            .apply(&upsert(&index, &i.to_string(), &document(i)))
            .await
            .unwrap();
    }
    staging.flush(true).await.unwrap();
    staging.mark_seeded(&index).await.unwrap();

    // ── Names are prefixed and the two deployments don't collide ─────────────
    assert_eq!(
        alias_targets(&client, &base, &dev_alias).await,
        vec![dev_gen1.clone()],
        "dev hash alias points at its own generation 1",
    );
    assert_eq!(
        alias_targets(&client, &base, &stg_alias).await,
        vec![stg_gen1.clone()],
        "staging hash alias points at its own generation 1",
    );
    assert_eq!(
        alias_targets(&client, &base, &dev_conv).await,
        vec![dev_gen1.clone()],
        "the convenience alias is prefixed too (dev_users -> dev_users_1_1)",
    );
    assert_eq!(
        alias_targets(&client, &base, &stg_conv).await,
        vec![stg_gen1.clone()],
    );
    assert!(
        index_exists(&client, &base, &dev_meta).await
            && index_exists(&client, &base, &stg_meta).await,
        "each deployment keeps its own prefixed meta index",
    );

    // Data isolation: id 3 lives in dev only; staging never received it.
    assert!(doc_exists(&client, &base, &dev_alias, "3").await);
    assert!(!doc_exists(&client, &base, &stg_alias, "3").await);

    // A fresh dev sink (a restart) re-announces the index, then reads dev's
    // prefixed meta and reports seeded — the slot/seed state round-trips through
    // the prefixed `dev_flusso_meta`.
    let dev_restart = sink_with_prefix(&base, "dev_");
    dev_restart.ensure_index(&mapping).await.unwrap();
    assert!(
        dev_restart.is_seeded(&index).await.unwrap(),
        "seed state is read back through the prefixed meta index",
    );

    // ── Reindex dev_ → generation 2; staging_ must be untouched ──────────────
    sink_with_prefix(&base, "dev_")
        .reindex(&mapping)
        .await
        .unwrap();
    let dev2 = sink_with_prefix(&base, "dev_");
    dev2.ensure_index(&mapping).await.unwrap();
    for i in [1, 2] {
        dev2.apply(&upsert(&index, &i.to_string(), &document(i)))
            .await
            .unwrap();
    }
    dev2.flush(true).await.unwrap();
    dev2.mark_seeded(&index).await.unwrap();

    assert_eq!(
        alias_targets(&client, &base, &dev_alias).await,
        vec![dev_gen2.clone()],
        "dev swapped onto its generation 2",
    );
    assert!(
        !index_exists(&client, &base, &dev_gen1).await,
        "dev's superseded generation 1 was dropped",
    );
    assert!(
        index_exists(&client, &base, &stg_gen1).await
            && alias_targets(&client, &base, &stg_alias).await == vec![stg_gen1.clone()],
        "the dev reindex left staging's generation and alias entirely untouched",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn deleted_generation_is_recreated_and_reported_unseeded() {
    // The seed marker says generation 1 is seeded, but an operator deleted the
    // generation index between runs. The next run must not trust the marker: it
    // recreates the (empty) generation, points the aliases at it, and reports
    // it unseeded so the engine backfills it (issue #120, case A).
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");

    // ── Run 1: seed generation 1 ─────────────────────────────────────────────
    let run1 = sink(&base);
    run1.ensure_index(&mapping).await.unwrap();
    run1.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    run1.flush(true).await.unwrap();
    run1.mark_seeded(&index).await.unwrap();
    assert!(run1.is_seeded(&index).await.unwrap());

    // ── The operator deletes the generation (its aliases go with it) ─────────
    let status = client
        .delete(format!("{base}/{gen1}"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "DELETE {gen1} returned {status}");
    assert!(alias_targets(&client, &base, &hash_alias).await.is_empty());

    // ── Run 2: the marker is contradicted; the sink must side with the index ─
    let run2 = sink(&base);
    run2.ensure_index(&mapping).await.unwrap();
    assert!(
        !run2.is_seeded(&index).await.unwrap(),
        "a seeded marker whose generation is gone must read back as unseeded"
    );
    assert!(
        index_exists(&client, &base, &gen1).await,
        "the generation is recreated under its previous number"
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()],
        "nothing else serves, so the aliases point at the recreated generation"
    );
    assert!(
        !doc_exists(&client, &base, &hash_alias, "1").await,
        "the recreated generation is empty until the backfill refills it"
    );

    // ── The engine's backfill then reseeds and marks it ──────────────────────
    run2.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    run2.flush(true).await.unwrap();
    run2.mark_seeded(&index).await.unwrap();
    assert!(run2.is_seeded(&index).await.unwrap());
    assert!(doc_exists(&client, &base, &hash_alias, "1").await);

    // ── Run 3: a plain restart trusts the (now consistent) marker again ──────
    let run3 = sink(&base);
    run3.ensure_index(&mapping).await.unwrap();
    assert!(run3.is_seeded(&index).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn generation_deleted_while_running_fails_the_flush_and_is_rebuilt() {
    // Issue #123: the active generation is deleted while the sink runs. The
    // next write must not let OpenSearch auto-create a dynamic-mapped index; it
    // must fail the flush, and the restart's `ensure_index` must rebuild it.
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");

    let sink = sink(&base);
    sink.ensure_index(&mapping).await.unwrap();
    sink.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();

    // ── A seeded write goes through the alias ────────────────────────────────
    sink.apply(&upsert(&index, "2", &document(2)))
        .await
        .unwrap();
    sink.flush(true).await.unwrap();
    assert!(doc_exists(&client, &base, &gen1, "2").await);

    // ── Delete the live generation (its aliases go with it) ──────────────────
    let status = client
        .delete(format!("{base}/{gen1}"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "DELETE {gen1} returned {status}");

    sink.apply(&upsert(&index, "3", &document(3)))
        .await
        .unwrap();
    let err = sink
        .flush(true)
        .await
        .expect_err("a write to a deleted generation must fail the flush");
    assert!(err.to_string().contains("missing"), "{err}");
    assert!(
        !index_exists(&client, &base, &gen1).await
            && !index_exists(&client, &base, &hash_alias).await,
        "nothing was auto-created under the generation or the alias name",
    );

    // ── The restart re-stages: ensure_index recreates it, unseeded ───────────
    sink.ensure_index(&mapping).await.unwrap();
    assert!(
        !sink.is_seeded(&index).await.unwrap(),
        "the recreated generation reports unseeded so the engine backfills it",
    );
    let mapping_body: Value = client
        .get(format!("{base}/{gen1}/_mapping"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        mapping_body[&gen1]["mappings"]["dynamic"], "strict",
        "the generation is recreated with flusso's mapping, not a dynamic one",
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()],
    );

    // ── The backfill plus the redelivered batch reseed it ────────────────────
    for i in [1, 2, 3] {
        sink.apply(&upsert(&index, &i.to_string(), &document(i)))
            .await
            .unwrap();
    }
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();
    assert!(sink.is_seeded(&index).await.unwrap());
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()],
    );
    assert_eq!(
        alias_targets(&client, &base, LOGICAL).await,
        vec![gen1.clone()],
    );
    for id in ["1", "2", "3"] {
        assert!(doc_exists(&client, &base, &hash_alias, id).await, "id {id}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_second_reindex_while_rebuilding_stages_nothing() {
    // Issue #121: a reindex while a rebuild is already pending must not stage
    // another generation and orphan the one being built.
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");
    let gen2 = format!("{hash_alias}_2");

    let sink = sink(&base);
    sink.reindex(&mapping).await.unwrap();
    sink.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen1.clone()],
        "a reindex before the first seed stages nothing",
    );
    sink.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();

    // Two reindex requests while the first rebuild is still unseeded.
    sink.reindex(&mapping).await.unwrap();
    sink.ensure_index(&mapping).await.unwrap();
    sink.reindex(&mapping).await.unwrap();
    sink.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen1.clone(), gen2.clone()],
        "the second reindex left the in-flight generation 2 alone",
    );
    assert!(!sink.is_seeded(&index).await.unwrap());

    sink.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen2.clone()],
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen2.clone()],
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn mark_seeded_sweeps_an_orphan_generation() {
    // The orphan an older flusso left behind (issue #121): generation 2 was
    // staged, then a second reindex flipped the meta to generation 3, so 2
    // never held the alias. Sweeping alias holders alone would keep it forever.
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");
    let gen2 = format!("{hash_alias}_2");
    let gen3 = format!("{hash_alias}_3");

    let run1 = sink(&base);
    run1.ensure_index(&mapping).await.unwrap();
    run1.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    run1.flush(true).await.unwrap();
    run1.mark_seeded(&index).await.unwrap();

    run1.reindex(&mapping).await.unwrap();
    run1.ensure_index(&mapping).await.unwrap();
    let status = client
        .put(format!("{base}/flusso_meta/_doc/{hash_alias}"))
        .json(&serde_json::json!({ "active_generation": 3, "seeded": false }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "meta put returned {status}");

    let run2 = sink(&base);
    run2.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen1.clone(), gen2.clone(), gen3.clone()],
    );
    run2.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    run2.flush(true).await.unwrap();
    run2.mark_seeded(&index).await.unwrap();

    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen3.clone()],
        "both the superseded generation 1 and the orphan generation 2 are dropped",
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen3.clone()],
    );
}

#[tokio::test]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn index_settings_land_on_the_created_generation_per_index_winning() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let settings =
        |value: Value| -> sink_opensearch::IndexSettings { serde_json::from_value(value).unwrap() };
    let wide = IndexName::try_new("wide").unwrap();
    let field = |name: String| ResolvedField {
        name: FieldName::try_new(&name).unwrap(),
        mapping: Mapping {
            mapping_type: MappingType::Long,
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
    };
    let wide_mapping = IndexMapping {
        index: wide.clone(),
        hash: ContentHash::new(2),
        fields: (0..1100).map(|i| field(format!("f{i}"))).collect(),
    };

    let mut config = sink_config(&base);
    config.index_settings = settings(serde_json::json!({
        "mapping.total_fields.limit": 1500,
        "index": { "max_result_window": 20000 },
    }));
    let sink = OpensearchSink::from_config(&kernel::SinkName::try_new("test").unwrap(), &config)
        .unwrap()
        .with_index_settings([(
            wide.clone(),
            settings(serde_json::json!({ "mapping.total_fields.limit": 2000 })),
        )]);
    sink.ensure_index(&mapping()).await.unwrap();
    sink.ensure_index(&wide_mapping)
        .await
        .expect("1100 fields fit under the raised per-index limit");

    let users = flat_settings(&client, &base, &format!("{LOGICAL}_{}_1", mapping().hash)).await;
    assert_eq!(users["index.mapping.total_fields.limit"], "1500");
    assert_eq!(users["index.max_result_window"], "20000");
    assert_eq!(users["index.refresh_interval"], "-1", "flusso's key wins");
    let wide = flat_settings(&client, &base, &format!("wide_{}_1", wide_mapping.hash)).await;
    assert_eq!(wide["index.mapping.total_fields.limit"], "2000");
    assert_eq!(wide["index.max_result_window"], "20000");
}

/// The settings of `index` as flat dotted keys.
async fn flat_settings(client: &reqwest::Client, base: &str, index: &str) -> Value {
    let body: Value = client
        .get(format!("{base}/{index}/_settings?flat_settings=true"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body[index]["settings"].clone()
}

/// The concrete generation indexes behind `hash_alias` (sorted).
async fn generations(client: &reqwest::Client, base: &str, hash_alias: &str) -> Vec<String> {
    let rows: Value = client
        .get(format!(
            "{base}/_cat/indices/{hash_alias}_*?h=index&format=json"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut names: Vec<String> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["index"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

/// A live-shaped upsert envelope for `doc` under `id`, positioned arbitrarily:
/// the sink only reads index, id, op, and document.
fn upsert(index: &IndexName, id: &str, doc: &GenericValue) -> Envelope {
    Envelope::upsert(
        index.clone(),
        id,
        doc.clone(),
        Some(Position(0)),
        chrono::Utc::now(),
    )
}
