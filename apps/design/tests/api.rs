//! Database-free integration test for the designer API: open a project, preview
//! a schema, save it back, and reopen — the round-trip every editor session
//! depends on. The catalog/validate endpoints need a live Postgres and are
//! exercised through the source crate's introspection e2e instead.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::path::PathBuf;

use design::api::{self, FileOp, OpKind, PreviewRequest, SaveRequest};
use kernel::IndexName;

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
fn an_aggregate_input_is_checked_by_the_loader_s_parser() {
    let check = |input: &str| {
        api::check_expression(&api::ExpressionRequest {
            input: input.into(),
        })
    };

    let plain = check(" Amount ");
    assert!(plain.error.is_none());
    assert_eq!(plain.columns, ["amount"]);

    let product = check("quantity*(unit_price - 1.5 * quantity)");
    assert!(product.error.is_none());
    assert_eq!(product.columns, ["quantity", "unit_price"]);

    let bad = check("quantity *");
    assert!(bad.columns.is_empty());
    let error = bad.error.expect("a malformed input reports an error");
    assert_eq!((error.code, error.position), ("expected_operand", 11));

    let error = check("a % b")
        .error
        .expect("an unknown character is refused");
    assert_eq!(
        (error.code, error.detail.as_deref()),
        ("unexpected_character", Some("%"))
    );
}

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

/// The preview compiles the schema's scripts: a broken one comes back with its
/// field and position, and input-only fields are left out of the document tree.
#[test]
fn preview_reports_script_compile_errors_and_drops_input_only_fields() {
    let yaml = "version: 1\ntable: products\nprimary_key: id\nfields:\n  \
                - integer: id\n    required: true\n  \
                - keyword: sku\n    required: true\n    emit: false\n  \
                - keyword: label\n    required: true\n    script: |\n      let a = 1;\n      a +\n";
    let schema = api::parse_index(&api::ParseRequest {
        yaml: yaml.to_owned(),
    })
    .schema
    .unwrap();
    let response = api::build_preview(PreviewRequest {
        index: IndexName::try_new("products").unwrap(),
        schema,
    })
    .unwrap();
    assert_eq!(response.script_errors.len(), 1);
    let error = &response.script_errors[0];
    assert_eq!(error.field, "label");
    assert!(error.line.is_some(), "{error:?}");
    let names: Vec<&str> = response
        .preview
        .document
        .iter()
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(names, ["id", "label"]);
}

/// A schema whose YAML parses back as another schema fails the self-check and
/// names the field: a `Double` constant (what the inspector used to post for
/// `1e21`) reads back as a `Decimal`.
#[test]
fn preview_fails_when_the_yaml_reads_back_as_another_schema() {
    use config::yaml::SchemaYaml;
    use kernel::{FieldSource, GenericValue, IndexSchema, ParseFrom};

    let preview = |value: GenericValue| {
        let mut schema = IndexSchema::try_from(
            SchemaYaml::try_parse(
                "version: 1\ntable: t\nprimary_key: id\nfields:\n- constant: big\n  value: 1\n",
            )
            .unwrap(),
        )
        .unwrap();
        schema.fields[0].source = FieldSource::Constant(value);
        api::build_preview(PreviewRequest {
            index: IndexName::try_new("t").unwrap(),
            schema,
        })
        .unwrap()
    };
    let response = preview(GenericValue::Double(1e21));
    assert!(!response.parse_ok);
    assert_eq!(response.differs_at.as_deref(), Some("big"));
    assert!(response.parse_error.is_none());

    let same = preview(GenericValue::Decimal(
        "1000000000000000000000".parse().unwrap(),
    ));
    assert!(same.parse_ok, "{:?}", same.differs_at);
}

#[test]
fn a_typed_value_is_read_as_its_saved_file_reads_it() {
    let read = |text: &str| {
        serde_json::to_value(api::read_value(&api::ValueRequest {
            text: text.to_owned(),
        }))
        .unwrap()
    };
    assert_eq!(
        read("9007199254740993"),
        serde_json::json!({ "value": { "BigInt": 9_007_199_254_740_993_i64 } })
    );
    assert_eq!(
        read("1e-7"),
        serde_json::json!({ "value": { "Decimal": "0.0000001" } })
    );
    assert_eq!(
        read("1.5e300"),
        serde_json::json!({ "error": { "code": "out_of_range", "number": "1.5e300" } })
    );
    assert_eq!(
        read("[1,"),
        serde_json::json!({ "error": { "code": "syntax", "position": 4 } })
    );
}
