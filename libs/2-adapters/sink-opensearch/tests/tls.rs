//! Real-service e2e for the sink over `https://` (issue #239): the trust
//! roots the sink pins, and the crypto provider underneath them.
//!
//! The container is the stock OpenSearch image with its security plugin's
//! **demo configuration**: TLS on the HTTP layer with a certificate signed by
//! the demo "Example Com Inc. Root CA", which no public root bundle trusts, and
//! HTTP Basic auth as `admin`. Nothing is committed: the image generates it.
//!
//! One test, one container, run in order:
//!
//! 1. **The OS trust store trusts the demo CA.** `SSL_CERT_FILE` names the
//!    container's `root-ca.pem`, and a reqwest client on the OS trust store
//!    (its default) reads the cluster. That is the control for step 2.
//! 2. **No process-default rustls provider** (asserted; the client falls back
//!    to aws-lc-rs):
//!    - `tls_verify = true` refuses the certificate: `ensure_index` and a flush
//!      fail promptly with a transport error, never a panic or a hang. The
//!      certificate names `127.0.0.1` and the OS store trusts its issuer, so
//!      the refusal is the Mozilla bundle (`tls_certs_only`) in force, not the
//!      OS store.
//!    - `tls_verify = false` creates the index, flushes a document, and the
//!      document reads back over `https://`.
//! 3. **`ring` installed as the process default**, as `pgwire-replication`
//!    does on its first TLS connection: new sinks (a reqwest client keeps the
//!    provider it was built with), the same two checks, on a second index.
//!
//! The steps share process-wide state (the provider, `SSL_CERT_FILE`), so they
//! are one test: the binary is safe under plain `cargo test` as well as nextest.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-sink-opensearch --run-ignored all -E 'binary(tls)'
//! ```

#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use kernel::{
    ContentHash, Envelope, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    Position, ResolvedField, Secret, SinkName,
};
use serde_json::Value;
use sink::Sink;
use sink_opensearch::{OpensearchConfig, OpensearchSink, TextAnalysis};
use testcontainers_modules::testcontainers::core::{ExecCommand, IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// The demo admin password. 2.12+ refuses to install the demo configuration
/// without one, and refuses a weak one.
const PASSWORD: &str = "Fl-usso_e2e_9Tls!";
const HASH: u32 = 0x239;
/// Where the demo installer writes the CA that signed the HTTP certificate.
const ROOT_CA: &str = "/usr/share/opensearch/config/root-ca.pem";
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

fn mapping(logical: &str) -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(logical).unwrap(),
        hash: ContentHash::new(HASH),
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

fn upsert(logical: &str, id: &str, title: &str) -> Envelope {
    let doc = BTreeMap::from([("title".to_owned(), GenericValue::String(title.to_owned()))]);
    Envelope::upsert(
        IndexName::try_new(logical).unwrap(),
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
async fn verifying_sink_refuses_the_certificate(base: &str, logical: &str) {
    let sink = sink(base, true);
    let mapping = mapping(logical);

    let ensure = tokio::time::timeout(PROMPT, sink.ensure_index(&mapping))
        .await
        .expect("ensure_index must fail promptly, not hang");
    let error = ensure
        .expect_err("an untrusted certificate must be refused")
        .to_string();
    assert!(is_transport_error(&error), "not a transport error: {error}");

    sink.apply(&upsert(logical, "1", "refused")).await.unwrap();
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
async fn insecure_sink_writes_over_https(base: &str, logical: &str) {
    let sink = sink(base, false);
    let mapping = mapping(logical);

    sink.ensure_index(&mapping).await.unwrap();
    sink.apply(&upsert(logical, "1", "over tls")).await.unwrap();
    let report = sink.flush(true).await.unwrap();
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    sink.mark_seeded(&mapping.index).await.unwrap();

    let doc: Value = insecure_reader()
        .get(format!("{base}/{logical}_{HASH:08x}/_doc/1"))
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

/// Copy the container's demo root CA to `path`, where `SSL_CERT_FILE` points.
async fn copy_root_ca(container: &ContainerAsync<GenericImage>, path: &PathBuf) {
    let mut cat = container
        .exec(ExecCommand::new(["cat", ROOT_CA]))
        .await
        .unwrap();
    let pem = cat.stdout_to_vec().await.unwrap();
    assert!(
        String::from_utf8_lossy(&pem).contains("BEGIN CERTIFICATE"),
        "no CA at {ROOT_CA}"
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, pem).unwrap();
}

/// A client on the OS trust store, reqwest's default: the control that the
/// demo CA in `SSL_CERT_FILE` is trusted by anything that reads the store.
async fn os_store_client_reads(base: &str) {
    let health = reqwest::Client::new()
        .get(format!("{base}/_cluster/health"))
        .basic_auth("admin", Some(PASSWORD))
        .send()
        .await
        .expect("the OS trust store holds the demo CA")
        .error_for_status()
        .unwrap();
    assert!(health.status().is_success());
}

#[test]
#[ignore = "needs Docker (starts an OpenSearch container)"]
fn https_trusts_only_the_pinned_bundle_under_either_crypto_provider() {
    let ca_file = std::env::temp_dir()
        .join(format!("flusso-sink-tls-{}", std::process::id()))
        .join("root-ca.pem");
    // SAFETY: set before the runtime starts, from the only test in this binary,
    // so no other thread reads the environment. The file is written once the
    // container is up; the store is read when a client is built.
    unsafe { std::env::set_var("SSL_CERT_FILE", &ca_file) };

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            assert!(
                rustls::crypto::CryptoProvider::get_default().is_none(),
                "nothing in this process installs a provider before this point"
            );
            let (container, base) = start_tls_opensearch().await;
            copy_root_ca(&container, &ca_file).await;
            os_store_client_reads(&base).await;

            verifying_sink_refuses_the_certificate(&base, "secure").await;
            insecure_sink_writes_over_https(&base, "secure").await;
            assert!(rustls::crypto::CryptoProvider::get_default().is_none());

            rustls::crypto::ring::default_provider()
                .install_default()
                .expect("no provider is installed yet in this process");
            verifying_sink_refuses_the_certificate(&base, "secure_ring").await;
            insecure_sink_writes_over_https(&base, "secure_ring").await;
        });
    let _ = std::fs::remove_dir_all(ca_file.parent().unwrap());
}
