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
    unsetenv("DATABASE_URL");
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
    unsetenv("DATABASE_URL");
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
    unsetenv("DATABASE_URL");
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
    unsetenv("DATABASE_URL");
    unsetenv("PRIMARY_OPENSEARCH_URL");
    unsetenv("PRIMARY_OPENSEARCH_USERNAME");
    unsetenv("PRIMARY_OPENSEARCH_PASSWORD");
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
    unsetenv("DATABASE_URL");
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
    unsetenv("DATABASE_URL");
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
    unsetenv("DATABASE_URL");
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
    unsetenv("DATABASE_URL");
    unsetenv("ES_OPENSEARCH_URL");
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

// ── reserved env-var overrides ───────────────────────────────────────────────

#[test]
fn database_url_overrides_literal_source_url() {
    setenv("DATABASE_URL", "postgres://env@envhost/envdb");
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://file@filehost/filedb"
    "#,
    );
    unsetenv("DATABASE_URL");

    // The reserved var wins over the value written in the file.
    let url = result.unwrap().source.connection_url;
    assert!(url.as_ref().contains("envhost"));
    assert!(!url.as_ref().contains("filehost"));
}

#[test]
fn database_url_fills_omitted_source_url() {
    setenv("DATABASE_URL", "postgres://env@envhost/envdb");
    let result = convert(
        r#"
        [source]
        type = "postgres"
    "#,
    );
    unsetenv("DATABASE_URL");

    // No connection_url in the file, but DATABASE_URL fills it — no error.
    assert!(
        result
            .unwrap()
            .source
            .connection_url
            .as_ref()
            .contains("envhost")
    );
}

#[test]
fn explicit_env_ref_beats_database_url_for_source() {
    setenv("DATABASE_URL", "postgres://reserved@reservedhost/db");
    setenv(
        "TEST_EXPLICIT_PG_URL",
        "postgres://explicit@explicithost/db",
    );
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "TEST_EXPLICIT_PG_URL" }
    "#,
    );
    unsetenv("DATABASE_URL");
    unsetenv("TEST_EXPLICIT_PG_URL");

    // The explicit `{ env }` reference names its own source and is not
    // overridden by the reserved DATABASE_URL.
    let url = result.unwrap().source.connection_url;
    assert!(url.as_ref().contains("explicithost"));
    assert!(!url.as_ref().contains("reservedhost"));
}

#[test]
fn derived_var_overrides_literal_opensearch_url() {
    unsetenv("DATABASE_URL");
    setenv("PRIMARY_OPENSEARCH_URL", "https://env.example:9200");
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://file.example:9200"
    "#,
    );
    unsetenv("PRIMARY_OPENSEARCH_URL");

    let config = result.unwrap();
    let (_, sink) = config.sinks.iter().next().unwrap();
    match sink {
        Sink::Opensearch(os) => assert_eq!(os.url.as_ref(), "https://env.example:9200"),
        _ => panic!("expected opensearch sink"),
    }
}

#[test]
fn derived_vars_fill_omitted_opensearch_credentials() {
    unsetenv("DATABASE_URL");
    unsetenv("PRIMARY_OPENSEARCH_URL");
    setenv("PRIMARY_OPENSEARCH_USERNAME", "svc");
    setenv("PRIMARY_OPENSEARCH_PASSWORD", "hunter2");
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = "https://file.example:9200"
    "#,
    );
    unsetenv("PRIMARY_OPENSEARCH_USERNAME");
    unsetenv("PRIMARY_OPENSEARCH_PASSWORD");

    let config = result.unwrap();
    let (_, sink) = config.sinks.iter().next().unwrap();
    match sink {
        Sink::Opensearch(os) => {
            assert_eq!(os.username.as_deref(), Some("svc"));
            assert_eq!(os.password.as_deref(), Some("hunter2"));
        }
        _ => panic!("expected opensearch sink"),
    }
}

#[test]
fn derived_var_is_namespaced_per_sink() {
    unsetenv("DATABASE_URL");
    // Only `secondary`'s var is set — it must not bleed into `primary`.
    unsetenv("PRIMARY_OPENSEARCH_URL");
    setenv("SECONDARY_OPENSEARCH_URL", "https://secondary.env:9200");
    let result = convert(
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
    unsetenv("SECONDARY_OPENSEARCH_URL");

    let config = result.unwrap();
    let url_of = |name: &str| match config
        .sinks
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, s)| s)
    {
        Some(Sink::Opensearch(os)) => os.url.as_ref().to_owned(),
        _ => panic!("expected opensearch sink `{name}`"),
    };
    // primary keeps its file value; secondary is overridden by its own var.
    assert_eq!(url_of("primary"), "https://primary.file:9200");
    assert_eq!(url_of("secondary"), "https://secondary.env:9200");
}

#[test]
fn explicit_env_ref_beats_derived_var_for_opensearch() {
    unsetenv("DATABASE_URL");
    setenv("PRIMARY_OPENSEARCH_URL", "https://reserved.example:9200");
    setenv("TEST_EXPLICIT_OS_URL", "https://explicit.example:9200");
    let result = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://user@localhost/mydb"

        [sinks.primary]
        type = "opensearch"
        url = { env = "TEST_EXPLICIT_OS_URL" }
    "#,
    );
    unsetenv("PRIMARY_OPENSEARCH_URL");
    unsetenv("TEST_EXPLICIT_OS_URL");

    let config = result.unwrap();
    let (_, sink) = config.sinks.iter().next().unwrap();
    match sink {
        Sink::Opensearch(os) => assert_eq!(os.url.as_ref(), "https://explicit.example:9200"),
        _ => panic!("expected opensearch sink"),
    }
}
