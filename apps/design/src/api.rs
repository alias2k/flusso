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

use crate::commit::{Disk, Write, commit};
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
}

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
pub fn load_project(config_path: &Path) -> Result<Project> {
    let raw = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let config = ConfigToml::try_parse(&raw)
        .with_context(|| format!("parsing {}", config_path.display()))?;

    let base_dir = config_path.parent().unwrap_or(Path::new("."));
    let indexes = config
        .index
        .iter()
        .map(|entry| {
            let schema_path = entry.schema.as_ref();
            let resolved = resolve(base_dir, schema_path);
            let raw = std::fs::read_to_string(&resolved).ok();
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

/// One schema-file operation in a save/diff request. `schema`/`raw` supply the
/// content (raw wins — the raw-edit escape hatch); `from` is the source path
/// for `Move`.
#[derive(Debug, Clone, Deserialize)]
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
    /// Raw YAML to write verbatim instead of rendering `schema`.
    #[serde(default)]
    pub raw: Option<String>,
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
#[derive(Debug, Deserialize)]
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
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// Render one op's file content: the raw escape hatch wins, else the schema
/// written into its base (the op's own, else `on_disk`).
fn render_op(op: &FileOp, on_disk: Option<&str>) -> Result<Saved> {
    match &op.raw {
        Some(raw) => Ok(Saved {
            text: raw.clone(),
            notice: None,
            dropped: 0,
        }),
        None => {
            let schema = op
                .schema
                .as_ref()
                .context("upsert/move op needs a schema or raw content")?;
            keep::schema_yaml(op.base.as_deref().or(on_disk), schema, op.identity.as_ref())
        }
    }
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
        let mut text = format!("{error:#}");
        for dir in [&self.base, &self.given_base] {
            text = text.replace(&format!("{}/", dir.display()), "");
        }
        anyhow::anyhow!(text)
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

    Ok(Plan {
        diffs,
        config_written,
    })
}

/// A checked save: the review's diffs, and whether `flusso.toml` is written.
struct Plan {
    diffs: Vec<OpDiff>,
    config_written: bool,
}

/// Compute, without writing anything, what a save of `request` would change on
/// disk — the config plus each schema-file op.
pub fn diff_project(config_path: &Path, request: SaveRequest) -> Result<Vec<OpDiff>> {
    let paths = Paths::new(config_path)?;
    plan(&paths, &request)
        .map(|plan| plan.diffs)
        .map_err(|e| paths.relative(e))
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
/// file back fails too does the error name the backups left on disk. Only
/// files that change are touched, and the review's unchecked paths (`skip`)
/// are left alone.
pub fn save_project(config_path: &Path, request: SaveRequest) -> Result<SaveResponse> {
    save_with(config_path, &request, &mut Disk::default())
}

fn save_with(config_path: &Path, request: &SaveRequest, disk: &mut Disk) -> Result<SaveResponse> {
    let paths = Paths::new(config_path)?;
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

    let writes: Vec<Write> = applied
        .iter()
        .map(|d| Write {
            dest: PathBuf::from(&d.path),
            text: d.next.clone(),
        })
        .collect();
    let removals: Vec<PathBuf> = applied
        .iter()
        .filter(|d| d.op == DiffOp::Move)
        .filter_map(|d| d.from.as_ref().map(PathBuf::from))
        .collect();
    commit(&writes, &removals, disk).map_err(|e| paths.relative(e))?;

    let mut resp = SaveResponse::default();
    for d in &applied {
        match (d.op, &d.from) {
            (DiffOp::Move, Some(from)) => resp.moved.push(MovedFile {
                from: from.clone(),
                to: d.path.clone(),
            }),
            _ => resp.written.push(d.path.clone()),
        }
    }
    resp.pruned = prune_empty_dirs(&paths.base, &removals);
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
            Some(body) => Some(body),
            None => return Ok(SampleOutcome::NotFound(id.clone())),
        },
        None => documents
            .sample_document(&name)
            .await
            .context("building a sample document")?,
    };
    if let Some(body) = built {
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

/// `path` (absolute) as the filesystem resolves it: its deepest existing
/// ancestor canonicalized (symlinks and `..` followed), then the rest
/// appended. A path that doesn't exist yet still resolves through the links
/// above it, so `sub/x.yml` with `sub -> .` is `x.yml`. Refused: a broken
/// link, a `..` in the part that doesn't exist (there is nothing to go up
/// from), and a path through a file.
fn on_disk(path: &Path) -> Result<PathBuf> {
    let parts: Vec<Component<'_>> = path.components().collect();
    for cut in (1..=parts.len()).rev() {
        let Some((head, tail)) = parts.split_at_checked(cut) else {
            continue;
        };
        let head: PathBuf = head.iter().collect();
        match std::fs::canonicalize(&head) {
            Ok(mut real) => {
                if !tail.is_empty() && !real.is_dir() {
                    bail!(
                        "{} goes through {}, which is a file",
                        path.display(),
                        real.display()
                    );
                }
                for part in tail {
                    match part {
                        Component::Normal(name) => real.push(name),
                        Component::CurDir => {}
                        _ => bail!(
                            "{} goes up from a folder that does not exist",
                            path.display()
                        ),
                    }
                }
                return Ok(real);
            }
            Err(e) if head.symlink_metadata().is_ok() => {
                bail!("{} cannot be resolved: {e}", head.display())
            }
            Err(_) => {}
        }
    }
    bail!("{} cannot be resolved", path.display())
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
