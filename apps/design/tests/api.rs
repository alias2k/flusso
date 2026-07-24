//! Database-free integration test for the designer API: open a project, preview
//! a schema, save it back, and reopen — the round-trip every editor session
//! depends on. The catalog/validate endpoints need a live Postgres and are
//! exercised through the source crate's introspection e2e instead.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::path::PathBuf;

use design::api::{self, FileOp, OpKind, PreviewRequest, SaveRequest};
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
    assert!(orders.raw.is_some(), "load includes the raw file text");
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

    // Save every index back through codegen (an Upsert per file).
    let save = SaveRequest {
        config: project.config.clone(),
        ops: project
            .indexes
            .iter()
            .map(|i| FileOp {
                kind: OpKind::Upsert,
                path: PathBuf::from(&i.schema_path),
                from: None,
                schema: i.schema.clone(),
                raw: None,
            })
            .collect(),
        skip: Vec::new(),
    };
    let written = api::save_project(&config_path, save).unwrap();
    assert_eq!(written.written.len(), 4, "config + three schemas");
    assert!(written.moved.is_empty() && written.deleted.is_empty());

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

    // Saving the reopened (already-canonical) project again writes nothing —
    // only changed files are touched.
    let resave = SaveRequest {
        config: reopened.config.clone(),
        ops: reopened
            .indexes
            .iter()
            .map(|i| FileOp {
                kind: OpKind::Upsert,
                path: PathBuf::from(&i.schema_path),
                from: None,
                schema: i.schema.clone(),
                raw: None,
            })
            .collect(),
        skip: Vec::new(),
    };
    let again = api::save_project(&config_path, resave).unwrap();
    assert!(
        again.written.is_empty(),
        "re-saving unchanged files writes nothing: {:?}",
        again.written
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The lifecycle ops: a move into a fresh subfolder creates it; a delete removes
/// the file; moving the file back out prunes the now-empty subfolder. The old
/// path is never left orphaned.
#[test]
fn save_moves_deletes_and_prunes() {
    let dir = fixture();
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();

    let moved = project.indexes.first().unwrap();
    let doomed = project.indexes.get(1).unwrap();
    let moved_old = dir.join(&moved.schema_path);
    let doomed_old = dir.join(&doomed.schema_path);
    assert!(moved_old.exists() && doomed_old.exists());

    // Move index 0 into a new `nested/` folder; delete index 1.
    let moved_rel = PathBuf::from("nested/moved.schema.yml");
    let resp = api::save_project(
        &config_path,
        SaveRequest {
            config: project.config.clone(),
            ops: vec![
                FileOp {
                    kind: OpKind::Move,
                    path: moved_rel.clone(),
                    from: Some(PathBuf::from(&moved.schema_path)),
                    schema: moved.schema.clone(),
                    raw: moved.raw.clone(),
                },
                FileOp {
                    kind: OpKind::Delete,
                    path: PathBuf::from(&doomed.schema_path),
                    from: None,
                    schema: None,
                    raw: None,
                },
            ],
            skip: Vec::new(),
        },
    )
    .unwrap();

    assert!(dir.join(&moved_rel).exists(), "moved file at new path");
    assert!(!moved_old.exists(), "old path gone after a move");
    assert!(!doomed_old.exists(), "deleted file gone");
    assert_eq!(resp.moved.len(), 1);
    assert_eq!(resp.deleted.len(), 1);

    // Move it back out; the emptied `nested/` folder is pruned.
    let resp2 = api::save_project(
        &config_path,
        SaveRequest {
            config: project.config.clone(),
            ops: vec![FileOp {
                kind: OpKind::Move,
                path: PathBuf::from(&moved.schema_path),
                from: Some(moved_rel),
                schema: moved.schema.clone(),
                raw: moved.raw.clone(),
            }],
            skip: Vec::new(),
        },
    )
    .unwrap();

    assert!(!dir.join("nested").exists(), "emptied folder pruned");
    assert!(resp2.pruned.iter().any(|p| p.ends_with("nested")));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parse_round_trips_generated_yaml_and_reports_errors() {
    // A generated schema parses back to the same model (the Code editor's
    // live sync relies on this identity)…
    let yaml = include_str!("../../../dev/users.schema.yml");
    let parsed = api::parse_index(&api::ParseRequest { yaml: yaml.into() });
    let schema = parsed.schema.expect("dev schema parses");
    assert!(parsed.error.is_none());
    let regenerated = design::codegen::schema_to_yaml(&schema).unwrap();
    let reparsed = api::parse_index(&api::ParseRequest { yaml: regenerated });
    assert!(reparsed.error.is_none());
    assert_eq!(
        serde_json::to_value(&schema).unwrap(),
        serde_json::to_value(reparsed.schema.expect("regenerated YAML parses")).unwrap(),
    );

    // …a top-level syntax error carries a trustworthy source location…
    let bad = api::parse_index(&api::ParseRequest {
        yaml: "fields:\n- integer: id\n   required: false".into(),
    });
    assert!(bad.schema.is_none());
    assert!(bad.error.is_some());
    assert!(bad.location.is_some(), "syntax errors report line/column");

    // …and a field-scoped error names the field structurally instead (its
    // reported position would be wrong — see the parser).
    let field = api::parse_index(&api::ParseRequest {
        yaml: "version: 1\ntable: t\nfields:\n- keyword: x\n  required: trues".into(),
    });
    assert!(field.schema.is_none());
    assert!(field.location.is_none());
    assert_eq!(field.type_tag.as_deref(), Some("keyword"));
    assert_eq!(field.field.as_deref(), Some("x"));
}

/// `list_dirs` enumerates subfolders (forward-slash, recursive) and skips hidden
/// dirs and the usual build/vendor noise — it backs the schema-folder picker.
#[test]
fn list_dirs_walks_subfolders_and_skips_noise() {
    let dir = fixture();
    std::fs::create_dir_all(dir.join("schemas/nested")).unwrap();
    std::fs::create_dir_all(dir.join(".hidden")).unwrap();
    std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();

    let dirs = api::list_dirs(&dir.join("flusso.toml"));
    assert!(dirs.contains(&"schemas".to_string()));
    assert!(dirs.contains(&"schemas/nested".to_string()));
    assert!(
        !dirs
            .iter()
            .any(|d| d.starts_with(".hidden") || d.starts_with("node_modules")),
        "hidden and vendor dirs are skipped: {dirs:?}",
    );

    std::fs::remove_dir_all(&dir).ok();
}
