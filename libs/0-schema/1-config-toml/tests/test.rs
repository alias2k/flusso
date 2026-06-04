#![allow(unused_crate_dependencies)]
#![allow(unsafe_code)]

use schema_config_toml::{ConfigToml, ConversionError, ParseError};
use schema_core::{Config, ParseFrom, Sink};

fn parse(toml: &str) -> Result<ConfigToml, ParseError> {
    ConfigToml::try_parse(toml)
}

fn convert(toml: &str) -> Result<Config, ConversionError> {
    let config = ConfigToml::try_parse(toml).expect("toml should be valid for a conversion test");
    Config::try_from(config)
}

// Tests use unique var names to avoid cross-test races.
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
fn parse_opensearch_sink_env_url() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
        url = { env = "OPENSEARCH_URL" }
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
fn parse_multiple_sinks() {
    parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://localhost:9200"

        [sinks.debug]
        type = "stdout"
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

        [[index]]
        name = "orders"
        schema = "orders.schema.yml"
        enabled = false
    "#,
    )
    .unwrap();
}

// ── parse: invalid ───────────────────────────────────────────────────────────

#[test]
fn parse_missing_source_fails() {
    assert!(
        parse(
            r#"
        [sinks.out]
        type = "stdout"
    "#
        )
        .is_err()
    );
}

#[test]
fn parse_unknown_source_type_fails() {
    assert!(
        parse(
            r#"
        [source]
        type = "mysql"
    "#
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
fn parse_unknown_sink_type_fails() {
    assert!(
        parse(
            r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.bad]
        type = "kafka"
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

#[test]
fn parse_unknown_field_in_opensearch_sink_fails() {
    assert!(
        parse(
            r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
        url = "https://localhost:9200"
        unknown_option = true
    "#
        )
        .is_err()
    );
}

// ── conversion: valid ────────────────────────────────────────────────────────

#[test]
fn convert_source_direct_url() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://app@db.internal/mydb"
    "#,
    )
    .unwrap();

    assert!(
        config
            .source
            .connection_url
            .as_ref()
            .contains("db.internal")
    );
}

#[test]
fn convert_source_env_url() {
    setenv("TEST_CONV_PG_URL", "postgres://admin@envhost/envdb");
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "TEST_CONV_PG_URL" }
    "#,
    );
    unsetenv("TEST_CONV_PG_URL");

    let config = result.unwrap();
    assert!(config.source.connection_url.as_ref().contains("envhost"));
}

#[test]
fn convert_source_parts() {
    let config = convert(r#"
        [source]
        type = "postgres"
        connection_url = { host = "parts-host", port = 5433, user = "parts-user", database = "parts-db" }
    "#)
    .unwrap();

    let url = config.source.connection_url.as_ref();
    assert!(url.contains("parts-host"));
    assert!(url.contains("5433"));
    assert!(url.contains("parts-db"));
}

#[test]
fn convert_source_parts_env_password() {
    setenv("TEST_CONV_PG_PASS", "s3cr3t");
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { host = "localhost", user = "app", database = "mydb", password = { env = "TEST_CONV_PG_PASS" } }
    "#,
    );
    unsetenv("TEST_CONV_PG_PASS");

    assert!(result.is_ok());
}

#[test]
fn convert_opensearch_sink_direct_url() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://search.internal:9200"
    "#,
    )
    .unwrap();

    let (_, sink) = config.sinks.iter().next().unwrap();
    match sink {
        Sink::Opensearch(os) => assert_eq!(os.url.as_ref(), "https://search.internal:9200"),
        _ => panic!("expected opensearch sink"),
    }
}

#[test]
fn convert_opensearch_sink_env_url() {
    setenv("TEST_CONV_OS_URL", "https://search.env:9200");
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = { env = "TEST_CONV_OS_URL" }
    "#,
    );
    unsetenv("TEST_CONV_OS_URL");

    let config = result.unwrap();
    let (_, sink) = config.sinks.iter().next().unwrap();
    match sink {
        Sink::Opensearch(os) => assert_eq!(os.url.as_ref(), "https://search.env:9200"),
        _ => panic!("expected opensearch sink"),
    }
}

#[test]
fn convert_empty_sinks_is_ok() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"
    "#,
    )
    .unwrap();

    assert!(config.sinks.is_empty());
}

// ── conversion: invalid ──────────────────────────────────────────────────────

#[test]
fn convert_source_missing_connection_url_fails() {
    let toml = ConfigToml::try_parse(
        r#"
        [source]
        type = "postgres"
    "#,
    )
    .unwrap();

    let err = Config::try_from(toml).unwrap_err();
    assert!(matches!(err, ConversionError::MissingConnectionUrl));
}

#[test]
fn convert_source_env_url_not_set_fails() {
    unsetenv("TEST_CONV_UNSET_PG_URL_XYZ");

    let toml = ConfigToml::try_parse(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "TEST_CONV_UNSET_PG_URL_XYZ" }
    "#,
    )
    .unwrap();

    let err = Config::try_from(toml).unwrap_err();
    assert!(matches!(err, ConversionError::EnvVar(_)));
}

#[test]
fn convert_source_invalid_url_fails() {
    let toml = ConfigToml::try_parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "not_a_valid_url"
    "#,
    )
    .unwrap();

    let err = Config::try_from(toml).unwrap_err();
    assert!(matches!(err, ConversionError::ConnectionUrl(_)));
}

#[test]
fn convert_opensearch_env_url_not_set_fails() {
    unsetenv("TEST_CONV_UNSET_OS_URL_XYZ");

    let toml = ConfigToml::try_parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
        url = { env = "TEST_CONV_UNSET_OS_URL_XYZ" }
    "#,
    )
    .unwrap();

    let err = Config::try_from(toml).unwrap_err();
    assert!(matches!(err, ConversionError::EnvVar(_)));
}

#[test]
fn convert_opensearch_invalid_url_fails() {
    let toml = ConfigToml::try_parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.es]
        type = "opensearch"
        url = "ftp://not-http-or-https"
    "#,
    )
    .unwrap();

    let err = Config::try_from(toml).unwrap_err();
    assert!(matches!(err, ConversionError::HttpUrl(_)));
}
