//! Live e2e for `flusso-query` over `https://` (issues #239, #243): the client
//! pins the Mozilla root bundle (`tls_certs_only`), so a certificate no public
//! root vouches for is refused, until its CA is added with `Client::ca_file`.
//!
//! The container is the stock OpenSearch image with its security plugin's
//! **demo configuration**: TLS on the HTTP layer with a certificate signed by
//! the demo "Example Com Inc. Root CA" and naming `127.0.0.1`, plus HTTP Basic
//! auth as `admin`. The sink, with `tls_verify = false`, seeds one document;
//! the client, which has no switch to skip verification, must refuse to read
//! it with an [`Error::Http`] whose cause is the certificate. Given the
//! container's root CA through `ca_file`, the same client reads the document,
//! verification on.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query-e2e --run-ignored all -E 'binary(tls)'
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    unused_crate_dependencies
)]

mod common;

use std::error::Error as _;
use std::time::{Duration, Instant};

use flusso_query::{Client, Error};
use kernel::{MappingType, Secret, SinkName};
use serde_json::Value;
use sink_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::{ExecCommand, IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// The demo admin password. 2.12+ refuses to install the demo configuration
/// without one, and refuses a weak one.
const PASSWORD: &str = "Fl-usso_e2e_9Tls!";
const LOGICAL: &str = "secure";
const HASH: u32 = 0x239;
/// Where the demo installer writes the CA that signed the HTTP certificate.
const ROOT_CA: &str = "/usr/share/opensearch/config/root-ca.pem";

async fn start_tls_opensearch() -> (ContainerAsync<GenericImage>, String) {
    let container = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::Nothing)
        .with_env_var("discovery.type", "single-node")
        .with_env_var("OPENSEARCH_INITIAL_ADMIN_PASSWORD", PASSWORD)
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .start()
        .await
        .expect("opensearch container should start");
    let port = container.get_host_port_ipv4(9200).await.unwrap();
    let base = format!("https://127.0.0.1:{port}");
    wait_until_healthy(&base).await;
    (container, base)
}

/// testcontainers' HTTP wait cannot skip certificate verification, so poll the
/// health endpoint here, until the security index is initialised and auth works.
async fn wait_until_healthy(base: &str) {
    let reader = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let last = match reader
            .get(format!("{base}/_cluster/health"))
            .basic_auth("admin", Some(PASSWORD))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => return,
            Ok(response) => format!("HTTP {}", response.status()),
            Err(error) => format!("{error:?}"),
        };
        assert!(
            Instant::now() < deadline,
            "opensearch at {base} never became healthy; last: {last}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn insecure_sink(base: &str) -> OpensearchSink {
    let config = sink_opensearch::OpensearchConfig {
        url: Secret::Value(base.to_owned()),
        username: Some(Secret::Value("admin".to_owned())),
        password: Some(Secret::Value(PASSWORD.to_owned())),
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
    OpensearchSink::from_config(&SinkName::try_new("e2e").unwrap(), &config).unwrap()
}

/// The container's demo root CA, written to a file of this test's own.
async fn root_ca_file(container: &ContainerAsync<GenericImage>) -> std::path::PathBuf {
    let mut cat = container
        .exec(ExecCommand::new(["cat", ROOT_CA]))
        .await
        .unwrap();
    let pem = cat.stdout_to_vec().await.unwrap();
    assert!(
        String::from_utf8_lossy(&pem).contains("BEGIN CERTIFICATE"),
        "no CA at {ROOT_CA}"
    );
    let path = std::env::temp_dir().join(format!("flusso-query-tls-{}.pem", std::process::id()));
    std::fs::write(&path, pem).unwrap();
    path
}

/// Every message in the error's cause chain, lowercased.
fn cause_chain(error: &reqwest::Error) -> String {
    let mut chain = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        chain.push_str(" / ");
        chain.push_str(&cause.to_string());
        source = cause.source();
    }
    chain.to_lowercase()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn the_client_refuses_a_private_ca_until_it_is_given_its_ca_file() {
    let (container, base) = start_tls_opensearch().await;
    common::seed(
        &insecure_sink(&base),
        common::mapping(LOGICAL, HASH, "title", MappingType::Keyword),
        "1",
        common::document("title", "over tls"),
    )
    .await;

    let client = Client::connect(&base)
        .unwrap()
        .basic_auth("admin", PASSWORD);
    let read = tokio::time::timeout(
        Duration::from_secs(30),
        client.get_one::<Value>(LOGICAL, &format!("{HASH:08x}"), "1"),
    )
    .await
    .expect("the read must fail promptly, not hang");

    match read {
        Err(Error::Http(error)) => {
            let chain = cause_chain(&error);
            assert!(
                chain.contains("certificate") || chain.contains("unknownissuer"),
                "not a certificate error: {chain}"
            );
        }
        other => panic!("expected a certificate error, got {other:?}"),
    }

    let ca_file = root_ca_file(&container).await;
    let trusting = Client::connect(&base)
        .unwrap()
        .basic_auth("admin", PASSWORD)
        .ca_file(&ca_file)
        .unwrap();
    let document = trusting
        .get_one::<Value>(LOGICAL, &format!("{HASH:08x}"), "1")
        .await
        .expect("the client trusts the private CA from ca_file")
        .expect("the seeded document");
    assert_eq!(document["title"], "over tls", "{document}");
    let _ = std::fs::remove_file(ca_file);
}
