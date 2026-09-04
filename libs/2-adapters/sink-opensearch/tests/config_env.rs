#![allow(
    unsafe_code,
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used
)]

//! Resolution of the OpenSearch URL and credentials in the running
//! environment: the per-sink `<NAME>_OPENSEARCH_*` overrides.
//!
//! Each test uses its own sink name, so the variables it touches are its own
//! and the tests cannot race each other.

use kernel::{AdapterConfig, Options, SinkName};
use sink_opensearch::OpensearchConfig;

fn config(toml: &str) -> OpensearchConfig {
    let options: Options = toml::from_str(toml).unwrap();
    OpensearchConfig::from_options(options).unwrap()
}

fn name(sink: &str) -> SinkName {
    SinkName::try_new(sink).unwrap()
}

fn set(var: &str, value: &str) {
    // SAFETY: each test owns the variables named after its own sink.
    unsafe { std::env::set_var(var, value) };
}

fn unset(var: &str) {
    // SAFETY: see `set`.
    unsafe { std::env::remove_var(var) };
}

#[test]
fn override_variable_beats_a_literal_url() {
    set("ALPHA_OPENSEARCH_URL", "https://env.example:9200");
    let cfg = config("url = \"https://literal.example:9200\"");
    assert_eq!(
        cfg.resolve_url(&name("alpha")).unwrap().as_ref(),
        "https://env.example:9200"
    );
    unset("ALPHA_OPENSEARCH_URL");
    assert_eq!(
        cfg.resolve_url(&name("alpha")).unwrap().as_ref(),
        "https://literal.example:9200"
    );
}

#[test]
fn credentials_are_filled_by_their_variables() {
    let cfg = config("url = \"https://search:9200\"");
    assert_eq!(cfg.resolve_username(&name("beta")).unwrap(), None);
    set("BETA_OPENSEARCH_USERNAME", "indexer");
    set("BETA_OPENSEARCH_PASSWORD", "pw");
    assert_eq!(
        cfg.resolve_username(&name("beta")).unwrap().as_deref(),
        Some("indexer")
    );
    assert_eq!(
        cfg.resolve_password(&name("beta")).unwrap().as_deref(),
        Some("pw")
    );
    unset("BETA_OPENSEARCH_USERNAME");
    unset("BETA_OPENSEARCH_PASSWORD");
}

#[test]
fn variables_are_namespaced_per_sink() {
    set("GAMMA_OPENSEARCH_URL", "https://gamma.env:9200");
    let cfg = config("url = \"https://literal:9200\"");
    assert_eq!(
        cfg.resolve_url(&name("gamma")).unwrap().as_ref(),
        "https://gamma.env:9200"
    );
    assert_eq!(
        cfg.resolve_url(&name("delta")).unwrap().as_ref(),
        "https://literal:9200"
    );
    unset("GAMMA_OPENSEARCH_URL");
}

#[test]
fn explicit_env_reference_beats_the_override() {
    set("EPSILON_OPENSEARCH_URL", "https://override:9200");
    set("TEST_EXPLICIT_OS_URL", "https://explicit:9200");
    let cfg = config("url = { env = \"TEST_EXPLICIT_OS_URL\" }");
    assert_eq!(
        cfg.resolve_url(&name("epsilon")).unwrap().as_ref(),
        "https://explicit:9200"
    );
    unset("EPSILON_OPENSEARCH_URL");
    unset("TEST_EXPLICIT_OS_URL");
}

#[test]
fn invalid_resolved_url_is_rejected() {
    let cfg = config("url = \"not a url\"");
    assert!(cfg.resolve_url(&name("zeta")).is_err());
}
