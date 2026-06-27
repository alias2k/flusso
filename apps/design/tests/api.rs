//! Database-free integration test for the designer API: open a project, preview
//! a schema, save it back, and reopen — the round-trip every editor session
//! depends on. The catalog/validate endpoints need a live Postgres and are
//! exercised through the source crate's introspection e2e instead.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::path::PathBuf;

use design::api::{self, PreviewRequest, SaveRequest, SaveSchema};
use schema_core::IndexName;

/// A unique scratch directory for this test process, seeded with the dev files.
fn fixture() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flusso-design-api-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        include_str!("../../../dev/flusso.toml"),
    )
    .unwrap();
    std::fs::write(
        dir.join("users.schema.yml"),
        include_str!("../../../dev/users.schema.yml"),
    )
    .unwrap();
    std::fs::write(
        dir.join("products.schema.yml"),
        include_str!("../../../dev/products.schema.yml"),
    )
    .unwrap();
    std::fs::write(
        dir.join("orders.schema.yml"),
        include_str!("../../../dev/orders.schema.yml"),
    )
    .unwrap();
    dir
}

#[test]
fn project_previews_saves_and_reopens() {
    let dir = fixture();
    let config_path = dir.join("flusso.toml");

    // Open.
    let project = api::load_project(&config_path).unwrap();
    assert_eq!(project.indexes.len(), 3);
    let orders = project.indexes.iter().find(|i| i.name == "orders").unwrap();
    let schema = orders.schema.clone().unwrap();

    // Preview: derives a mapping + document and self-checks the generated YAML.
    let response = api::build_preview(PreviewRequest {
        index: IndexName::try_new("orders").unwrap(),
        schema: schema.clone(),
    })
    .unwrap();
    assert!(
        response.parse_ok,
        "generated YAML must re-parse: {:?}",
        response.parse_error
    );
    assert!(response.preview.document.iter().any(|n| n.name == "items"));
    assert!(response.yaml.contains("- has_many: items"));

    // Save every index back through codegen.
    let save = SaveRequest {
        config: project.config.clone(),
        indexes: project
            .indexes
            .iter()
            .map(|i| SaveSchema {
                schema_path: PathBuf::from(&i.schema_path),
                schema: i.schema.clone().unwrap(),
            })
            .collect(),
    };
    let written = api::save_project(&config_path, save).unwrap();
    assert_eq!(written.written.len(), 4, "config + three schemas");

    // Reopen: the regenerated files load and resolve to the same mapping.
    let reopened = api::load_project(&config_path).unwrap();
    let reorders = reopened
        .indexes
        .iter()
        .find(|i| i.name == "orders")
        .unwrap()
        .schema
        .clone()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&schema).unwrap(),
        serde_json::to_value(&reorders).unwrap(),
    );

    std::fs::remove_dir_all(&dir).ok();
}
