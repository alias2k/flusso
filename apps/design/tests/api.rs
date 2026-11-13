//! Database-free integration test for the designer API: open a project, preview
//! a schema, save it back, and reopen — the round-trip every editor session
//! depends on. The catalog/validate endpoints need a live Postgres and are
//! exercised through the source crate's introspection e2e instead.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::path::{Path, PathBuf};

use config::ParseFrom;
use design::api::{self, FileOp, OpKind, PreviewRequest, SaveRequest};
use design::value::{MAX_DEPTH, ValueSlot};
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
    let response = api::build_preview(
        Path::new("flusso.toml"),
        PreviewRequest {
            index: IndexName::try_new("orders").unwrap(),
            schema: schema.clone(),
            path: None,
            base: None,
            identity: None,
        },
    )
    .unwrap();
    assert!(
        response.parse_ok,
        "generated YAML must re-parse: {:?}",
        response.parse_error
    );
    assert!(response.preview.document.iter().any(|n| n.name == "items"));
    assert!(response.yaml.contains("- has_many: items"));

    // Save every index back (an Upsert per file): the model is unchanged, so
    // every file keeps its text and nothing is written.
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
                base: None,
                identity: None,
            })
            .collect(),
        skip: Vec::new(),
        identity: None,
    };
    let written = api::save_project(&config_path, save).unwrap();
    assert!(written.written.is_empty(), "{:?}", written.written);
    assert!(written.moved.is_empty() && written.deleted.is_empty());

    // Reopen: the files load and resolve to the same mapping.
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

    // Saving the reopened project again writes nothing either.
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
                base: None,
                identity: None,
            })
            .collect(),
        skip: Vec::new(),
        identity: None,
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
                    base: None,
                    identity: None,
                },
                FileOp {
                    kind: OpKind::Delete,
                    path: PathBuf::from(&doomed.schema_path),
                    from: None,
                    schema: None,
                    raw: None,
                    base: None,
                    identity: None,
                },
            ],
            skip: Vec::new(),
            identity: None,
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
                base: None,
                identity: None,
            }],
            skip: Vec::new(),
            identity: None,
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
    let response = api::build_preview(
        Path::new("flusso.toml"),
        PreviewRequest {
            index: IndexName::try_new("products").unwrap(),
            schema,
            path: None,
            base: None,
            identity: None,
        },
    )
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
        api::build_preview(
            Path::new("flusso.toml"),
            PreviewRequest {
                index: IndexName::try_new("t").unwrap(),
                schema,
                path: None,
                base: None,
                identity: None,
            },
        )
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
    let read_in = |text: &str, slot: ValueSlot| {
        serde_json::to_value(api::read_value(&api::ValueRequest {
            text: text.to_owned(),
            slot,
        }))
        .unwrap()
    };
    let read = |text: &str| read_in(text, ValueSlot::Constant);
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
    assert_eq!(
        read("1e3"),
        serde_json::json!({ "value": { "Decimal": "1000.0" }, "saved_as": "1000.0" })
    );
    assert_eq!(
        read(&"[".repeat(500)),
        serde_json::json!({ "error": { "code": "too_deep", "limit": MAX_DEPTH } })
    );
    assert_eq!(
        read_in("[1]", ValueSlot::Default),
        serde_json::json!({ "error": { "code": "not_scalar" } })
    );
    assert_eq!(
        read_in("[1]", ValueSlot::Option),
        serde_json::json!({ "value": { "Array": [{ "BigInt": 1 }] } })
    );
    assert_eq!(
        read_in(" 5 ", ValueSlot::Option),
        serde_json::json!({ "value": { "BigInt": 5 }, "saved_as": "5" })
    );
}

/// The deepest value `/api/value` accepts, in a field ten joins deep, passes
/// through the same JSON bodies preview, diff and save read (serde_json's
/// recursion limit) and is written to a file that reads back the same.
#[test]
fn the_deepest_accepted_value_previews_diffs_and_saves_ten_joins_deep() {
    const JOINS: usize = 10;
    let text = r#"[{"a": "#.repeat(MAX_DEPTH / 2) + "1" + "}]".repeat(MAX_DEPTH / 2).as_str();
    let accepted = serde_json::to_value(api::read_value(&api::ValueRequest {
        text,
        slot: ValueSlot::Constant,
    }))
    .unwrap();
    let value = accepted.get("value").cloned().unwrap();

    let mut yaml = String::from("version: 1\ntable: t0\nprimary_key: id\nfields:\n");
    for level in 1..=JOINS {
        let pad = "  ".repeat(level - 1);
        yaml.push_str(&format!(
            "{pad}- has_many: l{level}\n{pad}  table: t{level}\n{pad}  foreign_key: parent_id\n{pad}  primary_key: id\n{pad}  fields:\n"
        ));
    }
    let pad = "  ".repeat(JOINS);
    yaml.push_str(&format!("{pad}- constant: deep\n{pad}  value: null\n"));
    let mut schema = serde_json::to_value(
        kernel::IndexSchema::try_from(config::yaml::SchemaYaml::try_parse(&yaml).unwrap()).unwrap(),
    )
    .unwrap();
    let mut field = schema.pointer_mut("/fields/0").unwrap();
    for _ in 1..JOINS {
        field = field.pointer_mut("/source/relation/join/fields/0").unwrap();
    }
    *field
        .pointer_mut("/source/relation/join/fields/0/source/constant")
        .unwrap() = value.clone();

    let preview: PreviewRequest =
        serde_json::from_str(&serde_json::json!({ "index": "deep", "schema": schema }).to_string())
            .unwrap();
    let response = api::build_preview(Path::new("flusso.toml"), preview).unwrap();
    assert!(response.parse_ok, "{:?}", response.parse_error);

    let dir = fixture();
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let body = serde_json::json!({
        "config": project.config,
        "ops": [{ "kind": "upsert", "path": "deep.schema.yml", "schema": schema }],
    })
    .to_string();
    let diff: SaveRequest = serde_json::from_str(&body).unwrap();
    assert!(
        api::diff_project(&config_path, diff)
            .unwrap()
            .iter()
            .any(|d| d.changed)
    );
    let save: SaveRequest = serde_json::from_str(&body).unwrap();
    api::save_project(&config_path, save).unwrap();
    let written = std::fs::read_to_string(dir.join("deep.schema.yml")).unwrap();
    let reread = serde_json::to_value(
        kernel::IndexSchema::try_from(config::yaml::SchemaYaml::try_parse(&written).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reread, schema);
    std::fs::remove_dir_all(&dir).ok();
}

/// The ids the client sends for `fields`: the uid of each field is its path
/// of old names, so `renamed` (new, old) keeps a renamed field's id.
fn ids(fields: &[kernel::Field], parent: &str, renamed: &[(&str, &str)]) -> serde_json::Value {
    fields
        .iter()
        .map(|field| {
            let name = field.field.as_ref();
            let was = renamed
                .iter()
                .find(|(new, _)| *new == name)
                .map_or(name, |(_, old)| *old);
            let uid = format!("{parent}/{was}");
            let children: &[kernel::Field] = match &field.source {
                kernel::FieldSource::Group(fields) => fields,
                kernel::FieldSource::Relation(kernel::Relation::Join(join)) => &join.fields,
                _ => &[],
            };
            serde_json::json!({ "name": name, "uid": uid, "fields": ids(children, &uid, renamed) })
        })
        .collect()
}

#[test]
fn a_diff_pairs_by_the_ids_it_is_sent_and_reports_dropped_comments() {
    let dir = fixture();
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let users = project.indexes.iter().find(|i| i.name == "users").unwrap();
    let before = users.schema.clone().unwrap();
    let mut after = before.clone();
    after.fields.retain(|f| f.field.as_ref() != "orders");
    let count = after
        .fields
        .iter_mut()
        .find(|f| f.field.as_ref() == "orderCount")
        .unwrap();
    count.field = kernel::FieldName::try_new("ordersTotal").unwrap();
    let mut config = serde_json::to_value(&project.config).unwrap();
    let index = config["index"].as_array_mut().unwrap();
    index.remove(0);
    index[0]["name"] = "goods".into();

    let request = |identity: bool| -> SaveRequest {
        let mut op = serde_json::json!({
            "kind": "upsert",
            "path": "users.schema.yml",
            "schema": after,
        });
        let mut body = serde_json::json!({ "config": config, "ops": [] });
        if identity {
            op["identity"] = serde_json::json!({
                "before": ids(&before.fields, "", &[]),
                "after": ids(&after.fields, "", &[("ordersTotal", "orderCount")]),
            });
            body["identity"] = serde_json::json!({
                "before": ["users", "products", "orders"],
                "after": [1, 2],
            });
        }
        body["ops"] = serde_json::json!([op]);
        serde_json::from_value(body).unwrap()
    };

    let diffs = api::diff_project(&config_path, request(true)).unwrap();
    let toml = serde_json::to_value(&diffs[0]).unwrap();
    assert!(toml.get("dropped").is_none(), "{toml}");
    let next = toml["next"].as_str().unwrap();
    assert!(
        next.contains("name = \"goods\"\nschema = \"products.schema.yml\""),
        "{next}"
    );
    assert!(!next.contains("name = \"users\""));
    let yaml = serde_json::to_value(&diffs[1]).unwrap();
    assert!(yaml.get("dropped").is_none(), "{yaml}");
    let next = yaml["next"].as_str().unwrap();
    assert!(
        next.contains("  # Aggregates over the user's orders.\n  - count: ordersTotal\n"),
        "{next}"
    );
    assert!(!next.contains("One-to-many: orders"), "{next}");

    // Without ids the renamed count is a new field, and either removed field
    // may be the one it was: their comments (4 on `orders`, 1 on the count)
    // are dropped, and the review says so.
    let diffs = api::diff_project(&config_path, request(false)).unwrap();
    let yaml = serde_json::to_value(&diffs[1]).unwrap();
    assert_eq!(yaml["dropped"], 5, "{yaml}");
    assert!(!yaml["next"].as_str().unwrap().contains("# Aggregates over"));
}
