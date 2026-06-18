#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::Path;

use schema::{ConnectionSpec, IndexName, LoadError, Secret, load};

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
    match &config.source.connection {
        Some(ConnectionSpec::Url(Secret::Value(v))) => assert!(v.contains("localhost")),
        other => panic!("expected a literal connection URL, got {other:?}"),
    }
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
    let compiled = schema::compile(fixture("config.toml")).unwrap();
    let bytes = schema::to_bytes(&compiled).unwrap();
    let config = schema::from_bytes(&bytes).unwrap();

    // The whole configuration survives the binary round-trip.
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
    use schema::{Compiled, Config, ConnectionSpec, FORMAT_VERSION, Secret, Source, SourceType};
    let config = Config {
        source: Source {
            source_type: SourceType::Postgres,
            connection: Some(ConnectionSpec::Url(Secret::Env("DATABASE_URL".to_owned()))),
            manage_publication: true,
        },
        sinks: Default::default(),
        indexes: Default::default(),
        on_error: Default::default(),
        server: Default::default(),
    };
    let compiled = Compiled {
        format_version: FORMAT_VERSION,
        flusso_version: "test".to_owned(),
        config,
    };
    let bytes = schema::to_bytes(&compiled).unwrap();
    let config = schema::from_bytes(&bytes).unwrap();

    // The env reference is carried through verbatim — never resolved or baked.
    match config.source.connection {
        Some(ConnectionSpec::Url(Secret::Env(var))) => assert_eq!(var, "DATABASE_URL"),
        other => panic!("expected an unresolved env secret, got {other:?}"),
    }
}
