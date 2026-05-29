#![allow(unused_crate_dependencies)]

use std::path::Path;

use schema::{IndexName, LoadError, load};

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

    // Source and sinks come from the TOML.
    assert!(config.source.connection_url.as_ref().contains("localhost"));
    assert_eq!(config.sinks.len(), 1);

    // Both index entries are loaded from their YAML files, keyed by name.
    assert_eq!(config.indexes.len(), 2);

    let users = config.indexes.get(&index_name("users")).expect("users index");
    assert!(users.enabled);
    assert_eq!(users.schema.table.as_ref(), "users");
    assert_eq!(users.schema.fields.len(), 2);

    let orders = config.indexes.get(&index_name("orders")).expect("orders index");
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
