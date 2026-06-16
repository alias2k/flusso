#![allow(unused_crate_dependencies)]
#![allow(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use schema::{Config, Sink};
use schema_config_toml::{ConfigToml, ParseError};
use schema_core::{ConnectionSpec, ParseFrom, Secret, SinkName};

fn parse(toml: &str) -> Result<ConfigToml, ParseError> {
    ConfigToml::try_parse(toml)
}

// Conversion is infallible now: secrets are deferred (not resolved) and URLs are
// validated at resolution time.
fn convert(toml: &str) -> Config {
    let config = ConfigToml::try_parse(toml).expect("toml should be valid for a conversion test");
    Config::from(config)
}

/// The single OpenSearch sink in a converted config (panics otherwise).
fn opensearch(config: &Config) -> (SinkName, schema_core::OpensearchSink) {
    let (name, sink) = config.sinks.iter().next().expect("a sink");
    match sink {
        Sink::Opensearch(os) => (name.clone(), os.clone()),
        _ => panic!("expected opensearch sink"),
    }
}

// Tests use unique var names to avoid cross-test races. The reserved-var tests
// touch fixed names (DATABASE_URL, …) and must run serially: `--test-threads=1`.
fn setenv(key: &str, val: &str) {
    unsafe { std::env::set_var(key, val) }
}
fn unsetenv(key: &str) {
    unsafe { std::env::remove_var(key) }
}

// ── parse: valid ─────────────────────────────────────────────────────────────

#[test]
fn parse_fixture() {
    parse(include_str!("config.toml")).unwrap();
}

// ── [server] surface addresses ───────────────────────────────────────────────

#[test]
fn server_addresses_convert() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgresql://localhost/db"

        [server]
        public_address = "0.0.0.0:9464"
        private_address = "127.0.0.1:9465"
    "#,
    );
    assert_eq!(
        config.server.public_address,
        Some("0.0.0.0:9464".parse().unwrap())
    );
    assert_eq!(
        config.server.private_address,
        Some("127.0.0.1:9465".parse().unwrap())
    );
}

#[test]
fn server_bad_address_is_rejected_at_parse() {
    let err = parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgresql://localhost/db"

        [server]
        public_address = "not-an-address"
    "#,
    )
    .unwrap_err();
    // `SocketAddr`'s own deserializer rejects it at config-read time, with the
    // toml span pointing at the offending line.
    let msg = err.to_string();
    assert!(msg.contains("socket address"), "{msg}");
}

#[test]
fn server_section_is_optional() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgresql://localhost/db"
    "#,
    );
    assert!(config.server.public_address.is_none());
    assert!(config.server.private_address.is_none());
}

#[test]
fn server_rejects_unknown_field() {
    let err = parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgresql://localhost/db"

        [server]
        public_address = "0.0.0.0:9464"
        bogus = "nope"
    "#,
    )
    .unwrap_err();
    assert!(format!("{err}").contains("bogus") || format!("{err:?}").contains("unknown"));
}

#[test]
fn parse_source_direct_url() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"
    "#,
    )
    .unwrap();
}

#[test]
fn parse_source_env_url() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "DATABASE_URL" }
    "#,
    )
    .unwrap();
}

#[test]
fn parse_source_parts() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = { host = "localhost", user = "app", database = "mydb" }
    "#,
    )
    .unwrap();
}

#[test]
fn parse_source_parts_env_password() {
    parse(r#"
        [source]
        type = "postgres"
        connection_url = { host = "localhost", database = "mydb", password = { env = "PG_PASSWORD" } }
    "#)
    .unwrap();
}

#[test]
fn parse_opensearch_sink_direct_url() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
        url = "https://localhost:9200"
    "#,
    )
    .unwrap();
}

#[test]
fn parse_opensearch_sink_env_credentials() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
        url = "https://localhost:9200"
        username = { env = "OPENSEARCH_USER" }
        password = { env = "OPENSEARCH_PASS" }
    "#,
    )
    .unwrap();
}

#[test]
fn parse_stdout_sink() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.out]
        type = "stdout"
        pretty = true
    "#,
    )
    .unwrap();
}

#[test]
fn parse_with_index_entries() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [[index]]
        name = "users"
        schema = "users.schema.yml"
        enabled = true
    "#,
    )
    .unwrap();
}

// ── parse: invalid ───────────────────────────────────────────────────────────

#[test]
fn parse_missing_source_fails() {
    assert!(
        parse(
            r#"[sinks.out]
        type = "stdout""#
        )
        .is_err()
    );
}

#[test]
fn parse_unknown_source_type_fails() {
    assert!(
        parse(
            r#"[source]
        type = "mysql""#
        )
        .is_err()
    );
}

#[test]
fn parse_unknown_field_in_source_fails() {
    assert!(
        parse(
            r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"
        extra_field = "oops"
    "#
        )
        .is_err()
    );
}

#[test]
fn parse_opensearch_missing_url_fails() {
    assert!(
        parse(
            r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
    "#
        )
        .is_err()
    );
}

// ── error messages are clear (no untagged-enum jargon) ───────────────────────

#[test]
fn sink_url_wrong_type_explains_accepted_shapes() {
    let err = parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
        url = 9200
    "#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains("untagged enum"),
        "leaked serde untagged-enum jargon: {msg}"
    );
    assert!(
        msg.contains(r#"env = "VAR""#),
        "missing accepted-shape hint: {msg}"
    );
}

#[test]
fn source_url_wrong_type_lists_the_three_forms() {
    let err = parse(
        r#"
        [source]
        type = "postgres"
        connection_url = 9200
    "#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(!msg.contains("untagged enum"), "{msg}");
    assert!(msg.contains("connection URL string"), "{msg}");
    assert!(msg.contains("connection-parts table"), "{msg}");
}

#[test]
fn env_reference_with_extra_key_is_explained() {
    let err = parse(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "DB", oops = "x" }
    "#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("`oops`"), "{msg}");
    assert!(msg.contains(r#"{ env = "VAR" }"#), "{msg}");
}

// ── conversion: deferred shape (no resolution) ───────────────────────────────

#[test]
fn convert_source_direct_url_is_literal_secret() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://app@db.internal/mydb"
    "#,
    );
    match config.source.connection {
        Some(ConnectionSpec::Url(Secret::Value(v))) => assert!(v.contains("db.internal")),
        other => panic!("expected a literal URL secret, got {other:?}"),
    }
}

#[test]
fn convert_source_env_url_is_env_secret() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "SOME_PG_URL" }
    "#,
    );
    match config.source.connection {
        Some(ConnectionSpec::Url(Secret::Env(var))) => assert_eq!(var, "SOME_PG_URL"),
        other => panic!("expected an env URL secret, got {other:?}"),
    }
}

#[test]
fn convert_source_parts_is_parts_spec() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { host = "parts-host", port = 5433, user = "parts-user", database = "parts-db" }
    "#,
    );
    match config.source.connection {
        Some(ConnectionSpec::Parts {
            host,
            port,
            database,
            ..
        }) => {
            assert_eq!(host, "parts-host");
            assert_eq!(port, 5433);
            assert_eq!(database, "parts-db");
        }
        other => panic!("expected parts, got {other:?}"),
    }
}

#[test]
fn convert_source_omitted_is_none() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
    "#,
    );
    assert!(config.source.connection.is_none());
}

#[test]
fn convert_opensearch_url_is_deferred_secret() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://search.internal:9200"
    "#,
    );
    let (_, os) = opensearch(&config);
    assert!(matches!(os.url, Secret::Value(v) if v == "https://search.internal:9200"));
}

#[test]
fn convert_empty_sinks_is_ok() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"
    "#,
    );
    assert!(config.sinks.is_empty());
}

// ── resolution (runtime) ─────────────────────────────────────────────────────

#[test]
fn resolve_source_env_url() {
    setenv("TEST_CONV_PG_URL", "postgres://admin@envhost/envdb");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "TEST_CONV_PG_URL" }
    "#,
    );
    let url = config.source.resolve_connection_url();
    unsetenv("TEST_CONV_PG_URL");
    assert!(url.unwrap().as_ref().contains("envhost"));
}

#[test]
fn resolve_source_parts_assembles_url() {
    unsetenv("DATABASE_URL");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { host = "parts-host", port = 5433, user = "parts-user", database = "parts-db" }
    "#,
    );
    let url = config.source.resolve_connection_url().unwrap();
    assert!(url.as_ref().contains("parts-host"));
    assert!(url.as_ref().contains("5433"));
    assert!(url.as_ref().contains("parts-db"));
}

#[test]
fn resolve_source_omitted_without_database_url_fails() {
    unsetenv("DATABASE_URL");
    let config = convert(
        r#"
        [source]
        type = "postgres"
    "#,
    );
    assert!(config.source.resolve_connection_url().is_err());
}

#[test]
fn resolve_source_env_not_set_fails() {
    unsetenv("TEST_CONV_UNSET_PG_URL_XYZ");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "TEST_CONV_UNSET_PG_URL_XYZ" }
    "#,
    );
    assert!(config.source.resolve_connection_url().is_err());
}

#[test]
fn resolve_source_invalid_url_fails() {
    unsetenv("DATABASE_URL");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "not_a_valid_url"
    "#,
    );
    assert!(config.source.resolve_connection_url().is_err());
}

#[test]
fn database_url_overrides_literal_source_url() {
    setenv("DATABASE_URL", "postgres://env@envhost/envdb");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://file@filehost/filedb"
    "#,
    );
    let url = config.source.resolve_connection_url();
    unsetenv("DATABASE_URL");
    let url = url.unwrap();
    assert!(url.as_ref().contains("envhost"));
    assert!(!url.as_ref().contains("filehost"));
}

#[test]
fn database_url_fills_omitted_source_url() {
    setenv("DATABASE_URL", "postgres://env@envhost/envdb");
    let config = convert(
        r#"
        [source]
        type = "postgres"
    "#,
    );
    let url = config.source.resolve_connection_url();
    unsetenv("DATABASE_URL");
    assert!(url.unwrap().as_ref().contains("envhost"));
}

#[test]
fn explicit_env_ref_beats_database_url_for_source() {
    setenv("DATABASE_URL", "postgres://reserved@reservedhost/db");
    setenv(
        "TEST_EXPLICIT_PG_URL",
        "postgres://explicit@explicithost/db",
    );
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "TEST_EXPLICIT_PG_URL" }
    "#,
    );
    let url = config.source.resolve_connection_url();
    unsetenv("DATABASE_URL");
    unsetenv("TEST_EXPLICIT_PG_URL");
    let url = url.unwrap();
    assert!(url.as_ref().contains("explicithost"));
    assert!(!url.as_ref().contains("reservedhost"));
}

#[test]
fn resolve_opensearch_url_reserved_var_overrides_literal() {
    setenv("PRIMARY_OPENSEARCH_URL", "https://env.example:9200");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://file.example:9200"
    "#,
    );
    let (name, os) = opensearch(&config);
    let url = os.resolve_url(&name);
    unsetenv("PRIMARY_OPENSEARCH_URL");
    assert_eq!(url.unwrap().as_ref(), "https://env.example:9200");
}

#[test]
fn resolve_opensearch_credentials_filled_by_reserved_vars() {
    unsetenv("PRIMARY_OPENSEARCH_URL");
    setenv("PRIMARY_OPENSEARCH_USERNAME", "svc");
    setenv("PRIMARY_OPENSEARCH_PASSWORD", "hunter2");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://file.example:9200"
    "#,
    );
    let (name, os) = opensearch(&config);
    let username = os.resolve_username(&name);
    let password = os.resolve_password(&name);
    unsetenv("PRIMARY_OPENSEARCH_USERNAME");
    unsetenv("PRIMARY_OPENSEARCH_PASSWORD");
    assert_eq!(username.unwrap().as_deref(), Some("svc"));
    assert_eq!(password.unwrap().as_deref(), Some("hunter2"));
}

#[test]
fn resolve_opensearch_reserved_var_is_namespaced_per_sink() {
    unsetenv("PRIMARY_OPENSEARCH_URL");
    setenv("SECONDARY_OPENSEARCH_URL", "https://secondary.env:9200");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://primary.file:9200"

        [sinks.secondary]
        type = "opensearch"
        url = "https://secondary.file:9200"
    "#,
    );
    let resolve = |name: &str| {
        let sink_name = SinkName::try_new(name).unwrap();
        match config.sinks.get(&sink_name) {
            Some(Sink::Opensearch(os)) => os.resolve_url(&sink_name).unwrap().as_ref().to_owned(),
            _ => panic!("expected opensearch sink `{name}`"),
        }
    };
    let primary = resolve("primary");
    let secondary = resolve("secondary");
    unsetenv("SECONDARY_OPENSEARCH_URL");
    assert_eq!(primary, "https://primary.file:9200");
    assert_eq!(secondary, "https://secondary.env:9200");
}

#[test]
fn explicit_env_ref_beats_reserved_var_for_opensearch() {
    setenv("PRIMARY_OPENSEARCH_URL", "https://reserved.example:9200");
    setenv("TEST_EXPLICIT_OS_URL", "https://explicit.example:9200");
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = { env = "TEST_EXPLICIT_OS_URL" }
    "#,
    );
    let (name, os) = opensearch(&config);
    let url = os.resolve_url(&name);
    unsetenv("PRIMARY_OPENSEARCH_URL");
    unsetenv("TEST_EXPLICIT_OS_URL");
    assert_eq!(url.unwrap().as_ref(), "https://explicit.example:9200");
}
