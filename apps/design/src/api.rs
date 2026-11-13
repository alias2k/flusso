//! The designer's JSON API: load the project, introspect the database, preview
//! a schema, validate against the live store, build a sample document from a
//! live row, and write the files back.
//!
//! Every operation works in the validated vocabulary — requests carry
//! [`IndexSchema`]/[`ConfigToml`] as JSON (so invalid identifiers are rejected
//! at deserialization), and the catalog/diagnostics types are the source
//! layer's own. The handlers in [`crate::server`] are thin wrappers over these
//! functions; the logic lives here so it stays testable without a server.

use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use config::Config;
use config::toml::ConfigToml;
use config::yaml::SchemaYaml;
use kernel::AdapterConfig;
use kernel::common::{IndexName, SchemaPath};
use kernel::{IndexSchema, ParseFrom};
use script::{Printer, ScriptContext, Scripts};
use serde::{Deserialize, Serialize};
use source::document::BuildFailure;
use source::{
    Diagnostic, JunctionCandidate, RelationalCatalog, SchemaIntrospection, Severity, SourceSpec,
    junction_candidates, validate_indexes,
};
use source_postgres::{
    PgDocumentBuilder, PostgresConfig, WalChangeCapture, replication_config, sql_connection_url,
};

use crate::commit::journal::Digest;
use crate::commit::{
    self, Changed, Direction, Disk, Expect, Fix, HandStep, Holds, LockError, Outcome, ProblemKind,
    ProjectLock, Remove, Report, State, Unusable, Write, commit, recover, strays,
};
use crate::fspath::{MAX_PATH, on_disk};
use crate::keep::{self, ConfigIdentity, Notice, Saved, SchemaIdentity};
use crate::preview::{self, Preview};

/// The whole project as the editor opens it: the parsed `flusso.toml` plus every
/// referenced index schema.
#[derive(Debug, Serialize)]
pub struct Project {
    /// Absolute path to the `flusso.toml` the designer edits.
    pub config_path: String,
    /// The editable deployment config (1:1 with the TOML file).
    pub config: ConfigToml,
    /// Every index the config references, with its loaded schema.
    pub indexes: Vec<IndexFile>,
    /// What the designer found of an earlier, interrupted save.
    #[serde(skip_serializing_if = "SaveRecovery::is_empty")]
    pub recovery: SaveRecovery,
    /// The digest of each file the page's model came from, as it was read:
    /// a diff or save sends them back ([`SaveRequest::digests`]).
    pub digests: Digests,
}

/// Per file, by its path relative to the config's folder, the SHA-256 of
/// its bytes in hex; `None` for a file that did not exist.
pub type Digests = BTreeMap<String, Option<String>>;

/// One index entry plus its loaded `*.schema.yml`.
#[derive(Debug, Serialize)]
pub struct IndexFile {
    /// Logical index name.
    pub name: String,
    /// Whether the index is enabled.
    pub enabled: bool,
    /// Schema file path, as written in the config (relative to it).
    pub schema_path: String,
    /// The parsed schema, or `None` when the file is missing/invalid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<IndexSchema>,
    /// The raw on-disk file text — the escape hatch when the visual editor can't
    /// represent something (and the source for the diff-before-save).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// Why the schema failed to load, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Load the project rooted at `config_path`.
///
/// Each file is read once: its digest is of the bytes its part of the model
/// was parsed from.
pub fn load_project(config_path: &Path) -> Result<Project> {
    let bytes =
        std::fs::read(config_path).with_context(|| format!("reading {}", config_path.display()))?;
    let paths = Paths::new(config_path)?;
    let mut digests = Digests::new();
    digests.insert(
        paths.relative_path(&paths.config),
        Some(Digest::of(&bytes).hex().to_owned()),
    );
    let raw = String::from_utf8(bytes)
        .with_context(|| format!("reading {}: not UTF-8", config_path.display()))?;
    let config = ConfigToml::try_parse(&raw)
        .with_context(|| format!("parsing {}", config_path.display()))?;

    let base_dir = config_path.parent().unwrap_or(Path::new("."));
    let indexes = config
        .index
        .iter()
        .map(|entry| {
            let schema_path = entry.schema.as_ref();
            let resolved = resolve(base_dir, schema_path);
            let bytes = std::fs::read(&resolved);
            if let Ok(at) = paths.at(schema_path) {
                match &bytes {
                    Ok(bytes) => {
                        digests.insert(
                            paths.relative_path(&at),
                            Some(Digest::of(bytes).hex().to_owned()),
                        );
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        digests.insert(paths.relative_path(&at), None);
                    }
                    Err(_) => {}
                }
            }
            let raw = bytes.ok().and_then(|b| String::from_utf8(b).ok());
            let (schema, error) = match raw.as_deref() {
                Some(text) => match parse_schema_text(text) {
                    Ok(s) => (Some(s), None),
                    Err(e) => (None, Some(format!("{e:#}"))),
                },
                None => (None, Some(format!("could not read {}", resolved.display()))),
            };
            IndexFile {
                name: entry.name.to_string(),
                enabled: entry.enabled,
                schema_path: schema_path.display().to_string(),
                schema,
                raw,
                error,
            }
        })
        .collect();

    Ok(Project {
        config_path: config_path.display().to_string(),
        config,
        indexes,
        recovery: SaveRecovery::default(),
        digests,
    })
}

fn parse_schema_text(text: &str) -> Result<IndexSchema> {
    let entity = SchemaYaml::try_parse(text).context("parsing schema")?;
    IndexSchema::try_from(entity).context("validating schema")
}

/// A raw `*.schema.yml` buffer to parse, sent by the editor's Code mode.
#[derive(Debug, Deserialize)]
pub struct ParseRequest {
    /// The YAML text as typed.
    pub yaml: String,
}

/// An aggregate input to check as the user types it: a column name or an
/// arithmetic expression.
#[derive(Debug, Deserialize)]
pub struct ExpressionRequest {
    /// The input as typed.
    pub input: String,
}

/// A checked aggregate input: the columns it reads, or what is wrong with it
/// and where.
#[derive(Debug, Serialize)]
pub struct ExpressionResponse {
    /// The columns the input reads, once each, in order of appearance.
    pub columns: Vec<String>,
    /// What makes the input malformed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ExpressionProblem>,
}

/// A malformed aggregate input, structured so the editor can translate it.
#[derive(Debug, Serialize)]
pub struct ExpressionProblem {
    /// A stable token naming the mistake (`expected_operand`, `too_deep`, …).
    pub code: &'static str,
    /// The 1-based character the parser stopped at.
    pub position: usize,
    /// The offending character or column name, when the mistake has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The parser's own English message.
    pub message: String,
}

/// Parse an aggregate input with the loader's own parser.
pub fn check_expression(request: &ExpressionRequest) -> ExpressionResponse {
    use kernel::{Expression, ExpressionErrorKind as Kind};

    match Expression::parse(&request.input) {
        Ok(expression) => ExpressionResponse {
            columns: expression.columns().iter().map(|c| c.to_string()).collect(),
            error: None,
        },
        Err(err) => {
            let (code, detail) = match &err.kind {
                Kind::Empty => ("empty", None),
                Kind::UnexpectedCharacter(c) => ("unexpected_character", Some(c.to_string())),
                Kind::IncompleteNumber => ("incomplete_number", None),
                Kind::InvalidColumn(name) => ("invalid_column", Some(name.clone())),
                Kind::ExpectedOperand(_) => ("expected_operand", None),
                Kind::ExpectedOperator(_) => ("expected_operator", None),
                Kind::UnclosedParenthesis => ("unclosed_parenthesis", None),
                Kind::UnmatchedParenthesis => ("unmatched_parenthesis", None),
                Kind::TooDeep => ("too_deep", Some(kernel::MAX_DEPTH.to_string())),
            };
            ExpressionResponse {
                columns: Vec::new(),
                error: Some(ExpressionProblem {
                    code,
                    position: err.position,
                    detail,
                    message: err.kind.to_string(),
                }),
            }
        }
    }
}

/// A value typed into the inspector (`constant`, `default`, an option), as JSON.
#[derive(Debug, Deserialize)]
pub struct ValueRequest {
    /// The text as typed.
    pub text: String,
    /// Where the value goes; a `constant` when omitted.
    #[serde(default)]
    pub slot: crate::value::ValueSlot,
}

/// The value the saved file reads back as, or why the text can't be one.
#[derive(Debug, Serialize)]
pub struct ValueResponse {
    /// The value, as a `GenericValue`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<kernel::GenericValue>,
    /// The value as the saved file spells it, when a number in it is written
    /// other than typed (`1e3` → `1000.0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_as: Option<String>,
    /// Why the text was refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<crate::value::ValueProblem>,
}

/// Read a typed value the way its saved file will (see [`crate::value`]).
pub fn read_value(request: &ValueRequest) -> ValueResponse {
    match crate::value::read(&request.text, request.slot) {
        Ok(typed) => ValueResponse {
            value: Some(typed.value),
            saved_as: typed.saved_as,
            error: None,
        },
        Err(problem) => ValueResponse {
            value: None,
            saved_as: None,
            error: Some(problem),
        },
    }
}

/// The 1-based position a parse error points at, when the parser's location is
/// trustworthy (field-scoped errors deliberately carry none — see the parser).
#[derive(Debug, Serialize)]
pub struct ParseErrorLocation {
    pub line: usize,
    pub column: usize,
}

/// The parsed schema (the Code editor applies it to its in-memory document),
/// or a structured error: the clean message (no baked-in snippet — the editor
/// draws its own context), plus either a source `location` or the
/// `field`/`type_tag` the error names, so the editor can anchor its squiggle
/// without parsing the prose.
#[derive(Debug, Serialize)]
pub struct ParseResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<IndexSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<ParseErrorLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_tag: Option<String>,
}

impl ParseResponse {
    fn ok(schema: IndexSchema) -> Self {
        Self {
            schema: Some(schema),
            error: None,
            location: None,
            field: None,
            type_tag: None,
        }
    }
}

/// Parse a schema buffer into the validated model — the Code editor's live
/// YAML → document sync.
pub fn parse_index(request: &ParseRequest) -> ParseResponse {
    use config::yaml::ParseError;

    let entity = match SchemaYaml::try_parse(&request.yaml) {
        Ok(entity) => entity,
        Err(err) => {
            // Field-scoped errors ship the detail alone — the `field`/`type_tag`
            // carry the prefix's information structurally, and the editor's rail
            // labels the row with the field itself.
            let (type_tag, field, detail) = match err.field_scope() {
                Some((tag, name, detail)) => (
                    Some(tag.to_string()),
                    Some(name.to_string()),
                    Some(detail.to_string()),
                ),
                None => (None, None, None),
            };
            let (error, location) = match (&detail, &err) {
                (Some(detail), _) => (detail.clone(), None),
                (
                    None,
                    ParseError::Syntax {
                        message, location, ..
                    },
                ) => (
                    message.clone(),
                    location.map(|at| ParseErrorLocation {
                        line: at.line,
                        column: at.column,
                    }),
                ),
                (None, other) => (other.to_string(), None),
            };
            return ParseResponse {
                schema: None,
                error: Some(error),
                location,
                field,
                type_tag,
            };
        }
    };
    match IndexSchema::try_from(entity) {
        Ok(schema) => ParseResponse::ok(schema),
        Err(err) => ParseResponse {
            schema: None,
            error: Some(err.to_string()),
            location: None,
            field: None,
            type_tag: None,
        },
    }
}

/// A schema to preview, sent by the editor.
#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    /// The index name to resolve the mapping under.
    pub index: IndexName,
    /// The schema being authored.
    pub schema: IndexSchema,
    /// The index's schema file, relative to the config: `yaml` is the schema
    /// written into it, keeping its comments.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// The text to write the schema into instead of the file (see
    /// [`FileOp::base`]).
    #[serde(default)]
    pub base: Option<String>,
    /// Which field of the base each field of `schema` is (see
    /// [`FileOp::identity`]).
    #[serde(default)]
    pub identity: Option<SchemaIdentity>,
}

/// The previewed document/mapping plus the exact YAML that would be written and
/// whether it round-trips through the parser (a self-check on codegen).
#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    /// The `*.schema.yml` text a save would write: the schema written into its
    /// file (or `base`), else the canonical form.
    pub yaml: String,
    /// Derived mapping + document tree.
    pub preview: Preview,
    /// Whether the generated YAML parses back to exactly this schema.
    pub parse_ok: bool,
    /// When the YAML parses but reads back as a different schema: the first
    /// field (dotted path, or a root key) that differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub differs_at: Option<String>,
    /// The parse error, when `parse_ok` is false (should not happen for a valid
    /// schema — surfaced rather than hidden if it does).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    /// Every script field that does not compile, with where.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub script_errors: Vec<ScriptErrorDto>,
}

/// A script failure — a compile error, or one raised while running over a
/// sample — flattened for JSON.
#[derive(Debug, Clone, Serialize)]
pub struct ScriptErrorDto {
    /// The failing field's path, e.g. `variants[2].grossPrice`.
    pub field: String,
    /// What went wrong, in one line.
    pub message: String,
    /// 1-based line in the script, when the failure has a position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// 1-based column in that line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    /// The script line the position points into.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

impl From<BuildFailure> for ScriptErrorDto {
    fn from(failure: BuildFailure) -> Self {
        let (line, column, excerpt) = match failure.position {
            Some(at) => (Some(at.line), Some(at.column), Some(at.excerpt)),
            None => (None, None, None),
        };
        Self {
            field: failure.field,
            message: failure.message,
            line,
            column,
            excerpt,
        }
    }
}

/// One index's scripts compiled on their own, for the designer.
fn compile_index(
    index: &IndexName,
    schema: &IndexSchema,
    printer: Option<Printer>,
) -> std::result::Result<Scripts, Vec<ScriptErrorDto>> {
    let spec = SourceSpec::new(BTreeMap::from([(index.clone(), schema.clone())]));
    Scripts::compile_with_printer(&spec, printer).map_err(|errors| {
        errors
            .0
            .into_iter()
            .map(|error| ScriptErrorDto::from(error.failure))
            .collect()
    })
}

/// Build a preview for `request`, reading its schema file next to `config_path`.
pub fn build_preview(config_path: &Path, request: PreviewRequest) -> Result<PreviewResponse> {
    let on_disk = match (&request.base, &request.path) {
        (None, Some(path)) => {
            let base_dir = config_path.parent().unwrap_or(Path::new("."));
            std::fs::read_to_string(resolve(base_dir, path)).ok()
        }
        _ => None,
    };
    let base = request.base.as_deref().or(on_disk.as_deref());
    let yaml = keep::schema_yaml(base, &request.schema, request.identity.as_ref())?.text;
    let preview = preview::preview(&request.schema, &request.index);
    let (parse_error, differs_at) = match SchemaYaml::try_parse(&yaml)
        .map_err(|e| e.to_string())
        .and_then(|entity| IndexSchema::try_from(entity).map_err(|e| e.to_string()))
    {
        Ok(reparsed) => (None, first_difference(&request.schema, &reparsed)),
        Err(e) => (Some(e), None),
    };
    let parse_ok = parse_error.is_none() && differs_at.is_none();
    let script_errors = compile_index(&request.index, &request.schema, None)
        .err()
        .unwrap_or_default();
    Ok(PreviewResponse {
        yaml,
        preview,
        parse_ok,
        differs_at,
        parse_error,
        script_errors,
    })
}

/// The first place `ours` and `theirs` differ, as a dotted path of field names
/// (or the root key), or `None` when they are the same schema.
fn first_difference(ours: &IndexSchema, theirs: &IndexSchema) -> Option<String> {
    let (ours, theirs) = (
        serde_json::to_value(ours).ok()?,
        serde_json::to_value(theirs).ok()?,
    );
    if ours == theirs {
        return None;
    }
    let mut path = Vec::new();
    descend(&ours, &theirs, &mut path);
    Some(if path.is_empty() {
        String::from("schema")
    } else {
        path.join(".")
    })
}

/// Walk into the first differing child, recording each named field passed.
fn descend(ours: &serde_json::Value, theirs: &serde_json::Value, path: &mut Vec<String>) {
    use serde_json::Value;
    match (ours, theirs) {
        (Value::Object(a), Value::Object(b)) => {
            if let Some(Value::String(name)) = a.get("field") {
                path.push(name.clone());
            }
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            let Some(key) = keys.into_iter().find(|k| a.get(*k) != b.get(*k)) else {
                return;
            };
            match (a.get(key), b.get(key)) {
                (Some(x), Some(y)) => descend(x, y, path),
                _ if path.is_empty() => path.push(key.clone()),
                _ => {}
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if let Some((x, y)) = a.iter().zip(b).find(|(x, y)| x != y) {
                descend(x, y, path);
            }
        }
        _ => {}
    }
}

/// What to do with one schema file. The client computes these from its saved
/// snapshot → current document (correlated by a stable index id); the server
/// holds no session state, checks them against the config being saved, and
/// applies them. There is no delete: a removed index's file stays on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// Create the file at `path`, or overwrite it if it exists.
    Upsert,
    /// Move `from` → `path` (its content may also change in the same step).
    Move,
}

/// One schema-file operation in a save/diff request. `schema` is the content,
/// written into `base`; `from` is the source path for `Move`. Unknown fields
/// are refused, as on [`SaveRequest`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileOp {
    /// What to do.
    pub kind: OpKind,
    /// The index whose schema file this is. `path` must be that index's
    /// schema path in the config being saved.
    pub index: IndexName,
    /// Destination, relative to the config dir.
    pub path: PathBuf,
    /// Source path for a `Move`, relative to the config dir: the index's
    /// schema file in the config on disk.
    #[serde(default)]
    pub from: Option<PathBuf>,
    /// The schema to render.
    #[serde(default)]
    pub schema: Option<IndexSchema>,
    /// The text to write `schema` into, keeping its comments: the Code-mode
    /// buffer the schema was last read from. Absent, the file on disk is used.
    #[serde(default)]
    pub base: Option<String>,
    /// Which field of the base each field of `schema` is, so a comment
    /// follows its field through a rename or a move. Absent, or not matching
    /// the base, fields pair by name.
    #[serde(default)]
    pub identity: Option<SchemaIdentity>,
}

/// A full save/diff request: the deployment config plus the schema-file ops.
/// Unknown fields are refused, so a page from an older designer is told to
/// reload instead of being served differently.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    /// The deployment config to write to `flusso.toml`.
    pub config: ConfigToml,
    /// The schema-file operations to apply.
    #[serde(default)]
    pub ops: Vec<FileOp>,
    /// Paths to skip on save — the review's unchecked entries, resolved like
    /// every other path. Empty by default (apply everything).
    #[serde(default)]
    pub skip: Vec<String>,
    /// Which `[[index]]` entry of the file on disk each entry of `config` is.
    /// Absent, or not matching the file, entries pair by name.
    #[serde(default)]
    pub identity: Option<ConfigIdentity>,
    /// The digests of the files as the page read them ([`Project::digests`],
    /// then what its own saves wrote). A save refuses to write, move or
    /// remove a file that no longer has the digest given here (a file not
    /// listed must not exist), and refuses anything while `flusso.toml` does
    /// not have its digest: every check of a save reads it. Required.
    pub digests: Digests,
}

/// A diff or save request the server does not read: answered with a 400.
#[derive(Debug)]
pub struct BadRequest {
    /// What is wrong, naming the field.
    pub message: String,
    /// The page is from another version of the designer: reloading it fixes
    /// this.
    pub reload: bool,
}

impl std::fmt::Display for BadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BadRequest {}

/// Read a diff or save request. One without `digests` (or with `null`), or
/// with a field this designer does not know, comes from a page served by
/// another version of the designer: refused, telling it to reload, so it
/// never saves without the check against files changed on disk.
pub fn read_save_request(body: &[u8]) -> std::result::Result<SaveRequest, BadRequest> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|e| BadRequest {
        message: format!("the request is not JSON: {e}"),
        reload: false,
    })?;
    if value.get("digests").is_none_or(serde_json::Value::is_null) {
        return Err(BadRequest {
            message: "the request has no `digests`: this page is from another version of the \
                      designer; reload this page (unsaved changes are lost, so copy them out first)"
                .to_owned(),
            reload: true,
        });
    }
    serde_json::from_value(value).map_err(|e| {
        let message = e.to_string();
        if message.starts_with("unknown field") {
            BadRequest {
                message: format!(
                    "{message}: this page is from another version of the designer; reload this \
                     page (unsaved changes are lost, so copy them out first)"
                ),
                reload: true,
            }
        } else {
            BadRequest {
                message,
                reload: false,
            }
        }
    })
}

/// What a diff found: the review, and the interrupted saves it rolled back
/// first (the files are as the page read them, so the diff goes on).
#[derive(Debug, Default, Serialize)]
pub struct Review {
    /// The review's entries, the config first.
    pub diffs: Vec<OpDiff>,
    /// Interrupted saves rolled back before the diff.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recovered: Vec<RecoveredSave>,
}

/// A file that was moved on save.
#[derive(Debug, Serialize)]
pub struct MovedFile {
    /// Absolute source path.
    pub from: String,
    /// Absolute destination path.
    pub to: String,
}

/// What a save did on disk.
#[derive(Debug, Default, Serialize)]
pub struct SaveResponse {
    /// Absolute paths written (config + upserts), in apply order.
    pub written: Vec<String>,
    /// Files moved (rename/relocate).
    pub moved: Vec<MovedFile>,
    /// Absolute paths of directories pruned because they became empty.
    pub pruned: Vec<String>,
    /// Interrupted saves rolled back before this one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recovered: Vec<RecoveredSave>,
    /// The digest of every file the save wrote, and `None` for each it
    /// removed: the page's digests from now on.
    pub digests: Digests,
    /// Files another program wrote in the instant the save replaced them:
    /// the file holds what the save wrote, the other bytes are in a copy.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub displaced: Vec<DisplacedFile>,
}

/// A file another program wrote, in place, in the instant a save replaced
/// it: it holds the save's text, and the program's bytes went to the save's
/// backup of it, which is kept for the user to check and delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisplacedFile {
    /// The file, relative to the config's folder.
    pub path: String,
    /// The backup holding the other bytes, relative to the config's folder.
    pub copy: String,
}

/// A file that is not as the page read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleFile {
    /// Its path, relative to the config's folder.
    pub path: String,
    /// What happened to it.
    pub change: StaleChange,
    /// The digest of the file on disk now; `None` when there is none. Sent
    /// back as the file's digest, it lets a save overwrite the file anyway.
    pub on_disk: Option<String>,
}

/// How a file differs from what the page read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleChange {
    /// It holds other bytes.
    Changed,
    /// It was deleted.
    Deleted,
    /// It did not exist, and now does.
    Created,
    /// The page never read it, and it exists.
    Unloaded,
}

/// A save refused because files it writes, moves or removes, or
/// `flusso.toml`, changed on disk since the page read them: nothing was
/// written.
#[derive(Debug)]
pub struct Stale {
    /// The files, the config first when it is one of them.
    pub files: Vec<StaleFile>,
    /// The config's path, as `files` names it.
    pub config: String,
}

impl std::fmt::Display for Stale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = self
            .files
            .iter()
            .map(|file| {
                let path = &file.path;
                match file.change {
                    StaleChange::Changed => {
                        format!("{path} changed on disk since this page loaded it")
                    }
                    StaleChange::Deleted => format!("{path} was deleted since this page loaded it"),
                    StaleChange::Created => {
                        format!("{path} was created since this page loaded the project")
                    }
                    StaleChange::Unloaded => {
                        format!("{path} exists on disk, and this page never loaded it")
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        let config = self.files.iter().any(|file| file.path == self.config);
        let others = self.files.iter().any(|file| file.path != self.config);
        let why = match (config, others) {
            (true, false) => format!(
                "Every save is checked against {}, so none goes through while it differs from \
                 what the page shows.",
                self.config
            ),
            (true, true) => format!(
                "Saving would overwrite what is on disk now, and every save is checked against {}.",
                self.config
            ),
            _ => "Saving would overwrite what is on disk now.".to_owned(),
        };
        write!(
            f,
            "Nothing was saved: {what}. {why} Reload the page to load what is on disk (unsaved \
             changes are lost), or copy your edits out first."
        )
    }
}

impl std::error::Error for Stale {}

/// What the designer found of saves a process left half done (see the
/// `commit` module): it settles them before it reads the project.
#[derive(Debug, Default, Clone, Serialize)]
pub struct SaveRecovery {
    /// Interrupted saves undone or finished, so the project is again exactly
    /// as it was before each, or after it.
    pub recovered: Vec<RecoveredSave>,
    /// An interrupted save that could not be settled: nothing was changed,
    /// and the project should not be edited until the files are sorted out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<BlockedSave>,
    /// Temp files and backups no journal names (left by a designer from
    /// before journals, or found holding other text than the save's):
    /// reported, never touched.
    pub leftovers: Vec<String>,
}

impl SaveRecovery {
    /// Whether there is nothing to tell.
    pub fn is_empty(&self) -> bool {
        self.recovered.is_empty() && self.blocked.is_none() && self.leftovers.is_empty()
    }

    /// Whether a diff or save must stop: a save could not be settled.
    fn stops(&self) -> bool {
        self.blocked.is_some()
    }

    fn busy() -> Self {
        Self {
            blocked: Some(BlockedSave {
                busy: true,
                problems: Vec::new(),
                leftovers: Vec::new(),
                report: None,
                direction: None,
                keep: None,
            }),
            ..Self::default()
        }
    }
}

/// An interrupted save the designer settled.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveredSave {
    /// Which save it was: a page shows each once.
    pub id: String,
    /// Undone, or finished.
    pub outcome: RecoveryOutcome,
    /// When the save started, in milliseconds since the Unix epoch.
    pub started_at_ms: u64,
    /// The files the save changed, relative to the config's folder.
    pub files: Vec<String>,
    /// Files another program wrote during or after the save, kept as they
    /// are instead of rolled back or finished.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub kept: Vec<KeptFile>,
    /// Files whose backup holds what another program wrote to them as the
    /// save replaced them ([`DisplacedFile`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub displaced: Vec<DisplacedFile>,
}

/// A file a recovered save left as another program wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeptFile {
    /// The file, relative to the config's folder.
    pub path: String,
    /// The save's backup of the file's text from before it, kept for the
    /// user to delete or copy back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
}

/// How an interrupted save was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    /// Its files had not all been written: every file is back as before it.
    RolledBack,
    /// Every file was written: only its backups were left, now deleted.
    Completed,
}

/// An interrupted save the designer could not settle.
#[derive(Debug, Clone, Serialize)]
pub struct BlockedSave {
    /// Another designer holds the project's lock: it is saving right now.
    pub busy: bool,
    /// The files that stop recovery, or the journal that can't be used.
    pub problems: Vec<BlockedFile>,
    /// The journal, temp files and backups on disk, relative to the config's
    /// folder. None is ever deleted while recovery is blocked.
    pub leftovers: Vec<String>,
    /// For a journal the designer does not act on: what each file it names
    /// holds now. Nothing in it was changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<JournalReport>,
    /// For a save of this designer's: which way recovery was taking it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<RecoveryDirection>,
    /// For a save of this designer's whose every problem is a file that
    /// changed: the steps by hand that keep those files as they are and
    /// settle the rest, the journal last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<Vec<Step>>,
}

/// Which way recovery was taking a save of this designer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDirection {
    /// Not every file was in place: back to before the save.
    Back,
    /// Every file was in place: only cleanup was left.
    Forward,
}

/// One step by hand on the blocked page, paths relative to the config's
/// folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Step {
    /// Copy `from` to `to`, replacing what is there.
    Copy {
        /// The file to copy.
        from: String,
        /// Where it goes.
        to: String,
    },
    /// Delete the file.
    Delete {
        /// The file.
        path: String,
    },
    /// Remove the folder, when it is empty.
    RemoveDir {
        /// The folder.
        path: String,
    },
}

/// A journal the designer does not act on, what the files it names hold
/// now (checked read-only, inside the config's folder only), and how to
/// settle it by hand.
#[derive(Debug, Clone, Serialize)]
pub struct JournalReport {
    /// Why it is not acted on.
    pub cause: JournalCause,
    /// The journal files found, relative to the config's folder: each is to
    /// be deleted.
    pub journals: Vec<String>,
    /// For a journal this designer wrote for another folder: that folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_for: Option<String>,
    /// The files it names, the first fifty.
    pub files: Vec<ReportedFile>,
    /// How many more it names.
    #[serde(skip_serializing_if = "is_zero")]
    pub more: usize,
    /// The steps by hand that take the project back to before the save,
    /// when every file inside the folder can be taken there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_route: Option<Vec<Step>>,
    /// The steps by hand that take the project forward to after the save,
    /// when every file inside the folder can be taken there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_route: Option<Vec<Step>>,
    /// The save's temp files and backups on disk, whatever they hold: the
    /// routes delete them.
    pub leftovers: Vec<String>,
}

/// Why a journal is not acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalCause {
    /// This designer has no record of it: another user or machine wrote it,
    /// it came with the project, the project was copied or moved, the record
    /// could not be kept (no usable state folder at save time, or pruned),
    /// or its save was settled already.
    NoRecord,
    /// This designer wrote it while saving another folder: the project, or
    /// the journal, was copied or moved.
    OtherFolder,
    /// The designer's state folder can't be used, so it can't be checked.
    NoState,
    /// It is larger than any save writes.
    TooLarge,
    /// It can't be read: a link, a folder, no permission.
    Unreadable,
    /// It is recorded but damaged, or of an unknown format.
    Malformed,
    /// It is recorded but names what no save of this folder writes.
    NotASave,
    /// Both `.flusso-save.journal` and `.flusso-save.done` are there.
    Conflict,
}

/// One file a journal the designer does not act on names.
#[derive(Debug, Clone, Serialize)]
pub struct ReportedFile {
    /// The file, relative to the config's folder; as the journal writes it
    /// when it is outside the folder or no path a save writes.
    pub path: String,
    /// It is outside the config's folder: never looked at.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub outside: bool,
    /// What it holds now.
    pub holds: FileHolds,
    /// The save's backup of it, holding its text from before the save,
    /// relative to the config's folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// The save's temp file for it, holding the text the save wrote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<String>,
    /// What to do to it to get back to before the save.
    pub back: FileFix,
    /// What to do to it to get forward to after the save.
    pub forward: FileFix,
}

/// What a file a journal names holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileHolds {
    /// Its text from before the save (a file the save creates: absent).
    Before,
    /// The text the save wrote (a file the save removes: absent).
    After,
    /// Something else.
    Neither,
    /// Not looked at: no path a save writes, a link on the way, not a plain
    /// file, or too large.
    Unchecked,
    /// Outside the config's folder: never looked at.
    Outside,
}

/// One step by hand that brings a file to one side of a save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum FileFix {
    /// It is there already.
    Nothing,
    /// Copy `from` over the file.
    Copy {
        /// Relative to the config's folder.
        from: String,
    },
    /// Delete the file.
    Delete,
    /// Not possible: no file holds the text, or it changed since.
    Unavailable,
}

/// One file that stops recovery.
#[derive(Debug, Clone, Serialize)]
pub struct BlockedFile {
    /// The file, relative to the config's folder.
    pub path: String,
    /// What is wrong with it.
    pub problem: BlockedProblem,
    /// The system's message, or why a journal can't be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The backup the save keeps of the file, by name, whatever it holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// The one step that gives the file a text recovery accepts, going its
    /// direction; absent when no copy of that text is left.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Step>,
    /// A file that holds the file's text from before the save right now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_copy: Option<String>,
}

/// Why a file stops recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedProblem {
    /// The backup holding the text from before the save is gone or damaged.
    BackupMissing,
    /// The file could not be read.
    Unreadable,
    /// The save's journal can't be read, or names what no save of this
    /// project would (a path outside it, a file of the save's own): nothing
    /// in it is acted on.
    JournalUnusable,
    /// A step of the recovery failed.
    Failed,
}

/// A load, diff or save refused because of an interrupted save: one can't
/// be settled, or another designer is saving.
#[derive(Debug)]
pub struct Interrupted(pub SaveRecovery);

impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(blocked) = &self.0.blocked else {
            return f.write_str("an interrupted save stops this request");
        };
        if blocked.busy {
            return f.write_str("another designer is saving this project; try again in a moment");
        }
        write!(
            f,
            "an interrupted save could not be recovered. {}{}",
            match blocked.direction {
                Some(RecoveryDirection::Back) =>
                    "The save was cut short before every file was in place, so the designer \
                     was rolling it back. ",
                Some(RecoveryDirection::Forward) =>
                    "Every file of the save was in place, so the designer was finishing it. ",
                None => "",
            },
            blocked
                .problems
                .iter()
                .map(|p| problem_help(p, blocked.direction))
                .collect::<Vec<_>>()
                .join(" ")
        )?;
        if let Some(keep) = &blocked.keep {
            write!(f, " {}", keep_help(blocked, keep))?;
        }
        if let Some(report) = &blocked.report {
            write!(f, " {}", way_out(report))?;
        }
        if !blocked.leftovers.is_empty() {
            write!(f, " Leftover files: {}.", blocked.leftovers.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for Interrupted {}

/// One step by hand, in words, for the log and the error text.
fn step_text(step: &Step) -> String {
    match step {
        Step::Copy { from, to } => format!("copy {from} to {to}"),
        Step::Delete { path } => format!("delete {path}"),
        Step::RemoveDir { path } => format!("remove the folder {path} if it is empty"),
    }
}

/// What is wrong with one file that stops recovery, and what to do about it,
/// for the log and the error text (the page has its own, translated).
fn problem_help(problem: &BlockedFile, direction: Option<RecoveryDirection>) -> String {
    let path = &problem.path;
    let detail = problem.detail.as_deref().unwrap_or("");
    let back = direction != Some(RecoveryDirection::Forward);
    match problem.problem {
        BlockedProblem::BackupMissing => {
            let what = format!(
                "The backup of {path}{} no longer holds its text from before the save.",
                problem
                    .backup
                    .as_ref()
                    .map(|b| format!(" ({b})"))
                    .unwrap_or_default()
            );
            let fix = match (&problem.fix, back) {
                (Some(step), true) => format!(
                    "To let the designer roll the save back, {} and reload; what {path} holds now \
                     is lost.",
                    step_text(step)
                ),
                (None, true) => format!(
                    "No copy of its text from before the save is left: to let the designer roll \
                     the save back, put that text back in {path} yourself (from version control, \
                     say) and reload."
                ),
                (Some(step), false) => format!(
                    "To let the designer finish the save, {} and reload; what {path} holds now is \
                     lost.",
                    step_text(step)
                ),
                (None, false) => format!(
                    "The text the save wrote to {path} is kept nowhere else, so the designer can't \
                     finish the save with it."
                ),
            };
            let before = match (&problem.before_copy, back) {
                (Some(copy), false) => {
                    format!(" {copy} holds its text from before the save, in case you want it.")
                }
                (None, false) => " No copy of its text from before the save is left.".to_owned(),
                _ => String::new(),
            };
            format!("{what} {fix}{before}")
        }
        BlockedProblem::Unreadable => format!(
            "{path} could not be read ({detail}). If it can be made readable, do so and reload: \
             the designer then carries on where it stopped."
        ),
        BlockedProblem::JournalUnusable => {
            format!("The save's journal {path} is not acted on: {detail}. Nothing was changed.")
        }
        BlockedProblem::Failed => format!(
            "A recovery step failed on {path} ({detail}). Fix the cause and reload: the designer \
             carries on where it stopped."
        ),
    }
}

/// The way to keep the files that changed as they are, in words.
fn keep_help(blocked: &BlockedSave, keep: &[Step]) -> String {
    let files = blocked
        .problems
        .iter()
        .map(|p| p.path.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let (lead, end) = if blocked.direction == Some(RecoveryDirection::Forward) {
        (
            "every other file already holds what the save wrote, so ",
            "as the save wrote it",
        )
    } else {
        ("", "as it was before the save")
    };
    format!(
        "Or, to keep the changed files ({files}) as they are now: {lead}{}, and reload. The \
         project is then {end}, except the changed files ({files}), which keep what they hold now.",
        keep.iter()
            .map(step_text)
            .collect::<Vec<_>>()
            .join(", then ")
    )
}

/// The way out of a journal the designer does not act on, in words, for the
/// log and the error text (the page has its own, translated).
fn way_out(report: &JournalReport) -> String {
    let mut out = Vec::new();
    for file in &report.files {
        let holds = match file.holds {
            FileHolds::Before => "holds its text from before the save",
            FileHolds::After => "holds the text the save wrote",
            FileHolds::Neither => "holds neither",
            FileHolds::Unchecked => "was not checked",
            FileHolds::Outside => "is outside the project, not checked",
        };
        out.push(format!("{} {holds}.", file.path));
    }
    let journals = report.journals.join(" and ");
    let route = |steps: &Option<Vec<Step>>, what: &str| match steps {
        Some(steps) => format!(
            "To get {what}: {}, then reload.",
            steps.iter().map(step_text).collect::<Vec<_>>().join(", ")
        ),
        None => format!("Getting {what} by hand is not possible."),
    };
    if report.back_route.is_some() || report.forward_route.is_some() {
        out.push(route(&report.back_route, "back to before the save"));
        out.push(route(&report.forward_route, "forward to after the save"));
    }
    out.push(format!(
        "To leave every file as it is now, delete only {journals} and reload{}.",
        if report.leftovers.is_empty() {
            String::new()
        } else {
            format!(
                "; the save's other files ({}) stay: check them and delete them yourself",
                report.leftovers.join(", ")
            )
        }
    ));
    out.join(" ")
}

/// Per project folder, the latest saves this process's recovery completed
/// ([`COMPLETED_KEPT`] at most). A page whose digests show it has not seen
/// what one wrote is told on its next diff or save ([`unseen_recoveries`]).
static COMPLETED: std::sync::Mutex<BTreeMap<PathBuf, Vec<RecoveredSave>>> =
    std::sync::Mutex::new(BTreeMap::new());

/// How many completed saves [`COMPLETED`] keeps per project.
const COMPLETED_KEPT: usize = 16;

/// `recovered`, plus each save this process completed earlier that the page
/// has not seen: a file it wrote that the page read (`digests`) differs from
/// the file now.
fn unseen_recoveries(
    paths: &Paths,
    digests: &Digests,
    mut recovered: Vec<RecoveredSave>,
) -> Vec<RecoveredSave> {
    let remembered = COMPLETED
        .lock()
        .ok()
        .and_then(|c| c.get(&paths.base).cloned())
        .unwrap_or_default();
    for save in remembered {
        if recovered.iter().any(|r| r.id == save.id) {
            continue;
        }
        let unseen = save.files.iter().any(|file| {
            let Some(read) = digests.get(file) else {
                return false;
            };
            let now = paths
                .at(Path::new(file))
                .ok()
                .and_then(|path| Digest::at(&path).ok())
                .map(|d| d.map(|d| d.hex().to_owned()));
            now.is_some_and(|now| now != *read)
        });
        if unseen {
            recovered.push(save);
        }
    }
    recovered
}

/// The tokens of this process's own saves whose cleanup was left to the
/// next recovery ([`commit::Committed::cleanup_left`]): finishing one is no
/// news to the page that made it.
static OWN_CLEANUP: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Keep the designer's records of saves in flight in `dir` instead of the
/// user's state folder, for this whole process (the first call wins): for
/// tests and tools that must not touch the user's home.
pub fn use_state_dir(dir: PathBuf) {
    commit::use_state_dir(dir);
}

/// Remove the records of saves whose project folder is gone, or that are
/// too old to recover: run when the designer starts.
pub fn prune_save_records() {
    if let Ok(state) = State::open() {
        state.prune();
    }
}

/// How long a load, diff or save waits for another designer's save to end.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// Settle any save a dead process left half done in the project of
/// `config_path`, and list the leftover files no journal names. Run before
/// the project is read; logs what it did.
pub fn recover_project(config_path: &Path) -> Result<SaveRecovery> {
    let paths = Paths::new(config_path)?;
    let lock = match ProjectLock::acquire(&paths.base, LOCK_WAIT) {
        Ok(lock) => lock,
        Err(LockError::Busy) => return Ok(SaveRecovery::busy()),
        Err(LockError::Io(e)) if !commit::pending(&paths.base) => {
            tracing::debug!(error = %e, "no lock on the project, and no interrupted save in it");
            return Ok(SaveRecovery::default());
        }
        Err(LockError::Io(e)) => bail!("locking the project to recover an interrupted save: {e}"),
        Err(e) => bail!(paths.relative_text(&e.to_string())),
    };
    Ok(recover_locked(&paths, &lock, &mut Disk::default()))
}

fn recover_locked(paths: &Paths, lock: &ProjectLock, disk: &mut Disk) -> SaveRecovery {
    let state = State::open();
    let why = state
        .as_ref()
        .err()
        .map(|e| paths.relative_text(&e.to_string()))
        .unwrap_or_default();
    let mut recovery = recover(
        lock,
        &paths.config,
        state.as_ref().map_err(|_| why.as_str()),
        disk,
    );
    if let Ok(mut own) = OWN_CLEANUP.lock() {
        recovery.settled.retain(|s| {
            let mine = s.outcome == Outcome::Completed && own.contains(&s.token);
            if mine {
                own.retain(|t| *t != s.token);
                tracing::debug!("finished cleaning up an earlier save of this designer");
            }
            !mine
        });
    }
    let rel = |p: &PathBuf| paths.relative_path(p);
    let step = |s: &HandStep| hand_step(paths, s);
    let mut out = SaveRecovery {
        recovered: recovery
            .settled
            .iter()
            .map(|s| RecoveredSave {
                outcome: match s.outcome {
                    Outcome::RolledBack => RecoveryOutcome::RolledBack,
                    Outcome::Completed => RecoveryOutcome::Completed,
                },
                id: s.token.clone(),
                started_at_ms: s.started_at_ms,
                files: s
                    .files
                    .iter()
                    .filter(|f| !s.kept.iter().any(|k| k.path == **f))
                    .map(rel)
                    .collect(),
                kept: s
                    .kept
                    .iter()
                    .map(|k| KeptFile {
                        path: rel(&k.path),
                        backup: k.backup.as_ref().map(rel),
                    })
                    .collect(),
                displaced: s
                    .displaced
                    .iter()
                    .map(|d| DisplacedFile {
                        path: rel(&d.path),
                        copy: rel(&d.copy),
                    })
                    .collect(),
            })
            .collect(),
        blocked: recovery.blocked.as_ref().map(|b| BlockedSave {
            busy: false,
            problems: b
                .problems
                .iter()
                .map(|p| BlockedFile {
                    path: rel(&p.path),
                    problem: match p.kind {
                        ProblemKind::BackupMissing => BlockedProblem::BackupMissing,
                        ProblemKind::Unreadable => BlockedProblem::Unreadable,
                        ProblemKind::JournalUnusable => BlockedProblem::JournalUnusable,
                        ProblemKind::Failed => BlockedProblem::Failed,
                    },
                    detail: p.detail.as_ref().map(|d| paths.relative_text(d)),
                    backup: p.backup.as_ref().map(rel),
                    fix: p.fix.as_deref().map(step),
                    before_copy: p.before_copy.as_ref().map(rel),
                })
                .collect(),
            leftovers: b.leftovers.iter().map(rel).collect(),
            report: b.report.as_ref().map(|r| journal_report(paths, r)),
            direction: b.direction.map(|d| match d {
                Direction::RollBack => RecoveryDirection::Back,
                Direction::Finish => RecoveryDirection::Forward,
            }),
            keep: b.keep.as_ref().map(|k| k.iter().map(step).collect()),
        }),
        leftovers: Vec::new(),
    };
    let completed: Vec<RecoveredSave> = out
        .recovered
        .iter()
        .filter(|r| r.outcome == RecoveryOutcome::Completed)
        .cloned()
        .collect();
    if !completed.is_empty()
        && let Ok(mut remembered) = COMPLETED.lock()
    {
        let saves = remembered.entry(paths.base.clone()).or_default();
        saves.extend(completed);
        let excess = saves.len().saturating_sub(COMPLETED_KEPT);
        saves.drain(..excess);
    }
    if out.blocked.is_none() {
        let mut folders = project_folders(paths);
        for folder in recovery.settled.iter().flat_map(|s| &s.folders) {
            if !folders.contains(folder) {
                folders.push(folder.clone());
            }
        }
        out.leftovers = strays(&folders).iter().map(rel).collect();
    }
    log_recovery(&out);
    out
}

fn hand_step(paths: &Paths, step: &HandStep) -> Step {
    let rel = |p: &PathBuf| paths.relative_path(p);
    match step {
        HandStep::Copy { from, to } => Step::Copy {
            from: rel(from),
            to: rel(to),
        },
        HandStep::Delete(path) => Step::Delete { path: rel(path) },
        HandStep::RemoveDir(path) => Step::RemoveDir { path: rel(path) },
    }
}

fn journal_report(paths: &Paths, report: &Report) -> JournalReport {
    let rel = |p: &PathBuf| paths.relative_path(p);
    let fix = |fix: &Fix| match fix {
        Fix::Nothing => FileFix::Nothing,
        Fix::CopyFrom(from) => FileFix::Copy { from: rel(from) },
        Fix::Delete => FileFix::Delete,
        Fix::Unavailable => FileFix::Unavailable,
    };
    JournalReport {
        cause: match report.cause {
            Unusable::NoRecord => JournalCause::NoRecord,
            Unusable::OtherFolder => JournalCause::OtherFolder,
            Unusable::NoState => JournalCause::NoState,
            Unusable::TooLarge => JournalCause::TooLarge,
            Unusable::Unreadable => JournalCause::Unreadable,
            Unusable::Malformed => JournalCause::Malformed,
            Unusable::NotASave => JournalCause::NotASave,
            Unusable::Conflict => JournalCause::Conflict,
        },
        journals: report.journals.iter().map(rel).collect(),
        recorded_for: report
            .recorded_for
            .as_ref()
            .map(|d| shortened(&d.display().to_string())),
        files: report
            .files
            .iter()
            .map(|f| ReportedFile {
                path: match &f.path {
                    Some(path) => rel(path),
                    None => shortened(&f.text),
                },
                outside: f.outside,
                holds: match f.holds {
                    Holds::Before => FileHolds::Before,
                    Holds::After => FileHolds::After,
                    Holds::Neither => FileHolds::Neither,
                    Holds::Unchecked => FileHolds::Unchecked,
                    Holds::Outside => FileHolds::Outside,
                },
                backup: f.backup.as_ref().map(rel),
                temp: f.temp.as_ref().map(rel),
                back: fix(&f.back),
                forward: fix(&f.forward),
            })
            .collect(),
        more: report.more,
        back_route: report
            .route(true)
            .map(|r| r.iter().map(|s| hand_step(paths, s)).collect()),
        forward_route: report
            .route(false)
            .map(|r| r.iter().map(|s| hand_step(paths, s)).collect()),
        leftovers: report.leftovers.iter().map(rel).collect(),
    }
}

/// `text` cut to a length a page can show, control characters escaped.
fn shortened(text: &str) -> String {
    const MAX: usize = 200;
    let escaped: String = text.escape_debug().collect();
    match escaped.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", escaped.get(..cut).unwrap_or("")),
        None => escaped,
    }
}

/// The config's folder and the folder of every schema file it names.
fn project_folders(paths: &Paths) -> Vec<PathBuf> {
    let mut folders = vec![paths.base.clone()];
    let config = std::fs::read_to_string(&paths.config)
        .ok()
        .and_then(|text| ConfigToml::try_parse(&text).ok());
    for entry in config.iter().flat_map(|c| &c.index) {
        if let Some(folder) = paths
            .at(entry.schema.as_ref())
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            && !folders.contains(&folder)
        {
            folders.push(folder);
        }
    }
    folders
}

fn log_recovery(recovery: &SaveRecovery) {
    for save in &recovery.recovered {
        let files = save.files.join(", ");
        let are = if save.files.len() == 1 { "is" } else { "are" };
        match save.outcome {
            RecoveryOutcome::RolledBack => tracing::warn!(
                started_at_ms = save.started_at_ms,
                "an interrupted save was rolled back: {files} {are} as before it"
            ),
            RecoveryOutcome::Completed => tracing::warn!(
                started_at_ms = save.started_at_ms,
                "an interrupted save was completed: {files} {are} as it wrote them; its backups are deleted"
            ),
        }
        for d in &save.displaced {
            tracing::warn!(
                "{} was written by another program as the save replaced it: it holds what the save \
                 wrote, and the other bytes are in {}; check them, then delete it",
                d.path,
                d.copy
            );
        }
        for kept in &save.kept {
            tracing::warn!(
                "{} was written by another program during or after the save, and is kept as it is{}",
                kept.path,
                kept.backup
                    .as_ref()
                    .map(|b| format!("; its text from before the save is in {b}"))
                    .unwrap_or_default()
            );
        }
    }
    if let Some(blocked) = &recovery.blocked
        && !blocked.busy
    {
        tracing::error!("{}", Interrupted(recovery.clone()));
    }
    if !recovery.leftovers.is_empty() {
        tracing::warn!(
            "files an earlier save left, which no journal names, are left alone: {}. Check them and delete them",
            recovery.leftovers.join(", ")
        );
    }
}

/// The resolved effect of one op on disk, for the review. `Write` covers both
/// create and modify; `Move` relocates a file. A `changed: false` entry is
/// a no-op (e.g. an upsert whose rendered content already matches disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffOp {
    Write,
    Move,
}

/// One op resolved against the filesystem: what it would change, plus the before
/// / after text for the diff view.
#[derive(Debug, Serialize)]
pub struct OpDiff {
    /// The resolved lifecycle op.
    pub op: DiffOp,
    /// The index whose schema file this is; `None` for the config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<IndexName>,
    /// Absolute, normalized destination path.
    pub path: String,
    /// Absolute source path (`Move` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// On-disk text now (empty when the file doesn't exist yet).
    pub current: String,
    /// Text after the op.
    pub next: String,
    /// Whether applying the op changes disk.
    pub changed: bool,
    /// A stable warning code when the path warrants one (`"outside_base"` when it
    /// resolves outside the config directory, `"overwrites"` when a move replaces
    /// a file already at its destination); the client translates it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// The op only fits the new `flusso.toml`: its index is new, renamed, or
    /// on another file than the config on disk says. It is saved together
    /// with the config or not at all.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub needs_config: bool,
    /// Set when the write could not keep the file's comments and formatting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<Notice>,
    /// How many comments the write drops although what they describe still
    /// exists (see [`Saved::dropped`]).
    #[serde(skip_serializing_if = "is_zero")]
    pub dropped: usize,
    /// The files this entry writes or removes that are not as the page read
    /// them ([`SaveRequest::digests`]); for `flusso.toml`, set whether or not
    /// it is written, since every save is checked against it. A save that
    /// writes anything refuses while the config's is set, and refuses an
    /// entry whose own is set.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<StaleFile>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Render one op's file content: its schema written into its base (the op's
/// own, else `on_disk`). A schema whose file would not load as that schema is
/// refused, so a diff shows nothing and a save writes nothing: `keep` falls
/// back to the canonical form when the patched file does not read back, and
/// that form fails the same way.
fn render_op(op: &FileOp, on_disk: Option<&str>) -> Result<Saved> {
    let schema = op
        .schema
        .as_ref()
        .context("upsert/move op needs a schema")?;
    let saved = keep::schema_yaml(op.base.as_deref().or(on_disk), schema, op.identity.as_ref())?;
    let read = SchemaYaml::try_parse(&saved.text)
        .map_err(anyhow::Error::from)
        .and_then(|entity| IndexSchema::try_from(entity).map_err(anyhow::Error::from))
        .context("the schema does not load, so it is not saved")?;
    if let Some(at) = first_difference(schema, &read) {
        bail!("the schema reads back differently at `{at}`, so it is not saved");
    }
    Ok(saved)
}

/// One `[[index]]` entry with its schema file resolved on disk.
struct Entry<'a> {
    name: &'a IndexName,
    path: PathBuf,
}

/// The project's paths as the filesystem resolves them.
struct Paths {
    /// The config file, resolved.
    config: PathBuf,
    /// Its directory, resolved.
    base: PathBuf,
    /// Its directory as given, which schema paths are joined to: the OS
    /// follows a `..` after a symlink from the link's target, and so does
    /// [`on_disk`].
    given_base: PathBuf,
}

impl Paths {
    fn new(config_path: &Path) -> Result<Self> {
        let given = std::path::absolute(config_path)?;
        let given_base = given.parent().unwrap_or(Path::new("/")).to_path_buf();
        let config = on_disk(&given)?;
        let base = config.parent().unwrap_or(Path::new("/")).to_path_buf();
        Ok(Self {
            config,
            base,
            given_base,
        })
    }

    /// A path relative to the config's directory, resolved.
    fn at(&self, path: &Path) -> Result<PathBuf> {
        if path.as_os_str().len() > MAX_PATH {
            bail!("a path is longer than {MAX_PATH} bytes");
        }
        on_disk(&resolve(&self.given_base, path))
    }

    /// A path as the request gives it (absolute, or relative to the config's
    /// directory), resolved; as given when it doesn't resolve.
    fn given(&self, path: &str) -> PathBuf {
        self.at(Path::new(path))
            .unwrap_or_else(|_| PathBuf::from(path))
    }

    /// `error` with every path in the project written relative to it, as
    /// the config names schema files.
    fn relative(&self, error: anyhow::Error) -> anyhow::Error {
        if error.is::<Interrupted>() || error.is::<Stale>() {
            return error;
        }
        anyhow::anyhow!(self.relative_text(&format!("{error:#}")))
    }

    /// `text` with every path in the project written relative to it.
    fn relative_text(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for dir in [&self.base, &self.given_base] {
            text = text.replace(&format!("{}/", dir.display()), "");
        }
        text
    }

    /// `path` relative to the config's folder: `../shared/x.schema.yml`
    /// for one beside it, as the config names it. Absolute only when the two
    /// share nothing but the root.
    fn relative_path(&self, path: &Path) -> String {
        if let Some(inside) = [&self.base, &self.given_base]
            .iter()
            .find_map(|dir| path.strip_prefix(dir).ok())
        {
            return inside.display().to_string();
        }
        let shared = self
            .base
            .components()
            .zip(path.components())
            .take_while(|(a, b)| a == b)
            .count();
        if shared > 1 {
            commit::journal::relative(&self.base, path)
        } else {
            path.display().to_string()
        }
    }

    /// A [`Stale`] refusal of `files` in this project.
    fn stale(&self, files: Vec<StaleFile>) -> Stale {
        Stale {
            files,
            config: self.relative_path(&self.config),
        }
    }

    /// Whether `path` is the config file, or would be on a case-insensitive
    /// filesystem.
    fn is_config(&self, path: &Path) -> bool {
        path == self.config || folded(path) == folded(&self.config)
    }

    /// Every index of `config`, resolved. Two indexes with one name refuse.
    fn entries<'a>(&self, config: &'a ConfigToml) -> Result<Vec<Entry<'a>>> {
        let mut entries: Vec<Entry<'a>> = Vec::with_capacity(config.index.len());
        for entry in &config.index {
            if entries.iter().any(|e| *e.name == entry.name) {
                bail!("two indexes are named `{}`", entry.name);
            }
            let path = self
                .at(entry.schema.as_ref())
                .with_context(|| format!("index `{}`", entry.name))?;
            entries.push(Entry {
                name: &entry.name,
                path,
            });
        }
        Ok(entries)
    }
}

/// Refuse a config flusso couldn't load or that would let one file stand for
/// another: a schema path that is the config file, a folder, or another
/// index's path spelled with other letter case (one file on a
/// case-insensitive filesystem, two elsewhere).
fn check_entries(paths: &Paths, entries: &[Entry<'_>]) -> Result<()> {
    for entry in entries {
        if paths.is_config(&entry.path) {
            bail!("index `{}` names the config file as its schema", entry.name);
        }
        if entry.path.is_dir() {
            bail!(
                "index `{}` names {}, which is a folder",
                entry.name,
                entry.path.display()
            );
        }
        if let Some(other) = entries
            .iter()
            .find(|o| o.path != entry.path && folded(&o.path) == folded(&entry.path))
        {
            bail!(
                "the schema paths of `{}` ({}) and `{}` ({}) differ only in letter case, \
                 which some filesystems read as one file",
                entry.name,
                entry.path.display(),
                other.name,
                other.path.display()
            );
        }
    }
    Ok(())
}

/// `config` with each schema path as flusso opens it: `.` segments, doubled
/// and trailing slashes dropped (`./a//b.yml/` → `a/b.yml`).
fn clean_schema_paths(config: &ConfigToml) -> Result<ConfigToml> {
    let mut config = config.clone();
    for entry in &mut config.index {
        let clean: PathBuf = entry
            .schema
            .components()
            .filter(|c| *c != Component::CurDir)
            .collect();
        if clean.as_os_str().is_empty() {
            bail!("index `{}` has no schema path", entry.name);
        }
        entry.schema = SchemaPath::new(clean);
    }
    Ok(config)
}

/// Resolve every op against the filesystem into the review diffs — the config
/// first, then each schema-file op. Shared by [`diff_project`] and
/// [`save_project`], so the review and the apply see exactly the same plan.
///
/// The rule a plan keeps: after the save, every index of the `flusso.toml` on
/// disk has a file of its own, holding its own schema. It is checked on what
/// the save writes, and nothing the request says is taken on trust:
///
/// - every path is resolved on disk first (symlinks and `..` followed, see
///   [`on_disk`]), the `skip` entries too, and every check runs on the
///   resolved paths;
/// - which index of the config on disk each saved index is comes from the
///   request's `identity` when it describes both configs, else from the names
///   ([`keep::index_pairs`]);
/// - an op the save leaves out (`skip`) is not written, so it is not checked
///   either; the ops that are written are checked against the config on disk
///   after the save: the request's when it is written, the current file when
///   the review leaves it out.
///
/// Refused, so nothing is written:
/// - two indexes with one name (as [`IndexName`] normalizes it);
/// - a schema path that is the config file, a folder, or another index's path
///   in other letter case;
/// - an op for an index the request's config doesn't declare, or writing
///   anywhere but that index's schema path;
/// - a write to a file another index also uses, unless that index writes it
///   too, and two ops writing different content to one file;
/// - a move whose source is not the same index's schema file on disk;
/// - an op that only fits the new config ([`OpDiff::needs_config`]) written
///   while the config is left out;
/// - a written config naming, for an index, a file the save doesn't write,
///   unless it is that index's own file on disk (shared only by the indexes
///   that already shared it), or a file no index used for a new index alone.
///
/// A move removes its source only when no index uses it after the save and it
/// lies in the project directory; otherwise the source stays.
fn plan(paths: &Paths, request: &SaveRequest) -> Result<Plan> {
    let config = clean_schema_paths(&request.config)?;
    let entries = paths.entries(&config)?;
    check_entries(paths, &entries)?;

    let on_disk_text = std::fs::read_to_string(&paths.config).ok();
    let disk = on_disk_text
        .as_deref()
        .and_then(|text| ConfigToml::try_parse(text).ok());
    let old: Vec<(&IndexName, Option<PathBuf>)> = disk
        .as_ref()
        .map(|d| {
            d.index
                .iter()
                .map(|e| (&e.name, paths.at(e.schema.as_ref()).ok()))
                .collect()
        })
        .unwrap_or_default();
    let pairs: Vec<Option<usize>> = match &disk {
        Some(disk) => keep::index_pairs(request.identity.as_ref(), disk, &config),
        None => vec![None; config.index.len()],
    };
    let old_of = |j: usize| pairs.get(j).copied().flatten().and_then(|i| old.get(i));
    let skip: Vec<PathBuf> = request.skip.iter().map(|s| paths.given(s)).collect();
    let skipped = |path: &Path| skip.iter().any(|s| s == path);

    let Saved {
        text: next,
        notice,
        dropped,
    } = keep::config_toml(on_disk_text.as_deref(), &config, request.identity.as_ref())?;
    let current = on_disk_text.unwrap_or_default();
    let config_changed = current != next;
    let config_left_out = config_changed && skipped(&paths.config);
    let config_written = config_changed && !config_left_out;
    let used_after = |path: &Path| {
        let same = |p: &Path| p == path || folded(p) == folded(path);
        if config_left_out {
            old.iter().any(|(_, p)| p.as_deref().is_some_and(same))
        } else {
            entries.iter().any(|e| same(&e.path))
        }
    };

    let mut diffs = Vec::with_capacity(request.ops.len() + 1);
    diffs.push(OpDiff {
        op: DiffOp::Write,
        index: None,
        path: paths.config.display().to_string(),
        from: None,
        changed: config_changed,
        warning: None,
        notice,
        dropped,
        needs_config: false,
        current,
        next,
        stale: Vec::new(),
    });

    // Each file written, with its text, its diff, and the saved indexes
    // (positions in `config`) whose ops write it.
    let mut written: Vec<(PathBuf, String, usize, Vec<usize>)> = Vec::new();
    for op in &request.ops {
        let dest = paths.at(&op.path)?;
        if paths.is_config(&dest) {
            bail!("an op for index `{}` writes the config file", op.index);
        }
        if dest.is_dir() {
            bail!(
                "an op for index `{}` writes {}, which is a folder",
                op.index,
                dest.display()
            );
        }
        let j = config
            .index
            .iter()
            .position(|e| e.name == op.index)
            .with_context(|| format!("an op names `{}`, which the config does not", op.index))?;
        if skipped(&dest) {
            continue;
        }
        let was = old_of(j);
        let own_before = was.and_then(|(_, path)| path.as_ref());
        let from = match op.kind {
            OpKind::Move => {
                Some(paths.at(op.from.as_deref().context("move op needs a `from` path")?)?)
            }
            OpKind::Upsert => None,
        };
        if let Some(from) = &from
            && (paths.is_config(from) || own_before != Some(from))
        {
            bail!(
                "a move for index `{}` takes {}, which is not its schema file in flusso.toml on disk",
                op.index,
                from.display()
            );
        }
        let needs_config = was.is_none_or(|(name, _)| **name != op.index)
            || own_before != Some(&dest)
            || from.as_ref().is_some_and(|f| *f != dest);
        if config_left_out && needs_config {
            bail!(
                "{} (index `{}`) only fits the new flusso.toml, which the save leaves out",
                dest.display(),
                op.index
            );
        }
        let own = entries.get(j).map(|e| &e.path);
        if !config_left_out && own != Some(&dest) {
            bail!(
                "an op writes {} for index `{}`, whose schema file is {}",
                dest.display(),
                op.index,
                own.map_or_else(String::new, |p| p.display().to_string())
            );
        }

        // The comments to keep are in the index's own file on disk; a file
        // another index left behind lends none. With no readable config on
        // disk, the file at the path is the best guess.
        let base_file = if disk.is_some() {
            own_before
        } else {
            Some(&dest)
        };
        let base_text = base_file.and_then(|p| std::fs::read_to_string(p).ok());
        let Saved {
            text: next,
            notice,
            dropped,
        } = render_op(op, base_text.as_deref())?;
        if let Some((_, other, at, writers)) = written.iter_mut().find(|(p, ..)| *p == dest) {
            if *other != next {
                bail!(
                    "two schema-file ops write {} with different content",
                    dest.display()
                );
            }
            writers.push(j);
            if let Some(shared) = diffs.get_mut(*at) {
                shared.needs_config |= needs_config;
            }
            continue;
        }
        written.push((dest.clone(), next.clone(), diffs.len(), vec![j]));

        let removed = from
            .filter(|f| f.is_file() && *f != dest && !used_after(f) && f.starts_with(&paths.base));
        let warning = out_of_tree(&paths.base, &dest).or_else(|| {
            (dest.exists() && own_before != Some(&dest)).then(|| "overwrites".to_owned())
        });
        let (kind, current, changed) = match &removed {
            Some(from) => (
                DiffOp::Move,
                std::fs::read_to_string(from).unwrap_or_default(),
                true,
            ),
            None => {
                let current = std::fs::read_to_string(&dest).unwrap_or_default();
                let changed = current != next;
                (DiffOp::Write, current, changed)
            }
        };
        diffs.push(OpDiff {
            op: kind,
            index: Some(op.index.clone()),
            path: dest.display().to_string(),
            from: removed.map(|f| f.display().to_string()),
            current,
            next,
            changed,
            warning,
            notice,
            dropped,
            needs_config,
            stale: Vec::new(),
        });
    }

    let name = |j: usize| {
        config
            .index
            .get(j)
            .map(|e| e.name.to_string())
            .unwrap_or_default()
    };
    for (dest, _, _, writers) in &written {
        let other = if config_left_out {
            old.iter()
                .enumerate()
                .find(|(i, (_, p))| {
                    p.as_ref() == Some(dest)
                        && !writers.iter().any(|&j| pairs.get(j) == Some(&Some(*i)))
                })
                .map(|(_, (n, _))| n.to_string())
        } else {
            entries
                .iter()
                .enumerate()
                .find(|(k, e)| e.path == *dest && !writers.contains(k))
                .map(|(_, e)| e.name.to_string())
        };
        if let Some(other) = other {
            bail!(
                "an op for index `{}` writes {}, which index `{other}` also uses",
                writers.first().map(|&j| name(j)).unwrap_or_default(),
                dest.display()
            );
        }
    }

    if config_written {
        for (j, entry) in entries.iter().enumerate() {
            if written.iter().any(|(p, ..)| *p == entry.path) {
                continue;
            }
            if !entry.path.is_file() {
                bail!(
                    "index `{}` has no schema file: {} does not exist and the save does not write it; \
                     design its schema, or remove the index",
                    entry.name,
                    entry.path.display()
                );
            }
            let owners: Vec<usize> = old
                .iter()
                .enumerate()
                .filter(|(_, (_, p))| p.as_ref() == Some(&entry.path))
                .map(|(i, _)| i)
                .collect();
            let sharing: Vec<&Entry<'_>> =
                entries.iter().filter(|e| e.path == entry.path).collect();
            let paired = pairs.get(j).copied().flatten();
            let own = match (owners.first(), paired) {
                (None, None) => sharing.len() == 1,
                (_, Some(i)) => owners.contains(&i),
                (Some(_), None) => false,
            };
            if own {
                if owners.is_empty() && paired.is_none() {
                    let text = std::fs::read_to_string(&entry.path)
                        .with_context(|| format!("reading {}", entry.path.display()))?;
                    parse_schema_text(&text).with_context(|| {
                        format!(
                            "flusso.toml would point the new index `{}` at {}, which does not \
                             read as a schema",
                            entry.name,
                            entry.path.display()
                        )
                    })?;
                }
                continue;
            }
            match owners.first().and_then(|&i| old.get(i)) {
                Some((owner, _)) => bail!(
                    "flusso.toml would point index `{}` at {}, the schema file of `{owner}` on disk, \
                     which the save does not write",
                    entry.name,
                    entry.path.display()
                ),
                None if sharing.len() > 1 => bail!(
                    "indexes `{}` would share {}, which the save does not write",
                    sharing
                        .iter()
                        .map(|e| e.name.as_str())
                        .collect::<Vec<_>>()
                        .join("`, `"),
                    entry.path.display()
                ),
                None => bail!(
                    "flusso.toml would point index `{}` at {}, which is not its schema file \
                     and the save does not write",
                    entry.name,
                    entry.path.display()
                ),
            }
        }
    }

    for diff in &mut diffs {
        diff.stale = stale_files(paths, diff, &request.digests)?;
    }
    Ok(Plan {
        diffs,
        config_written,
    })
}

/// The files `diff` writes or removes that are not as `digests` says the
/// page read them. A file `digests` doesn't list must not exist.
fn stale_files(paths: &Paths, diff: &OpDiff, digests: &Digests) -> Result<Vec<StaleFile>> {
    let mut stale = Vec::new();
    for path in std::iter::once(&diff.path).chain(&diff.from) {
        stale.extend(stale_file(paths, Path::new(path), digests)?);
    }
    Ok(stale)
}

/// `path`, which a commit found not holding `expected` (`None`: no file).
fn changed_file(paths: &Paths, path: &Path, expected: Option<&Digest>) -> Option<StaleFile> {
    let on_disk = Digest::at(path).ok().flatten();
    let change = match (expected, &on_disk) {
        (Some(_), Some(_)) => StaleChange::Changed,
        (Some(_), None) => StaleChange::Deleted,
        (None, Some(_)) => StaleChange::Created,
        (None, None) => return None,
    };
    Some(StaleFile {
        path: paths.relative_path(path),
        change,
        on_disk: on_disk.map(|d| d.hex().to_owned()),
    })
}

/// `path` when it is not as `digests` says the page read it.
fn stale_file(paths: &Paths, path: &Path, digests: &Digests) -> Result<Option<StaleFile>> {
    let key = paths.relative_path(path);
    let on_disk = Digest::at(path)
        .with_context(|| format!("reading {key}"))?
        .map(|d| d.hex().to_owned());
    let change = match (digests.get(&key), on_disk.is_some()) {
        (Some(read), _) if *read == on_disk => return Ok(None),
        (None, false) => return Ok(None),
        (Some(Some(_)), true) => StaleChange::Changed,
        (Some(Some(_)), false) => StaleChange::Deleted,
        (Some(None), _) => StaleChange::Created,
        (None, true) => StaleChange::Unloaded,
    };
    Ok(Some(StaleFile {
        path: key,
        change,
        on_disk,
    }))
}

/// A checked save: the review's diffs, and whether `flusso.toml` is written.
struct Plan {
    diffs: Vec<OpDiff>,
    config_written: bool,
}

/// Compute, without writing anything, what a save of `request` would change on
/// disk — the config plus each schema-file op.
pub fn diff_project(config_path: &Path, request: SaveRequest) -> Result<Vec<OpDiff>> {
    review_project(config_path, request).map(|review| review.diffs)
}

/// [`diff_project`], with the interrupted saves it rolled back first.
///
/// An interrupted save found first is settled. Rolled back, the files are as
/// they were, and the diff goes on; one that can't be settled is refused with
/// [`Interrupted`]. Each entry is against the file on disk now, and lists the
/// files that changed since the page read them ([`OpDiff::stale`]): the diff
/// shows them, the save refuses them.
pub fn review_project(config_path: &Path, request: SaveRequest) -> Result<Review> {
    let paths = Paths::new(config_path)?;
    let recovery = if commit::pending(&paths.base) {
        recover_project(config_path)?
    } else {
        SaveRecovery::default()
    };
    if recovery.stops() {
        return Err(Interrupted(recovery).into());
    }
    let recovered = unseen_recoveries(&paths, &request.digests, recovery.recovered);
    if let Some(stale) = stale_file(&paths, &paths.config, &request.digests)? {
        return Ok(Review {
            diffs: vec![stale_config(&paths, &request, stale)],
            recovered,
        });
    }
    let diffs = plan(&paths, &request)
        .map(|plan| plan.diffs)
        .map_err(|e| paths.relative(e))?;
    Ok(Review { diffs, recovered })
}

/// The review of a request whose `flusso.toml` changed since the page read
/// it: that file alone, marked, against the file on disk. Nothing else is
/// planned, since every check would read a config the page never saw.
fn stale_config(paths: &Paths, request: &SaveRequest, stale: StaleFile) -> OpDiff {
    let current = std::fs::read_to_string(&paths.config).unwrap_or_default();
    let next = keep::config_toml(
        Some(current.as_str()).filter(|_| paths.config.is_file()),
        &request.config,
        request.identity.as_ref(),
    )
    .map_or_else(|_| current.clone(), |saved| saved.text);
    OpDiff {
        op: DiffOp::Write,
        index: None,
        path: paths.config.display().to_string(),
        from: None,
        changed: current != next,
        warning: None,
        notice: None,
        dropped: 0,
        needs_config: false,
        current,
        next,
        stale: vec![stale],
    }
}

/// Every subdirectory under the config directory (relative, forward-slash
/// paths), for the designer's schema-directory picker. Hidden directories and
/// the usual build/vendor noise are skipped, and the walk is depth-bounded so a
/// huge tree can't stall the request. Sorted; the config directory itself is ""
/// (not listed — the picker offers root separately).
pub fn list_dirs(config_path: &Path) -> Vec<String> {
    let base = dirs_base(config_path);
    let mut out = Vec::new();
    collect_dirs(base, base, 0, &mut out);
    out.sort();
    out
}

/// The directory to walk for the picker. A relative config path like
/// `flusso.toml` has an *empty* parent (not `None`), and `read_dir("")` fails —
/// treat that as the current directory.
fn dirs_base(config_path: &Path) -> &Path {
    match config_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

fn collect_dirs(base: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    const SKIP: [&str; 5] = ["node_modules", "target", "dist", "vendor", ".git"];
    if depth >= 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIP.contains(&name.as_ref()) {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(base) {
            // Forward slashes so the path reads the same everywhere.
            out.push(
                rel.components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
        collect_dirs(base, &path, depth + 1, out);
    }
}

/// Apply the request's ops. Every check [`diff_project`] makes runs before
/// anything is written, then the files are committed all or nothing (see
/// the `commit` module): schema files first, `flusso.toml` last, then the move
/// sources removed. A save that fails at any step leaves the project as it
/// was, with no temp file, backup or new folder behind; only when putting a
/// file back fails too does the error name the backups left on disk. A save
/// cut short by a kill or a power loss is settled by the next load, diff or
/// save ([`recover_project`]). Only files that change are touched, and the
/// review's unchecked paths (`skip`) are left alone.
///
/// The save holds the project's lock, waiting up to five seconds for another
/// designer's save, and first settles a save a dead process left, as
/// [`review_project`] does: rolled back, the save goes on and says so;
/// refused with [`Interrupted`] when it can't be settled.
///
/// Under the lock, before anything is written, the files are compared with
/// the request's [`SaveRequest::digests`]: when a file the save writes, moves
/// or removes, or `flusso.toml` (which every check reads), is not as the page
/// read it, the save is refused with [`Stale`] and nothing is written.
pub fn save_project(config_path: &Path, request: SaveRequest) -> Result<SaveResponse> {
    save_with(config_path, &request, &mut Disk::default())
}

fn save_with(config_path: &Path, request: &SaveRequest, disk: &mut Disk) -> Result<SaveResponse> {
    let paths = Paths::new(config_path)?;
    let config_stale = || -> Result<()> {
        match stale_file(&paths, &paths.config, &request.digests)? {
            Some(stale) => Err(paths.stale(vec![stale]).into()),
            None => Ok(()),
        }
    };
    // Before the folder is made again for the lock, when it is gone.
    config_stale()?;
    std::fs::create_dir_all(&paths.base)
        .with_context(|| format!("creating {}", paths.base.display()))?;
    let lock = match ProjectLock::acquire(&paths.base, LOCK_WAIT) {
        Ok(lock) => lock,
        Err(LockError::Busy) => return Err(Interrupted(SaveRecovery::busy()).into()),
        Err(e) => bail!(paths.relative_text(&e.to_string())),
    };
    let recovery = recover_locked(&paths, &lock, disk);
    if recovery.stops() {
        return Err(Interrupted(recovery).into());
    }
    config_stale()?;
    // Before anything is written: a file this save writes would otherwise
    // read as unseen.
    let recovered = unseen_recoveries(&paths, &request.digests, recovery.recovered);
    let plan = plan(&paths, request).map_err(|e| paths.relative(e))?;
    let (config, schemas) = plan
        .diffs
        .split_first()
        .context("a plan starts with the config")?;
    let applied: Vec<&OpDiff> = schemas
        .iter()
        .filter(|d| d.changed)
        .chain((plan.config_written).then_some(config))
        .collect();
    if !applied.is_empty() {
        let mut stale: Vec<StaleFile> = config.stale.clone();
        for file in applied.iter().flat_map(|d| &d.stale) {
            if !stale.iter().any(|s| s.path == file.path) {
                stale.push(file.clone());
            }
        }
        if !stale.is_empty() {
            return Err(paths.stale(stale).into());
        }
    }

    let expect = |path: &Path| {
        Expect::Holds(
            request
                .digests
                .get(&paths.relative_path(path))
                .cloned()
                .flatten()
                .map(Digest::from_hex),
        )
    };
    let writes: Vec<Write> = applied
        .iter()
        .map(|d| {
            let dest = PathBuf::from(&d.path);
            Write {
                expect: expect(&dest),
                dest,
                text: d.next.clone(),
            }
        })
        .collect();
    let removals: Vec<Remove> = applied
        .iter()
        .filter(|d| d.op == DiffOp::Move)
        .filter_map(|d| d.from.as_ref().map(PathBuf::from))
        .map(|path| Remove {
            expect: expect(&path),
            path,
        })
        .collect();
    let state = State::for_saving();
    let committed = commit(&lock, &writes, &removals, state.as_ref(), disk).map_err(|e| {
        match e.downcast_ref::<Changed>() {
            Some(Changed(files)) => paths
                .stale(
                    files
                        .iter()
                        .filter_map(|(path, expected)| {
                            changed_file(&paths, path, expected.as_ref())
                        })
                        .collect(),
                )
                .into(),
            None => paths.relative(e),
        }
    })?;
    // A stopped disk stands for a killed process, which remembers nothing.
    if let (Some(token), false, Ok(mut own)) =
        (committed.cleanup_left, disk.stopped(), OWN_CLEANUP.lock())
    {
        own.push(token);
    }

    let mut resp = SaveResponse::default();
    for write in &writes {
        resp.digests.insert(
            paths.relative_path(&write.dest),
            Some(Digest::of(write.text.as_bytes()).hex().to_owned()),
        );
    }
    for removed in &removals {
        resp.digests
            .insert(paths.relative_path(&removed.path), None);
    }
    for d in &applied {
        match (d.op, &d.from) {
            (DiffOp::Move, Some(from)) => resp.moved.push(MovedFile {
                from: from.clone(),
                to: d.path.clone(),
            }),
            _ => resp.written.push(d.path.clone()),
        }
    }
    let removed: Vec<PathBuf> = removals.into_iter().map(|r| r.path).collect();
    resp.pruned = prune_empty_dirs(&paths.base, &removed);
    resp.recovered = recovered;
    resp.displaced = committed
        .displaced
        .iter()
        .map(|d| DisplacedFile {
            path: paths.relative_path(&d.path),
            copy: paths.relative_path(&d.copy),
        })
        .collect();
    for d in &resp.displaced {
        tracing::warn!(
            "{} was written by another program as the save replaced it: it holds what the save \
             wrote, and the other bytes are in {}; check them, then delete it",
            d.path,
            d.copy
        );
    }
    Ok(resp)
}

/// The introspected relational catalog plus detected junction tables.
#[derive(Debug, Serialize, Default)]
pub struct CatalogResponse {
    /// Every table the source can stream, with columns/keys/FKs.
    pub catalog: RelationalCatalog,
    /// Tables that look like many-to-many junctions.
    pub junctions: Vec<JunctionCandidate>,
    /// Set when introspection failed (DB unreachable, etc.) — the UI shows it
    /// rather than the page erroring out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Introspect the source configured by the on-disk `config_path`. A failure to
/// reach the database is reported in [`CatalogResponse::error`], not as an
/// error result — the designer still works for offline authoring.
pub async fn introspect(config_path: &Path) -> CatalogResponse {
    match introspect_inner(config_path).await {
        Ok((catalog, junctions)) => CatalogResponse {
            catalog,
            junctions,
            error: None,
        },
        Err(e) => CatalogResponse {
            error: Some(format!("{e:#}")),
            ..Default::default()
        },
    }
}

async fn introspect_inner(
    config_path: &Path,
) -> Result<(RelationalCatalog, Vec<JunctionCandidate>)> {
    let config =
        config::load(config_path).with_context(|| format!("loading {}", config_path.display()))?;
    introspect_with(&config).await
}

/// Introspect the source described by an *edited* (unsaved) config, so the UI
/// can test a connection URL before it's written to disk.
pub async fn test_connection(
    config: ConfigToml,
    validator: &crate::server::ConfigValidator,
) -> CatalogResponse {
    if let Err(e) = validator.check(&config) {
        return CatalogResponse {
            error: Some(format!("invalid configuration: {e:#}")),
            ..Default::default()
        };
    }
    let config = Config::from(config);
    match introspect_with(&config).await {
        Ok((catalog, junctions)) => CatalogResponse {
            catalog,
            junctions,
            error: None,
        },
        Err(e) => CatalogResponse {
            error: Some(format!("{e:#}")),
            ..Default::default()
        },
    }
}

async fn introspect_with(config: &Config) -> Result<(RelationalCatalog, Vec<JunctionCandidate>)> {
    let capture = build_capture(config)?;
    let catalog = capture.introspect().await?;
    let junctions = junction_candidates(&catalog);
    Ok((catalog, junctions))
}

/// A validation request: the edited config (for its connection) and the schemas
/// to check against the live database.
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    /// The edited config — its connection drives which database to check.
    pub config: ConfigToml,
    /// The schemas to validate, keyed by index name.
    pub indexes: Vec<ValidateIndex>,
}

/// One index to validate.
#[derive(Debug, Deserialize)]
pub struct ValidateIndex {
    /// Logical index name.
    pub name: IndexName,
    /// The schema to validate.
    pub schema: IndexSchema,
}

/// The outcome of validating the edited schemas against the live database.
#[derive(Debug, Serialize, Default)]
pub struct ValidateResponse {
    /// One entry per disagreement between a schema and the database.
    pub diagnostics: Vec<DiagnosticDto>,
    /// Whether the database was reachable. When false, `error` says why the
    /// connection failed and `diagnostics` is empty. When true, validation ran;
    /// any `error` is a validation/query failure (the DB was fine).
    pub db_reachable: bool,
    /// A connection failure (when `db_reachable` is false) or a validation/query
    /// error that occurred after connecting (when true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A schema↔database disagreement, flattened for JSON.
#[derive(Debug, Serialize)]
pub struct DiagnosticDto {
    /// The index the disagreement is in.
    pub index: String,
    /// The field involved: the last segment of `path`.
    pub field: String,
    /// The field's path from the document root, or an index-level key alone
    /// (`table`, `primary_key`, `filters`, `soft_delete`); see `anchor`.
    pub path: Vec<String>,
    /// `field` when `path` names a field, `index_key` when it names an
    /// index-level key (a root field may share its name).
    pub anchor: String,
    /// What was found, e.g. `unknown_column` (absent on the designer's own
    /// client-side checks).
    pub kind: String,
    /// `error` or `warning`.
    pub severity: String,
    /// Human-readable explanation.
    pub message: String,
}

/// Validate `request`'s schemas against the live database.
///
/// Connecting and validating are kept distinct: only a failure to *connect*
/// sets `db_reachable: false` ("Database not reachable"). Once connected, a
/// schema↔DB mismatch comes back as a per-field [`DiagnosticDto`] (an unknown
/// column is one of these now, not a hard error), and any residual query failure
/// is reported with `db_reachable: true` — never mislabelled as unreachable.
pub async fn validate(
    request: ValidateRequest,
    validator: &crate::server::ConfigValidator,
) -> ValidateResponse {
    let unreachable = |context: &str, e: String| ValidateResponse {
        db_reachable: false,
        error: Some(format!("{context}: {e}")),
        ..Default::default()
    };

    if let Err(e) = validator.check(&request.config) {
        return unreachable("invalid configuration", format!("{e:#}"));
    }
    let config = Config::from(request.config);
    let postgres = match postgres_config(&config) {
        Ok(postgres) => postgres,
        Err(e) => return unreachable("reading the source configuration", format!("{e:#}")),
    };
    let connection_url = match postgres.resolve_connection_url() {
        Ok(url) => url,
        Err(e) => return unreachable("resolving the source connection URL", e.to_string()),
    };

    let sql_url = match source_sql_url(connection_url.as_ref(), &postgres) {
        Ok(url) => url,
        Err(e) => return unreachable("applying the source TLS settings", e.to_string()),
    };

    let indexes: BTreeMap<IndexName, IndexSchema> = request
        .indexes
        .into_iter()
        .map(|index| (index.name, index.schema))
        .collect();
    let spec = Arc::new(SourceSpec::new(indexes));

    let documents = match PgDocumentBuilder::connect(&sql_url, Arc::clone(&spec)).await {
        Ok(documents) => documents,
        Err(e) => return unreachable("connecting to the database", e.to_string()),
    };

    // Connected → the database is reachable; a failure here is about the schemas.
    match validate_indexes(&spec, &documents).await {
        Ok(diagnostics) => ValidateResponse {
            diagnostics: diagnostics.into_iter().map(diagnostic_dto).collect(),
            db_reachable: true,
            error: None,
        },
        Err(e) => ValidateResponse {
            db_reachable: true,
            error: Some(format!("validating schemas: {e}")),
            ..Default::default()
        },
    }
}

/// A sample-document request: the edited config (for its connection) and one
/// index's schema to build a real document from.
#[derive(Debug, Deserialize)]
pub struct SampleRequest {
    /// The edited config — its connection drives which database to read.
    pub config: ConfigToml,
    /// Logical index name.
    pub name: IndexName,
    /// The schema to build a sample document for.
    pub schema: IndexSchema,
    /// The root row's primary key to build; the first row when omitted.
    #[serde(default)]
    pub id: Option<String>,
}

/// A real document built from one arbitrary root row — exactly what the sink
/// would write — for previewing a schema against live data.
#[derive(Debug, Serialize, Default)]
pub struct SampleResponse {
    /// The sample document, or `None` when none could be produced (see `note`).
    /// When `synthetic` is true it's example data derived from the schema types,
    /// not a real row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<serde_json::Value>,
    /// Whether `document` is synthesized from the schema (the root table had no
    /// rows) rather than built from a real row.
    pub synthetic: bool,
    /// Whether the database was reachable; when false, `error` says why.
    pub db_reachable: bool,
    /// Context for the document — e.g. that it's example data, or why none could
    /// be produced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Why the database could not be reached/queried.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// A script that failed on the sample. `document` is then the row as built,
    /// before any script ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_failure: Option<ScriptErrorDto>,
    /// What the scripts printed, as `field: text`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub printed: Vec<String>,
}

/// The outcome of a sample request against a reachable database.
enum SampleOutcome {
    /// A real document built from a live row, finished by the scripts unless
    /// one failed on it.
    Document {
        document: serde_json::Value,
        failure: Option<ScriptErrorDto>,
        printed: Vec<String>,
    },
    /// No row has the requested id.
    NotFound(String),
    /// The row's document failed to build (before any script ran).
    BuildFailed(BuildFailure),
    /// The root table had no rows, so this is example data from the schema types.
    Synthetic(serde_json::Value),
    /// The index has no single-column primary key, so a row can't be keyed.
    NoPrimaryKey,
}

/// Build a sample document for `request`'s index against the live database. A DB
/// connection failure is reported in [`SampleResponse::error`], not as an error.
pub async fn sample(request: SampleRequest) -> SampleResponse {
    match sample_inner(request).await {
        Ok(SampleOutcome::Document {
            document,
            failure,
            printed,
        }) => SampleResponse {
            document: Some(document),
            db_reachable: true,
            script_failure: failure,
            printed,
            ..Default::default()
        },
        Ok(SampleOutcome::NotFound(id)) => SampleResponse {
            db_reachable: true,
            note: Some(format!(
                "no row with id `{id}` (it is missing or soft-deleted)"
            )),
            ..Default::default()
        },
        Ok(SampleOutcome::BuildFailed(failure)) => SampleResponse {
            db_reachable: true,
            note: Some(format!("the row's document failed to build: {failure}")),
            ..Default::default()
        },
        Ok(SampleOutcome::Synthetic(document)) => SampleResponse {
            document: Some(document),
            synthetic: true,
            db_reachable: true,
            note: Some(
                "the root table has no rows — showing example data from the schema".to_owned(),
            ),
            ..Default::default()
        },
        Ok(SampleOutcome::NoPrimaryKey) => SampleResponse {
            db_reachable: true,
            note: Some(
                "this index has no single-column primary key, so a row can't be sampled".to_owned(),
            ),
            ..Default::default()
        },
        Err(e) => SampleResponse {
            db_reachable: false,
            error: Some(format!("{e:#}")),
            ..Default::default()
        },
    }
}

async fn sample_inner(request: SampleRequest) -> Result<SampleOutcome> {
    // A missing single-column primary key is a schema property — report it
    // without touching the database (it's also why `sample_document` would
    // return `None`, so distinguishing it here keeps the empty-table case clean).
    if request.schema.primary_key.is_none() {
        return Ok(SampleOutcome::NoPrimaryKey);
    }

    let config = Config::from(request.config);
    let postgres = postgres_config(&config)?;
    let connection_url = postgres
        .resolve_connection_url()
        .context("resolving the source connection URL")?;

    let sql_url = source_sql_url(connection_url.as_ref(), &postgres)?;

    let name = request.name.clone();
    let mut indexes: BTreeMap<IndexName, IndexSchema> = BTreeMap::new();
    indexes.insert(name.clone(), request.schema);
    let spec = Arc::new(SourceSpec::new(indexes));

    let documents = PgDocumentBuilder::connect(&sql_url, Arc::clone(&spec))
        .await
        .context("connecting to the database")?;
    let built = match &request.id {
        Some(id) => match documents
            .document_by_id(&name, id)
            .await
            .context("building the document")?
        {
            Some(built) => Some(built),
            None => return Ok(SampleOutcome::NotFound(id.clone())),
        },
        None => documents
            .sample_document(&name)
            .await
            .context("building a sample document")?,
    };
    if let Some(built) = built {
        let body = match built {
            Ok(body) => body,
            Err(failure) => return Ok(SampleOutcome::BuildFailed(failure)),
        };
        let schema = spec
            .schema(&name)
            .context("index missing from its own spec")?;
        return Ok(run_sample_scripts(&name, schema, body));
    }

    // No live row — synthesize example data from the schema's declared types.
    let schema = spec
        .schema(&name)
        .context("index missing from its own spec")?;
    Ok(SampleOutcome::Synthetic(preview::example_document(
        schema, &name,
    )))
}

/// Finish a sampled row with its index's scripts, keeping what they printed.
fn run_sample_scripts(
    index: &IndexName,
    schema: &IndexSchema,
    built: kernel::GenericValue,
) -> SampleOutcome {
    let printed: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&printed);
    let printer: Printer = Arc::new(move |context: &ScriptContext<'_>, text: &str| {
        if let Ok(mut lines) = sink.lock() {
            lines.push(format!("{}: {text}", context.field));
        }
    });
    let outcome =
        compile_index(index, schema, Some(printer)).map_err(|mut errors| errors.remove(0));
    let mut finished = built.clone();
    let failure = match outcome {
        Ok(scripts) => scripts
            .apply(index, &mut finished)
            .err()
            .map(ScriptErrorDto::from),
        Err(compile_error) => Some(compile_error),
    };
    let document = if failure.is_some() { &built } else { &finished };
    SampleOutcome::Document {
        document: sink::to_json(document),
        failure,
        printed: printed.lock().map(|l| l.clone()).unwrap_or_default(),
    }
}

fn diagnostic_dto(diagnostic: Diagnostic) -> DiagnosticDto {
    DiagnosticDto {
        index: diagnostic.index.to_string(),
        field: diagnostic.field.to_string(),
        path: diagnostic.path.iter().map(ToString::to_string).collect(),
        anchor: diagnostic.anchor.as_str().to_owned(),
        kind: diagnostic.kind.as_str().to_owned(),
        severity: match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
        .to_owned(),
        message: diagnostic.message,
    }
}

/// The source entry as the Postgres adapter's config. The designer introspects
/// Postgres, so it is one of the two places (with the CLI) that name it.
fn postgres_config(config: &Config) -> Result<PostgresConfig> {
    anyhow::ensure!(
        config.source.kind == PostgresConfig::KIND,
        "the designer supports a postgres source, not `{}`",
        config.source.kind
    );
    PostgresConfig::from_options(config.source.options.clone()).context("[source]")
}

/// Build a Postgres capture from `config`'s resolved connection — introspection
/// only uses its admin pool (the slot/publication names are irrelevant here).
fn build_capture(config: &Config) -> Result<WalChangeCapture> {
    let postgres = postgres_config(config)?;
    let connection_url = postgres
        .resolve_connection_url()
        .context("resolving the source connection URL")?;
    let replication = replication_config(
        connection_url.as_ref(),
        &postgres.tls(),
        "flusso_design",
        "flusso_design",
    )
    .context("building the replication connection config")?;
    let sql_url = source_sql_url(connection_url.as_ref(), &postgres)?;
    Ok(WalChangeCapture::new(replication, sql_url))
}

/// The SQL-side connection URL with the config's declared TLS settings applied
/// — what every sqlx connection in the designer must use.
fn source_sql_url(connection_url: &str, postgres: &PostgresConfig) -> Result<String> {
    sql_connection_url(connection_url, &postgres.tls())
        .context("applying the source TLS settings to the connection URL")
}

/// Resolve a schema path against the config directory (absolute paths as-is).
fn resolve(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

/// `path` with every letter lowercased: two paths that fold alike are one file
/// on a case-insensitive filesystem.
fn folded(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

/// Normalize `.`/`..` away lexically (no filesystem access).
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A warning code when `dest` (resolved) lies outside the config directory
/// (resolved). The designer still allows it — this only flags it in the
/// review; `None` when contained.
fn out_of_tree(base_dir: &Path, dest: &Path) -> Option<String> {
    (!dest.starts_with(base_dir)).then(|| "outside_base".to_owned())
}

/// Remove any directory left empty by the removed files, walking upward from each
/// removed file's parent but never past (or including) `base_dir`. Only truly
/// empty directories go (`remove_dir` refuses a non-empty one), so an unrelated
/// file always keeps its folder. Returns the pruned directories.
fn prune_empty_dirs(base_dir: &Path, removed: &[PathBuf]) -> Vec<String> {
    let base = lexical_normalize(base_dir);
    let mut pruned = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for file in removed {
        let mut dir = file.parent().map(Path::to_path_buf);
        while let Some(d) = dir {
            let norm = lexical_normalize(&d);
            if norm == base || !norm.starts_with(&base) || !seen.insert(norm) {
                break;
            }
            if std::fs::remove_dir(&d).is_ok() {
                pruned.push(d.display().to_string());
                dir = d.parent().map(Path::to_path_buf);
            } else {
                break;
            }
        }
    }
    pruned
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;

    /// The dev project in a fresh folder, with this thread's records of
    /// saves in flight in a folder beside it; both removed on drop.
    struct Fixture(PathBuf);

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
            std::fs::remove_dir_all(self.state_dir()).ok();
            commit::state::use_test_dir(None);
        }
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "flusso-design-recovery-{}-{tag}",
                std::process::id()
            ));
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir).unwrap();
            for (name, text) in Self::FILES {
                std::fs::write(dir.join(name), text).unwrap();
            }
            let fixture = Self(dir);
            std::fs::remove_dir_all(fixture.state_dir()).ok();
            commit::state::use_test_dir(Some(fixture.state_dir()));
            fixture
        }

        const FILES: [(&str, &str); 4] = [
            ("flusso.toml", include_str!("../../../dev/flusso.toml")),
            (
                "users.schema.yml",
                include_str!("../../../dev/users.schema.yml"),
            ),
            (
                "orders.schema.yml",
                include_str!("../../../dev/orders.schema.yml"),
            ),
            (
                "products.schema.yml",
                include_str!("../../../dev/products.schema.yml"),
            ),
        ];

        /// The digests of a page that read the fresh project.
        fn loaded() -> Digests {
            Self::FILES
                .iter()
                .map(|(name, text)| {
                    (
                        (*name).to_owned(),
                        Some(Digest::of(text.as_bytes()).hex().to_owned()),
                    )
                })
                .collect()
        }

        fn state_dir(&self) -> PathBuf {
            self.0.with_extension("state")
        }

        /// Whether a save in flight is past its commit point.
        fn committed(&self) -> bool {
            State::at(self.state_dir()).is_ok_and(|s| s.records().iter().any(|r| r.committed))
        }

        fn config(&self) -> PathBuf {
            self.0.join("flusso.toml")
        }

        /// Every file in the folder, hidden ones included, with its text.
        fn files(&self) -> BTreeMap<String, String> {
            std::fs::read_dir(&self.0)
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

        /// The orders and products indexes swap schema files.
        fn swap(&self) -> SaveRequest {
            let project = load_project(&self.config()).unwrap();
            let mut config = serde_json::to_value(&project.config).unwrap();
            for entry in config["index"].as_array_mut().unwrap() {
                let swapped = match entry["name"].as_str().unwrap() {
                    "orders" => "products.schema.yml",
                    "products" => "orders.schema.yml",
                    _ => continue,
                };
                entry["schema"] = swapped.into();
            }
            serde_json::from_value(serde_json::json!({
                "config": config,
                "digests": Self::loaded(),
                "ops": [
                    { "kind": "move", "index": "orders", "path": "products.schema.yml",
                      "from": "orders.schema.yml", "schema": schema("orders: 1"),
                      "base": tagged("orders: 1") },
                    { "kind": "move", "index": "products", "path": "orders.schema.yml",
                      "from": "products.schema.yml", "schema": schema("products: 1"),
                      "base": tagged("products: 1") },
                ],
            }))
            .unwrap()
        }

        /// Kill a swap save right after its first schema file was replaced.
        fn kill_mid_swap(&self) {
            // record 1, journal 2, temps 3, backups 3, then the first rename.
            save_with(&self.config(), &self.swap(), &mut Disk::stopping_after(10)).unwrap_err();
            assert_eq!(
                std::fs::read_to_string(self.0.join("products.schema.yml")).unwrap(),
                tagged("orders: 1")
            );
        }
    }

    /// A schema file that loads, told apart from others by its `tag` comment.
    fn tagged(tag: &str) -> String {
        format!(
            "# {tag}\nversion: 1\ntable: t\nprimary_key: id\nfields:\n  - keyword: id\n    required: true\n"
        )
    }

    /// The schema [`tagged`] holds, as the page sends it.
    fn schema(tag: &str) -> serde_json::Value {
        let entity = SchemaYaml::try_parse(&tagged(tag)).unwrap();
        serde_json::to_value(IndexSchema::try_from(entity).unwrap()).unwrap()
    }

    fn interrupted(result: Result<impl std::fmt::Debug>) -> SaveRecovery {
        let err = result.unwrap_err();
        err.downcast_ref::<Interrupted>()
            .unwrap_or_else(|| panic!("not an interrupted save: {err:#}"))
            .0
            .clone()
    }

    #[test]
    fn a_load_after_a_killed_swap_rolls_it_back_and_says_so() {
        let fixture = Fixture::new("load");
        let before = fixture.files();
        fixture.kill_mid_swap();

        let recovery = recover_project(&fixture.config()).unwrap();
        assert_eq!(recovery.recovered.len(), 1, "{recovery:?}");
        let save = &recovery.recovered[0];
        assert_eq!(save.outcome, RecoveryOutcome::RolledBack);
        assert_eq!(
            save.files,
            ["products.schema.yml", "orders.schema.yml", "flusso.toml"]
        );
        assert!(recovery.blocked.is_none() && recovery.leftovers.is_empty());
        assert_eq!(fixture.files(), before);

        save_project(&fixture.config(), fixture.swap()).unwrap();
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("products.schema.yml")).unwrap(),
            tagged("orders: 1")
        );
        assert!(fixture.files().keys().all(|name| !name.starts_with('.')));
    }

    /// A diff or save that finds a dead process's save rolled back goes on:
    /// the files are as the page read them. Each says what it rolled back.
    #[test]
    fn a_save_or_diff_over_a_rolled_back_save_goes_on_and_says_so() {
        let fixture = Fixture::new("save");
        let before = fixture.files();

        fixture.kill_mid_swap();
        let review = review_project(&fixture.config(), fixture.swap()).unwrap();
        assert_eq!(review.recovered.len(), 1);
        assert_eq!(review.recovered[0].outcome, RecoveryOutcome::RolledBack);
        assert!(review.diffs.iter().any(|d| d.changed));
        assert_eq!(fixture.files(), before);

        fixture.kill_mid_swap();
        let saved = save_project(&fixture.config(), fixture.swap()).unwrap();
        assert_eq!(saved.recovered.len(), 1);
        assert_eq!(saved.recovered[0].outcome, RecoveryOutcome::RolledBack);
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("products.schema.yml")).unwrap(),
            tagged("orders: 1")
        );
        assert!(fixture.files().keys().all(|name| !name.starts_with('.')));
    }

    /// A page that read the project before an interrupted save was completed
    /// is refused every time it saves a file that save changed, until it
    /// reads the project again; its diff shows those files.
    #[test]
    fn a_page_older_than_a_completed_save_must_reload_to_save() {
        let fixture = Fixture::new("stale");
        let page = load_project(&fixture.config()).unwrap().digests;
        // Killed right after its commit point, before deleting a backup.
        let mut point = 1;
        loop {
            let mut request = fixture.swap();
            request.digests = page.clone();
            save_with(
                &fixture.config(),
                &request,
                &mut Disk::stopping_after(point),
            )
            .ok();
            if fixture.committed() {
                break;
            }
            recover_project(&fixture.config()).unwrap();
            point += 1;
        }
        let recovery = recover_project(&fixture.config()).unwrap();
        assert_eq!(recovery.recovered[0].outcome, RecoveryOutcome::Completed);
        let after = fixture.files();

        let request = || {
            let mut request: SaveRequest = serde_json::from_value(serde_json::json!({
                "config": load_project(&fixture.config()).unwrap().config,
                "digests": {},
                "ops": [{ "kind": "upsert", "index": "orders", "path": "products.schema.yml",
                          "schema": schema("orders: 2"), "base": tagged("orders: 2") }],
            }))
            .unwrap();
            request.digests = page.clone();
            request
        };
        for _ in 0..2 {
            let err = save_project(&fixture.config(), request()).unwrap_err();
            let refused = &err.downcast_ref::<Stale>().expect("stale").files;
            let paths: Vec<&str> = refused.iter().map(|f| f.path.as_str()).collect();
            assert_eq!(paths, ["flusso.toml"], "the config is compared first");
            assert!(err.to_string().contains("Reload the page"), "{err}");
            let review = review_project(&fixture.config(), request()).unwrap();
            assert_eq!(review.diffs.len(), 1, "nothing planned on an unseen config");
            assert_eq!(review.diffs[0].stale[0].change, StaleChange::Changed);
        }
        assert_eq!(fixture.files(), after, "a refused save wrote");

        let mut reloaded = request();
        reloaded.digests = load_project(&fixture.config()).unwrap().digests;
        save_project(&fixture.config(), reloaded).unwrap();
    }

    /// A save stopped because a program that takes no lock wrote a file
    /// between the save's checks and its rename: the refusal names that
    /// file, which keeps the program's bytes, and nothing of the save is
    /// left, files, temp files, backups, journal or record.
    fn stopped_by_an_editor(fixture: &Fixture, request: &SaveRequest, check: usize, edited: &str) {
        let before = fixture.files();
        let err = save_with(
            &fixture.config(),
            request,
            &mut Disk::editing_at_check(check, Some("editor\n")),
        )
        .unwrap_err();
        let stale = &err.downcast_ref::<Stale>().expect("stale").files;
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert_eq!(stale[0].path, edited);
        assert_eq!(stale[0].change, StaleChange::Changed);
        let mut expected = before;
        expected.insert(edited.to_owned(), "editor\n".to_owned());
        assert_eq!(fixture.files(), expected);
        assert!(
            State::at(fixture.state_dir()).is_ok_and(|s| s.records().is_empty()),
            "a record is left"
        );
    }

    #[test]
    fn a_file_written_between_the_check_and_its_rename_stops_the_save() {
        let fixture = Fixture::new("race-write");
        let request: SaveRequest = serde_json::from_value(serde_json::json!({
            "config": load_project(&fixture.config()).unwrap().config,
            "digests": Fixture::loaded(),
            "ops": [{ "kind": "upsert", "index": "users", "path": "users.schema.yml",
                      "schema": schema("users: 1"), "base": tagged("users: 1") }],
        }))
        .unwrap();
        stopped_by_an_editor(&fixture, &request, 1, "users.schema.yml");
    }

    #[test]
    fn a_file_written_before_its_removal_stops_the_save() {
        let fixture = Fixture::new("race-remove");
        let mut config =
            serde_json::to_value(load_project(&fixture.config()).unwrap().config).unwrap();
        for entry in config["index"].as_array_mut().unwrap() {
            if entry["name"] == "users" {
                entry["schema"] = "moved.schema.yml".into();
            }
        }
        let request: SaveRequest = serde_json::from_value(serde_json::json!({
            "config": config,
            "digests": Fixture::loaded(),
            "ops": [{ "kind": "move", "index": "users", "path": "moved.schema.yml",
                      "from": "users.schema.yml", "schema": schema("users: 1"), "base": tagged("users: 1") }],
        }))
        .unwrap();
        // moved.schema.yml, flusso.toml, then the removal of users.schema.yml.
        stopped_by_an_editor(&fixture, &request, 3, "users.schema.yml");
    }

    /// The change lands on the second of three files, after the first was
    /// renamed into place: that rename is undone.
    #[test]
    fn a_file_written_mid_swap_undoes_the_renames_already_done() {
        let fixture = Fixture::new("race-second");
        // products.schema.yml, orders.schema.yml, then flusso.toml.
        stopped_by_an_editor(&fixture, &fixture.swap(), 2, "orders.schema.yml");
    }

    /// A page that read the project before a recovery completed a save is
    /// told so by its next save, even of a file that save did not write; a
    /// page that read it afterwards is not.
    #[test]
    fn a_save_from_a_page_older_than_a_completed_save_reports_it() {
        let fixture = Fixture::new("told");
        let upsert = |index: &str, digests: Digests| -> SaveRequest {
            serde_json::from_value(serde_json::json!({
                "config": load_project(&fixture.config()).unwrap().config,
                "digests": digests,
                "ops": [{ "kind": "upsert", "index": index, "path": format!("{index}.schema.yml"),
                          "schema": schema(&format!("{index}: 1")), "base": tagged(&format!("{index}: 1")) }],
            }))
            .unwrap()
        };
        let mut point = 1;
        loop {
            save_with(
                &fixture.config(),
                &upsert("users", Fixture::loaded()),
                &mut Disk::stopping_after(point),
            )
            .ok();
            if fixture.committed() {
                break;
            }
            recover_project(&fixture.config()).unwrap();
            point += 1;
        }
        let recovery = recover_project(&fixture.config()).unwrap();
        assert_eq!(recovery.recovered[0].outcome, RecoveryOutcome::Completed);

        let saved = save_project(&fixture.config(), upsert("orders", Fixture::loaded())).unwrap();
        assert_eq!(saved.recovered.len(), 1, "{saved:?}");
        assert_eq!(saved.recovered[0].files, ["users.schema.yml"]);
        let id = saved.recovered[0].id.clone();
        assert!(
            !id.is_empty(),
            "a page tells one recovery from another by its id"
        );

        // A page that read the project after the recovery saves the very file
        // the recovered save wrote: compared before writing, it has seen it.
        let fresh = load_project(&fixture.config()).unwrap().digests;
        let saved = save_project(&fixture.config(), upsert("users", fresh)).unwrap();
        assert!(saved.recovered.is_empty(), "{saved:?}");
    }

    /// Another program rewrites two files a save writes, right after any
    /// step of the save: the save answers 200 (both files as it wrote them
    /// except the rewritten ones), or the stale refusal naming only rewritten
    /// files (everything else as before). Never another error, never a file
    /// of the save left behind, and the project loads after.
    #[test]
    fn an_editor_writing_during_a_save_gets_a_save_or_the_refusal() {
        let request = |fixture: &Fixture| -> SaveRequest {
            serde_json::from_value(serde_json::json!({
                "config": load_project(&fixture.config()).unwrap().config,
                "digests": Fixture::loaded(),
                "ops": [
                    { "kind": "upsert", "index": "users", "path": "users.schema.yml", "schema": schema("users: 1"), "base": tagged("users: 1") },
                    { "kind": "upsert", "index": "products", "path": "products.schema.yml", "schema": schema("products: 1"), "base": tagged("products: 1") },
                ],
            }))
            .unwrap()
        };
        let edited = ["users.schema.yml", "products.schema.yml"];
        let mut both = 0;
        for n in 1..40 {
            let fixture = Fixture::new(&format!("two-edits-{n}"));
            let before = fixture.files();
            let mut disk = Disk::counting();
            for file in edited {
                disk = disk.with_edit(commit::Edit {
                    after_step: n,
                    path: fixture.0.join(file),
                    bytes: b"editor\n".to_vec(),
                    by_rename: false,
                    done: false,
                });
            }
            let saved = save_with(&fixture.config(), &request(&fixture), &mut disk);
            let done = disk.edits().iter().all(|e| e.done);
            let files = fixture.files();
            assert!(
                files.keys().all(|f| !f.starts_with('.')),
                "after {n}: {files:?}"
            );
            assert!(State::at(fixture.state_dir()).is_ok_and(|s| s.records().is_empty()));
            match saved {
                Ok(_) => {
                    for file in edited {
                        let want = if done {
                            "editor\n".to_owned()
                        } else {
                            tagged(&format!("{}: 1", &file[..file.find('.').unwrap()]))
                        };
                        assert_eq!(files[file], want, "after {n}");
                    }
                }
                Err(err) => {
                    let stale = &err
                        .downcast_ref::<Stale>()
                        .unwrap_or_else(|| panic!("after {n}: {err:#}"))
                        .files;
                    assert!(
                        stale.iter().all(|f| edited.contains(&f.path.as_str())),
                        "after {n}: {stale:?}"
                    );
                    both += usize::from(stale.len() == 2);
                    let mut expected = before;
                    for file in edited {
                        expected.insert(file.to_owned(), "editor\n".to_owned());
                    }
                    assert_eq!(files, expected, "after {n}");
                }
            }
            load_project(&fixture.config()).unwrap();
        }
        assert!(both > 0, "no step had the undo meet a rewritten file");
    }

    /// The one write a save can't see coming: another program rewriting a
    /// file in place in the instant between the save's last look and its
    /// rename. The file ends as the save wrote it, but the program wrote
    /// through the hard link the backup is: that backup, the only copy of
    /// its bytes, is kept and named, and nothing else is left.
    #[test]
    fn a_write_in_the_last_instant_before_the_rename_is_kept_aside_and_named() {
        let fixture = Fixture::new("displaced");
        let request: SaveRequest = serde_json::from_value(serde_json::json!({
            "config": load_project(&fixture.config()).unwrap().config,
            "digests": Fixture::loaded(),
            "ops": [{ "kind": "upsert", "index": "users", "path": "users.schema.yml",
                      "schema": schema("users: 1"), "base": tagged("users: 1") }],
        }))
        .unwrap();
        let saved = save_with(
            &fixture.config(),
            &request,
            &mut Disk::editing_after_look(1, "editor\n"),
        )
        .unwrap();
        let files = fixture.files();
        assert_eq!(files["users.schema.yml"], tagged("users: 1"));
        assert_eq!(saved.displaced.len(), 1, "{saved:?}");
        assert_eq!(saved.displaced[0].path, "users.schema.yml");
        assert_eq!(files[&saved.displaced[0].copy], "editor\n");
        let others: Vec<&String> = files
            .keys()
            .filter(|f| f.starts_with('.') && **f != saved.displaced[0].copy)
            .collect();
        assert!(others.is_empty(), "{others:?}");
        assert!(State::at(fixture.state_dir()).is_ok_and(|s| s.records().is_empty()));
    }

    /// A journal naming a file outside the project blocks loading and saving
    /// with the journal named project-relative, and what to do.
    #[test]
    fn a_journal_naming_a_file_outside_the_project_blocks_and_says_so() {
        let fixture = Fixture::new("planted");
        let journal = serde_json::json!({
            "format": 2, "started_at_ms": 1, "pid": 1, "token": "1-2", "dirs": [],
            "writes": [{ "dest": "../elsewhere.yml", "before": null, "after": "00" }],
            "removals": [],
        });
        std::fs::write(fixture.0.join(".flusso-save.journal"), journal.to_string()).unwrap();
        let left = fixture.files();

        let recovery = recover_project(&fixture.config()).unwrap();
        let blocked = recovery.blocked.as_ref().expect("blocked");
        assert_eq!(blocked.problems[0].path, ".flusso-save.journal");
        assert_eq!(blocked.problems[0].problem, BlockedProblem::JournalUnusable);
        assert_eq!(blocked.leftovers, [".flusso-save.journal"]);
        let report = blocked.report.as_ref().expect("a report");
        assert_eq!(report.cause, JournalCause::NoRecord);
        assert_eq!(report.files[0].holds, FileHolds::Outside);
        assert!(report.back_route.is_none() && report.forward_route.is_none());
        let text = Interrupted(recovery.clone()).to_string();
        assert!(
            text.contains("../elsewhere.yml is outside the project, not checked"),
            "{text}"
        );
        assert!(
            text.contains("delete only .flusso-save.journal and reload"),
            "{text}"
        );

        let refused = interrupted(save_project(&fixture.config(), fixture.swap()));
        assert!(refused.blocked.is_some());
        assert_eq!(fixture.files(), left);
    }

    #[test]
    fn a_file_edited_after_the_kill_is_kept_and_named() {
        let fixture = Fixture::new("kept");
        fixture.kill_mid_swap();
        std::fs::write(fixture.0.join("products.schema.yml"), "mine\n").unwrap();

        let recovery = recover_project(&fixture.config()).unwrap();
        assert!(recovery.blocked.is_none(), "{recovery:?}");
        let save = &recovery.recovered[0];
        assert_eq!(save.outcome, RecoveryOutcome::RolledBack);
        assert!(!save.files.contains(&"products.schema.yml".to_owned()));
        assert_eq!(save.kept.len(), 1, "{save:?}");
        assert_eq!(save.kept[0].path, "products.schema.yml");
        let backup = save.kept[0]
            .backup
            .as_ref()
            .expect("the text before is named");
        assert_eq!(
            std::fs::read_to_string(fixture.0.join(backup)).unwrap(),
            include_str!("../../../dev/products.schema.yml")
        );
        let files = fixture.files();
        assert_eq!(files["products.schema.yml"], "mine\n");
        assert_eq!(
            files["flusso.toml"],
            include_str!("../../../dev/flusso.toml")
        );
        assert!(!fixture.0.join(".flusso-save.journal").exists());
        load_project(&fixture.config()).unwrap();
    }

    #[test]
    fn a_project_another_designer_is_saving_is_left_to_it() {
        let fixture = Fixture::new("busy");
        let held = ProjectLock::acquire(&fixture.0, Duration::ZERO).unwrap();
        let (writes, removals) = (
            vec![Write {
                dest: fixture.0.join("users.schema.yml"),
                text: "users: 1\n".into(),
                expect: Expect::AsFound,
            }],
            Vec::new(),
        );
        commit(
            &held,
            &writes,
            &removals,
            None,
            &mut Disk::stopping_after(3),
        )
        .unwrap_err();
        let mid = fixture.files();

        let start = std::time::Instant::now();
        let recovery = recover_project(&fixture.config()).unwrap();
        assert!(start.elapsed() >= LOCK_WAIT);
        assert!(recovery.blocked.as_ref().is_some_and(|b| b.busy));
        assert_eq!(fixture.files(), mid);
        drop(held);
    }

    /// The step of a swap save that is its commit point.
    fn commit_point(tag: &str) -> usize {
        (1..)
            .find(|&n| {
                let fixture = Fixture::new(&format!("{tag}-{n}"));
                save_with(
                    &fixture.config(),
                    &fixture.swap(),
                    &mut Disk::stopping_after(n),
                )
                .ok();
                fixture.committed()
            })
            .unwrap()
    }

    /// A save whose commit mark could not be flushed returns, leaving its
    /// cleanup to the next recovery. Finishing it is no news to the page that
    /// made the save: with the digests the save returned, its next save goes
    /// on, and reports nothing recovered.
    #[test]
    fn a_save_left_for_cleanup_does_not_make_its_own_page_stale() {
        let point = commit_point("own-point");
        let fixture = Fixture::new("own");
        let mut page = load_project(&fixture.config()).unwrap().digests;
        let mut request = fixture.swap();
        request.digests = page.clone();
        let saved = save_with(
            &fixture.config(),
            &request,
            &mut Disk::failing_sync_from(point),
        )
        .unwrap();
        assert!(fixture.0.join(".flusso-save.journal").exists());
        assert!(fixture.committed());
        page.extend(saved.digests);

        let again: SaveRequest = serde_json::from_value(serde_json::json!({
            "config": request.config,
            "digests": {},
            "ops": [{ "kind": "upsert", "index": "orders", "path": "products.schema.yml",
                      "schema": schema("orders: 2"), "base": tagged("orders: 2") }],
        }))
        .unwrap();
        let again = SaveRequest {
            digests: page,
            ..again
        };
        let saved = save_project(&fixture.config(), again).unwrap();
        assert_eq!(saved.written.len(), 1, "{saved:?}");
        assert!(saved.recovered.is_empty(), "{saved:?}");
        assert!(fixture.files().keys().all(|name| !name.starts_with('.')));
    }

    /// The reviewer's case through the API: a journal this designer has no
    /// record of, naming `.git/HEAD` as created by the save, blocks with a
    /// report that neither checks that file nor gives a step for it (it is
    /// no schema or config file), changes nothing, and deleting only the
    /// journal (the way that leaves every file as it is) lets the project
    /// load.
    #[test]
    fn an_unrecorded_journal_is_reported_and_deleting_it_loads_the_project() {
        let fixture = Fixture::new("unrecorded");
        let head = "ref: refs/heads/main\n";
        std::fs::create_dir(fixture.0.join(".git")).unwrap();
        std::fs::write(fixture.0.join(".git/HEAD"), head).unwrap();
        let after = serde_json::to_value(commit::journal::Digest::of(head.as_bytes())).unwrap();
        let journal = serde_json::json!({
            "format": 2, "started_at_ms": 1, "pid": 1, "token": "1-2", "dirs": [],
            "writes": [{ "dest": ".git/HEAD", "before": null, "after": after }],
            "removals": [],
        });
        std::fs::write(fixture.0.join(".flusso-save.journal"), journal.to_string()).unwrap();
        let left = fixture.files();

        let recovery = recover_project(&fixture.config()).unwrap();
        let report = recovery
            .blocked
            .as_ref()
            .and_then(|b| b.report.clone())
            .expect("a report");
        assert_eq!(report.cause, JournalCause::NoRecord);
        assert_eq!(report.journals, [".flusso-save.journal"]);
        assert_eq!(report.files[0].path, ".git/HEAD");
        assert_eq!(report.files[0].holds, FileHolds::Unchecked);
        assert!(report.back_route.is_none() && report.forward_route.is_none());
        assert_eq!(fixture.files(), left);
        assert_eq!(
            std::fs::read_to_string(fixture.0.join(".git/HEAD")).unwrap(),
            head
        );
        let refused = interrupted(save_project(&fixture.config(), fixture.swap()));
        assert!(refused.blocked.is_some());

        std::fs::remove_file(fixture.0.join(".flusso-save.journal")).unwrap();
        let recovery = recover_project(&fixture.config()).unwrap();
        assert!(recovery.blocked.is_none(), "{recovery:?}");
        load_project(&fixture.config()).unwrap();
    }

    #[test]
    fn old_leftovers_are_listed_on_load_and_kept() {
        let fixture = Fixture::new("old");
        let old = fixture.0.join(".users.schema.yml.flusso-save.1-2.0.bak");
        std::fs::write(&old, "users\n").unwrap();
        let recovery = recover_project(&fixture.config()).unwrap();
        assert_eq!(
            recovery.leftovers,
            [".users.schema.yml.flusso-save.1-2.0.bak"]
        );
        assert!(old.exists());
        save_project(&fixture.config(), fixture.swap()).unwrap();
        assert!(old.exists());
    }
}
