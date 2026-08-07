//! Real-service e2e for HTTP basic auth against a **secured** OpenSearch
//! (security plugin enabled, special-character admin password).
//!
//! Repro harness for issue #93: a `flusso-query` consumer reported 401 on
//! every operation with credentials the flusso engine (the OpenSearch sink)
//! accepts on the same cluster. Both crates attach auth the same way
//! (`reqwest::RequestBuilder::basic_auth` on every request), so this suite
//! pins down whether the query client's auth actually works end to end:
//!
//! * the **write side** seeds through the real sink with the credentials —
//!   proving the cluster + credentials are valid, exactly as reported;
//! * the **read side** runs every query-client operation (`search`, `count`,
//!   `get`) through [`flusso_query::Client::basic_auth`] with the same
//!   username/password — including a password full of special characters;
//! * wrong/missing credentials must yield `Error::Status { status: 401 }`,
//!   proving the cluster genuinely enforces auth (the pass isn't vacuous);
//! * whitespace-mangled credentials (the CI failure mode behind the report)
//!   are trimmed by [`flusso_query::Client::basic_auth`] and authenticate;
//! * a second test keeps the demo config's self-signed **https** on and
//!   reaches it with [`flusso_query::Client::tls_verify`]`(false)` — the
//!   read-side mirror of the sink's `tls_verify` config.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query --run-ignored all
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use flusso_query::{Client, Error, FlussoDocument, FlussoIndex, Segment};
use schema_core::{
    ContentHash, FieldName, GenericValue, IndexMapping, IndexName, Mapping, MappingType,
    ResolvedField,
};
use sinks_core::Sink;
use sinks_opensearch::OpensearchSink;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

const USERNAME: &str = "admin";

/// The real-world special-character password from the issue-#93 report.
/// Strong enough for OpenSearch 2.12+'s initial-admin password strength check.
const PASSWORD: &str = "971b#o^1!*#7Hqf8K&!@";

#[derive(Debug, serde::Deserialize)]
struct Account {
    email: String,
}
impl FlussoDocument for Account {
    const PATH: &'static [Segment] = &[];
}
impl FlussoIndex for Account {
    const INDEX: &'static str = "accounts";
    const SCHEMA_HASH: &'static str = "00000001";
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts a security-enabled OpenSearch container)"]
async fn query_client_authenticates_like_the_sink() {
    let (_container, base) = start_secured_opensearch().await;

    // ── Write side: the real sink, same credentials — the engine's path ──
    let sink = sink(&base);
    let mapping = mapping("accounts", 1, "email", MappingType::Keyword);
    let index = mapping.index.clone();
    sink.ensure_index(&mapping).await.unwrap();
    sink.upsert(&index, "1", &document("email", "ada@example.com"))
        .await
        .unwrap();
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();

    // ── Read side: every client operation with the same credentials ──
    let client = Client::connect(&base)
        .unwrap()
        .basic_auth(USERNAME, PASSWORD);

    let page = await_hits(&client, &base, 1).await;
    assert_eq!(page.total, 1, "authenticated search sees the seeded doc");
    assert_eq!(page.hits.first().unwrap().source.email, "ada@example.com");

    assert_eq!(
        Account::query().count(&client).await.unwrap(),
        1,
        "authenticated count works",
    );

    let fetched = Account::get(&client, "1").await.unwrap();
    assert_eq!(
        fetched.map(|account| account.email),
        Some("ada@example.com".to_owned()),
        "authenticated get works",
    );

    // ── Auth is genuinely enforced: bad or missing credentials 401 ──
    let wrong = Client::connect(&base)
        .unwrap()
        .basic_auth(USERNAME, "not-the-password");
    assert_eq!(
        status_of(Account::query().send(&wrong).await),
        Some(401),
        "a wrong password is rejected",
    );

    let anonymous = Client::connect(&base).unwrap();
    assert_eq!(
        status_of(Account::query().send(&anonymous).await),
        Some(401),
        "no credentials is rejected",
    );

    // The issue-#93 failure mode: a CI-mangled secret. A trailing newline
    // base64-encodes fine (no header error), so before the trim in
    // `basic_auth` it reached the cluster as silently *different*
    // credentials — a 401 on every operation. The trim absorbs it.
    let mangled = Client::connect(&base)
        .unwrap()
        .basic_auth(format!(" {USERNAME}\t"), format!("{PASSWORD}\n"));
    assert_eq!(
        Account::query().count(&mangled).await.unwrap(),
        1,
        "whitespace-mangled credentials are trimmed and authenticate",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts a security-enabled OpenSearch container)"]
async fn tls_verify_off_reaches_a_self_signed_cluster() {
    let (_container, base) = start_tls_opensearch().await;

    let sink = sink(&base);
    let mapping = mapping("accounts", 1, "email", MappingType::Keyword);
    let index = mapping.index.clone();
    sink.ensure_index(&mapping).await.unwrap();
    sink.upsert(&index, "1", &document("email", "ada@example.com"))
        .await
        .unwrap();
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&index).await.unwrap();

    // Default verification must reject the self-signed cert (a transport
    // error, not a 401) — the knob is load-bearing, not decorative.
    let verifying = Client::connect(&base)
        .unwrap()
        .basic_auth(USERNAME, PASSWORD);
    assert!(
        matches!(
            Account::query().count(&verifying).await,
            Err(Error::Http(_))
        ),
        "a verifying client refuses the self-signed certificate",
    );

    let client = Client::connect(&base)
        .unwrap()
        .basic_auth(USERNAME, PASSWORD)
        .tls_verify(false)
        .unwrap();
    let page = await_hits(&client, &base, 1).await;
    assert_eq!(page.total, 1, "tls_verify(false) + auth reads the doc");
}

// ─────────────────────────────── helpers ────────────────────────────────

fn status_of<T>(result: flusso_query::Result<T>) -> Option<u16> {
    match result {
        Err(Error::Status { status, .. }) => Some(status),
        _ => None,
    }
}

/// Refresh the index (authenticated — the cluster requires it) and search
/// until the seeded document is visible or a deadline trips.
async fn await_hits(
    client: &Client,
    base: &str,
    want: usize,
) -> flusso_query::SearchResponse<Account> {
    let http = insecure_http();
    let deadline = Instant::now() + Duration::from_secs(60);
    let search = Account::query();
    loop {
        let _ = http
            .post(format!("{base}/{}/_refresh", Account::physical_index()))
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .await;
        let page = search.send(client).await.unwrap();
        if page.hits.len() >= want {
            return page;
        }
        assert!(
            Instant::now() < deadline,
            "authenticated search did not return {want} hits in time (got {})",
            page.hits.len(),
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn mapping(logical: &str, hash: u32, field: &str, ty: MappingType) -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(logical).unwrap(),
        hash: ContentHash::new(hash),
        fields: vec![ResolvedField {
            name: FieldName::try_new(field).unwrap(),
            mapping: Mapping {
                mapping_type: ty,
                extra: BTreeMap::new(),
                map_values: None,
                decimal: false,
                enum_order: None,
            },
            nullable: true,
            array: false,
            children: Vec::new(),
        }],
    }
}

fn document(field: &str, value: &str) -> GenericValue {
    let mut map = BTreeMap::new();
    map.insert(field.to_owned(), GenericValue::String(value.to_owned()));
    GenericValue::Map(map)
}

fn sink(base_url: &str) -> OpensearchSink {
    let config = schema_core::OpensearchSink {
        url: schema_core::Secret::Value(base_url.to_owned()),
        username: Some(schema_core::Secret::Value(USERNAME.to_owned())),
        password: Some(schema_core::Secret::Value(PASSWORD.to_owned())),
        tls_verify: false,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 30,
        max_retries: 3,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 0,
        refresh_interval: "1s".to_owned(),
        text_analysis: schema_core::TextAnalysis::Builtin,
        auto_subfields: true,
    };
    let name = schema_core::SinkName::try_new("e2e").unwrap();
    OpensearchSink::from_config(&name, &config).unwrap()
}

/// A raw HTTP client that tolerates the container's self-signed certificate,
/// for out-of-band `_refresh`/health calls in both the http and https tests.
fn insecure_http() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap()
}

/// A single-node OpenSearch with the security plugin **enabled**: the demo
/// security config is installed (users, roles), TLS on the HTTP layer is
/// switched off (the query client speaks plain `http`; auth is what's under
/// test), and the admin password is the hostile [`PASSWORD`].
async fn start_secured_opensearch() -> (ContainerAsync<GenericImage>, String) {
    let container = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/_cluster/health")
                .with_port(9200.tcp())
                .with_basic_auth(USERNAME, PASSWORD)
                .with_expected_status_code(200u16)
                .with_poll_interval(Duration::from_secs(1)),
        ))
        .with_env_var("discovery.type", "single-node")
        .with_env_var("OPENSEARCH_INITIAL_ADMIN_PASSWORD", PASSWORD)
        .with_env_var("plugins.security.ssl.http.enabled", "false")
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let port = container.get_host_port_ipv4(9200).await.unwrap();
    (container, format!("http://127.0.0.1:{port}"))
}

/// Like [`start_secured_opensearch`] but with the demo config's self-signed
/// TLS left **on** — the HTTP layer serves `https`. Readiness is polled by
/// hand (the stock HTTP wait strategy can't speak https to a self-signed
/// cert): the container is up when `/_cluster/health` answers 200 over an
/// insecure client with the admin credentials.
async fn start_tls_opensearch() -> (ContainerAsync<GenericImage>, String) {
    let container = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_env_var("discovery.type", "single-node")
        .with_env_var("OPENSEARCH_INITIAL_ADMIN_PASSWORD", PASSWORD)
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let port = container.get_host_port_ipv4(9200).await.unwrap();
    let base = format!("https://127.0.0.1:{port}");

    let http = insecure_http();
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let ready = http
            .get(format!("{base}/_cluster/health"))
            .basic_auth(USERNAME, Some(PASSWORD))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if ready {
            return (container, base);
        }
        assert!(
            Instant::now() < deadline,
            "secured https OpenSearch did not become ready in time",
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
