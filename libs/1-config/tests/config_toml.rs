#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

//! Parsing and conversion of `flusso.toml`: the port entries pass through as
//! `type` + uninterpreted options, the globals convert, and the strict parts
//! (unknown top-level keys, a missing `type`) are rejected here. What the
//! options *mean* is each adapter's business and is tested in its crate.

use config::toml::{ConfigToml, ParseError};
use config::{Config, DEFAULT_STREAM_KIND, ParseFrom};
use kernel::{OptionValue, PortEntry, SinkName};

fn parse(toml: &str) -> Result<ConfigToml, ParseError> {
    ConfigToml::try_parse(toml)
}

fn convert(toml: &str) -> Config {
    Config::from(parse(toml).expect("valid toml"))
}

fn sink(name: &str) -> SinkName {
    SinkName::try_new(name).unwrap()
}

#[test]
fn parse_fixture() {
    let raw = include_str!("config.toml");
    let config = parse(raw).unwrap();
    assert_eq!(config.source.kind, "postgres");
    assert_eq!(config.sinks.len(), 2);
    assert_eq!(config.index.len(), 1);
}

#[test]
fn source_entry_keeps_its_options_uninterpreted() {
    let config = convert(
        r#"
        [source]
        type = "postgres"
        connection_url = { env = "PG_URL" }
        ssl_mode = "verify-full"
        anything_the_adapter_understands = 42
        "#,
    );
    assert_eq!(config.source.kind, "postgres");
    let env = config
        .source
        .options
        .get("connection_url")
        .and_then(OptionValue::as_map)
        .unwrap();
    assert_eq!(env.get("env").and_then(OptionValue::as_str), Some("PG_URL"));
    assert_eq!(
        config
            .source
            .options
            .get("ssl_mode")
            .and_then(OptionValue::as_str),
        Some("verify-full")
    );
    assert_eq!(
        config
            .source
            .options
            .get("anything_the_adapter_understands")
            .and_then(OptionValue::as_i64),
        Some(42)
    );
}

#[test]
fn source_without_type_is_rejected() {
    let error = parse(
        r#"
        [source]
        connection_url = "postgres://u@h/d"
        "#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("type"), "{error}");
}

#[test]
fn missing_source_is_rejected() {
    let error = parse(
        r#"
        [sinks.primary]
        type = "stdout"
        "#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("source"), "{error}");
}

#[test]
fn stream_defaults_to_the_channel_with_no_options() {
    let config = convert("[source]\ntype = \"postgres\"\n");
    assert_eq!(config.stream, PortEntry::new(DEFAULT_STREAM_KIND));
}

#[test]
fn stream_entry_is_carried() {
    let config = convert(
        r#"
        [source]
        type = "postgres"

        [stream]
        type = "channel"
        capacity = 256
        "#,
    );
    assert_eq!(config.stream.kind, "channel");
    assert_eq!(
        config
            .stream
            .options
            .get("capacity")
            .and_then(OptionValue::as_i64),
        Some(256)
    );
}

#[test]
fn sinks_are_named_entries() {
    let config = convert(
        r#"
        [source]
        type = "postgres"

        [sinks.primary]
        type = "opensearch"
        url = "https://search.internal:9200"
        password = { env = "OS_PW" }

        [sinks.audit]
        type = "stdout"
        pretty = true
        "#,
    );
    assert_eq!(config.sinks.len(), 2);
    let primary = &config.sinks[&sink("primary")];
    assert_eq!(primary.kind, "opensearch");
    assert_eq!(
        primary.options.get("url").and_then(OptionValue::as_str),
        Some("https://search.internal:9200")
    );
    let audit = &config.sinks[&sink("audit")];
    assert_eq!(audit.kind, "stdout");
    assert_eq!(
        audit.options.get("pretty").and_then(OptionValue::as_bool),
        Some(true)
    );
}

#[test]
fn sink_without_type_is_rejected() {
    let error = parse(
        r#"
        [source]
        type = "postgres"

        [sinks.primary]
        url = "https://search.internal:9200"
        "#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("type"), "{error}");
}

#[test]
fn empty_sinks_is_ok() {
    let config = convert("[source]\ntype = \"postgres\"\n");
    assert!(config.sinks.is_empty());
}

#[test]
fn unknown_top_level_key_is_rejected() {
    let error = parse(
        r#"
        [source]
        type = "postgres"

        [sinkz.primary]
        type = "stdout"
        "#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("sinkz"), "{error}");
}

#[test]
fn server_addresses_convert() {
    let config = convert(
        r#"
        [source]
        type = "postgres"

        [server]
        public_address = "0.0.0.0:9464"
        private_address = "127.0.0.1:9465"
        "#,
    );
    assert_eq!(
        config.server.public_address.map(|a| a.to_string()),
        Some("0.0.0.0:9464".to_owned())
    );
    assert_eq!(
        config.server.private_address.map(|a| a.to_string()),
        Some("127.0.0.1:9465".to_owned())
    );
}

#[test]
fn server_section_is_optional_and_strict() {
    let config = convert("[source]\ntype = \"postgres\"\n");
    assert!(config.server.public_address.is_none());
    let error = parse(
        r#"
        [source]
        type = "postgres"

        [server]
        public_adress = "0.0.0.0:9464"
        "#,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("public_adress"), "{error}");
    let error = parse(
        r#"
        [source]
        type = "postgres"

        [server]
        public_address = "not-an-address"
        "#,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("address") || error.contains("socket"),
        "{error}"
    );
}

#[test]
fn index_prefix_converts_and_defaults_to_empty() {
    assert_eq!(convert("[source]\ntype = \"postgres\"\n").prefix, "");
    assert_eq!(
        convert("prefix = \"staging_\"\n[source]\ntype = \"postgres\"\n").prefix,
        "staging_"
    );
}

#[test]
fn on_error_converts_and_defaults_to_stop() {
    assert_eq!(
        convert("[source]\ntype = \"postgres\"\n").on_error,
        kernel::FailurePolicy::Stop
    );
    assert_eq!(
        convert("on_error = \"skip\"\n[source]\ntype = \"postgres\"\n").on_error,
        kernel::FailurePolicy::Skip
    );
}

#[test]
fn parse_with_index_entries() {
    let config = parse(
        r#"
        [source]
        type = "postgres"

        [[index]]
        name = "users"
        schema = "users.schema.yml"
        enabled = true
        on_error = "skip"

        [[index]]
        name = "orders"
        schema = "orders.schema.yml"
        enabled = false
        "#,
    )
    .unwrap();
    assert_eq!(config.index.len(), 2);
    assert_eq!(config.index[0].name.as_ref(), "users");
    assert_eq!(config.index[0].on_error, Some(kernel::FailurePolicy::Skip));
    assert!(!config.index[1].enabled);
}

#[test]
fn port_entries_serialize_as_written() {
    let config = parse(
        r#"
        [source]
        type = "postgres"
        connection_url = "postgres://u@h/d"

        [sinks.primary]
        type = "opensearch"
        url = "https://search.internal:9200"
        batch_size = 500
        "#,
    )
    .unwrap();
    let text = toml::to_string(&config).unwrap();
    assert!(
        text.contains("[source]\ntype = \"postgres\"\nconnection_url = \"postgres://u@h/d\""),
        "{text}"
    );
    assert!(text.contains("[sinks.primary]\ntype = \"opensearch\"\nbatch_size = 500\nurl = \"https://search.internal:9200\""), "{text}");
    let again = parse(&text).unwrap();
    assert_eq!(again.source, config.source);
    assert_eq!(again.sinks, config.sinks);
}
