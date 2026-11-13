//! Database-free integration test for the designer API: open a project, preview
//! a schema, save it back, and reopen — the round-trip every editor session
//! depends on. The catalog/validate endpoints need a live Postgres and are
//! exercised through the source crate's introspection e2e instead.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use config::ParseFrom;
use config::toml::ConfigToml;
use design::api::{self, FileOp, OpKind, PreviewRequest, SaveRequest};
use design::keep::ConfigIdentity;
use design::value::{MAX_DEPTH, ValueSlot};
use kernel::IndexName;

/// A unique scratch directory for this test process, seeded with the dev files.
fn fixture() -> PathBuf {
    keep_records_in_target();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-design-api-{}", std::process::id()));
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
                index: IndexName::try_new(&i.name).unwrap(),
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
        loaded_at_ms: None,
    };
    let written = api::save_project(&config_path, save).unwrap();
    assert!(written.written.is_empty(), "{:?}", written.written);
    assert!(written.moved.is_empty());

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
                index: IndexName::try_new(&i.name).unwrap(),
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
        loaded_at_ms: None,
    };
    let again = api::save_project(&config_path, resave).unwrap();
    assert!(
        again.written.is_empty(),
        "re-saving unchanged files writes nothing: {:?}",
        again.written
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// A move into a fresh subfolder creates it; moving the file back out prunes
/// the now-empty subfolder. The old path is never left orphaned.
#[test]
fn save_moves_and_prunes() {
    let dir = own_fixture("prune");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let users_old = dir.join("users.schema.yml");

    let moved = SaveRequest {
        config: config_with(&project.config, &[("users", "nested/moved.schema.yml")]),
        ops: vec![op(
            OpKind::Move,
            "users",
            "nested/moved.schema.yml",
            Some("users.schema.yml"),
            "users: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    let resp = api::save_project(&config_path, moved).unwrap();
    assert!(
        dir.join("nested/moved.schema.yml").exists(),
        "moved file at new path"
    );
    assert!(!users_old.exists(), "old path gone after a move");
    assert_eq!(resp.moved.len(), 1);

    let back = SaveRequest {
        config: project.config.clone(),
        ops: vec![op(
            OpKind::Move,
            "users",
            "users.schema.yml",
            Some("nested/moved.schema.yml"),
            "users: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    let resp = api::save_project(&config_path, back).unwrap();
    assert!(!dir.join("nested").exists(), "emptied folder pruned");
    assert!(resp.pruned.iter().any(|p| p.ends_with("nested")));

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
        "config": config_with(&project.config, &[("deep", "deep.schema.yml")]),
        "ops": [{ "kind": "upsert", "index": "deep", "path": "deep.schema.yml", "schema": schema }],
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

/// A scratch copy of the dev project of its own, for a test that rewrites files.
fn own_fixture(tag: &str) -> PathBuf {
    keep_records_in_target();
    let shared = fixture();
    let dir = shared.with_file_name(format!("flusso-design-api-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for file in [
        "flusso.toml",
        "users.schema.yml",
        "products.schema.yml",
        "orders.schema.yml",
    ] {
        std::fs::copy(shared.join(file), dir.join(file)).unwrap();
    }
    // `x/../a.yml` spells `a.yml` only while `x` exists.
    std::fs::create_dir_all(dir.join("x")).unwrap();
    dir
}

fn op(kind: OpKind, index: &str, path: &str, from: Option<&str>, raw: &str) -> FileOp {
    FileOp {
        kind,
        index: IndexName::try_new(index).unwrap(),
        path: PathBuf::from(path),
        from: from.map(PathBuf::from),
        schema: None,
        raw: Some(raw.to_owned()),
        base: None,
        identity: None,
    }
}

/// `config` with each `(name, schema path)` set: an existing index's path
/// changed, or a new index appended.
fn config_with(config: &ConfigToml, paths: &[(&str, &str)]) -> ConfigToml {
    let mut value = serde_json::to_value(config).unwrap();
    let entries = value["index"].as_array_mut().unwrap();
    for (name, path) in paths {
        match entries.iter_mut().find(|e| e["name"] == *name) {
            Some(entry) => entry["schema"] = (*path).into(),
            None => {
                entries.push(serde_json::json!({ "name": name, "schema": path, "enabled": true }))
            }
        }
    }
    serde_json::from_value(value).unwrap()
}

/// Every file in the fixture project, by name, with its content.
fn files(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(e.path()).unwrap(),
            )
        })
        .collect()
}

/// `request` is refused by both the review and the save, and the project's
/// files are left exactly as they were.
fn refused(dir: &Path, request: impl Fn() -> SaveRequest) {
    let config_path = dir.join("flusso.toml");
    let before = files(dir);
    assert!(api::diff_project(&config_path, request()).is_err());
    assert!(api::save_project(&config_path, request()).is_err());
    assert_eq!(files(dir), before);
}

/// Two ops writing different content to one file would leave it holding one
/// index's schema under another's name (issue #270): the plan is refused and
/// nothing is written.
#[test]
fn a_save_refuses_two_different_writes_to_one_file() {
    let dir = own_fixture("clash");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    refused(&dir, || SaveRequest {
        config: project.config.clone(),
        ops: vec![
            op(
                OpKind::Upsert,
                "orders",
                "orders.schema.yml",
                None,
                "a: 1\n",
            ),
            op(
                OpKind::Upsert,
                "orders",
                "./orders.schema.yml",
                None,
                "b: 2\n",
            ),
        ],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });
    std::fs::remove_dir_all(&dir).ok();
}

/// Two names that differ only by case or surrounding spaces are one index
/// name: the config would not load, so it is never written.
#[test]
fn a_save_refuses_two_indexes_with_one_normalized_name() {
    let dir = own_fixture("dup-names");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    for spelling in ["orders", "Orders", "orders ", " ORDERS"] {
        let mut config = serde_json::to_value(&project.config).unwrap();
        config["index"][0]["name"] = spelling.into();
        let config: ConfigToml = serde_json::from_value(config).unwrap();
        refused(&dir, || SaveRequest {
            config: config.clone(),
            ops: vec![op(
                OpKind::Move,
                "orders",
                "orders.schema.yml",
                Some("users.schema.yml"),
                "x: 1\n",
            )],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        });
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// An index pointed at another index's file, however the path is spelled,
/// can't write it: the other index's schema would be lost (issue #270).
#[test]
fn a_save_refuses_to_write_another_index_s_file() {
    let dir = own_fixture("other-file");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    for path in [
        "orders.schema.yml",
        "./orders.schema.yml",
        "x/../orders.schema.yml",
    ] {
        for kind in [OpKind::Upsert, OpKind::Move] {
            refused(&dir, || SaveRequest {
                config: config_with(&project.config, &[("users", path)]),
                ops: vec![op(
                    kind,
                    "users",
                    path,
                    Some("users.schema.yml"),
                    "users: 1\n",
                )],
                skip: Vec::new(),
                identity: None,
                loaded_at_ms: None,
            });
        }
        refused(&dir, || SaveRequest {
            config: project.config.clone(),
            ops: vec![op(OpKind::Upsert, "users", path, None, "users: 1\n")],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        });
    }
    refused(&dir, || SaveRequest {
        config: project.config.clone(),
        ops: vec![op(
            OpKind::Upsert,
            "nowhere",
            "nowhere.schema.yml",
            None,
            "x: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });
    std::fs::remove_dir_all(&dir).ok();
}

/// Two indexes sharing a file both write it, with the same content, or the
/// save is refused.
#[test]
fn a_shared_file_is_written_only_by_every_index_that_uses_it() {
    let dir = own_fixture("shared");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let config = config_with(&project.config, &[("copy", "./users.schema.yml")]);
    refused(&dir, || SaveRequest {
        config: config.clone(),
        ops: vec![op(
            OpKind::Upsert,
            "copy",
            "users.schema.yml",
            None,
            "x: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });
    let both = SaveRequest {
        config,
        ops: vec![
            op(OpKind::Upsert, "users", "users.schema.yml", None, "x: 1\n"),
            op(OpKind::Upsert, "copy", "./users.schema.yml", None, "x: 1\n"),
        ],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    api::save_project(&config_path, both).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("users.schema.yml")).unwrap(),
        "x: 1\n"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A schema op never replaces or moves the config file.
#[test]
fn a_save_refuses_a_schema_op_on_the_config_file() {
    let dir = own_fixture("config-path");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    for path in ["flusso.toml", "./flusso.toml", "x/../flusso.toml"] {
        for kind in [OpKind::Upsert, OpKind::Move] {
            refused(&dir, || SaveRequest {
                config: config_with(&project.config, &[("users", path)]),
                ops: vec![op(
                    kind,
                    "users",
                    path,
                    Some("users.schema.yml"),
                    "users: 1\n",
                )],
                skip: Vec::new(),
                identity: None,
                loaded_at_ms: None,
            });
        }
        refused(&dir, || SaveRequest {
            config: config_with(&project.config, &[("users", "people.schema.yml")]),
            ops: vec![op(
                OpKind::Move,
                "users",
                "people.schema.yml",
                Some(path),
                "users: 1\n",
            )],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        });
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// There is no delete op, and a move never takes a file outside the project
/// as its source: it is not the index's own file.
#[test]
fn a_save_removes_nothing_outside_the_project() {
    let dir = own_fixture("outside");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let victim = dir.with_file_name(format!(
        "{}-victim.txt",
        dir.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(&victim, "keep\n").unwrap();
    let victim_rel = format!("../{}", victim.file_name().unwrap().to_string_lossy());

    let delete = serde_json::json!({
        "config": project.config,
        "ops": [{ "kind": "delete", "index": "users", "path": victim_rel }],
    });
    assert!(serde_json::from_value::<SaveRequest>(delete).is_err());

    let moved = || SaveRequest {
        config: project.config.clone(),
        ops: vec![op(
            OpKind::Move,
            "users",
            "users.schema.yml",
            Some(&victim_rel),
            "users: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    refused(&dir, moved);
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep\n");

    std::fs::remove_file(&victim).ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// A move whose source an index of the config still uses keeps that file:
/// swapping two files' contents, or moving an index off a file a new index
/// takes, loses neither.
#[test]
fn a_move_never_removes_a_file_an_index_uses() {
    let dir = own_fixture("swap");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();

    let swap = || SaveRequest {
        config: config_with(
            &project.config,
            &[
                ("orders", "products.schema.yml"),
                ("products", "orders.schema.yml"),
            ],
        ),
        ops: vec![
            op(
                OpKind::Move,
                "orders",
                "products.schema.yml",
                Some("orders.schema.yml"),
                "orders: 1\n",
            ),
            op(
                OpKind::Move,
                "products",
                "orders.schema.yml",
                Some("products.schema.yml"),
                "products: 1\n",
            ),
        ],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    let diffs = api::diff_project(&config_path, swap()).unwrap();
    assert!(diffs.iter().all(|d| d.op == api::DiffOp::Write));
    api::save_project(&config_path, swap()).unwrap();
    assert_eq!(read("products.schema.yml"), "orders: 1\n");
    assert_eq!(read("orders.schema.yml"), "products: 1\n");

    let project = api::load_project(&config_path).unwrap();
    let replaced = SaveRequest {
        config: config_with(
            &project.config,
            &[
                ("users", "archive/users.schema.yml"),
                ("fresh", "users.schema.yml"),
            ],
        ),
        ops: vec![
            op(
                OpKind::Move,
                "users",
                "archive/users.schema.yml",
                Some("users.schema.yml"),
                "users: 1\n",
            ),
            op(
                OpKind::Upsert,
                "fresh",
                "users.schema.yml",
                None,
                "fresh: 1\n",
            ),
        ],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    api::save_project(&config_path, replaced).unwrap();
    assert_eq!(read("archive/users.schema.yml"), "users: 1\n");
    assert_eq!(read("users.schema.yml"), "fresh: 1\n");

    // A move's source must be the index's own file on disk.
    let project = api::load_project(&config_path).unwrap();
    refused(&dir, || SaveRequest {
        config: config_with(&project.config, &[("orders", "sales.schema.yml")]),
        ops: vec![op(
            OpKind::Move,
            "orders",
            "sales.schema.yml",
            Some("./orders.schema.yml"),
            "orders: 2\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });

    std::fs::remove_dir_all(&dir).ok();
}

/// A move onto a file that already exists says so in the review, however the
/// destination is spelled; a write reads the file it replaces on the same
/// normalized path.
#[test]
fn a_move_onto_an_existing_file_is_flagged() {
    let dir = own_fixture("overwrite");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    std::fs::write(dir.join("old.schema.yml"), "old: 1\n").unwrap();
    for path in ["old.schema.yml", "./old.schema.yml", "x/../old.schema.yml"] {
        let request = SaveRequest {
            config: config_with(&project.config, &[("orders", path)]),
            ops: vec![op(
                OpKind::Move,
                "orders",
                path,
                Some("orders.schema.yml"),
                "orders: 1\n",
            )],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        };
        let diffs = api::diff_project(&config_path, request).unwrap();
        let moved = diffs.last().unwrap();
        assert_eq!(moved.op, api::DiffOp::Move);
        assert_eq!(moved.warning.as_deref(), Some("overwrites"), "{path}");
        assert!(moved.path.ends_with("/old.schema.yml") && !moved.path.contains(".."));
    }

    let orders = std::fs::read_to_string(dir.join("orders.schema.yml")).unwrap();
    let request = SaveRequest {
        config: config_with(&project.config, &[("orders", "x/../orders.schema.yml")]),
        ops: vec![op(
            OpKind::Upsert,
            "orders",
            "x/../orders.schema.yml",
            None,
            "orders: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    let diffs = api::diff_project(&config_path, request).unwrap();
    assert_eq!(diffs.last().unwrap().current, orders);

    std::fs::remove_dir_all(&dir).ok();
}

/// An `op` for a renamed index, `was` its name on disk.
fn renamed(kind: OpKind, was: &str, index: &str, path: &str, from: Option<&str>) -> FileOp {
    op(kind, index, path, from, &format!("{was}: renamed\n"))
}

/// The config identity the client sends: the names on disk, and for each
/// saved index its position among them.
fn identity(on_disk: &ConfigToml, after: &[Option<usize>]) -> Option<ConfigIdentity> {
    Some(ConfigIdentity {
        before: on_disk.index.iter().map(|e| e.name.to_string()).collect(),
        after: after.to_vec(),
        sinks: None,
    })
}

/// `config` with the index `name` removed.
fn config_without(config: &ConfigToml, name: &str) -> ConfigToml {
    let mut config = config.clone();
    config.index.retain(|e| e.name.as_str() != name);
    config
}

/// `config` with the index `from` renamed to `to` (its schema path kept).
fn config_renamed(config: &ConfigToml, from: &str, to: &str) -> ConfigToml {
    let mut value = serde_json::to_value(config).unwrap();
    for entry in value["index"].as_array_mut().unwrap() {
        if entry["name"] == from {
            entry["name"] = to.into();
        }
    }
    serde_json::from_value(value).unwrap()
}

/// The config file's path as the review reports it (resolved on disk).
fn config_shown(dir: &Path) -> String {
    std::fs::canonicalize(dir.join("flusso.toml"))
        .unwrap()
        .display()
        .to_string()
}

/// `request` is reviewed (with every file checked), but refused by the save
/// with its unchecked files, which writes nothing.
fn save_refused(dir: &Path, request: impl Fn() -> SaveRequest) {
    let config_path = dir.join("flusso.toml");
    let before = files(dir);
    let review = SaveRequest {
        skip: Vec::new(),
        ..request()
    };
    api::diff_project(&config_path, review).unwrap();
    assert!(api::save_project(&config_path, request()).is_err());
    assert_eq!(files(dir), before);
}

/// Leaving `flusso.toml` out of a save (unchecked in the review) checks the
/// ops against the config on disk, which a rename doesn't fit: removing
/// `orders` and renaming `users` to `orders` must not put the users schema in
/// `orders.schema.yml`, and a plain rename must not move `users.schema.yml`
/// away from the config that still names it (issue #270).
#[test]
fn a_rename_is_refused_when_the_config_is_left_out() {
    let dir = own_fixture("skip-config");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    let skip = vec![config_shown(&dir)];

    let taken_over = config_renamed(
        &config_with(
            &config_without(&project.config, "orders"),
            &[("users", "orders.schema.yml")],
        ),
        "users",
        "orders",
    );
    for kind in [OpKind::Move, OpKind::Upsert] {
        save_refused(&dir, || SaveRequest {
            config: taken_over.clone(),
            ops: vec![renamed(
                kind,
                "users",
                "orders",
                "orders.schema.yml",
                Some("users.schema.yml"),
            )],
            skip: skip.clone(),
            identity: identity(&project.config, &[Some(0), Some(1)]),
            loaded_at_ms: None,
        });
    }

    let people = config_renamed(
        &config_with(&project.config, &[("users", "people.schema.yml")]),
        "users",
        "people",
    );
    save_refused(&dir, || SaveRequest {
        config: people.clone(),
        ops: vec![renamed(
            OpKind::Move,
            "users",
            "people",
            "people.schema.yml",
            Some("users.schema.yml"),
        )],
        skip: skip.clone(),
        identity: identity(&project.config, &[Some(0), Some(1), Some(2)]),
        loaded_at_ms: None,
    });
    assert!(dir.join("users.schema.yml").is_file());
    assert!(!dir.join("people.schema.yml").exists());

    std::fs::remove_dir_all(&dir).ok();
}

/// The other way round: a written config may not name a file the save leaves
/// out, so every index of the config on disk keeps its file.
#[test]
fn the_config_is_refused_when_a_file_it_needs_is_left_out() {
    let dir = own_fixture("skip-op");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    let people = config_renamed(
        &config_with(&project.config, &[("users", "people.schema.yml")]),
        "users",
        "people",
    );
    let request = |skip: Vec<String>| SaveRequest {
        config: people.clone(),
        ops: vec![renamed(
            OpKind::Move,
            "users",
            "people",
            "people.schema.yml",
            Some("users.schema.yml"),
        )],
        skip,
        identity: identity(&project.config, &[Some(0), Some(1), Some(2)]),
        loaded_at_ms: None,
    };
    let diffs = api::diff_project(&dir.join("flusso.toml"), request(Vec::new())).unwrap();
    let moved = diffs.last().unwrap();
    assert!(moved.needs_config);
    let moved = moved.path.clone();
    save_refused(&dir, || request(vec![moved.clone()]));

    let fresh = config_with(&project.config, &[("fresh", "fresh.schema.yml")]);
    refused(&dir, || SaveRequest {
        config: fresh.clone(),
        ops: Vec::new(),
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });
    std::fs::remove_dir_all(&dir).ok();
}

/// What doesn't depend on the new config is saved without it: an edit of an
/// index on its own file, with `flusso.toml` left out.
#[test]
fn an_edit_is_saved_with_the_config_left_out() {
    let dir = own_fixture("skip-edit");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let config_text = std::fs::read_to_string(&config_path).unwrap();
    let people = config_renamed(&project.config, "users", "people");
    let request = SaveRequest {
        config: people,
        ops: vec![op(
            OpKind::Upsert,
            "orders",
            "orders.schema.yml",
            None,
            "orders: 1\n",
        )],
        skip: vec![config_shown(&dir)],
        identity: None,
        loaded_at_ms: None,
    };
    let diffs = api::diff_project(&config_path, request).unwrap();
    assert!(!diffs.last().unwrap().needs_config);
    let request = SaveRequest {
        config: config_renamed(&project.config, "users", "people"),
        ops: vec![op(
            OpKind::Upsert,
            "orders",
            "orders.schema.yml",
            None,
            "orders: 1\n",
        )],
        skip: vec![config_shown(&dir)],
        identity: None,
        loaded_at_ms: None,
    };
    api::save_project(&config_path, request).unwrap();
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), config_text);
    assert_eq!(
        std::fs::read_to_string(dir.join("orders.schema.yml")).unwrap(),
        "orders: 1\n"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Paths are compared as the filesystem resolves them: through `sub -> .`,
/// `sub/orders.schema.yml` is orders' file and `sub/flusso.toml` the config.
#[cfg(unix)]
#[test]
fn a_symlink_cannot_reach_another_index_s_file_or_the_config() {
    let dir = own_fixture("symlink");
    std::os::unix::fs::symlink(".", dir.join("sub")).unwrap();
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    for path in [
        "sub/orders.schema.yml",
        "sub/flusso.toml",
        "sub/sub/flusso.toml",
    ] {
        for kind in [OpKind::Upsert, OpKind::Move] {
            refused(&dir, || SaveRequest {
                config: config_with(&project.config, &[("users", path)]),
                ops: vec![op(
                    kind,
                    "users",
                    path,
                    Some("users.schema.yml"),
                    "users: 1\n",
                )],
                skip: Vec::new(),
                identity: None,
                loaded_at_ms: None,
            });
        }
    }
    refused(&dir, || SaveRequest {
        config: config_with(&project.config, &[("users", "people.schema.yml")]),
        ops: vec![op(
            OpKind::Move,
            "users",
            "people.schema.yml",
            Some("sub/flusso.toml"),
            "users: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });

    let orders = std::fs::read_to_string(dir.join("orders.schema.yml")).unwrap();
    let from_orders = || SaveRequest {
        config: config_with(&project.config, &[("users", "people.schema.yml")]),
        ops: vec![op(
            OpKind::Move,
            "users",
            "people.schema.yml",
            Some("sub/orders.schema.yml"),
            "users: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    refused(&dir, from_orders);
    assert_eq!(
        std::fs::read_to_string(dir.join("orders.schema.yml")).unwrap(),
        orders
    );
    assert!(dir.join("users.schema.yml").is_file());

    std::fs::remove_dir_all(&dir).ok();
}

/// A project opened through a symlinked directory: the config's real path and
/// a `..` back through the link both reach the same files.
#[cfg(unix)]
#[test]
fn a_project_opened_through_a_symlink_resolves_its_paths() {
    let dir = own_fixture("linked");
    let link = dir.with_file_name(format!(
        "{}-link",
        dir.file_name().unwrap().to_string_lossy()
    ));
    std::fs::remove_file(&link).ok();
    std::os::unix::fs::symlink(&dir, &link).unwrap();
    let config_path = link.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let real_config = config_shown(&dir);
    let up = format!(
        "../{}/orders.schema.yml",
        dir.file_name().unwrap().to_string_lossy()
    );
    for path in [real_config.as_str(), up.as_str()] {
        let before = files(&dir);
        let request = || SaveRequest {
            config: config_with(&project.config, &[("users", path)]),
            ops: vec![op(OpKind::Upsert, "users", path, None, "users: 1\n")],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        };
        assert!(
            api::diff_project(&config_path, request()).is_err(),
            "{path}"
        );
        assert!(
            api::save_project(&config_path, request()).is_err(),
            "{path}"
        );
        assert_eq!(files(&dir), before);
    }
    std::fs::remove_file(&link).ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// On a case-insensitive filesystem `Orders.schema.yml` is orders' file and
/// `FLUSSO.TOML` the config: a path differing only in case is refused
/// everywhere.
#[test]
fn a_path_differing_only_in_case_is_refused() {
    let dir = own_fixture("case");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    for path in ["Orders.schema.yml", "FLUSSO.TOML", "x/../Flusso.toml"] {
        for kind in [OpKind::Upsert, OpKind::Move] {
            refused(&dir, || SaveRequest {
                config: config_with(&project.config, &[("users", path)]),
                ops: vec![op(
                    kind,
                    "users",
                    path,
                    Some("users.schema.yml"),
                    "users: 1\n",
                )],
                skip: Vec::new(),
                identity: None,
                loaded_at_ms: None,
            });
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// A schema path that is a folder is refused before anything is written: no
/// config rewrite, no temp file.
#[test]
fn a_folder_as_a_schema_path_changes_nothing() {
    let dir = own_fixture("folder");
    std::fs::create_dir_all(dir.join("people.schema.yml")).unwrap();
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    for kind in [OpKind::Upsert, OpKind::Move] {
        refused(&dir, || SaveRequest {
            config: config_with(&project.config, &[("users", "people.schema.yml")]),
            ops: vec![op(
                kind,
                "users",
                "people.schema.yml",
                Some("users.schema.yml"),
                "users: 1\n",
            )],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        });
    }
    assert_eq!(
        std::fs::read_dir(dir.join("people.schema.yml"))
            .unwrap()
            .count(),
        0
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A write that fails while staging leaves every file as it was and no temp
/// file or created folder behind.
#[cfg(unix)]
#[test]
fn a_failed_write_leaves_no_temp_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = own_fixture("readonly");
    let locked = dir.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(locked.join("probe"), "").is_ok() {
        // Running as root: permissions don't stop the write.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        return;
    }
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    let before = files(&dir);
    let request = SaveRequest {
        config: config_with(
            &project.config,
            &[
                ("users", "fresh/users.schema.yml"),
                ("orders", "locked/orders.schema.yml"),
            ],
        ),
        ops: vec![
            op(
                OpKind::Move,
                "users",
                "fresh/users.schema.yml",
                Some("users.schema.yml"),
                "users: 1\n",
            ),
            op(
                OpKind::Move,
                "orders",
                "locked/orders.schema.yml",
                Some("orders.schema.yml"),
                "orders: 1\n",
            ),
        ],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    assert!(api::save_project(&dir.join("flusso.toml"), request).is_err());
    assert_eq!(files(&dir), before);
    assert!(!dir.join("fresh").exists());
    assert_eq!(std::fs::read_dir(&locked).unwrap().count(), 0);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

/// A move takes only the same index's file on disk as its source: one from
/// any other file in the project (`flusso.lock`, a removed index's file, as a
/// forged request could send) is refused, and the file stays.
#[test]
fn a_move_from_a_file_that_is_not_the_index_s_is_refused() {
    let dir = own_fixture("not-own");
    let config_path = dir.join("flusso.toml");
    std::fs::write(dir.join("flusso.lock"), "lock\n").unwrap();
    let project = api::load_project(&config_path).unwrap();
    for from in ["flusso.lock", "products.schema.yml"] {
        let config = config_without(&project.config, "products");
        let request = || SaveRequest {
            config: config.clone(),
            ops: vec![op(
                OpKind::Move,
                "users",
                "users.schema.yml",
                Some(from),
                "users: 1\n",
            )],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        };
        refused(&dir, request);
        assert!(dir.join(from).is_file(), "{from}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// A schema path with a trailing slash or `./` is saved as flusso opens it.
#[test]
fn a_schema_path_is_saved_normalized() {
    let dir = own_fixture("trailing");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let schema = project
        .indexes
        .iter()
        .find(|i| i.name == "users")
        .unwrap()
        .schema
        .clone();
    let request = SaveRequest {
        config: config_with(&project.config, &[("users", "./people.schema.yml/")]),
        ops: vec![FileOp {
            schema,
            raw: None,
            ..op(
                OpKind::Move,
                "users",
                "people.schema.yml/",
                Some("users.schema.yml"),
                "",
            )
        }],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    };
    api::save_project(&config_path, request).unwrap();
    let text = std::fs::read_to_string(&config_path).unwrap();
    assert!(text.contains("schema = \"people.schema.yml\""), "{text}");
    let reopened = api::load_project(&config_path).unwrap();
    let users = reopened.indexes.iter().find(|i| i.name == "users").unwrap();
    assert!(users.schema.is_some(), "{:?}", users.error);
    assert!(!dir.join("users.schema.yml").exists());
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
    index.remove(2);
    index[1]["name"] = "goods".into();

    let request = |identity: bool| -> SaveRequest {
        let mut op = serde_json::json!({
            "kind": "upsert",
            "index": "users",
            "path": "users.schema.yml",
            "schema": after,
        });
        let mut body = serde_json::json!({
            "config": config,
            "ops": [],
            "identity": { "before": ["users", "products", "orders"], "after": [0, 1] },
        });
        if identity {
            op["identity"] = serde_json::json!({
                "before": ids(&before.fields, "", &[]),
                "after": ids(&after.fields, "", &[("ordersTotal", "orderCount")]),
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
    assert!(!next.contains("name = \"orders\""));
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

/// Leaving `flusso.toml` out, with the files linked to it (a renamed, moved
/// or new index), still saves an unrelated schema edit: the ops left out are
/// not written, so they are not checked against the config on disk (#270).
#[test]
fn the_config_and_its_linked_files_can_be_left_out_of_a_save() {
    let dir = own_fixture("skip-linked");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let all = identity(&project.config, &[Some(0), Some(1), Some(2)]);
    let edit = op(
        OpKind::Upsert,
        "orders",
        "orders.schema.yml",
        None,
        "orders: edited\n",
    );
    let cases: Vec<(ConfigToml, FileOp, Option<ConfigIdentity>)> = vec![
        (
            config_renamed(
                &config_with(&project.config, &[("users", "people.schema.yml")]),
                "users",
                "people",
            ),
            renamed(
                OpKind::Move,
                "users",
                "people",
                "people.schema.yml",
                Some("users.schema.yml"),
            ),
            all.clone(),
        ),
        (
            config_with(&project.config, &[("users", "sub/users.schema.yml")]),
            op(
                OpKind::Move,
                "users",
                "sub/users.schema.yml",
                Some("users.schema.yml"),
                "users: moved\n",
            ),
            all.clone(),
        ),
        (
            config_with(&project.config, &[("fresh", "fresh.schema.yml")]),
            op(
                OpKind::Upsert,
                "fresh",
                "fresh.schema.yml",
                None,
                "fresh: 1\n",
            ),
            identity(&project.config, &[Some(0), Some(1), Some(2), None]),
        ),
    ];
    let before = files(&dir);
    for (config, linked, identity) in cases {
        let request = |skip: Vec<String>| SaveRequest {
            config: config.clone(),
            ops: vec![linked.clone(), edit.clone()],
            skip,
            identity: identity.clone(),
            loaded_at_ms: None,
        };
        let diffs = api::diff_project(&config_path, request(Vec::new())).unwrap();
        let linked_path = diffs.get(1).unwrap();
        assert!(linked_path.needs_config);
        let skip = vec![config_shown(&dir), linked_path.path.clone()];
        let saved = api::save_project(&config_path, request(skip)).unwrap();
        assert_eq!(saved.written.len(), 1, "{:?}", saved.written);
        let mut expected = before.clone();
        expected.insert("orders.schema.yml".into(), "orders: edited\n".into());
        assert_eq!(files(&dir), expected);
        assert!(!dir.join("sub").exists());
        std::fs::write(dir.join("orders.schema.yml"), &before["orders.schema.yml"]).unwrap();
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// The server works out which index on disk each saved index is (from the
/// config identity, else by name) and never takes the request's word for it:
/// with `orders` removed and `users` renamed to `orders`, the new `orders`
/// is `users`, so its schema can't go into `orders.schema.yml` while the
/// config on disk still names the old `orders` there.
#[test]
fn which_index_an_op_is_comes_from_the_config_identity() {
    let dir = own_fixture("derived");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    let taken_over = config_renamed(
        &config_with(
            &config_without(&project.config, "orders"),
            &[("users", "orders.schema.yml")],
        ),
        "users",
        "orders",
    );
    let users_is_orders = identity(&project.config, &[Some(0), Some(1)]);
    let upsert = || {
        op(
            OpKind::Upsert,
            "orders",
            "orders.schema.yml",
            None,
            "users: schema\n",
        )
    };
    // The config left out: the op only fits the new config.
    save_refused(&dir, || SaveRequest {
        config: taken_over.clone(),
        ops: vec![upsert()],
        skip: vec![config_shown(&dir)],
        identity: users_is_orders.clone(),
        loaded_at_ms: None,
    });
    // The config written without the op: `orders.schema.yml` holds the old
    // `orders`, which is not the new `orders`.
    refused(&dir, || SaveRequest {
        config: taken_over.clone(),
        ops: Vec::new(),
        skip: Vec::new(),
        identity: users_is_orders.clone(),
        loaded_at_ms: None,
    });
    // A move must start at the index's own file on disk.
    refused(&dir, || SaveRequest {
        config: taken_over.clone(),
        ops: vec![op(
            OpKind::Move,
            "orders",
            "orders.schema.yml",
            Some("products.schema.yml"),
            "users: schema\n",
        )],
        skip: Vec::new(),
        identity: users_is_orders.clone(),
        loaded_at_ms: None,
    });
    // The request's own `was` is not read.
    let forged = serde_json::json!({
        "config": config_without(&project.config, "products"),
        "ops": [{
            "kind": "move", "index": "users", "was": "products",
            "from": "products.schema.yml", "path": "users.schema.yml", "raw": "x: 1\n",
        }],
    });
    refused(&dir, || serde_json::from_value(forged.clone()).unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

/// A written config can't point an index at a file that holds another
/// index's schema, unless the save writes it for both; two indexes may keep
/// sharing a file only when they already did.
#[test]
fn a_config_cannot_repoint_an_index_at_another_index_s_file() {
    let dir = own_fixture("repoint");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    refused(&dir, || SaveRequest {
        config: config_with(&project.config, &[("users", "orders.schema.yml")]),
        ops: Vec::new(),
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });
    refused(&dir, || SaveRequest {
        config: config_with(&project.config, &[("users", "orders.schema.yml")]),
        ops: vec![op(
            OpKind::Upsert,
            "users",
            "orders.schema.yml",
            None,
            "users: 1\n",
        )],
        skip: Vec::new(),
        identity: None,
        loaded_at_ms: None,
    });

    // Two indexes already sharing a file keep it without writing it.
    let shared = config_with(&project.config, &[("copy", "users.schema.yml")]);
    api::save_project(
        &config_path,
        SaveRequest {
            config: shared.clone(),
            ops: vec![
                op(OpKind::Upsert, "users", "users.schema.yml", None, "u: 1\n"),
                op(OpKind::Upsert, "copy", "users.schema.yml", None, "u: 1\n"),
            ],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        },
    )
    .unwrap();
    let mut toggled = serde_json::to_value(&shared).unwrap();
    toggled["index"][3]["enabled"] = false.into();
    api::save_project(
        &config_path,
        SaveRequest {
            config: serde_json::from_value(toggled).unwrap(),
            ops: Vec::new(),
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        },
    )
    .unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

/// A `skip` path is resolved like every other path: the config left out
/// through the symlinked project path is left out.
#[cfg(unix)]
#[test]
fn a_skip_path_through_a_symlink_is_skipped() {
    let dir = own_fixture("skip-link");
    let link = dir.with_file_name(format!(
        "{}-link",
        dir.file_name().unwrap().to_string_lossy()
    ));
    std::fs::remove_file(&link).ok();
    std::os::unix::fs::symlink(&dir, &link).unwrap();
    let config_path = link.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let config_text = std::fs::read_to_string(dir.join("flusso.toml")).unwrap();
    let people = config_renamed(
        &config_with(&project.config, &[("users", "people.schema.yml")]),
        "users",
        "people",
    );
    let skip = vec![link.join("flusso.toml").display().to_string()];
    save_refused(&dir, || SaveRequest {
        config: people.clone(),
        ops: vec![renamed(
            OpKind::Move,
            "users",
            "people",
            "people.schema.yml",
            Some("users.schema.yml"),
        )],
        skip: skip.clone(),
        identity: identity(&project.config, &[Some(0), Some(1), Some(2)]),
        loaded_at_ms: None,
    });
    api::save_project(
        &config_path,
        SaveRequest {
            config: people.clone(),
            ops: vec![op(
                OpKind::Upsert,
                "orders",
                "orders.schema.yml",
                None,
                "orders: 1\n",
            )],
            skip,
            identity: identity(&project.config, &[Some(0), Some(1), Some(2)]),
            loaded_at_ms: None,
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("flusso.toml")).unwrap(),
        config_text
    );
    std::fs::remove_file(&link).ok();
    std::fs::remove_dir_all(&dir).ok();
}

/// An index added with no schema file can't be saved, and the refusal names
/// the file relative to the project.
#[test]
fn an_added_index_needs_its_schema_file() {
    let dir = own_fixture("no-schema");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let request = || SaveRequest {
        config: config_with(&project.config, &[("new_index", "new_index.schema.yml")]),
        ops: Vec::new(),
        skip: Vec::new(),
        identity: identity(&project.config, &[Some(0), Some(1), Some(2), None]),
        loaded_at_ms: None,
    };
    refused(&dir, request);
    let err = api::diff_project(&config_path, request())
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("`new_index` has no schema file: new_index.schema.yml does not exist"),
        "{err}"
    );
    assert!(!err.contains(&*dir.to_string_lossy()), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A new index may take a file no index used only when that file reads as a
/// schema: anything else would leave a project that does not load.
#[test]
fn a_new_index_cannot_take_a_file_that_is_not_a_schema() {
    let dir = own_fixture("not-a-schema");
    let config_path = dir.join("flusso.toml");
    std::fs::write(dir.join("notes.txt"), "not a schema\n").unwrap();
    let project = api::load_project(&config_path).unwrap();
    let request = || SaveRequest {
        config: config_with(&project.config, &[("extra", "notes.txt")]),
        ops: Vec::new(),
        skip: Vec::new(),
        identity: identity(&project.config, &[Some(0), Some(1), Some(2), None]),
        loaded_at_ms: None,
    };
    refused(&dir, request);
    let err = format!(
        "{:#}",
        api::diff_project(&config_path, request()).unwrap_err()
    );
    assert!(err.contains("does not read as a schema"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// Removing an index and adding one with its name writes the new schema over
/// the old file in the designer's own form, and the review flags that it
/// replaces an existing file.
#[test]
fn a_new_index_over_a_removed_one_s_file_is_flagged() {
    let dir = own_fixture("readd");
    let config_path = dir.join("flusso.toml");
    let project = api::load_project(&config_path).unwrap();
    let request = SaveRequest {
        config: project.config.clone(),
        ops: vec![op(
            OpKind::Upsert,
            "orders",
            "orders.schema.yml",
            None,
            "orders: new\n",
        )],
        skip: Vec::new(),
        identity: identity(&project.config, &[Some(0), Some(1), None]),
        loaded_at_ms: None,
    };
    let diffs = api::diff_project(&config_path, request).unwrap();
    let written = diffs.last().unwrap();
    assert_eq!(written.warning.as_deref(), Some("overwrites"));
    assert!(written.needs_config);

    // The old file lends the new index none of its comments.
    let orders = project
        .indexes
        .iter()
        .find(|i| i.name == "orders")
        .unwrap()
        .schema
        .clone();
    let readd = SaveRequest {
        config: project.config.clone(),
        ops: vec![FileOp {
            schema: orders,
            raw: None,
            ..op(OpKind::Upsert, "orders", "orders.schema.yml", None, "")
        }],
        skip: Vec::new(),
        identity: identity(&project.config, &[Some(0), Some(1), None]),
        loaded_at_ms: None,
    };
    let diffs = api::diff_project(&config_path, readd).unwrap();
    let written = diffs.last().unwrap();
    assert!(written.current.contains('#'));
    assert!(!written.next.contains('#'), "{}", written.next);

    let edit = SaveRequest {
        config: project.config.clone(),
        ops: vec![op(
            OpKind::Upsert,
            "orders",
            "orders.schema.yml",
            None,
            "orders: edit\n",
        )],
        skip: Vec::new(),
        identity: identity(&project.config, &[Some(0), Some(1), Some(2)]),
        loaded_at_ms: None,
    };
    let diffs = api::diff_project(&config_path, edit).unwrap();
    assert_eq!(diffs.last().unwrap().warning, None);
    std::fs::remove_dir_all(&dir).ok();
}

/// A `*.schema.yml` with CRLF line endings keeps them, and its comments,
/// through a field rename and through a value edit, with no notice (the
/// field ids the designer sends say which field was renamed).
#[test]
fn a_crlf_schema_keeps_its_line_endings_and_comments() {
    let dir = own_fixture("crlf");
    let config_path = dir.join("flusso.toml");
    let lf = std::fs::read_to_string(dir.join("users.schema.yml")).unwrap();
    std::fs::write(dir.join("users.schema.yml"), lf.replace('\n', "\r\n")).unwrap();
    let project = api::load_project(&config_path).unwrap();
    let schema = project
        .indexes
        .iter()
        .find(|i| i.name == "users")
        .unwrap()
        .schema
        .clone()
        .unwrap();
    let mut rename = schema.clone();
    let field = rename
        .fields
        .iter_mut()
        .find(|f| f.field.as_ref() == "fullName")
        .unwrap();
    field.field = kernel::FieldName::try_new("displayName").unwrap();
    let mut value = schema.clone();
    value.primary_key = None;
    for (label, edited, renames) in [
        ("rename", rename, &[("displayName", "fullName")][..]),
        ("value", value, &[][..]),
    ] {
        let ids = serde_json::json!({
            "before": ids(&schema.fields, "", &[]),
            "after": ids(&edited.fields, "", renames),
        });
        let request = SaveRequest {
            config: project.config.clone(),
            ops: vec![FileOp {
                schema: Some(edited),
                identity: Some(serde_json::from_value(ids).unwrap()),
                raw: None,
                ..op(OpKind::Upsert, "users", "users.schema.yml", None, "")
            }],
            skip: Vec::new(),
            identity: None,
            loaded_at_ms: None,
        };
        let diffs = api::diff_project(&config_path, request).unwrap();
        let written = diffs.last().unwrap();
        assert!(written.changed, "{label}");
        assert_eq!(written.notice, None, "{label}");
        assert_eq!(written.dropped, 0, "{label}");
        assert!(
            !written.next.replace("\r\n", "").contains('\n'),
            "{label}: a bare LF"
        );
        assert_eq!(
            written.next.matches('#').count(),
            lf.matches('#').count(),
            "{label}"
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// Keep the designer's save records in the build's temp folder, never in the
/// user's state folder.
fn keep_records_in_target() {
    api::use_state_dir(PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("design-saves"));
}

/// A schema whose file would not load (here a `keyword` array whose `default`
/// lists an object, which the parser refuses) is refused by the review and the
/// save, and the file on disk keeps its comments: `keep` writes the canonical
/// form when the patched file does not read back, and that form would not
/// load either.
#[test]
fn a_schema_that_would_not_load_is_not_saved() {
    use config::yaml::SchemaYaml;
    use kernel::{FieldSource, GenericValue, IndexSchema, ParseFrom};

    let dir = own_fixture("not-loading");
    let project = api::load_project(&dir.join("flusso.toml")).unwrap();
    let text = std::fs::read_to_string(dir.join("orders.schema.yml")).unwrap();
    assert!(text.contains('#'), "the fixture keeps comments to lose");
    let mut schema = IndexSchema::try_from(SchemaYaml::try_parse(&text).unwrap()).unwrap();
    let tags = IndexSchema::try_from(
        SchemaYaml::try_parse(
            "version: 1\ntable: t\nprimary_key: id\nfields:\n- keyword: tags\n  required: false\n  array: true\n",
        )
        .unwrap(),
    )
    .unwrap();
    let mut field = tags.fields.into_iter().next().unwrap();
    let FieldSource::Column(column) = &mut field.source else {
        panic!("a column field");
    };
    column.default = Some(GenericValue::Array(vec![GenericValue::Map(
        [("a".to_owned(), GenericValue::BigInt(1))].into(),
    )]));
    schema.fields.push(field);
    refused(&dir, || SaveRequest {
        config: project.config.clone(),
        ops: vec![FileOp {
            schema: Some(schema.clone()),
            raw: None,
            ..op(OpKind::Upsert, "orders", "orders.schema.yml", None, "")
        }],
        skip: Vec::new(),
        identity: None,
    });
    std::fs::remove_dir_all(&dir).ok();
}
