//! Live helpers shared by the e2e binaries of `flusso-cli`: Postgres and
//! OpenSearch containers, and the real `flusso run` binary started over a
//! config, so a test sees what a deployment does at start.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use sqlx::AssertSqlSafe;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// A Postgres 16 with logical replication, seeded with `setup` (statements
/// split on `;`), and its URL.
pub(crate) async fn start_postgres(setup: &str) -> (ContainerAsync<Postgres>, String) {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .with_cmd(["postgres", "-c", "wal_level=logical"])
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in setup.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(AssertSqlSafe(statement.to_owned()))
            .execute(&pool)
            .await
            .unwrap();
    }
    (container, url)
}

/// An OpenSearch 2 with security off, and its URL.
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

/// One deployment: an index `name` of the root table `t` whose fields are
/// `fields` (YAML list items, indented for the top level), with `index_extra`
/// appended to its `[[index]]` table and `sink_extra` to `[sinks.primary]`.
#[derive(Debug, Clone)]
pub(crate) struct Deployment<'a> {
    pub(crate) name: &'a str,
    pub(crate) fields: &'a str,
    pub(crate) index_extra: &'a str,
    pub(crate) sink_extra: &'a str,
}

/// What `flusso run` did with a deployment.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// It started and the index holds this many documents.
    Indexed(u64),
    /// It exited before indexing the expected count; its stderr.
    Exited(String),
    /// It kept running but never reached the expected count.
    Stalled { count: u64, stderr: String },
}

/// Start `flusso run` over `deployment` and wait until its index holds
/// `expected` documents, it exits, or a minute passes; then stop it. Counted
/// over every generation (`{name}_*`), refreshed: a sink that stops on a
/// rejection never finishes its seed, so the alias never points at the
/// documents it wrote, and a seed is written with refresh off.
pub(crate) async fn run(
    pg_url: &str,
    os_url: &str,
    deployment: &Deployment<'_>,
    expected: u64,
) -> Outcome {
    let name = deployment.name;
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-start-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut schema =
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n"
            .to_owned();
    for line in deployment.fields.lines() {
        schema.push_str("  ");
        schema.push_str(line);
        schema.push('\n');
    }
    std::fs::write(dir.join(format!("{name}.schema.yml")), schema).unwrap();
    let config = format!(
        "[source]\ntype = \"postgres\"\nconnection_url = \"{pg_url}\"\nslot = \"s_{name}\"\n\
         publication = \"p_{name}\"\n\n[sinks.primary]\ntype = \"opensearch\"\nurl = \"{os_url}\"\n\
         number_of_replicas = 0\n{}\n[[index]]\nname = \"{name}\"\nschema = \"{name}.schema.yml\"\n\
         enabled = true\n{}\n",
        deployment.sink_extra, deployment.index_extra
    );
    std::fs::write(dir.join("flusso.toml"), config).unwrap();
    let stderr_path = dir.join("stderr.log");
    let mut child: Child = Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(&dir)
        .args([
            "run",
            "--config",
            "flusso.toml",
            "--public-address",
            "127.0.0.1:0",
            "--private-address",
            "127.0.0.1:0",
        ])
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let http = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut count = 0;
    let outcome = loop {
        if child.try_wait().unwrap().is_some() {
            break Outcome::Exited(std::fs::read_to_string(&stderr_path).unwrap_or_default());
        }
        count = document_count(&http, os_url, name).await.unwrap_or(count);
        if count == expected {
            break Outcome::Indexed(count);
        }
        if Instant::now() > deadline {
            break Outcome::Stalled {
                count,
                stderr: std::fs::read_to_string(&stderr_path).unwrap_or_default(),
            };
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let _ = child.kill();
    let _ = child.wait();
    outcome
}

async fn document_count(http: &reqwest::Client, os_url: &str, index: &str) -> Option<u64> {
    // A seed is written with refresh off, so refresh before counting.
    http.post(format!("{os_url}/{index}_*/_refresh"))
        .send()
        .await
        .ok()?;
    let response = http
        .get(format!("{os_url}/{index}_*/_count"))
        .send()
        .await
        .ok()?;
    let body: serde_json::Value = response.json().await.ok()?;
    body.get("count")?.as_u64()
}

/// Create an ingest pipeline `id` that removes the field `v`.
pub(crate) async fn pipeline_removing_v(os_url: &str, id: &str) {
    let response = reqwest::Client::new()
        .put(format!("{os_url}/_ingest/pipeline/{id}"))
        .json(&serde_json::json!({
            "processors": [{ "remove": { "field": "v", "ignore_missing": true } }],
        }))
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{:?}",
        response.text().await
    );
}
