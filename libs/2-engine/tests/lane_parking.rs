//! One index's failure stops its own lane, not the sink (ADR 0012 stage 9),
//! against a real Postgres and a real OpenSearch, with the real engines:
//!
//! - [`a_write_blocked_index_parks_alone_and_catches_up_once_lifted`]: an
//!   operator's `index.blocks.write` on two indexes, one under
//!   `on_error = "stop"` and one under `skip`. Each parks as blocked; nothing
//!   is quarantined (#404); a third index stays current meanwhile; the blocked
//!   lane pins the slot; once the block is lifted both catch up with no
//!   restart.
//! - [`a_generation_deleted_mid_run_is_staged_again_alone`]: the generation
//!   behind one index is deleted while the deployment runs. That lane is staged
//!   again (a fresh generation, backfilled) while the other index stays
//!   current; no restart.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-engine --test lane_parking --run-ignored all
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine::{
    FailurePolicies, FailurePolicy, IngestEngine, LaneHealth, Observer, Progress, SinkControl,
    SinkEngine,
};
use kernel::{
    Column, DatabaseSchema, Field, FieldSource, FlussoType, IndexName, IndexSchema, Secret,
    SinkName,
};
use sink::Sink;
use sink_opensearch::OpensearchSink;
use source::SourceSpec;
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::AssertSqlSafe;
use sqlx::postgres::{PgPool, PgPoolOptions};
use stream::{LaneKey, Stream};
use stream_channel::ChannelStream;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

const SINK: &str = "e2e";

/// Every document a sink engine quarantined.
#[derive(Debug, Default)]
struct Quarantined(Mutex<Vec<String>>);

impl Observer for Quarantined {
    fn on_document_quarantined(&self, _: &SinkName, index: &str, id: &str, _: &str) {
        self.0.lock().unwrap().push(format!("{index}/{id}"));
    }
}

struct Pg {
    _container: test_postgres::Container,
    pool: PgPool,
    url: String,
    port: u16,
}

struct Os {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

/// A running deployment: both engines on a task, the sink engine and the
/// progress kept to read its lanes.
struct Deployment {
    task: tokio::task::JoinHandle<engine::Result<()>>,
    sink: Arc<SinkEngine>,
    progress: Arc<Progress>,
    quarantined: Arc<Quarantined>,
    http: reqwest::Client,
    os: String,
}

impl Deployment {
    fn still_running(&self) {
        assert!(
            !self.task.is_finished(),
            "the deployment stopped; a lane's failure must not end it"
        );
    }

    fn health(&self, index: &str) -> LaneHealth {
        self.sink.lane_health(&lane(index))
    }

    /// Poll `index/_doc/id` (realtime: no refresh wait) until its `name` is
    /// `want`, for at most `within`.
    async fn sees(&self, index: &str, id: &str, want: &str, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            self.still_running();
            if self.name(index, id).await.as_deref() == Some(want) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn name(&self, index: &str, id: &str) -> Option<String> {
        let response = self
            .http
            .get(format!("{}/{index}/_doc/{id}", self.os))
            .send()
            .await
            .ok()?;
        let body: serde_json::Value = response.json().await.ok()?;
        body.pointer("/_source/name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    }

    async fn settings(&self, index: &str, body: serde_json::Value) {
        let response = self
            .http
            .put(format!("{}/{index}/_settings", self.os))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "{index}/_settings: {}",
            response.text().await.unwrap()
        );
    }

    /// Wait until `count` indexes are marked seeded.
    async fn seeded(&self, count: u64) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.still_running();
            let _ = self
                .http
                .post(format!("{}/flusso_meta/_refresh", self.os))
                .send()
                .await;
            let seeded = match self
                .http
                .post(format!("{}/flusso_meta/_count", self.os))
                .json(&serde_json::json!({ "query": { "term": { "seeded": true } } }))
                .send()
                .await
            {
                Ok(response) => response
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|body| body.get("count").and_then(serde_json::Value::as_u64))
                    .unwrap_or(0),
                Err(_) => 0,
            };
            if seeded >= count {
                return;
            }
            assert!(Instant::now() < deadline, "{count} indexes never seeded");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

fn lane(index: &str) -> LaneKey {
    LaneKey::new(
        IndexName::try_new(index).unwrap(),
        SinkName::try_new(SINK).unwrap(),
    )
}

/// The write block on two indexes, one `stop` and one `skip`: each parks as
/// blocked and quarantines nothing while the third stays current, the blocked
/// lanes hold the slot, and lifting the block lets both catch up, no restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_write_blocked_index_parks_alone_and_catches_up_once_lifted() {
    let pg = start_postgres("block").await;
    let os = start_opensearch("block").await;
    let live = deploy(
        &pg,
        &os,
        &["held_stop", "held_skip", "steady"],
        FailurePolicies::new(FailurePolicy::Skip).with_override("held_stop", FailurePolicy::Stop),
    )
    .await;
    live.seeded(3).await;

    for index in ["held_stop", "held_skip"] {
        live.settings(index, serde_json::json!({ "index.blocks.write": true }))
            .await;
    }
    for index in ["held_stop", "held_skip", "steady"] {
        exec(
            &pg.pool,
            &format!("INSERT INTO {index} (id, name) VALUES ('1', 'one'), ('2', 'two')"),
        )
        .await;
    }
    assert!(
        live.sees("steady", "1", "one", Duration::from_secs(30))
            .await,
        "the steady index stays current beside the blocked ones"
    );
    for index in ["held_stop", "held_skip"] {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !matches!(live.health(index), LaneHealth::Parked { .. }) {
            live.still_running();
            assert!(Instant::now() < deadline, "{index} never parked");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(live.health(index).kind(), "blocked", "{index}");
        if let LaneHealth::Parked { reason, .. } = live.health(index) {
            assert!(
                reason.to_string().contains("FORBIDDEN/8"),
                "OpenSearch's reason is kept: {reason}"
            );
        }
    }
    exec(
        &pg.pool,
        "INSERT INTO steady (id, name) VALUES ('3', 'three')",
    )
    .await;
    assert!(
        live.sees("steady", "3", "three", Duration::from_secs(30))
            .await,
        "the steady index keeps up while the others are parked"
    );
    assert!(
        live.quarantined.0.lock().unwrap().is_empty(),
        "a write block quarantines nothing (#404): {:?}",
        live.quarantined.0.lock().unwrap()
    );
    let pinning = live.progress.pinned_by();
    assert!(
        pinning == Some(lane("held_stop")) || pinning == Some(lane("held_skip")),
        "a blocked lane pins the slot: {pinning:?}"
    );

    for index in ["held_stop", "held_skip"] {
        live.settings(index, serde_json::json!({ "index.blocks.write": null }))
            .await;
    }
    for index in ["held_stop", "held_skip"] {
        for (id, name) in [("1", "one"), ("2", "two")] {
            assert!(
                live.sees(index, id, name, Duration::from_secs(90)).await,
                "{index}/{id} lands once the block is lifted"
            );
        }
        assert!(live.health(index).is_open(), "{index} opened again");
    }
    exec(
        &pg.pool,
        "INSERT INTO held_stop (id, name) VALUES ('4', 'four')",
    )
    .await;
    assert!(
        live.sees("held_stop", "4", "four", Duration::from_secs(30))
            .await,
        "the reopened lane writes as usual"
    );
    assert!(live.quarantined.0.lock().unwrap().is_empty());
    live.task.abort();
}

/// The generation behind `lost` is deleted while the deployment runs: its
/// lane is staged again into a fresh generation and backfilled, while
/// `steady` stays current. No restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_generation_deleted_mid_run_is_staged_again_alone() {
    let pg = start_postgres("lost").await;
    let os = start_opensearch("lost").await;
    let live = deploy(
        &pg,
        &os,
        &["lost", "steady"],
        FailurePolicies::new(FailurePolicy::Stop),
    )
    .await;
    exec(
        &pg.pool,
        "INSERT INTO lost (id, name) VALUES ('1', 'one'), ('2', 'two')",
    )
    .await;
    live.seeded(2).await;
    assert!(live.sees("lost", "1", "one", Duration::from_secs(30)).await);

    let generations: serde_json::Value = live
        .http
        .get(format!("{}/_alias/lost", live.os))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let generation = generations
        .as_object()
        .and_then(|held| held.keys().next().cloned())
        .expect("the convenience alias names the generation");
    let deleted = live
        .http
        .delete(format!("{}/{generation}", live.os))
        .send()
        .await
        .unwrap();
    assert!(deleted.status().is_success());

    exec(
        &pg.pool,
        "INSERT INTO lost (id, name) VALUES ('3', 'three')",
    )
    .await;
    exec(
        &pg.pool,
        "INSERT INTO steady (id, name) VALUES ('1', 'one')",
    )
    .await;
    assert!(
        live.sees("steady", "1", "one", Duration::from_secs(30))
            .await,
        "the other index stays current"
    );
    for (id, name) in [("1", "one"), ("2", "two"), ("3", "three")] {
        assert!(
            live.sees("lost", id, name, Duration::from_secs(90)).await,
            "lost/{id} is back: the change, and the backfill of the rest"
        );
    }
    assert!(live.health("lost").is_open());
    live.task.abort();
}

/// A table per index (`id text`, `name text`), a publication and a slot, then
/// both engines over a real OpenSearch sink, the sink engine staged first.
async fn deploy(pg: &Pg, os: &Os, indexes: &[&str], policies: FailurePolicies) -> Deployment {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
    for index in indexes {
        exec(
            &pg.pool,
            &format!("CREATE TABLE {index} (id text PRIMARY KEY, name text)"),
        )
        .await;
    }
    exec(
        &pg.pool,
        &format!("CREATE PUBLICATION flusso FOR TABLE {}", indexes.join(", ")),
    )
    .await;
    exec(
        &pg.pool,
        "SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')",
    )
    .await;
    let spec = SourceSpec::new(
        indexes
            .iter()
            .map(|index| (IndexName::try_new(*index).unwrap(), schema(index)))
            .collect::<BTreeMap<_, _>>(),
    );
    let pre_images = spec.pre_images();
    let documents: Arc<dyn DocumentBuilder> = Arc::new(
        PgDocumentBuilder::connect(&pg.url, Arc::new(spec))
            .await
            .unwrap(),
    );
    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(pg.port);
    let capture: Arc<dyn ChangeCapture> = Arc::new(
        WalChangeCapture::new(replication, pg.url.clone())
            .with_pre_image_management(pre_images, true),
    );
    let name = SinkName::try_new(SINK).unwrap();
    let stream: Arc<dyn Stream> = Arc::new(ChannelStream::over(
        1024,
        documents.backfill_indexes(),
        [name.clone()],
    ));
    let progress = Arc::new(Progress::of(documents.backfill_indexes(), [name.clone()]));
    let quarantined = Arc::new(Quarantined::default());
    let sink: Arc<dyn Sink> = Arc::new(opensearch_sink(&os.url));
    let sink_engine = Arc::new(
        SinkEngine::new(
            name.clone(),
            sink,
            Arc::clone(&stream),
            documents.index_mappings().await.unwrap(),
            Arc::clone(&progress),
        )
        .with_writes(4)
        .with_failure_policies(policies)
        .with_backoff(engine::Backoff::new(Duration::from_secs(1), Duration::from_secs(2)).unwrap())
        .with_observer(Arc::clone(&quarantined) as Arc<dyn Observer>),
    );
    sink_engine
        .stage(capture.continuity().await.unwrap())
        .await
        .unwrap();
    let ingest = IngestEngine::new(
        capture,
        documents,
        stream,
        vec![name],
        Arc::clone(&progress),
    )
    .with_builds(common::e2e_builds())
    .with_budget(common::e2e_budget());
    let running = Arc::clone(&sink_engine);
    let task = tokio::spawn(async move {
        let (_control, mut control) = tokio::sync::mpsc::channel::<SinkControl>(1);
        tokio::select! {
            result = ingest.run() => result,
            result = running.run(&mut control) => result,
        }
    });
    Deployment {
        task,
        sink: sink_engine,
        progress,
        quarantined,
        http: reqwest::Client::new(),
        os: os.url.clone(),
    }
}

fn schema(table: &str) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: common::table(table),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(common::column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![keyword("id"), keyword("name")],
    }
}

fn keyword(name: &str) -> Field {
    Field {
        field: common::field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: common::column(name),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

fn opensearch_sink(url: &str) -> OpensearchSink {
    let config = sink_opensearch::OpensearchConfig {
        url: Secret::Value(url.to_owned()),
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
        refresh_interval: "1s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    OpensearchSink::from_config(&SinkName::try_new(SINK).unwrap(), &config).unwrap()
}

/// This test process's containers carry its pid, so a leftover one is named.
fn container(role: &str, test: &str) -> String {
    format!("flusso-lane-parking-{}-{test}-{role}", std::process::id())
}

async fn start_postgres(test: &str) -> Pg {
    let container = test_postgres::image()
        .with_cmd([
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_wal_senders=10",
            "-c",
            "max_replication_slots=10",
        ])
        .with_container_name(container("pg", test))
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    Pg {
        _container: container,
        pool,
        url,
        port,
    }
}

async fn start_opensearch(test: &str) -> Os {
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
        .with_container_name(container("os", test))
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let port = container.get_host_port_ipv4(9200).await.unwrap();
    Os {
        _container: container,
        url: format!("http://127.0.0.1:{port}"),
    }
}

async fn exec(pool: &PgPool, sql: &str) {
    sqlx::query(AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}
