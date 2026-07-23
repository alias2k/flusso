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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use schema::Config;
use schema_config_toml::ConfigToml;
use schema_core::common::IndexName;
use schema_core::{IndexSchema, ParseFrom};
use schema_index_yaml::SchemaYaml;
use serde::{Deserialize, Serialize};
use sources_core::{
    Diagnostic, JunctionCandidate, RelationalCatalog, SchemaIntrospection, Severity, SourceSpec,
    junction_candidates, validate_indexes,
};
use sources_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};

use crate::codegen;
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
    use schema_index_yaml::ParseError;

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
}

/// The previewed document/mapping plus the exact YAML that would be written and
/// whether it round-trips through the parser (a self-check on codegen).
#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    /// The `*.schema.yml` text codegen would write.
    pub yaml: String,
    /// Derived mapping + document tree.
    pub preview: Preview,
    /// Whether the generated YAML parses back cleanly.
    pub parse_ok: bool,
    /// The parse error, when `parse_ok` is false (should not happen for a valid
    /// schema — surfaced rather than hidden if it does).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
}

/// Build a preview for `request`.
pub fn build_preview(request: PreviewRequest) -> Result<PreviewResponse> {
    let yaml = codegen::schema_to_yaml(&request.schema)?;
    let preview = preview::preview(&request.schema, &request.index);
    let (parse_ok, parse_error) = match SchemaYaml::try_parse(&yaml)
        .map_err(|e| e.to_string())
        .and_then(|entity| IndexSchema::try_from(entity).map_err(|e| e.to_string()))
    {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e)),
    };
    Ok(PreviewResponse {
        yaml,
        preview,
        parse_ok,
        parse_error,
    })
}

/// What to do with one schema file. The client computes these from its saved
/// snapshot → current document (correlated by a stable index id); the server
/// holds no session state and just applies them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    /// Create the file at `path`, or overwrite it if it exists.
    Upsert,
    /// Move `from` → `path` (its content may also change in the same step).
    Move,
    /// Remove the file at `path`.
    Delete,
}

/// One schema-file operation in a save/diff request. `schema`/`raw` supply the
/// content for `Upsert`/`Move` (raw wins — the raw-edit escape hatch); `from` is
/// the source path for `Move`.
#[derive(Debug, Deserialize)]
pub struct FileOp {
    /// What to do.
    pub kind: OpKind,
    /// Destination (`Upsert`/`Move`) or target (`Delete`), relative to the config dir.
    pub path: PathBuf,
    /// Source path for a `Move`, relative to the config dir.
    #[serde(default)]
    pub from: Option<PathBuf>,
    /// The schema to render for `Upsert`/`Move`.
    #[serde(default)]
    pub schema: Option<IndexSchema>,
    /// Raw YAML to write verbatim instead of rendering `schema`.
    #[serde(default)]
    pub raw: Option<String>,
}

/// A full save/diff request: the deployment config plus the schema-file ops.
#[derive(Debug, Deserialize)]
pub struct SaveRequest {
    /// The deployment config to write to `flusso.toml`.
    pub config: ConfigToml,
    /// The schema-file operations to apply.
    #[serde(default)]
    pub ops: Vec<FileOp>,
    /// Absolute paths to skip on save — the review's unchecked entries. Empty by
    /// default (apply everything).
    #[serde(default)]
    pub skip: Vec<String>,
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
    /// Absolute paths deleted.
    pub deleted: Vec<String>,
    /// Absolute paths of directories pruned because they became empty.
    pub pruned: Vec<String>,
}

/// The resolved effect of one op on disk, for the review. `Write` covers both
/// create and modify; `Move`/`Delete` are lifecycle. A `changed: false` entry is
/// a no-op (e.g. an upsert whose rendered content already matches disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffOp {
    Write,
    Move,
    Delete,
}

/// One op resolved against the filesystem: what it would change, plus the before
/// / after text for the diff view.
#[derive(Debug, Serialize)]
pub struct OpDiff {
    /// The resolved lifecycle op.
    pub op: DiffOp,
    /// Absolute destination / target path.
    pub path: String,
    /// Absolute source path (`Move` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// On-disk text now (empty when the file doesn't exist yet).
    pub current: String,
    /// Text after the op ("" for a delete).
    pub next: String,
    /// Whether applying the op changes disk.
    pub changed: bool,
    /// A stable warning code when the path warrants one (`"outside_base"` when it
    /// resolves outside the config directory); the client translates it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Render one op's file content — the raw escape hatch wins, else codegen.
fn render_op(op: &FileOp) -> Result<String> {
    match &op.raw {
        Some(raw) => Ok(raw.clone()),
        None => {
            let schema = op
                .schema
                .as_ref()
                .context("upsert/move op needs a schema or raw content")?;
            codegen::schema_to_yaml(schema)
        }
    }
}

/// Resolve every op against the filesystem into the review diffs — the config
/// first, then each schema-file op. Shared by [`diff_project`] and [`save_project`]
/// so the review and the apply see exactly the same plan.
fn plan(config_path: &Path, request: &SaveRequest) -> Result<Vec<OpDiff>> {
    let base = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut plan = Vec::with_capacity(request.ops.len() + 1);

    let next = codegen::config_to_toml(&request.config)?;
    let current = std::fs::read_to_string(config_path).unwrap_or_default();
    plan.push(OpDiff {
        op: DiffOp::Write,
        path: config_path.display().to_string(),
        from: None,
        changed: current != next,
        warning: None,
        current,
        next,
    });

    for op in &request.ops {
        match op.kind {
            OpKind::Upsert => {
                let dest = resolve(&base, &op.path);
                let next = render_op(op)?;
                let current = std::fs::read_to_string(&dest).unwrap_or_default();
                plan.push(OpDiff {
                    op: DiffOp::Write,
                    changed: current != next,
                    warning: out_of_tree(&base, &dest),
                    path: dest.display().to_string(),
                    from: None,
                    current,
                    next,
                });
            }
            OpKind::Move => {
                let from = resolve(
                    &base,
                    op.from.as_deref().context("move op needs a `from` path")?,
                );
                let dest = resolve(&base, &op.path);
                let next = render_op(op)?;
                // Source already gone (a prior save, or an external move) — there's
                // nothing to relocate, so degrade to a plain write at the destination.
                if from.exists() {
                    let current = std::fs::read_to_string(&from).unwrap_or_default();
                    plan.push(OpDiff {
                        op: DiffOp::Move,
                        from: Some(from.display().to_string()),
                        changed: true,
                        warning: out_of_tree(&base, &dest),
                        path: dest.display().to_string(),
                        current,
                        next,
                    });
                } else {
                    let current = std::fs::read_to_string(&dest).unwrap_or_default();
                    plan.push(OpDiff {
                        op: DiffOp::Write,
                        from: None,
                        changed: current != next,
                        warning: out_of_tree(&base, &dest),
                        path: dest.display().to_string(),
                        current,
                        next,
                    });
                }
            }
            OpKind::Delete => {
                let target = resolve(&base, &op.path);
                if target.exists() {
                    let current = std::fs::read_to_string(&target).unwrap_or_default();
                    plan.push(OpDiff {
                        op: DiffOp::Delete,
                        path: target.display().to_string(),
                        from: None,
                        changed: true,
                        warning: None,
                        current,
                        next: String::new(),
                    });
                }
            }
        }
    }

    Ok(plan)
}

/// Compute, without writing anything, what a save of `request` would change on
/// disk — the config plus each schema-file op.
pub fn diff_project(config_path: &Path, request: SaveRequest) -> Result<Vec<OpDiff>> {
    plan(config_path, &request)
}

/// Apply the request's ops **atomically and in order**: render + stage every
/// write to a sibling temp file first (so a codegen error changes nothing on
/// disk), then commit the renames, then remove deletes and move-sources, then
/// prune any directory left empty. Only files that actually change are touched,
/// and the review's unchecked paths (`skip`) are left alone.
pub fn save_project(config_path: &Path, request: SaveRequest) -> Result<SaveResponse> {
    let base = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let plan = plan(config_path, &request)?;
    let skip: HashSet<&str> = request.skip.iter().map(String::as_str).collect();

    let mut resp = SaveResponse::default();
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut removals: Vec<PathBuf> = Vec::new();

    // Stage: write every create/modify/move destination to a temp sibling. No
    // final path is touched until every render + temp write has succeeded.
    for (i, d) in plan.iter().enumerate() {
        if !d.changed || skip.contains(d.path.as_str()) {
            continue;
        }
        match d.op {
            DiffOp::Write | DiffOp::Move => {
                let dest = PathBuf::from(&d.path);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("creating {}", parent.display()))?;
                }
                let tmp = temp_sibling(&dest, i);
                std::fs::write(&tmp, &d.next)
                    .with_context(|| format!("staging {}", dest.display()))?;
                staged.push((tmp, dest));
                if d.op == DiffOp::Move {
                    if let Some(from) = &d.from {
                        removals.push(PathBuf::from(from));
                        resp.moved.push(MovedFile {
                            from: from.clone(),
                            to: d.path.clone(),
                        });
                    }
                } else {
                    resp.written.push(d.path.clone());
                }
            }
            DiffOp::Delete => {
                removals.push(PathBuf::from(&d.path));
                resp.deleted.push(d.path.clone());
            }
        }
    }

    // Commit: flip each staged temp into place (a same-dir rename is atomic).
    for (tmp, dest) in &staged {
        std::fs::rename(tmp, dest).with_context(|| format!("committing {}", dest.display()))?;
    }

    // Remove deletes + move-sources, then prune any now-empty directory upward.
    for path in &removals {
        std::fs::remove_file(path).ok();
    }
    resp.pruned = prune_empty_dirs(&base, &removals);

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
        schema::load(config_path).with_context(|| format!("loading {}", config_path.display()))?;
    introspect_with(&config).await
}

/// Introspect the source described by an *edited* (unsaved) config, so the UI
/// can test a connection URL before it's written to disk.
pub async fn test_connection(config: ConfigToml) -> CatalogResponse {
    match introspect_with(&Config::from(config)).await {
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
    /// The field involved.
    pub field: String,
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
pub async fn validate(request: ValidateRequest) -> ValidateResponse {
    let unreachable = |context: &str, e: String| ValidateResponse {
        db_reachable: false,
        error: Some(format!("{context}: {e}")),
        ..Default::default()
    };

    let config = Config::from(request.config);
    let connection_url = match config.source.resolve_connection_url() {
        Ok(url) => url,
        Err(e) => return unreachable("resolving the source connection URL", e.to_string()),
    };

    let indexes: BTreeMap<IndexName, IndexSchema> = request
        .indexes
        .into_iter()
        .map(|index| (index.name, index.schema))
        .collect();
    let spec = Arc::new(SourceSpec::new(indexes));

    let documents =
        match PgDocumentBuilder::connect(connection_url.as_ref(), Arc::clone(&spec)).await {
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
}

/// The outcome of a sample request against a reachable database.
enum SampleOutcome {
    /// A real document built from a live row.
    Document(serde_json::Value),
    /// The root table had no rows, so this is example data from the schema types.
    Synthetic(serde_json::Value),
    /// The index has no single-column primary key, so a row can't be keyed.
    NoPrimaryKey,
}

/// Build a sample document for `request`'s index against the live database. A DB
/// connection failure is reported in [`SampleResponse::error`], not as an error.
pub async fn sample(request: SampleRequest) -> SampleResponse {
    match sample_inner(request).await {
        Ok(SampleOutcome::Document(document)) => SampleResponse {
            document: Some(document),
            db_reachable: true,
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
    let connection_url = config
        .source
        .resolve_connection_url()
        .context("resolving the source connection URL")?;

    let name = request.name.clone();
    let mut indexes: BTreeMap<IndexName, IndexSchema> = BTreeMap::new();
    indexes.insert(name.clone(), request.schema);
    let spec = Arc::new(SourceSpec::new(indexes));

    let documents = PgDocumentBuilder::connect(connection_url.as_ref(), Arc::clone(&spec))
        .await
        .context("connecting to the database")?;
    if let Some(body) = documents
        .sample_document(&name)
        .await
        .context("building a sample document")?
    {
        return Ok(SampleOutcome::Document(sinks_core::to_json(&body)));
    }

    // No live row — synthesize example data from the schema's declared types.
    let schema = spec
        .schema(&name)
        .context("index missing from its own spec")?;
    Ok(SampleOutcome::Synthetic(preview::example_document(
        schema, &name,
    )))
}

fn diagnostic_dto(diagnostic: Diagnostic) -> DiagnosticDto {
    DiagnosticDto {
        index: diagnostic.index.to_string(),
        field: diagnostic.field.to_string(),
        severity: match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
        .to_owned(),
        message: diagnostic.message,
    }
}

/// Build a Postgres capture from `config`'s resolved connection — introspection
/// only uses its admin pool (the slot/publication names are irrelevant here).
fn build_capture(config: &Config) -> Result<WalChangeCapture> {
    let connection_url = config
        .source
        .resolve_connection_url()
        .context("resolving the source connection URL")?;
    let connection_url = connection_url.as_ref().to_owned();
    let replication = replication_config(&connection_url)?;
    Ok(WalChangeCapture::new(replication, connection_url))
}

fn replication_config(connection_url: &str) -> Result<ReplicationConfig> {
    let url = url::Url::parse(connection_url).context("parsing connection URL")?;
    let host = url
        .host_str()
        .context("connection URL has no host")?
        .to_owned();
    let port = url.port().unwrap_or(5432);
    let user = url.username();
    anyhow::ensure!(!user.is_empty(), "connection URL has no user");
    let password = url.password().unwrap_or_default();
    let database = url.path().trim_start_matches('/');
    let database = if database.is_empty() { user } else { database };
    Ok(ReplicationConfig::new(
        host,
        user,
        password,
        database,
        "flusso_design",
        "flusso_design",
    )
    .with_port(port))
}

/// Resolve a schema path against the config directory (absolute paths as-is).
fn resolve(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

/// A unique sibling temp path for staging a write — same directory as the final
/// file, so the commit rename is atomic; `i` disambiguates within one save.
fn temp_sibling(dest: &Path, i: usize) -> PathBuf {
    let name = dest.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    let dir = dest.parent().unwrap_or(Path::new("."));
    dir.join(format!(".{name}.flusso-save.{i}.tmp"))
}

/// Normalize `.`/`..` away lexically (no filesystem access — the destination may
/// not exist yet), so containment can be checked on a would-be path.
fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
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

/// A warning code when `dest` resolves outside the config directory (a `../`
/// escape or an absolute path elsewhere). The designer still allows it — this
/// only flags it in the review; `None` when contained.
fn out_of_tree(base_dir: &Path, dest: &Path) -> Option<String> {
    let base = lexical_normalize(base_dir);
    (!lexical_normalize(dest).starts_with(&base)).then(|| "outside_base".to_owned())
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
