//! Seeding an index and creating a generation, request by request, on a live
//! OpenSearch behind a fault proxy (issues #354, #403, #410).
//!
//! - **The marker comes last.** `mark_seeded` writes the seeded marker only
//!   once the superseded generation is gone. Written earlier, a failed removal
//!   would leave the old generation behind for good: a restart reads
//!   "seeded" and never calls `mark_seeded` again (#354).
//! - **A crash after any request of `mark_seeded` recovers.** For every `n`,
//!   the sink dies after its `n`th request. Meanwhile the hash alias serves
//!   exactly one whole generation, and the convenience alias at most one
//!   index. A restart (a new sink: `ensure_index`, the backfill again,
//!   `mark_seeded`) ends with the new generation alone, serving, seeded.
//!   The same for a first seed, and for each request of the first round (the
//!   refresh, the settings, the two lookups, run at once) failing alone: no
//!   alias moves until all of them succeeded, so an alias never names a
//!   generation that was not refreshed.
//! - **A generation is created with the aliases it may carry** (#403): the
//!   first one with both aliases in its create request, a rebuild's with
//!   none (the old generation serves until the swap), and never the
//!   convenience alias while another index holds it.
//! - **A backfill checkpoint lives with its unseeded generation** (ADR 0012
//!   stage 8b): kept across a start, forgotten on request, gone once the
//!   generation is created anew, seeded or reindexed, and never written into
//!   a missing meta doc.
//! - **Indexes seed at once.** `mark_seeded` for several indexes, first seeds
//!   and rebuilds alike, runs concurrently on one sink beside writes to other
//!   indexes: each index ends on its own generation, alone, seeded, and no
//!   write is lost (ADR 0012, seeding the indexes of a sink concurrently).
//!
//! - **Staging reads the detach records first** (ADR 0012 stage 10, slice
//!   1): one `_mget` of the seed marker and both records, a missing meta
//!   index read as nothing kept and a refused read as an error, then exactly
//!   the requests staging made before. A record written by hand is read and
//!   left as it is by `ensure_index`. A marker a detach moved with no record
//!   beside it is put back, and the old generation serves again.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-sink-opensearch --test seeding --run-ignored all
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
    ContentHash, Envelope, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    Position, ResolvedField,
};
use reqwest::StatusCode;
use serde_json::Value;
use sink::Sink;
use sink_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

#[path = "support/buffered.rs"]
mod buffered;
#[path = "support/fault_proxy.rs"]
mod fault_proxy;

use buffered::Buffered;
use fault_proxy::{FaultProxy, Seen};

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

/// A fresh sink for `base_url`, one per run, as each start builds one.
fn sink(base_url: &str) -> Buffered {
    let name = kernel::SinkName::try_new("test").unwrap();
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
        refresh_interval: "10s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    Buffered::new(OpensearchSink::from_config(&name, &config).unwrap())
}

/// The stamp the record tests write with.
fn stamp() -> sink::Stamp {
    sink::Stamp::new("seeding", 1)
}

fn mapping(logical: &str, hash: u32) -> IndexMapping {
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
        index: IndexName::try_new(logical).unwrap(),
        hash: ContentHash::new(hash),
        fields: vec![
            field("id", MappingType::Long),
            field("email", MappingType::Keyword),
        ],
    }
}

fn upsert(index: &IndexName, i: usize) -> Envelope {
    let mut map = BTreeMap::new();
    map.insert("id".to_owned(), GenericValue::BigInt(i as i64));
    map.insert(
        "email".to_owned(),
        GenericValue::String(format!("user{i}@example.com")),
    );
    Envelope::upsert(
        index.clone(),
        i.to_string(),
        GenericValue::Map(map),
        Some(Position::new(0)),
        chrono::Utc::now(),
    )
}

/// Write `ids` into `index` as one caught-up write.
async fn write(sink: &Buffered, index: &IndexName, ids: &[usize]) {
    for &i in ids {
        sink.apply(&upsert(index, i)).await.unwrap();
    }
    sink.flush(true).await.unwrap();
}

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

async fn generations(client: &reqwest::Client, base: &str, hash_alias: &str) -> Vec<String> {
    let body: Value = client
        .get(format!(
            "{base}/_cat/indices/{hash_alias}_*?h=index&format=json"
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut names: Vec<String> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["index"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

/// The ids searchable through `target`.
async fn searchable(client: &reqwest::Client, base: &str, target: &str) -> Vec<String> {
    let body: Value = client
        .get(format!("{base}/{target}/_search?size=100"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut ids: Vec<String> = body["hits"]["hits"]
        .as_array()
        .map(|hits| {
            hits.iter()
                .map(|hit| hit["_id"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

/// The meta document: `(active generation, seeded)`.
async fn marker(client: &reqwest::Client, base: &str, hash_alias: &str) -> (u64, bool) {
    let body: Value = client
        .get(format!("{base}/flusso_meta/_doc/{hash_alias}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    (
        body["_source"]["active_generation"].as_u64().unwrap(),
        body["_source"]["seeded"].as_bool().unwrap(),
    )
}

/// The `refresh_interval` set on `index`, if any.
async fn refresh_interval(client: &reqwest::Client, base: &str, index: &str) -> Option<String> {
    let body: Value = client
        .get(format!("{base}/{index}/_settings"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body[index]["settings"]["index"]["refresh_interval"]
        .as_str()
        .map(str::to_owned)
}

/// The requests of `mark_seeded`'s first round, which run at once: each is
/// failed alone by `the_first_round_moves_no_alias_until_every_request_succeeded`.
fn first_round(seen: &Seen, kind: &str) -> bool {
    match kind {
        "refresh" => seen.method == "POST" && seen.path.ends_with("/_refresh"),
        "settings" => seen.method == "PUT" && seen.path.ends_with("/_settings"),
        "holders" => seen.method == "GET" && seen.path.starts_with("/_alias/"),
        "generations" => seen.method == "GET" && seen.path.starts_with("/_cat/indices/"),
        _ => unreachable!("{kind}"),
    }
}

/// Whether a request takes generation `generation` away: a delete of it, or
/// an `_aliases` call removing it.
fn removes(seen: &Seen, generation: &str) -> bool {
    (seen.method == "DELETE" && seen.path.trim_start_matches('/').starts_with(generation))
        || (seen.method == "POST"
            && seen.path.starts_with("/_aliases")
            && seen.body.contains("remove_index")
            && seen.body.contains(&format!("\"{generation}\"")))
}

/// `logical` seeded into generation 1 with ids 1–3, then a reindex staged and
/// generation 2 written with ids 1 and 2, ready for its `mark_seeded`.
async fn rebuilt(sink_a: &Buffered, sink_b: &Buffered, mapping: &IndexMapping) {
    let index = &mapping.index;
    sink_a.ensure_index(mapping).await.unwrap();
    write(sink_a, index, &[1, 2, 3]).await;
    sink_a.mark_seeded(index).await.unwrap();
    sink_a.reindex(mapping).await.unwrap();
    sink_b.ensure_index(mapping).await.unwrap();
    write(sink_b, index, &[1, 2]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn the_seed_marker_waits_until_the_old_generation_is_gone() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    let mapping = mapping("users", 1);
    let index = mapping.index.clone();
    let hash_alias = format!("users_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");
    let gen2 = format!("{hash_alias}_2");

    let run = sink(&proxy.url);
    rebuilt(&sink(&base), &run, &mapping).await;

    let doomed = gen1.clone();
    proxy.refuse(move |seen, _| removes(seen, &doomed));
    run.mark_seeded(&index)
        .await
        .expect_err("removing the old generation was refused");
    assert_eq!(
        marker(&client, &base, &hash_alias).await,
        (2, false),
        "the marker is not written while the old generation stands"
    );
    assert!(
        generations(&client, &base, &hash_alias)
            .await
            .contains(&gen1)
    );

    proxy.heal();
    let restarted = sink(&base);
    restarted.ensure_index(&mapping).await.unwrap();
    assert!(!restarted.is_seeded(&index).await.unwrap());
    write(&restarted, &index, &[1, 2]).await;
    restarted.mark_seeded(&index).await.unwrap();
    assert_eq!(marker(&client, &base, &hash_alias).await, (2, true));
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        [gen2.clone()]
    );
    assert_eq!(
        alias_targets(&client, &base, &hash_alias).await,
        [gen2.clone()]
    );
    assert_eq!(alias_targets(&client, &base, "users").await, [gen2]);
    assert_eq!(searchable(&client, &base, &hash_alias).await, ["1", "2"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_crash_after_any_request_of_mark_seeded_recovers_on_restart() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    for n in 0.. {
        let logical = format!("users{n}");
        let mapping = mapping(&logical, 1);
        let index = mapping.index.clone();
        let hash_alias = format!("{logical}_{}", mapping.hash);
        let gen1 = format!("{hash_alias}_1");
        let gen2 = format!("{hash_alias}_2");

        let run = sink(&proxy.url);
        proxy.heal();
        rebuilt(&sink(&base), &run, &mapping).await;
        proxy.take();
        proxy.crash_after(n);
        let finished = run.mark_seeded(&index).await.is_ok();
        let requests = proxy.take().iter().filter(|seen| !seen.refused).count();
        proxy.heal();

        let serving = alias_targets(&client, &base, &hash_alias).await;
        assert_eq!(serving.len(), 1, "after {n} requests: {serving:?}");
        let whole = if serving == [gen1.clone()] {
            vec!["1", "2", "3"]
        } else {
            vec!["1", "2"]
        };
        assert_eq!(
            searchable(&client, &base, &hash_alias).await,
            whole,
            "after {n} requests the hash alias serves a whole generation ({serving:?})"
        );
        let named = alias_targets(&client, &base, &logical).await;
        assert!(
            named.len() <= 1,
            "after {n} requests the convenience alias names one index at most"
        );
        if named == [gen2.clone()] {
            assert_eq!(
                searchable(&client, &base, &logical).await,
                ["1", "2"],
                "after {n} requests the convenience alias serves a refreshed generation"
            );
        }
        if !finished {
            assert!(
                !marker(&client, &base, &hash_alias).await.1,
                "a mark_seeded cut short after {n} requests left the marker unseeded"
            );
            let restarted = sink(&base);
            restarted.ensure_index(&mapping).await.unwrap();
            assert!(!restarted.is_seeded(&index).await.unwrap());
            write(&restarted, &index, &[1, 2]).await;
            restarted.mark_seeded(&index).await.unwrap();
        }
        assert_eq!(marker(&client, &base, &hash_alias).await, (2, true));
        assert_eq!(
            generations(&client, &base, &hash_alias).await,
            [gen2.clone()]
        );
        assert_eq!(
            alias_targets(&client, &base, &hash_alias).await,
            [gen2.clone()]
        );
        assert_eq!(alias_targets(&client, &base, &logical).await, [gen2]);
        assert_eq!(searchable(&client, &base, &hash_alias).await, ["1", "2"]);
        if finished {
            assert!(
                n > 0 && requests <= n,
                "{requests} requests, crash after {n}"
            );
            return;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_first_generation_is_created_with_both_aliases_in_one_request() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    let h1 = format!("users_{}", ContentHash::new(1));
    let mapping = mapping("users", 1);

    sink(&proxy.url).ensure_index(&mapping).await.unwrap();

    let seen = proxy.take();
    let creates: Vec<&Seen> = seen
        .iter()
        .filter(|seen| seen.method == "PUT" && seen.path == format!("/{h1}_1"))
        .collect();
    assert_eq!(creates.len(), 1, "{seen:#?}");
    let body: Value = serde_json::from_str(&creates[0].body).unwrap();
    let mut aliases: Vec<&String> = body["aliases"].as_object().unwrap().keys().collect();
    aliases.sort();
    assert_eq!(aliases, [&"users".to_owned(), &h1]);
    assert!(
        !seen.iter().any(|seen| seen.path.starts_with("/_aliases")),
        "no alias update after the create: {seen:#?}"
    );
    assert_eq!(
        alias_targets(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")]
    );
    assert_eq!(
        alias_targets(&client, &base, "users").await,
        [format!("{h1}_1")]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_rebuilding_generation_is_created_without_aliases() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    let h1 = format!("users_{}", ContentHash::new(1));
    let mapping = mapping("users", 1);
    let first = sink(&base);
    first.ensure_index(&mapping).await.unwrap();
    write(&first, &mapping.index, &[1, 2, 3]).await;
    first.mark_seeded(&mapping.index).await.unwrap();
    first.reindex(&mapping).await.unwrap();

    sink(&proxy.url).ensure_index(&mapping).await.unwrap();

    let seen = proxy.take();
    let create = seen
        .iter()
        .find(|seen| seen.method == "PUT" && seen.path == format!("/{h1}_2"))
        .expect("generation 2 is created");
    let body: Value = serde_json::from_str(&create.body).unwrap();
    assert!(body.get("aliases").is_none(), "{body}");
    assert_eq!(
        alias_targets(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")]
    );
    assert_eq!(
        alias_targets(&client, &base, "users").await,
        [format!("{h1}_1")]
    );
    assert_eq!(
        searchable(&client, &base, h1.as_str()).await,
        ["1", "2", "3"],
        "reads stay on the whole old generation while the new one is built"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn the_convenience_alias_another_schema_holds_is_moved_never_shared() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    let h2 = format!("users_{}", ContentHash::new(2));
    let old = mapping("users", 1);
    let first = sink(&base);
    first.ensure_index(&old).await.unwrap();
    write(&first, &old.index, &[1]).await;
    first.mark_seeded(&old.index).await.unwrap();

    let new = mapping("users", 2);
    sink(&proxy.url).ensure_index(&new).await.unwrap();

    let seen = proxy.take();
    let create = seen
        .iter()
        .find(|seen| seen.method == "PUT" && seen.path == format!("/{h2}_1"))
        .expect("the new schema's generation is created");
    let body: Value = serde_json::from_str(&create.body).unwrap();
    let carried: Vec<&String> = body["aliases"]
        .as_object()
        .map(|aliases| aliases.keys().collect())
        .unwrap_or_default();
    assert_eq!(
        carried,
        [&h2],
        "the convenience alias isn't added beside the index holding it"
    );
    assert_eq!(
        alias_targets(&client, &base, h2.as_str()).await,
        [format!("{h2}_1")]
    );
    assert_eq!(
        alias_targets(&client, &base, "users").await.len(),
        1,
        "the convenience alias names one index"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_rebuild_swaps_both_aliases_and_drops_the_old_generation_in_one_request() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    let h1 = format!("users_{}", ContentHash::new(1));
    let mapping = mapping("users", 1);
    let run = sink(&proxy.url);
    rebuilt(&sink(&base), &run, &mapping).await;
    proxy.take();

    run.mark_seeded(&mapping.index).await.unwrap();

    let seen = proxy.take();
    let lines: Vec<String> = seen.iter().map(Seen::line).collect();
    let alias_updates: Vec<&Seen> = seen
        .iter()
        .filter(|seen| seen.path.starts_with("/_aliases"))
        .collect();
    assert_eq!(alias_updates.len(), 1, "{lines:#?}");
    assert!(
        !seen.iter().any(|seen| seen.method == "DELETE"),
        "{lines:#?}"
    );
    let body = &alias_updates[0].body;
    assert!(
        body.contains("remove_index") && body.contains(&format!("\"{h1}_1\"")),
        "{body}"
    );
    assert_eq!(
        generations(&client, &base, h1.as_str()).await,
        [format!("{h1}_2")]
    );
    assert_eq!(
        alias_targets(&client, &base, h1.as_str()).await,
        [format!("{h1}_2")]
    );
    assert_eq!(
        alias_targets(&client, &base, "users").await,
        [format!("{h1}_2")]
    );
    assert_eq!(searchable(&client, &base, h1.as_str()).await, ["1", "2"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn several_indexes_seed_at_once_beside_writes_to_others() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let rebuilds: Vec<IndexMapping> = (0..4)
        .map(|i| mapping(&format!("rebuilt{i}"), 10 + i))
        .collect();
    let fresh: Vec<IndexMapping> = (0..4)
        .map(|i| mapping(&format!("fresh{i}"), 20 + i))
        .collect();
    let busy: Vec<IndexMapping> = (0..2)
        .map(|i| mapping(&format!("busy{i}"), 30 + i))
        .collect();

    let before = sink(&base);
    for mapping in &rebuilds {
        before.ensure_index(mapping).await.unwrap();
        write(&before, &mapping.index, &[1, 2, 3]).await;
        before.mark_seeded(&mapping.index).await.unwrap();
        before.reindex(mapping).await.unwrap();
    }
    let run = std::sync::Arc::new(sink(&base));
    for mapping in rebuilds.iter().chain(&fresh).chain(&busy) {
        run.ensure_index(mapping).await.unwrap();
    }
    for mapping in rebuilds.iter().chain(&fresh) {
        write(&run, &mapping.index, &[1, 2]).await;
    }
    for mapping in &busy {
        write(&run, &mapping.index, &[0]).await;
        run.mark_seeded(&mapping.index).await.unwrap();
    }

    let mut tasks = tokio::task::JoinSet::new();
    for mapping in rebuilds.iter().chain(&fresh) {
        let run = std::sync::Arc::clone(&run);
        let index = mapping.index.clone();
        tasks.spawn(async move { run.mark_seeded(&index).await.unwrap() });
    }
    for mapping in &busy {
        let run = std::sync::Arc::clone(&run);
        let index = mapping.index.clone();
        tasks.spawn(async move {
            for i in 1..=20 {
                let envelopes = [upsert(&index, i)];
                let report = run.write(&envelopes, i == 20).await.unwrap();
                assert!(report.rejected.is_empty(), "{report:?}");
            }
        });
    }
    while let Some(done) = tasks.join_next().await {
        done.unwrap();
    }

    for (mapping, generation) in rebuilds
        .iter()
        .map(|m| (m, 2))
        .chain(fresh.iter().map(|m| (m, 1)))
    {
        let logical = mapping.index.as_ref();
        let hash_alias = format!("{logical}_{}", mapping.hash);
        let serving = format!("{hash_alias}_{generation}");
        assert_eq!(
            generations(&client, &base, &hash_alias).await,
            [serving.clone()],
            "{logical}"
        );
        assert_eq!(
            alias_targets(&client, &base, &hash_alias).await,
            [serving.clone()],
            "{logical}"
        );
        assert_eq!(
            alias_targets(&client, &base, logical).await,
            [serving],
            "{logical}"
        );
        assert_eq!(
            marker(&client, &base, &hash_alias).await,
            (generation, true),
            "{logical}"
        );
        assert_eq!(
            searchable(&client, &base, &hash_alias).await,
            ["1", "2"],
            "{logical}"
        );
    }
    for mapping in &busy {
        let hash_alias = format!("{}_{}", mapping.index.as_ref(), mapping.hash);
        let mut expected: Vec<String> = (0..=20).map(|i| i.to_string()).collect();
        expected.sort();
        assert_eq!(searchable(&client, &base, &hash_alias).await, expected);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn the_first_round_moves_no_alias_until_every_request_succeeded() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    for kind in ["refresh", "settings", "holders", "generations"] {
        let logical = format!("users_{kind}");
        let mapping = mapping(&logical, 1);
        let index = mapping.index.clone();
        let hash_alias = format!("{logical}_{}", mapping.hash);
        let gen1 = format!("{hash_alias}_1");
        let gen2 = format!("{hash_alias}_2");

        let run = sink(&proxy.url);
        proxy.heal();
        rebuilt(&sink(&base), &run, &mapping).await;
        proxy.refuse(move |seen, _| first_round(seen, kind));
        run.mark_seeded(&index)
            .await
            .expect_err("one request of the first round was refused");
        let refused: Vec<String> = proxy
            .take()
            .iter()
            .filter(|seen| seen.refused)
            .map(Seen::line)
            .collect();
        assert!(!refused.is_empty(), "{kind}: nothing was refused");
        proxy.heal();

        for alias in [hash_alias.as_str(), logical.as_str()] {
            assert_eq!(
                alias_targets(&client, &base, alias).await,
                [gen1.clone()],
                "{kind} refused: {alias} still serves the old generation"
            );
            assert_eq!(
                searchable(&client, &base, alias).await,
                ["1", "2", "3"],
                "{kind} refused: {alias} serves the whole old generation"
            );
        }
        assert_eq!(marker(&client, &base, &hash_alias).await, (2, false));

        let restarted = sink(&base);
        restarted.ensure_index(&mapping).await.unwrap();
        assert!(!restarted.is_seeded(&index).await.unwrap());
        write(&restarted, &index, &[1, 2]).await;
        restarted.mark_seeded(&index).await.unwrap();
        assert_eq!(marker(&client, &base, &hash_alias).await, (2, true));
        assert_eq!(
            generations(&client, &base, &hash_alias).await,
            [gen2.clone()]
        );
        assert_eq!(
            alias_targets(&client, &base, &logical).await,
            [gen2.clone()]
        );
        assert_eq!(searchable(&client, &base, &logical).await, ["1", "2"]);
        assert_eq!(
            refresh_interval(&client, &base, &gen2).await.as_deref(),
            Some("10s"),
            "{kind}: the steady refresh interval is restored"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_crash_after_any_request_of_a_first_seed_recovers_on_restart() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let client = reqwest::Client::new();
    for n in 0.. {
        let logical = format!("first{n}");
        let mapping = mapping(&logical, 1);
        let index = mapping.index.clone();
        let hash_alias = format!("{logical}_{}", mapping.hash);
        let gen1 = format!("{hash_alias}_1");

        let run = sink(&proxy.url);
        proxy.heal();
        run.ensure_index(&mapping).await.unwrap();
        write(&run, &index, &[1, 2]).await;
        proxy.take();
        proxy.crash_after(n);
        let finished = run.mark_seeded(&index).await.is_ok();
        proxy.take();
        proxy.heal();

        for alias in [hash_alias.as_str(), logical.as_str()] {
            assert_eq!(
                alias_targets(&client, &base, alias).await,
                [gen1.clone()],
                "after {n} requests {alias} serves the first generation"
            );
        }
        if !finished {
            assert!(
                !marker(&client, &base, &hash_alias).await.1,
                "a mark_seeded cut short after {n} requests left the marker unseeded"
            );
            let restarted = sink(&base);
            restarted.ensure_index(&mapping).await.unwrap();
            assert!(!restarted.is_seeded(&index).await.unwrap());
            write(&restarted, &index, &[1, 2]).await;
            restarted.mark_seeded(&index).await.unwrap();
        }
        assert_eq!(marker(&client, &base, &hash_alias).await, (1, true));
        assert_eq!(
            generations(&client, &base, &hash_alias).await,
            [gen1.clone()]
        );
        assert_eq!(searchable(&client, &base, &logical).await, ["1", "2"]);
        assert_eq!(
            refresh_interval(&client, &base, &gen1).await.as_deref(),
            Some("10s")
        );
        if finished {
            assert!(n > 0);
            return;
        }
    }
}

/// What `seeding` answers for `index`: `None` when seeded, else the recorded
/// checkpoint (`Some(None)` for none).
async fn walked(sink: &Buffered, index: &IndexName) -> Option<Option<String>> {
    match sink.seeding(index).await.unwrap() {
        sink::Seeding::Seeded => None,
        sink::Seeding::Unseeded { walked } => Some(walked),
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_backfill_checkpoint_lives_with_its_unseeded_generation() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let mapping = mapping("users", 1);
    let index = mapping.index.clone();
    let hash_alias = format!("users_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");

    let run = sink(&base);
    run.ensure_index(&mapping).await.unwrap();
    assert_eq!(walked(&run, &index).await, Some(None));
    run.record_walked(&index, "k1").await.unwrap();
    run.record_walked(&index, "k2").await.unwrap();
    assert_eq!(walked(&run, &index).await, Some(Some("k2".to_owned())));

    let restarted = sink(&base);
    restarted.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        walked(&restarted, &index).await,
        Some(Some("k2".to_owned())),
        "a start over the existing unseeded generation keeps it"
    );

    restarted.forget_walked(&index).await.unwrap();
    assert_eq!(walked(&restarted, &index).await, Some(None), "forgotten");
    restarted.record_walked(&index, "k3").await.unwrap();

    client
        .delete(format!("{base}/{gen1}"))
        .send()
        .await
        .unwrap();
    let recreated = sink(&base);
    recreated.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        walked(&recreated, &index).await,
        Some(None),
        "a generation created anew holds nothing, so neither does its checkpoint"
    );

    recreated.record_walked(&index, "k4").await.unwrap();
    write(&recreated, &index, &[1]).await;
    recreated.mark_seeded(&index).await.unwrap();
    assert_eq!(walked(&recreated, &index).await, None, "seeded");
    assert_eq!(marker(&client, &base, &hash_alias).await, (1, true));
    recreated.record_walked(&index, "stray").await.unwrap();
    assert_eq!(
        walked(&recreated, &index).await,
        None,
        "a stray record never unseeds a seeded index"
    );

    recreated.reindex(&mapping).await.unwrap();
    let rebuilding = sink(&base);
    rebuilding.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        walked(&rebuilding, &index).await,
        Some(None),
        "a reindex starts the new generation's walk from the first key"
    );
    rebuilding.record_walked(&index, "r1").await.unwrap();
    assert_eq!(
        walked(&rebuilding, &index).await,
        Some(Some("r1".to_owned()))
    );

    client
        .delete(format!("{base}/flusso_meta"))
        .send()
        .await
        .unwrap();
    assert!(
        rebuilding.record_walked(&index, "r2").await.is_err(),
        "a record creates no meta doc"
    );
}

/// Final review, finding 1: a plain index already named like the convenience
/// alias (`users`) must not stop a first start. The create request can't
/// carry that alias (OpenSearch refuses the whole request), so the first
/// generation is created with the hash alias alone and the convenience alias
/// stays best effort, as `main` had it; the seed and the writes go through
/// the hash alias, and the plain index is left as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_plain_index_named_like_the_convenience_alias_never_stops_a_first_start() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let h1 = format!("users_{}", ContentHash::new(1));
    let mapping = mapping("users", 1);
    let created = client.put(format!("{base}/users")).send().await.unwrap();
    assert!(created.status().is_success(), "{:?}", created.text().await);

    let sink = sink(&base);
    sink.ensure_index(&mapping)
        .await
        .expect("a first start goes on beside a plain index named like the alias");
    assert_eq!(
        alias_targets(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")]
    );
    write(&sink, &mapping.index, &[1, 2, 3]).await;
    sink.mark_seeded(&mapping.index)
        .await
        .expect("the seed moves the hash alias alone");

    assert_eq!(
        generations(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")]
    );
    assert_eq!(
        alias_targets(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")]
    );
    assert_eq!(
        searchable(&client, &base, h1.as_str()).await,
        ["1", "2", "3"]
    );
    assert_eq!(marker(&client, &base, h1.as_str()).await, (1, true));
    assert_eq!(
        searchable(&client, &base, "users").await,
        Vec::<String>::new(),
        "the plain index is left as it was"
    );
    let plain = client.get(format!("{base}/users")).send().await.unwrap();
    let plain: Value = plain.json().await.unwrap();
    assert!(
        plain.get("users").is_some(),
        "`users` is still the plain index: {plain}"
    );
}

/// Final review, finding 3: a crash between creating a first generation (with
/// its aliases) and writing its meta doc. The next start finds no meta doc and
/// an unseeded generation already holding the hash alias: it reuses that
/// generation rather than create generation 2 beside it, which would leave
/// readers on the empty generation 1 until the seed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_first_generation_left_without_its_meta_doc_is_reused() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let h1 = format!("users_{}", ContentHash::new(1));
    let mapping = mapping("users", 1);
    sink(&base).ensure_index(&mapping).await.unwrap();
    let deleted = client
        .delete(format!("{base}/flusso_meta/_doc/{h1}?refresh=true"))
        .send()
        .await
        .unwrap();
    assert!(deleted.status().is_success(), "{:?}", deleted.text().await);

    let restarted = sink(&base);
    restarted.ensure_index(&mapping).await.unwrap();
    assert_eq!(
        generations(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")],
        "no second generation beside the one serving"
    );
    assert_eq!(
        alias_targets(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")]
    );
    assert_eq!(
        alias_targets(&client, &base, "users").await,
        [format!("{h1}_1")]
    );
    assert_eq!(marker(&client, &base, h1.as_str()).await, (1, false));

    write(&restarted, &mapping.index, &[1, 2]).await;
    restarted.mark_seeded(&mapping.index).await.unwrap();
    assert_eq!(
        generations(&client, &base, h1.as_str()).await,
        [format!("{h1}_1")]
    );
    assert_eq!(searchable(&client, &base, "users").await, ["1", "2"]);
    assert_eq!(marker(&client, &base, h1.as_str()).await, (1, true));
}

/// Final review, finding 4: the sink says when reads see another generation
/// than the one it writes. A first seed: no (both aliases name the generation
/// being filled). A reindex restarted by a new process: yes, until the seed
/// swaps the aliases.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn the_sink_says_when_reads_see_another_generation_than_it_writes() {
    let (_container, base) = start_opensearch().await;
    let mapping = mapping("users", 1);
    let index = &mapping.index;
    let first = sink(&base);
    first.ensure_index(&mapping).await.unwrap();
    assert!(!first.is_rebuilding(index).await.unwrap(), "a first seed");
    write(&first, index, &[1, 2]).await;
    first.mark_seeded(index).await.unwrap();
    assert!(!first.is_rebuilding(index).await.unwrap(), "seeded");
    first.reindex(&mapping).await.unwrap();

    let restarted = sink(&base);
    restarted.ensure_index(&mapping).await.unwrap();
    assert!(
        restarted.is_rebuilding(index).await.unwrap(),
        "reads stay on generation 1 while generation 2 is written"
    );
    write(&restarted, index, &[1]).await;
    restarted.mark_seeded(index).await.unwrap();
    assert!(!restarted.is_rebuilding(index).await.unwrap(), "swapped");
}

/// A fresh sink for `base_url` whose names all carry `prefix`.
fn prefixed(base_url: &str, prefix: &str) -> Buffered {
    let name = kernel::SinkName::try_new("test").unwrap();
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
        refresh_interval: "10s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    Buffered::new(
        OpensearchSink::from_config(&name, &config)
            .unwrap()
            .with_index_prefix(prefix),
    )
}

/// The requests seen, as lines, with `prefix` taken off every name.
fn unprefixed(seen: &[Seen], prefix: &str) -> Vec<String> {
    seen.iter()
        .map(|seen| {
            seen.line()
                .replace(&format!("/{prefix}"), "/")
                .replace(&format!(",{prefix}"), ",")
        })
        .collect()
}

/// On a fresh cluster (no meta index yet), staging's read is one `_mget` and
/// reads nothing kept. What staging then asks is exactly what it asked
/// before the read existed: a deployment that never detached anything sees
/// one more request and nothing else.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn staging_reads_the_records_in_one_request_then_asks_what_it_asked_before() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let mapping = mapping("users", 1);

    let reading = prefixed(&proxy.url, "a_");
    let read = reading.detachment(&mapping, &stamp()).await.unwrap();
    assert!(read.is_empty(), "nothing kept on a fresh cluster: {read:?}");
    reading.ensure_index(&mapping).await.unwrap();
    reading.seeding(&mapping.index).await.unwrap();
    let with_read = proxy.take();

    let before = prefixed(&proxy.url, "b_");
    before.ensure_index(&mapping).await.unwrap();
    before.seeding(&mapping.index).await.unwrap();
    let without_read = proxy.take();

    assert_eq!(with_read[0].line(), "POST /a_flusso_meta/_mget");
    assert_eq!(
        unprefixed(&with_read[1..], "a_"),
        unprefixed(&without_read, "b_"),
        "after the read, the same requests in the same order"
    );

    // Once the meta index exists, nothing kept still reads as nothing kept.
    let again = prefixed(&base, "a_")
        .detachment(&mapping, &stamp())
        .await
        .unwrap();
    assert!(again.is_empty(), "{again:?}");
}

/// A record and a stuck clock written by hand (as a later flusso would) are
/// read, and `ensure_index` and a seed leave both documents as they were: the
/// seed marker is a document of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_record_written_by_hand_is_read_and_left_as_it_is() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let mapping = mapping("users", 1);
    let index = mapping.index.clone();
    let hash_alias = format!("users_{}", mapping.hash);

    let first = sink(&base);
    first.ensure_index(&mapping).await.unwrap();
    write(&first, &index, &[1]).await;
    first.mark_seeded(&index).await.unwrap();

    let put = |id: String, body: Value| {
        let client = client.clone();
        let base = base.clone();
        async move {
            let status = client
                .put(format!("{base}/flusso_meta/_doc/{id}?refresh=true"))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status();
            assert!(status.is_success(), "writing {id} returned {status}");
        }
    };
    put(
        format!("detach:{hash_alias}"),
        serde_json::json!({ "state": "detached", "live_only": false, "serving": 1, "attempts": 1 }),
    )
    .await;
    put(
        format!("stuck:{hash_alias}"),
        serde_json::json!({ "since": "2026-11-24T08:00:00Z", "seen_at": "2026-11-24T08:10:00Z" }),
    )
    .await;
    let ids = [
        format!("detach:{hash_alias}"),
        format!("stuck:{hash_alias}"),
    ];
    let written = meta_documents(&client, &base, &ids).await;

    let restart = sink(&base);
    let read = restart.detachment(&mapping, &stamp()).await.unwrap();
    assert_eq!(
        read.record,
        Some(sink::DetachRecord::Detached { live_only: false })
    );
    assert_eq!(
        read.stuck,
        Some(sink::StuckClock::new(
            Some("2026-11-24T08:00:00Z".to_owned()),
            Some("2026-11-24T08:10:00Z".to_owned())
        ))
    );
    assert_eq!(read.restored, None);
    restart.ensure_index(&mapping).await.unwrap();
    assert!(restart.is_seeded(&index).await.unwrap(), "staged as before");
    assert_eq!(
        meta_documents(&client, &base, &ids).await,
        written,
        "ensure_index left both documents as they were"
    );

    put(
        format!("detach:{hash_alias}"),
        serde_json::json!({ "state": "frozen" }),
    )
    .await;
    let read = sink(&base).detachment(&mapping, &stamp()).await.unwrap();
    assert!(
        matches!(&read.record, Some(sink::DetachRecord::Unreadable { reason }) if reason.contains("frozen")),
        "{read:?}"
    );
}

/// What the stamps of round 11 rely on, on a real OpenSearch (the adapter's
/// unit tests pin the requests; this pins the answers):
/// - a stamped detach and a stamped forget leave each stamp readable, the
///   forget as a tombstone that reads as no record, with its own version;
/// - a forget at a version the tombstone replaced conflicts, never deletes;
/// - a detach over the tombstone replaces it on its version;
/// - the checkpoint's and the uuid's partial `_update`s keep the marker's
///   stamp, and the gaps' `_update` replaces it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn who_wrote_each_record_survives_every_write_as_opensearch_answers_it() {
    let (_container, base) = start_opensearch().await;
    let mapping = mapping("users", 1);
    let index = mapping.index.clone();
    let mine = sink::Stamp::new("mine", 1);
    let theirs = sink::Stamp::new("theirs", 7);

    let first = sink(&base);
    first.ensure_index_as(&mapping, &mine).await.unwrap();
    write(&first, &index, &[1]).await;
    first.mark_seeded_as(&index, &mine).await.unwrap();
    let read = first.detachment(&mapping, &mine).await.unwrap();
    assert_eq!(read.marker_stamp, Some(mine.clone()), "{read:?}");

    first.record_walked(&index, "k").await.unwrap();
    let read = first.detachment(&mapping, &mine).await.unwrap();
    assert_eq!(
        read.marker_stamp,
        Some(mine.clone()),
        "a checkpoint keeps the stamp"
    );

    let detach = sink::Detach::new("blocked")
        .live_only(true)
        .through(Some("0/10".to_owned()))
        .stamp(mine.clone());
    first.detach(&mapping, &detach).await.unwrap();
    let detached = first.detachment(&mapping, &mine).await.unwrap();
    assert_eq!(detached.record_stamp, Some(mine.clone()));

    first
        .record_gaps(
            &mapping,
            &|_| {
                Some(vec![sink::Gap {
                    from: Some("0/10".to_owned()),
                    to: None,
                }])
            },
            &theirs,
        )
        .await
        .unwrap();
    let read = first.detachment(&mapping, &mine).await.unwrap();
    assert_eq!(
        read.marker_stamp,
        Some(theirs.clone()),
        "the gaps' write stamps"
    );

    let version = detached.version.clone().unwrap();
    first
        .forget_detachment(&mapping, &version, &theirs)
        .await
        .unwrap();
    let forgotten = first.detachment(&mapping, &mine).await.unwrap();
    assert_eq!(forgotten.record, None, "a tombstone is no record");
    assert_eq!(forgotten.record_stamp, Some(theirs.clone()));
    assert!(forgotten.version.is_some() && forgotten.version != Some(version.clone()));
    assert!(
        first
            .forget_detachment(&mapping, &version, &mine)
            .await
            .is_err(),
        "a forget at the replaced version conflicts"
    );
    assert_eq!(
        first
            .detachment(&mapping, &mine)
            .await
            .unwrap()
            .record_stamp,
        Some(theirs.clone()),
        "and leaves the tombstone as it was"
    );

    first.detach(&mapping, &detach).await.unwrap();
    let again = first.detachment(&mapping, &mine).await.unwrap();
    assert_eq!(
        again.record,
        Some(sink::DetachRecord::Detached { live_only: true })
    );
    assert_eq!(again.record_stamp, Some(mine.clone()));
}

/// Each of `ids` in the meta index: its sequence number and source.
async fn meta_documents(
    client: &reqwest::Client,
    base: &str,
    ids: &[String],
) -> Vec<(Value, Value)> {
    let mut documents = Vec::new();
    for id in ids {
        let body: Value = client
            .get(format!("{base}/flusso_meta/_doc/{id}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        documents.push((body["_seq_no"].clone(), body["_source"].clone()));
    }
    documents
}

/// A refused read is an error, never "nothing kept" (H6).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_refused_read_is_never_nothing_kept() {
    let (_container, base) = start_opensearch().await;
    let proxy = FaultProxy::start(&base).await;
    let mapping = mapping("users", 1);
    let first = sink(&base);
    first.ensure_index(&mapping).await.unwrap();

    proxy.refuse(|seen, _| seen.path.contains("_mget"));
    let error = sink(&proxy.url)
        .detachment(&mapping, &stamp())
        .await
        .expect_err("a refused read fails");
    assert!(error.to_string().contains("503"), "{error}");
}

/// A detach that died between moving the seed marker and writing its record
/// leaves the marker on a generation nobody asked for. The next read puts it
/// back on the generation it came from, seeded, with its uuid: staging then
/// serves that generation as before, and no rebuild starts around it (H5).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_half_detached_marker_is_put_back_and_the_old_generation_serves() {
    let (_container, base) = start_opensearch().await;
    let client = reqwest::Client::new();
    let mapping = mapping("users", 1);
    let index = mapping.index.clone();
    let hash_alias = format!("users_{}", mapping.hash);
    let gen1 = format!("{hash_alias}_1");

    let first = sink(&base);
    first.ensure_index(&mapping).await.unwrap();
    write(&first, &index, &[1, 2]).await;
    first.mark_seeded(&index).await.unwrap();
    let source: Value = client
        .get(format!("{base}/flusso_meta/_doc/{hash_alias}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let uuid = source["_source"]["uuid"].clone();
    assert!(uuid.is_string(), "{source}");

    let status = client
        .post(format!(
            "{base}/flusso_meta/_update/{hash_alias}?refresh=true"
        ))
        .json(&serde_json::json!({ "doc": {
            "active_generation": 2, "seeded": false, "walked": null, "uuid": null,
            "detaching_from": { "generation": 1, "seeded": true, "uuid": uuid },
        } }))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "moving the marker returned {status}");

    let restart = sink(&base);
    let read = restart.detachment(&mapping, &stamp()).await.unwrap();
    assert_eq!(read.restored, Some(1));
    assert_eq!(marker(&client, &base, &hash_alias).await, (1, true));
    let source: Value = client
        .get(format!("{base}/flusso_meta/_doc/{hash_alias}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(source["_source"]["uuid"], uuid);
    assert!(
        source["_source"].get("detaching_from").is_none(),
        "{source}"
    );

    restart.ensure_index(&mapping).await.unwrap();
    assert!(restart.is_seeded(&index).await.unwrap());
    assert_eq!(
        generations(&client, &base, &hash_alias).await,
        [gen1.clone()]
    );
    assert_eq!(alias_targets(&client, &base, &hash_alias).await, [gen1]);
    assert!(
        restart
            .detachment(&mapping, &stamp())
            .await
            .unwrap()
            .is_empty(),
        "nothing is left to put back"
    );
}
