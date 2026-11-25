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
//! past OpenSearch's 1000-field default. And one covers the visibility a flush
//! reports (issue #136): searchable at once when caught up, within
//! `refresh_interval` otherwise, and deferred to the next caught-up flush,
//! which then really makes it searchable, when automatic refresh is off.
//! Staging many indexes at once (issue #398) runs on a fresh cluster, where
//! every index's first meta write races to create the shared meta index, then
//! on a restart, then as concurrent reindexes.
//! And a reindex past an old generation an operator blocked or closed (issue
//! #479): one test per state of the old generation (`index.blocks.write`,
//! `index.blocks.read_only`, `index.blocks.read_only_allow_delete`, closed),
//! each asserting what `mark_seeded` promises: the aliases serve the new
//! generation, every other generation is deleted, the index reads seeded, and
//! a restart stages nothing more.
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

#[path = "support/buffered.rs"]
mod buffered;
use buffered::Buffered;
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
fn sink(base_url: &str) -> Buffered {
    let name = kernel::SinkName::try_new("test").unwrap();
    Buffered::new(OpensearchSink::from_config(&name, &sink_config(base_url)).unwrap())
}

/// The test sink's configuration: replicas off for a single-node cluster.
fn sink_config(base_url: &str) -> sink_opensearch::OpensearchConfig {
    sink_opensearch::OpensearchConfig {
        url: kernel::Secret::Value(base_url.to_owned()),
        username: None,
        password: None,
        tls_verify: false,
        ca_file: None,
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
fn sink_with_prefix(base_url: &str, prefix: &str) -> Buffered {
    let name = kernel::SinkName::try_new("test").unwrap();
    Buffered::new(
        OpensearchSink::from_config(&name, &sink_config(base_url))
            .unwrap()
            .with_index_prefix(prefix),
    )
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
async fn generation_deleted_while_running_is_reported_missing_and_is_rebuilt() {
    // Issue #123: the active generation is deleted while the sink runs. The
    // next write must not let OpenSearch auto-create a dynamic-mapped index; it
    // reports the index missing (ADR 0012 stage 9: the engine stages that lane
    // again), and `ensure_index` rebuilds it.
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
    let report = sink
        .flush(true)
        .await
        .expect("a missing target is reported for its index, not a failed write");
    assert_eq!(
        report.missing.iter().map(|m| &m.index).collect::<Vec<_>>(),
        [&index],
        "{report:?}"
    );
    assert!(report.rejected.is_empty() && report.blocked.is_empty());
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
async fn a_generation_deleted_mid_backfill_is_staged_as_created_without_its_checkpoint() {
    // Issue #428, auto-create forbidden for flusso's generations: the write
    // after the deletion finds the target missing, and staging again creates
    // it under another identity, with no checkpoint.
    const ROWS: usize = 20;
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let status = client
        .put(format!("{base}/_cluster/settings"))
        .json(&serde_json::json!({
            "persistent": { "action.auto_create_index": format!("+flusso_meta,-{LOGICAL}_*,+*") }
        }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(
        status.is_success(),
        "setting auto_create_index returned {status}"
    );
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");

    let sink = sink(&base);
    let first = sink.ensure_index(&mapping).await.unwrap();
    assert!(first.created, "the first generation is new");
    assert!(first.target.is_some(), "a generation is named by its uuid");

    // ── The backfill lands rows 1..=10 and records how far it got ────────────
    hold_rows(&sink, &index, 1..=10).await;
    sink.flush(false).await.unwrap();
    sink.record_walked(&index, "10").await.unwrap();
    assert_eq!(
        sink.seeding(&index).await.unwrap(),
        sink::Seeding::Unseeded {
            walked: Some("10".to_owned())
        }
    );

    // ── An operator deletes the generation; the next page finds it missing ───
    let status = client
        .delete(format!("{base}/{gen1}"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "DELETE {gen1} returned {status}");
    hold_rows(&sink, &index, 11..=12).await;
    let report = sink.flush(false).await.unwrap();
    assert_eq!(
        report.missing.iter().map(|m| &m.index).collect::<Vec<_>>(),
        [&index],
        "{report:?}"
    );

    // ── Staged again: a new, empty generation, its checkpoint gone ───────────
    let again = sink.ensure_index(&mapping).await.unwrap();
    assert!(again.created, "{again:?}");
    assert_ne!(
        again.target, first.target,
        "the recreated generation must not be named as the deleted one"
    );
    assert_eq!(
        sink.seeding(&index).await.unwrap(),
        sink::Seeding::Unseeded { walked: None },
        "the deleted generation's checkpoint must not resume the fresh one"
    );

    // ── The old backfill's remaining pages land, and are not the table ───────
    hold_rows(&sink, &index, 11..=ROWS).await;
    sink.flush(true).await.unwrap();
    assert!(!sink.is_seeded(&index).await.unwrap());
    assert_eq!(search_count(&client, &base, &hash_alias).await, 10);
    assert!(!doc_exists(&client, &base, &hash_alias, "1").await);

    // ── The fresh backfill writes every row; only then is it seeded ──────────
    hold_rows(&sink, &index, 1..=ROWS).await;
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();
    assert!(sink.is_seeded(&index).await.unwrap());
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()]
    );
    assert_eq!(
        search_count(&client, &base, &hash_alias).await,
        ROWS as u64,
        "the seeded generation holds the table"
    );
    for id in 1..=ROWS {
        assert!(
            doc_exists(&client, &base, &hash_alias, &id.to_string()).await,
            "id {id}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn an_auto_created_generation_is_never_seeded_and_is_replaced_by_a_strict_one() {
    // Issue #428 with OpenSearch's default `action.auto_create_index`: the
    // unseeded generation is deleted mid-backfill and the next write quietly
    // recreates it, dynamically mapped, holding only what came after. It is
    // not the generation flusso made: it is never seeded, and staging (here a
    // fresh sink, as after a restart) replaces it with a strict, empty one
    // with no checkpoint, which only a full backfill seeds.
    const ROWS: usize = 20;
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");

    let sink = sink(&base);
    let first = sink.ensure_index(&mapping).await.unwrap();
    hold_rows(&sink, &index, 1..=10).await;
    sink.flush(false).await.unwrap();
    sink.record_walked(&index, "10").await.unwrap();

    let status = client
        .delete(format!("{base}/{gen1}"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "DELETE {gen1} returned {status}");
    hold_rows(&sink, &index, 11..=ROWS).await;
    let report = sink.flush(true).await.unwrap();
    assert!(
        report.missing.is_empty(),
        "auto-create on: the write recreated the generation ({report:?})"
    );
    assert_ne!(dynamic_of(&client, &base, &gen1).await, "strict");

    let refused = sink.mark_seeded(&index).await;
    assert!(
        matches!(refused, Err(sink::SinkError::TargetChanged(_))),
        "an auto-created generation must never be seeded: {refused:?}"
    );
    let message = refused.unwrap_err().to_string();
    assert_eq!(
        message,
        format!(
            "the index's target changed: {gen1} is not the generation staged for {LOGICAL} \
             (deleted, or recreated by a write under action.auto_create_index)"
        )
    );
    assert!(!sink.is_seeded(&index).await.unwrap());

    let restarted = self::sink(&base);
    let replaced = restarted.ensure_index(&mapping).await.unwrap();
    assert!(replaced.created, "{replaced:?}");
    assert_ne!(replaced.target, first.target);
    assert_eq!(dynamic_of(&client, &base, &gen1).await, "strict");
    assert_eq!(
        restarted.seeding(&index).await.unwrap(),
        sink::Seeding::Unseeded { walked: None },
        "the impostor's checkpoint must not resume the new generation"
    );
    assert_eq!(search_count(&client, &base, &gen1).await, 0);
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()]
    );

    hold_rows(&restarted, &index, 1..=ROWS).await;
    restarted.flush(true).await.unwrap();
    restarted.mark_seeded(&index).await.unwrap();
    assert!(restarted.is_seeded(&index).await.unwrap());
    assert_eq!(dynamic_of(&client, &base, &gen1).await, "strict");
    assert_eq!(search_count(&client, &base, &hash_alias).await, ROWS as u64);
}

/// The `dynamic` setting of `index`'s mapping, as OpenSearch reports it.
async fn dynamic_of(client: &reqwest::Client, base: &str, index: &str) -> Value {
    let body: Value = client
        .get(format!("{base}/{index}/_mapping"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body[index]["mappings"]["dynamic"].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_seeded_generation_recreated_by_a_staging_that_then_failed_is_never_served_seeded() {
    // A seeded generation is deleted; staging recreates it empty, then fails
    // to write the meta doc (here the meta index refuses writes for a while).
    // The meta doc must never still say "seeded" beside the empty generation:
    // the next staging reports it unseeded, so the engine backfills it.
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");

    let run1 = sink(&base);
    let first = run1.ensure_index(&mapping).await.unwrap();
    hold_rows(&run1, &index, 1..=5).await;
    run1.flush(true).await.unwrap();
    run1.mark_seeded(&index).await.unwrap();
    let status = client
        .delete(format!("{base}/{gen1}"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "DELETE {gen1} returned {status}");

    let block_meta = |blocked: bool| {
        let client = client.clone();
        let base = base.clone();
        async move {
            let status = client
                .put(format!("{base}/flusso_meta/_settings"))
                .json(&serde_json::json!({ "index.blocks.write": blocked }))
                .send()
                .await
                .unwrap()
                .status();
            assert!(
                status.is_success(),
                "blocking the meta index returned {status}"
            );
        }
    };
    block_meta(true).await;
    assert!(
        sink(&base).ensure_index(&mapping).await.is_err(),
        "the meta write is refused, so staging fails"
    );
    block_meta(false).await;

    let run2 = sink(&base);
    let staged = run2.ensure_index(&mapping).await.unwrap();
    assert_ne!(staged.target, first.target);
    assert!(
        !run2.is_seeded(&index).await.unwrap(),
        "an empty generation must not be served as seeded"
    );
    assert_eq!(
        run2.seeding(&index).await.unwrap(),
        sink::Seeding::Unseeded { walked: None }
    );
    assert_eq!(search_count(&client, &base, &hash_alias).await, 0);

    // What a failure after the create leaves now: an unseeded doc naming no
    // uuid beside the new generation. Staging adopts it, still unseeded.
    let status = client
        .put(format!("{base}/flusso_meta/_doc/{hash_alias}"))
        .json(&serde_json::json!({ "active_generation": 1, "seeded": false }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(
        status.is_success(),
        "rewriting the meta doc returned {status}"
    );
    let run3 = sink(&base);
    let adopted = run3.ensure_index(&mapping).await.unwrap();
    assert_eq!(adopted.target, staged.target);
    assert!(!run3.is_seeded(&index).await.unwrap());
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
    let sink = Buffered::new(
        OpensearchSink::from_config(&kernel::SinkName::try_new("test").unwrap(), &config)
            .unwrap()
            .with_index_settings([(
                wide.clone(),
                settings(serde_json::json!({ "mapping.total_fields.limit": 2000 })),
            )]),
    );
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

/// Hold an upsert of each row in `rows` for the next flush.
async fn hold_rows(sink: &Buffered, index: &IndexName, rows: std::ops::RangeInclusive<usize>) {
    for i in rows {
        sink.apply(&upsert(index, &i.to_string(), &document(i)))
            .await
            .unwrap();
    }
}

/// How many documents a search through `target` matches right now.
async fn search_count(client: &reqwest::Client, base: &str, target: &str) -> u64 {
    let body: Value = client
        .get(format!("{base}/{target}/_count"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["count"].as_u64().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_flush_reports_when_its_documents_become_searchable() {
    use sink::Visibility;

    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);

    // ── A refresh interval: an unforced flush is searchable within it ────────
    let periodic = sink(&base);
    periodic.ensure_index(&mapping).await.unwrap();
    periodic.mark_seeded(&index).await.unwrap();
    periodic
        .apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    assert_eq!(
        periodic.flush(false).await.unwrap().visibility,
        Visibility::Within(Duration::from_secs(10)),
    );
    assert_eq!(
        periodic.flush(false).await.unwrap().visibility,
        Visibility::Immediate,
        "a flush that wrote nothing has nothing to wait for",
    );
    periodic
        .apply(&upsert(&index, "2", &document(2)))
        .await
        .unwrap();
    assert_eq!(
        periodic.flush(true).await.unwrap().visibility,
        Visibility::Immediate,
    );

    // ── Automatic refresh off: deferred until a caught-up flush ──────────────
    let manual = Buffered::new(
        OpensearchSink::from_config(
            &kernel::SinkName::try_new("test").unwrap(),
            &sink_opensearch::OpensearchConfig {
                refresh_interval: "-1".to_owned(),
                ..sink_config(&base)
            },
        )
        .unwrap(),
    );
    manual.ensure_index(&mapping).await.unwrap();
    manual.mark_seeded(&index).await.unwrap();
    client
        .post(format!("{base}/{hash_alias}/_refresh"))
        .send()
        .await
        .unwrap();
    let before = search_count(&client, &base, &hash_alias).await;

    manual
        .apply(&upsert(&index, "3", &document(3)))
        .await
        .unwrap();
    assert_eq!(
        manual.flush(false).await.unwrap().visibility,
        Visibility::Deferred,
    );
    assert_eq!(
        manual.flush(false).await.unwrap().visibility,
        Visibility::Immediate,
        "an empty write that isn't caught up reports on itself alone: it wrote nothing",
    );
    assert!(
        doc_exists(&client, &base, &hash_alias, "3").await,
        "applied: a get by id sees it",
    );
    assert_eq!(
        search_count(&client, &base, &hash_alias).await,
        before,
        "not searchable: nothing refreshed it",
    );

    assert_eq!(
        manual.flush(true).await.unwrap().visibility,
        Visibility::Immediate,
        "the caught-up flush refreshes what the earlier one left",
    );
    assert_eq!(search_count(&client, &base, &hash_alias).await, before + 1);

    // ── A restart forgets what was left unrefreshed, so staging refreshes ────
    manual
        .apply(&upsert(&index, "4", &document(4)))
        .await
        .unwrap();
    assert_eq!(
        manual.flush(false).await.unwrap().visibility,
        Visibility::Deferred,
    );
    assert_eq!(search_count(&client, &base, &hash_alias).await, before + 1);
    let restarted = Buffered::new(
        OpensearchSink::from_config(
            &kernel::SinkName::try_new("test").unwrap(),
            &sink_opensearch::OpensearchConfig {
                refresh_interval: "-1".to_owned(),
                ..sink_config(&base)
            },
        )
        .unwrap(),
    );
    restarted.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        search_count(&client, &base, &hash_alias).await,
        before + 2,
        "a new run starts with everything written so far searchable",
    );
}

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
        Some(Position::new(0)),
        chrono::Utc::now(),
    )
}

/// How many indexes the concurrent staging test stages at once: every one at
/// the same time, more than the sink engine's bound.
const AT_ONCE: usize = 24;

/// `AT_ONCE` mappings of the shape [`mapping`] gives, each its own logical index.
fn many_mappings() -> Vec<IndexMapping> {
    (0..AT_ONCE)
        .map(|i| IndexMapping {
            index: IndexName::try_new(format!("many_{i:02}")).unwrap(),
            ..mapping()
        })
        .collect()
}

/// Run `step` on `sink` for every mapping at once and wait for all of them.
async fn all_at_once<F, Fut>(sink: &std::sync::Arc<Buffered>, step: F)
where
    F: Fn(std::sync::Arc<Buffered>, IndexMapping) -> Fut,
    Fut: std::future::Future<Output = sink::Result<()>> + Send + 'static,
{
    let mut tasks = tokio::task::JoinSet::new();
    for mapping in many_mappings() {
        tasks.spawn(step(std::sync::Arc::clone(sink), mapping));
    }
    while let Some(joined) = tasks.join_next().await {
        joined.unwrap().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn many_indexes_staged_at_once_each_get_their_own_generation() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let hash = mapping().hash;

    // A fresh cluster: no meta index yet, so the first meta writes race to
    // create it.
    let first = std::sync::Arc::new(sink(&base));
    all_at_once(&first, |sink, mapping| async move {
        sink.ensure_index(&mapping).await?;
        assert!(!sink.is_seeded(&mapping.index).await?);
        Ok(())
    })
    .await;
    for mapping in many_mappings() {
        let hash_alias = format!("{}_{hash}", mapping.index);
        assert_eq!(
            alias_targets(&client, &base, &hash_alias).await,
            vec![format!("{hash_alias}_1")],
        );
        first.mark_seeded(&mapping.index).await.unwrap();
    }

    // A restart: every index found seeded, nothing recreated.
    let restarted = std::sync::Arc::new(sink(&base));
    all_at_once(&restarted, |sink, mapping| async move {
        sink.ensure_index(&mapping).await?;
        assert!(sink.is_seeded(&mapping.index).await?);
        Ok(())
    })
    .await;

    // Rebuilds staged at once, as a `Fresh` source stages them: each index
    // gets its own next generation, its reads left on the first.
    let rebuilt = std::sync::Arc::new(sink(&base));
    all_at_once(&rebuilt, |sink, mapping| async move {
        sink.ensure_index(&mapping).await?;
        sink.reindex(&mapping).await?;
        sink.ensure_index(&mapping).await?;
        assert!(!sink.is_seeded(&mapping.index).await?);
        Ok(())
    })
    .await;
    for mapping in many_mappings() {
        let hash_alias = format!("{}_{hash}", mapping.index);
        assert_eq!(
            generations(&client, &base, &hash_alias).await,
            vec![format!("{hash_alias}_1"), format!("{hash_alias}_2")],
        );
        assert_eq!(
            alias_targets(&client, &base, &hash_alias).await,
            vec![format!("{hash_alias}_1")],
        );
        rebuilt.mark_seeded(&mapping.index).await.unwrap();
        assert_eq!(
            alias_targets(&client, &base, &hash_alias).await,
            vec![format!("{hash_alias}_2")],
        );
    }
}

/// What the old generation of [`a_seed_past_an_old_generation`] is left in
/// before the reindex's seed: an operator's block, or closed.
#[derive(Debug, Clone, Copy)]
enum OldGeneration {
    WriteBlocked,
    ReadOnly,
    ReadOnlyAllowDelete,
    Closed,
}

impl OldGeneration {
    /// Put `index` in this state.
    async fn apply(self, client: &reqwest::Client, base: &str, index: &str) {
        let block = match self {
            OldGeneration::WriteBlocked => "index.blocks.write",
            OldGeneration::ReadOnly => "index.blocks.read_only",
            OldGeneration::ReadOnlyAllowDelete => "index.blocks.read_only_allow_delete",
            OldGeneration::Closed => {
                let status = client
                    .post(format!("{base}/{index}/_close"))
                    .send()
                    .await
                    .unwrap()
                    .status();
                assert!(status.is_success(), "closing {index} returned {status}");
                return;
            }
        };
        let status = client
            .put(format!("{base}/{index}/_settings"))
            .json(&serde_json::json!({ block: true }))
            .send()
            .await
            .unwrap()
            .status();
        assert!(
            status.is_success(),
            "setting {block} on {index} returned {status}"
        );
    }

    /// Whether `index` is still in this state: read right before the seed, so
    /// the test proves the seed met it.
    async fn holds(self, client: &reqwest::Client, base: &str, index: &str) -> bool {
        let rows: Value = client
            .get(format!("{base}/_cat/indices/{index}?h=status&format=json"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let closed = rows[0]["status"].as_str() == Some("close");
        let block = match self {
            OldGeneration::Closed => return closed,
            OldGeneration::WriteBlocked => "index.blocks.write",
            OldGeneration::ReadOnly => "index.blocks.read_only",
            OldGeneration::ReadOnlyAllowDelete => "index.blocks.read_only_allow_delete",
        };
        !closed && flat_settings(client, base, index).await[block].as_str() == Some("true")
    }
}

/// Seed generation 1, put it in `state`, reindex into generation 2 from a
/// fresh sink (a restart), write, and seed it. Then assert what `mark_seeded`
/// promises (`sink_impl.rs`): the hash alias and the convenience alias serve
/// generation 2, every other generation is deleted, the index reads seeded,
/// and a restart's `ensure_index` stages nothing more (no new generation, the
/// aliases unmoved, still seeded). Every step's error is printed whole, so a
/// failing state shows what OpenSearch answered.
async fn a_seed_past_an_old_generation(state: OldGeneration) {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    // The disk monitor releases a `read_only_allow_delete` block it finds on
    // a node below the high watermark: off, the block stays as an operator
    // (or a flood stage that has not passed yet) left it.
    let status = client
        .put(format!("{base}/_cluster/settings"))
        .json(&serde_json::json!({
            "persistent": { "cluster.routing.allocation.disk.threshold_enabled": false }
        }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(
        status.is_success(),
        "turning the disk monitor off returned {status}"
    );
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");
    let gen2 = format!("{hash_alias}_2");

    let run1 = sink(&base);
    run1.ensure_index(&mapping).await.unwrap();
    for i in [1, 2, 3] {
        run1.apply(&upsert(&index, &i.to_string(), &document(i)))
            .await
            .unwrap();
    }
    run1.flush(true).await.unwrap();
    run1.mark_seeded(&index).await.unwrap();
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()]
    );

    state.apply(&client, &base, &gen1).await;

    sink(&base)
        .reindex(&mapping)
        .await
        .unwrap_or_else(|error| panic!("{state:?}: reindex failed: {error}"));
    let run2 = sink(&base);
    run2.ensure_index(&mapping)
        .await
        .unwrap_or_else(|error| panic!("{state:?}: ensure_index of generation 2 failed: {error}"));
    for i in [1, 2] {
        run2.apply(&upsert(&index, &i.to_string(), &document(i)))
            .await
            .unwrap();
    }
    run2.flush(true)
        .await
        .unwrap_or_else(|error| panic!("{state:?}: writing generation 2 failed: {error}"));
    assert!(
        state.holds(&client, &base, &gen1).await,
        "{state:?} still holds on generation 1 when the seed runs"
    );
    run2.mark_seeded(&index)
        .await
        .unwrap_or_else(|error| panic!("{state:?}: mark_seeded failed: {error}"));

    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen2.clone()],
        "{state:?}: the hash alias serves generation 2",
    );
    assert_eq!(
        alias_targets(&client, &base, LOGICAL).await,
        vec![gen2.clone()],
        "{state:?}: the convenience alias serves generation 2",
    );
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen2.clone()],
        "{state:?}: the old generation is deleted",
    );
    assert!(
        run2.is_seeded(&index).await.unwrap(),
        "{state:?}: seeded after mark_seeded"
    );
    assert!(
        !doc_exists(&client, &base, &hash_alias, "3").await,
        "{state:?}: reads come from the rebuilt generation"
    );

    let restart = sink(&base);
    restart
        .ensure_index(&mapping)
        .await
        .unwrap_or_else(|error| panic!("{state:?}: a restart's ensure_index failed: {error}"));
    assert!(
        restart.is_seeded(&index).await.unwrap(),
        "{state:?}: a restart reads the index seeded"
    );
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen2.clone()],
        "{state:?}: a restart stages no new generation",
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen2],
        "{state:?}: a restart leaves the aliases on generation 2",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_seed_past_a_write_blocked_generation_serves_the_new_one_and_deletes_the_old() {
    a_seed_past_an_old_generation(OldGeneration::WriteBlocked).await;
}

/// Under `index.blocks.read_only` the old generation can be neither taken
/// off the aliases nor deleted (#479). The seed is refused with the typed
/// `RetireBlocked`, naming the new generation, the old one and the block;
/// nothing moves: the aliases stay on the old generation, the new one is
/// kept, the index reads unseeded. Once the block is lifted, the same seed
/// serves the new generation and deletes the old one, and a restart stages
/// nothing more.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_seed_past_a_read_only_generation_is_refused_until_the_block_is_lifted() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");
    let gen2 = format!("{hash_alias}_2");

    let run1 = sink(&base);
    run1.ensure_index(&mapping).await.unwrap();
    run1.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    run1.flush(true).await.unwrap();
    run1.mark_seeded(&index).await.unwrap();
    OldGeneration::ReadOnly.apply(&client, &base, &gen1).await;

    sink(&base).reindex(&mapping).await.unwrap();
    let run2 = sink(&base);
    run2.ensure_index(&mapping).await.unwrap();
    run2.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    run2.flush(true).await.unwrap();

    match run2.mark_seeded(&index).await {
        Err(sink::SinkError::RetireBlocked {
            new,
            old,
            block,
            deletable,
        }) => {
            assert_eq!(new, gen2);
            assert_eq!(old, gen1);
            assert!(block.contains("read-only"), "{block}");
            assert!(!deletable, "a read-only index can't be deleted");
        }
        other => panic!("expected RetireBlocked, got {other:?}"),
    }
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()]
    );
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen1.clone(), gen2.clone()]
    );
    assert!(!run2.is_seeded(&index).await.unwrap());

    let status = client
        .put(format!("{base}/{gen1}/_settings"))
        .json(&serde_json::json!({ "index.blocks.read_only": null }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "lifting the block returned {status}");
    run2.mark_seeded(&index).await.unwrap();
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen2.clone()]
    );
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen2.clone()]
    );
    assert!(run2.is_seeded(&index).await.unwrap());

    let restart = sink(&base);
    restart.ensure_index(&mapping).await.unwrap();
    assert!(restart.is_seeded(&index).await.unwrap());
    assert_eq!(generations(&client, &base, &hash_alias).await, vec![gen2]);
}

/// Under `index.blocks.read_only_allow_delete` the old generation can't be
/// taken off the aliases, by any `_aliases` form, but it can be deleted
/// (spike B1). The seed is refused with `RetireBlocked`, marked deletable;
/// nothing moves. Once an operator deletes the old generation, its aliases
/// go with it, and the same seed (the parked lane's next probe) adds the
/// aliases to the new generation alone and only then writes the marker.
/// A restart stages nothing more.
///
/// Whether flusso deletes the old generation itself under this block (form
/// (a) of #479) is the owner's call; until then this is the exit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_seed_past_a_read_only_allow_delete_generation_finishes_once_it_is_deleted() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let status = client
        .put(format!("{base}/_cluster/settings"))
        .json(&serde_json::json!({
            "persistent": { "cluster.routing.allocation.disk.threshold_enabled": false }
        }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(
        status.is_success(),
        "turning the disk monitor off returned {status}"
    );
    let index = IndexName::try_new(LOGICAL).unwrap();
    let mapping = mapping();
    let hash_alias = format!("{LOGICAL}_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");
    let gen2 = format!("{hash_alias}_2");

    let run1 = sink(&base);
    run1.ensure_index(&mapping).await.unwrap();
    run1.apply(&upsert(&index, "1", &document(1)))
        .await
        .unwrap();
    run1.flush(true).await.unwrap();
    run1.mark_seeded(&index).await.unwrap();
    OldGeneration::ReadOnlyAllowDelete
        .apply(&client, &base, &gen1)
        .await;

    sink(&base).reindex(&mapping).await.unwrap();
    let run2 = sink(&base);
    run2.ensure_index(&mapping).await.unwrap();
    run2.apply(&upsert(&index, "2", &document(2)))
        .await
        .unwrap();
    run2.flush(true).await.unwrap();

    match run2.mark_seeded(&index).await {
        Err(sink::SinkError::RetireBlocked {
            new,
            old,
            block,
            deletable,
        }) => {
            assert_eq!(new, gen2);
            assert_eq!(old, gen1);
            assert!(block.contains("read-only-allow-delete"), "{block}");
            assert!(deletable, "this block allows deleting the old generation");
        }
        other => panic!("expected RetireBlocked, got {other:?}"),
    }
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen1.clone()]
    );
    assert!(!run2.is_seeded(&index).await.unwrap());

    let status = client
        .delete(format!("{base}/{gen1}"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(
        status.is_success(),
        "the operator's DELETE returned {status}"
    );
    // The aliases went with the old generation: until the seed's next probe
    // adds them to the new one, reads through them find nothing.
    for alias in [hash_alias.as_str(), LOGICAL] {
        let status = client
            .get(format!("{base}/{alias}/_search"))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{alias} between the delete and the seed"
        );
    }
    assert!(alias_targets(&client, &base, &hash_alias).await.is_empty());
    run2.mark_seeded(&index).await.unwrap();
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        vec![gen2.clone()]
    );
    assert_eq!(
        alias_targets(&client, &base, LOGICAL).await,
        vec![gen2.clone()]
    );
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        vec![gen2.clone()]
    );
    assert!(run2.is_seeded(&index).await.unwrap());
    assert!(doc_exists(&client, &base, &hash_alias, "2").await);

    let restart = sink(&base);
    restart.ensure_index(&mapping).await.unwrap();
    assert!(restart.is_seeded(&index).await.unwrap());
    assert_eq!(generations(&client, &base, &hash_alias).await, vec![gen2]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_seed_past_a_closed_generation_serves_the_new_one_and_deletes_the_old() {
    a_seed_past_an_old_generation(OldGeneration::Closed).await;
}

/// One request of [`spike_b1_what_opensearch_answers_past_a_read_only_generation`]:
/// its status and, refused, the error's type and reason.
async fn answer(response: reqwest::Response) -> String {
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    match body.get("error") {
        Some(error) => format!(
            "{status} {} {}",
            error["type"].as_str().unwrap_or("?"),
            error["reason"].as_str().unwrap_or("?")
        ),
        None => format!("{status}"),
    }
}

/// Spike B1 (#479): what OpenSearch answers, under each read-only block on an
/// old generation, to each way of taking the aliases off it. Every case runs
/// on fresh indexes: an old one holding the alias, blocked, and a new one.
/// The table of answers is compared with the one recorded on OpenSearch 2 (CI
/// run 45715512518), and printed whole when it differs: under `read_only` all
/// three are refused (403); under `read_only_allow_delete` both `_aliases`
/// forms are refused (429) and only a plain `DELETE` passes, which leaves the
/// alias on nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn spike_b1_what_opensearch_answers_past_a_read_only_generation() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let status = client
        .put(format!("{base}/_cluster/settings"))
        .json(&serde_json::json!({
            "persistent": { "cluster.routing.allocation.disk.threshold_enabled": false }
        }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(
        status.is_success(),
        "turning the disk monitor off returned {status}"
    );

    let blocks = [
        "index.blocks.read_only",
        "index.blocks.read_only_allow_delete",
    ];
    let forms = ["remove alias + add", "add + remove_index", "DELETE"];
    let mut seen = Vec::new();
    for (b, block) in blocks.into_iter().enumerate() {
        for (f, form) in forms.iter().enumerate() {
            let old = format!("spike_{b}_{f}_old");
            let new = format!("spike_{b}_{f}_new");
            let alias = format!("spike_{b}_{f}");
            for index in [&old, &new] {
                let status = client
                    .put(format!("{base}/{index}"))
                    .json(&serde_json::json!({
                        "settings": { "number_of_shards": 1, "number_of_replicas": 0 }
                    }))
                    .send()
                    .await
                    .unwrap()
                    .status();
                assert!(status.is_success(), "creating {index} returned {status}");
            }
            let status = client
                .post(format!("{base}/_aliases"))
                .json(&serde_json::json!({ "actions": [{ "add": { "index": old, "alias": alias } }] }))
                .send()
                .await
                .unwrap()
                .status();
            assert!(status.is_success(), "aliasing {old} returned {status}");
            let status = client
                .put(format!("{base}/{old}/_settings"))
                .json(&serde_json::json!({ block: true }))
                .send()
                .await
                .unwrap()
                .status();
            assert!(status.is_success(), "blocking {old} returned {status}");

            let response = match *form {
                "remove alias + add" => client
                    .post(format!("{base}/_aliases"))
                    .json(&serde_json::json!({ "actions": [
                        { "remove": { "index": old, "alias": alias } },
                        { "add": { "index": new, "alias": alias } },
                    ] }))
                    .send()
                    .await
                    .unwrap(),
                "add + remove_index" => client
                    .post(format!("{base}/_aliases"))
                    .json(&serde_json::json!({ "actions": [
                        { "add": { "index": new, "alias": alias } },
                        { "remove_index": { "index": old } },
                    ] }))
                    .send()
                    .await
                    .unwrap(),
                _ => client.delete(format!("{base}/{old}")).send().await.unwrap(),
            };
            let answered = answer(response).await;
            let after = format!(
                "old {}, alias on {:?}",
                if index_exists(&client, &base, &old).await {
                    "kept"
                } else {
                    "gone"
                },
                alias_targets(&client, &base, &alias).await
            );
            seen.push(format!("{block} | {form} | {answered} | {after}"));
        }
    }
    let codes: Vec<String> = seen
        .iter()
        .map(|row| {
            let mut cells = row.split(" | ");
            let block = cells.next().unwrap_or_default();
            let form = cells.next().unwrap_or_default();
            let status = cells
                .next()
                .and_then(|answered| answered.split(' ').next())
                .unwrap_or_default();
            format!("{block} | {form} | {status}")
        })
        .collect();
    let expected = [
        "index.blocks.read_only | remove alias + add | 403",
        "index.blocks.read_only | add + remove_index | 403",
        "index.blocks.read_only | DELETE | 403",
        "index.blocks.read_only_allow_delete | remove alias + add | 429",
        "index.blocks.read_only_allow_delete | add + remove_index | 429",
        "index.blocks.read_only_allow_delete | DELETE | 200",
    ];
    assert_eq!(
        codes,
        expected,
        "what OpenSearch answered:\n{}",
        seen.join("\n")
    );
}
