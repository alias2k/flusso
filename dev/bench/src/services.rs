//! Postgres and OpenSearch for a run: containers by default, or an existing
//! pair through `BENCH_PG_URL` / `BENCH_OS_URL` (the Postgres must have
//! `wal_level = logical` and both must be empty — the harness creates the
//! scenario's tables and flusso creates its indexes).

use std::time::Duration;

use anyhow::{Context, Result};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// The same images the e2e suites use; recorded in every report so an image
/// bump is visible as one.
pub(crate) const POSTGRES_IMAGE: &str = "postgres:16-alpine";
pub(crate) const OPENSEARCH_IMAGE: &str = "opensearchproject/opensearch:2";

#[derive(Debug)]
pub(crate) struct Services {
    pub(crate) pg_url: String,
    pub(crate) os_url: String,
    /// What actually served the run: the image, or `external` for an env URL.
    pub(crate) pg_image: String,
    pub(crate) os_image: String,
    _pg: Option<ContainerAsync<Postgres>>,
    _os: Option<ContainerAsync<GenericImage>>,
}

pub(crate) async fn start() -> Result<Services> {
    let (pg_url, pg_image, pg) = match std::env::var("BENCH_PG_URL") {
        Ok(url) => (url, "external".to_owned(), None),
        Err(_) => {
            tracing::info!(image = POSTGRES_IMAGE, "starting Postgres");
            let container = Postgres::default()
                .with_tag("16-alpine")
                .with_cmd([
                    "postgres",
                    "-c",
                    "wal_level=logical",
                    "-c",
                    "max_wal_senders=10",
                    "-c",
                    "max_replication_slots=10",
                ])
                .start()
                .await
                .context("starting the Postgres container")?;
            let port = container.get_host_port_ipv4(5432).await?;
            (
                format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres"),
                POSTGRES_IMAGE.to_owned(),
                Some(container),
            )
        }
    };
    let (os_url, os_image, os) = match std::env::var("BENCH_OS_URL") {
        Ok(url) => (url, "external".to_owned(), None),
        Err(_) => {
            tracing::info!(image = OPENSEARCH_IMAGE, "starting OpenSearch");
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
                .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms1g -Xmx1g")
                .with_startup_timeout(Duration::from_secs(180))
                .start()
                .await
                .context("starting the OpenSearch container")?;
            let port = container.get_host_port_ipv4(9200).await?;
            (
                format!("http://127.0.0.1:{port}"),
                OPENSEARCH_IMAGE.to_owned(),
                Some(container),
            )
        }
    };
    Ok(Services {
        pg_url,
        os_url,
        pg_image,
        os_image,
        _pg: pg,
        _os: os,
    })
}
