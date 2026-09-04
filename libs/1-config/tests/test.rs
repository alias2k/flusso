#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::Path;

use config::{IndexName, LoadError, load};
use kernel::{OptionValue, PortEntry};

fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn loads_config_with_indexes() {
    let config = load(fixture("config.toml")).unwrap();

    // Source and sinks come from the TOML; the connection stays deferred.
    assert_eq!(config.source.kind, "postgres");
    assert!(
        config
            .source
            .options
            .get("connection_url")
            .and_then(OptionValue::as_str)
            .is_some_and(|url| url.contains("localhost"))
    );
    assert_eq!(config.sinks.len(), 1);

    // Both index entries are loaded from their YAML files, keyed by name.
    assert_eq!(config.indexes.len(), 2);

    let users = config
        .indexes
        .get(&index_name("users"))
        .expect("users index");
    assert!(users.enabled);
    assert_eq!(users.schema.table.as_ref(), "users");
    assert_eq!(users.schema.fields.len(), 2);

    let orders = config
        .indexes
        .get(&index_name("orders"))
        .expect("orders index");
    assert!(!orders.enabled);
    assert_eq!(orders.schema.table.as_ref(), "orders");
}

#[test]
fn missing_config_file_errors() {
    let err = load(fixture("does-not-exist.toml")).unwrap_err();
    assert!(matches!(err, LoadError::ReadConfig { .. }));
}

#[test]
fn missing_schema_file_errors() {
    // A config that references a schema file which does not exist on disk.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let config_path = dir.join("missing_schema_config.toml");
    std::fs::write(
        &config_path,
        r#"
[source]
type = "postgres"
connection_url = "postgres://app@localhost/mydb"

[[index]]
name = "ghost"
schema = "ghost.schema.yml"
enabled = true
"#,
    )
    .unwrap();

    let err = load(&config_path).unwrap_err();
    std::fs::remove_file(&config_path).ok();

    assert!(matches!(err, LoadError::ReadSchema { .. }));
}

#[test]
fn compiled_artifact_roundtrips_and_preserves_mappings() {
    let compiled = config::compile(fixture("config.toml")).unwrap();
    let bytes = config::to_bytes(&compiled).unwrap();
    let config = config::from_bytes(&bytes).unwrap();

    // The whole configuration survives the round-trip.
    assert_eq!(config.indexes.len(), 2);

    // The mapping (and its content hash → physical index name) is identical to
    // the one derived directly from source — the artifact is faithful.
    let from_source = load(fixture("config.toml")).unwrap().resolve_mappings();
    let from_artifact = config.resolve_mappings();
    assert_eq!(from_source.len(), from_artifact.len());
    for (a, b) in from_source.iter().zip(&from_artifact) {
        assert_eq!(a.index, b.index);
        assert_eq!(a.hash, b.hash);
    }
}

#[test]
fn compiled_artifact_keeps_env_secret_unresolved() {
    use config::{Compiled, Config, FORMAT_VERSION};
    let mut source = PortEntry::new("postgres");
    let mut env = kernel::Options::empty();
    env.insert("env", "PG_URL");
    source.options.insert("connection_url", env);
    source.options.insert("ssl_mode", "verify-full");
    let config = Config {
        source,
        stream: PortEntry::new(config::DEFAULT_STREAM_KIND),
        sinks: Default::default(),
        indexes: Default::default(),
        on_error: Default::default(),
        server: Default::default(),
        prefix: String::new(),
    };
    let compiled = Compiled {
        format_version: FORMAT_VERSION,
        config,
    };
    let bytes = config::to_bytes(&compiled).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(
        text.contains("[config.source.connection_url]\nenv = \"PG_URL\""),
        "{text}"
    );
    let config = config::from_bytes(&bytes).unwrap();
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
}

#[test]
fn compiled_artifact_without_stream_defaults_to_the_channel() {
    let text = r#"
format_version = 3

[config]

[config.source]
type = "postgres"
"#;
    let config = config::from_bytes(text.as_bytes()).unwrap();
    assert_eq!(config.stream, PortEntry::new(config::DEFAULT_STREAM_KIND));
}
