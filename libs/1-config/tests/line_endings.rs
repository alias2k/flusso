//! Line endings never move an index (issue #488).
//!
//! A lock or a schema whose line endings were converted (git's `core.autocrlf`
//! on Windows, an editor) holds the same config as its LF twin: the same
//! values, so the same content hash and the same physical index name. An LF
//! lock reads exactly as it always did.

#![allow(unused_crate_dependencies, clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

const CONFIG: &str = "\
[source]
type = \"postgres\"
connection_url = \"postgres://app@localhost/mydb\"

[sinks.primary]
type = \"opensearch\"
url = \"https://localhost:9200\"

[[index]]
name = \"products\"
schema = \"products.schema.yml\"
enabled = true
";

/// Every value of a schema that the lock writes as a multi-line string: a
/// script's source, a raw filter, a filter's value, a column's `default`, a
/// mapping option, a constant. (An enum variant, an identifier or an
/// expression holds no line break.)
const MULTI_LINE_SCHEMA: &str = r#"version: 1
table: products
primary_key: id
filters:
  - raw: |
      price > 0
      AND currency = 'USD'
  - { column: note, op: eq, value: "note one\nnote two" }
fields:
  - integer: id
    required: true
  - keyword: name
    required: false
    default: "default one\ndefault two"
    options:
      null_value: "option one\noption two"
  - constant: banner
    value: "constant one\nconstant two"
  - keyword: label
    required: false
    script: |
      let name = this.name;
      name
"#;

/// The first line of each multi-line value in [`MULTI_LINE_SCHEMA`].
const FIRST_LINES: [&str; 6] = [
    "price > 0",
    "note one",
    "default one",
    "option one",
    "constant one",
    "let name = this.name;",
];

fn project(name: &str, schema: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("line_endings")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("flusso.toml"), CONFIG).unwrap();
    std::fs::write(dir.join("products.schema.yml"), schema).unwrap();
    dir
}

fn lf_lock(name: &str, schema: &str) -> String {
    let compiled = config::compile(project(name, schema).join("flusso.toml")).unwrap();
    String::from_utf8(config::to_bytes(&compiled).unwrap()).unwrap()
}

fn hashes(config: &config::Config) -> Vec<(String, String)> {
    config
        .resolve_mappings()
        .into_iter()
        .map(|mapping| (mapping.index.to_string(), mapping.hash.to_string()))
        .collect()
}

fn script_sources(config: &config::Config) -> Vec<String> {
    fn walk(fields: &[config::Field], out: &mut Vec<String>) {
        for field in fields {
            match &field.source {
                config::FieldSource::Script(script) => out.push(script.source.clone()),
                config::FieldSource::Group(fields) => walk(fields, out),
                config::FieldSource::Relation(config::Relation::Join(join)) => {
                    walk(&join.fields, out);
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for index in config.indexes.values() {
        walk(&index.schema.fields, &mut out);
    }
    out
}

/// The text `config` writes as a lock.
fn relocked(config: config::Config) -> String {
    let compiled = config::Compiled {
        format_version: config::FORMAT_VERSION,
        config,
    };
    String::from_utf8(config::to_bytes(&compiled).unwrap()).unwrap()
}

/// A lock converted to CRLF holds the config its LF twin holds: every
/// multi-line value reads without `\r`, so `run --locked` names the index as
/// a `run` on Linux does.
#[test]
fn a_crlf_lock_holds_the_config_of_its_lf_twin() {
    let lf = lf_lock("crlf-lock", MULTI_LINE_SCHEMA);
    for first in FIRST_LINES {
        assert!(
            lf.contains(&format!("\"\"\"\n{first}\n")),
            "`{first}` is not written as a multi-line string:\n{lf}"
        );
    }
    let crlf = lf.replace('\n', "\r\n");

    let from_lf = config::from_bytes(lf.as_bytes()).unwrap();
    let from_crlf = config::from_bytes(crlf.as_bytes()).unwrap();
    assert_eq!(hashes(&from_crlf), hashes(&from_lf));
    assert_eq!(
        script_sources(&from_crlf),
        vec!["let name = this.name;\nname\n".to_owned()]
    );
    assert_eq!(relocked(from_crlf), lf);
}

/// A carriage return a value really holds (a YAML `\r` escape) is written as
/// the escape `\r`, never as the byte, and stays through a CRLF conversion of
/// the lock: only a converted line break is read back as `\n`. A bare `\r`
/// byte in a lock is not a line ending and is left as written, so the TOML
/// parser refuses it as it always did.
#[test]
fn a_carriage_return_that_is_not_a_converted_line_ending_stays() {
    let schema = "version: 1\ntable: products\nprimary_key: id\nfields:\n  - integer: id\n    \
                  required: true\n  - keyword: label\n    required: false\n    \
                  script: \"let a = \\\"x\\ry\\\";\\nlet b = \\\"p\\r\\nq\\\";\\na + b\\n\"\n";
    let lf = lf_lock("lone-cr", schema);
    assert!(
        !lf.contains('\r'),
        "a lock holds no carriage return byte:\n{lf}"
    );
    let source = "let a = \"x\ry\";\nlet b = \"p\r\nq\";\na + b\n".to_owned();
    for text in [lf.clone(), lf.replace('\n', "\r\n")] {
        let config = config::from_bytes(text.as_bytes()).unwrap();
        assert_eq!(script_sources(&config), vec![source.clone()]);
    }

    let bare = lf.replacen("let a", "let\ra", 1);
    assert!(
        matches!(
            config::from_bytes(bare.as_bytes()),
            Err(config::CompileError::Decode { .. })
        ),
        "a bare carriage return is not a line ending"
    );
}

/// An LF lock reads exactly as before: the golden lock and the repository's
/// own lock hold no carriage return, and the golden lock names its indexes as
/// they were named before #488.
#[test]
fn an_lf_lock_keeps_every_index_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let golden = root.join("tests/fixtures/golden");
    for lock in [golden.join("flusso.lock"), root.join("../../flusso.lock")] {
        let bytes = std::fs::read(&lock).unwrap();
        assert!(!bytes.contains(&b'\r'), "{}", lock.display());
    }
    let from_lock = config::load_compiled(golden.join("flusso.lock")).unwrap();
    let from_config = config::load(golden.join("flusso.toml")).unwrap();
    assert_eq!(hashes(&from_lock), hashes(&from_config));
    assert_eq!(
        hashes(&from_lock),
        [("catalog".to_owned(), "bf4e0069".to_owned())]
    );
}

/// Every spelling of a multi-line string a schema can hold.
const SCHEMA_FORMS: &str = r#"version: 1
table: products
primary_key: id
filters:
  - raw: |
      price > 0
      AND currency = 'USD'
fields:
  - integer: id
    required: true
  - keyword: literal
    required: false
    script: |
      let a = 1;
      a
  - keyword: folded
    required: false
    script: >
      let a = 1;

      a
  - keyword: double
    required: false
    script: "let a = 1;
      a"
  - keyword: single
    required: false
    script: 'let a = 1;
      a'
  - keyword: plain
    required: false
    script: let a = 1;
      a
  - { keyword: flow, required: false, script: "let a = 1;
      a" }
"#;

/// A schema with CRLF (or bare CR) line endings hashes as its LF twin: YAML
/// reads every line break as `\n` in a block scalar and folds it in a
/// quoted, plain or flow scalar, so no `\r` reaches a value (the answer to
/// option (b) of #488: the schema is no second door).
#[test]
fn a_schema_with_crlf_line_endings_hashes_as_its_lf_twin() {
    let lf = config::load(project("schema-lf", SCHEMA_FORMS).join("flusso.toml")).unwrap();
    let sources = script_sources(&lf);
    assert_eq!(
        sources,
        [
            "let a = 1;\na\n",
            "let a = 1;\na\n",
            "let a = 1; a",
            "let a = 1; a",
            "let a = 1; a",
            "let a = 1; a",
        ]
    );
    for (name, ending) in [("schema-crlf", "\r\n"), ("schema-cr", "\r")] {
        let converted = SCHEMA_FORMS.replace('\n', ending);
        let config = config::load(project(name, &converted).join("flusso.toml")).unwrap();
        assert_eq!(hashes(&config), hashes(&lf), "{name}");
        assert_eq!(script_sources(&config), sources, "{name}");
    }
}
