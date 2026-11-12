//! Real-service e2e for the sink over `https://` (issue #239): the trust
//! roots the sink pins, and the crypto provider underneath them.
//!
//! The container is the stock OpenSearch image with its security plugin's
//! **demo configuration**: TLS on the HTTP layer with a certificate signed by
//! the demo "Example Com Inc. Root CA", which no public root bundle trusts, and
//! HTTP Basic auth as `admin`. Nothing is committed: the image generates it.
//!
//! Against it, through the real sink, it proves:
//!
//! - **`tls_verify = true` refuses the certificate**: `ensure_index` and a flush
//!   fail promptly with a transport error, never a panic or a hang. The
//!   certificate names `127.0.0.1`, so the only thing wrong with it is its
//!   issuer: this is the Mozilla bundle (`tls_certs_only`) in force. The sink's
//!   message keeps only reqwest's top line (`error sending request for url`),
//!   not the certificate cause, so the cause is proven by difference: the same
//!   endpoint and credentials with `tls_verify = false` succeed.
//! - **`tls_verify = false` writes**: the sink creates the index, flushes a
//!   document, and the document reads back over `https://`.
//! - **Both crypto-provider paths work**: with no process-default rustls
//!   provider (the client falls back to aws-lc-rs) and with `ring` installed as
//!   the default first, as `pgwire-replication` does on its first TLS
//!   connection. nextest runs each test in its own process, so the install
//!   does not leak.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-sink-opensearch --run-ignored all -E 'binary(tls)'
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use kernel::{
    ContentHash, Envelope, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    Position, ResolvedField, Secret, SinkName,
};
use serde_json::Value;
use sink::Sink;
use sink_opensearch::{OpensearchConfig, OpensearchSink, TextAnalysis};
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// The demo admin password. 2.12+ refuses to install the demo configuration
/// without one, and refuses a weak one.
const PASSWORD: &str = "Fl-usso_e2e_9Tls!";
const LOGICAL: &str = "secure";
/// The hash alias the sink serves a seeded index through: `{logical}_{hash}`.
const ALIAS: &str = "secure_00000239";
/// Long enough for a TLS handshake on a loaded runner, short enough that a
/// hang fails the test instead of the nextest slow-timeout.
const PROMPT: Duration = Duration::from_secs(30);

/// A client for the test's own reads: it trusts anything, like `curl -k`.
fn insecure_reader() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap()
}

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
    let reader = insecure_reader();
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let healthy = reader
            .get(format!("{base}/_cluster/health"))
            .basic_auth("admin", Some(PASSWORD))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if healthy {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "opensearch at {base} never became healthy"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn sink(base: &str, tls_verify: bool) -> OpensearchSink {
    let config = OpensearchConfig {
        url: Secret::Value(base.to_owned()),
        username: Some(Secret::Value("admin".to_owned())),
        password: Some(Secret::Value(PASSWORD.to_owned())),
        tls_verify,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 10,
        max_retries: 0,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 0,
        refresh_interval: "1s".to_owned(),
        text_analysis: TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    OpensearchSink::from_config(&SinkName::try_new("secure").unwrap(), &config).unwrap()
}

fn mapping() -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(LOGICAL).unwrap(),
        hash: ContentHash::new(0x239),
        fields: vec![ResolvedField {
            name: FieldName::try_new("title").unwrap(),
            mapping: Mapping {
                mapping_type: MappingType::Keyword,
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
        }],
    }
}

fn upsert(id: &str, title: &str) -> Envelope {
    let doc = BTreeMap::from([("title".to_owned(), GenericValue::String(title.to_owned()))]);
    Envelope::upsert(
        IndexName::try_new(LOGICAL).unwrap(),
        id,
        GenericValue::Map(doc),
        Some(Position(0)),
        chrono::Utc::now(),
    )
}

/// A request that never got an HTTP response: what a refused handshake
/// surfaces as. A status error or a panic is not this.
fn is_transport_error(message: &str) -> bool {
    message.contains("error sending request")
}

/// The sink with `tls_verify = true` refuses the demo certificate on every
/// request path, promptly.
async fn verifying_sink_refuses_the_certificate(base: &str) {
    let sink = sink(base, true);
    let mapping = mapping();

    let ensure = tokio::time::timeout(PROMPT, sink.ensure_index(&mapping))
        .await
        .expect("ensure_index must fail promptly, not hang");
    let error = ensure
        .expect_err("an untrusted certificate must be refused")
        .to_string();
    assert!(is_transport_error(&error), "not a transport error: {error}");

    sink.apply(&upsert("1", "refused")).await.unwrap();
    let flush = tokio::time::timeout(PROMPT, sink.flush(true))
        .await
        .expect("flush must fail promptly, not hang");
    let error = flush
        .expect_err("an untrusted certificate must be refused")
        .to_string();
    assert!(is_transport_error(&error), "not a transport error: {error}");
}

/// The sink with `tls_verify = false` creates the index, flushes a document,
/// and the document reads back over `https://`.
async fn insecure_sink_writes_over_https(base: &str) {
    let sink = sink(base, false);
    let mapping = mapping();

    sink.ensure_index(&mapping).await.unwrap();
    sink.apply(&upsert("1", "over tls")).await.unwrap();
    let report = sink.flush(true).await.unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    sink.mark_seeded(&mapping.index).await.unwrap();

    let doc: Value = insecure_reader()
        .get(format!("{base}/{ALIAS}/_doc/1"))
        .basic_auth("admin", Some(PASSWORD))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(doc["_source"]["title"], "over tls", "{doc}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn https_with_no_process_crypto_provider() {
    assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    let (_container, base) = start_tls_opensearch().await;

    verifying_sink_refuses_the_certificate(&base).await;
    insecure_sink_writes_over_https(&base).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn https_with_ring_installed_as_the_process_default() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("no provider is installed yet in this process");
    let (_container, base) = start_tls_opensearch().await;

    verifying_sink_refuses_the_certificate(&base).await;
    insecure_sink_writes_over_https(&base).await;
}
