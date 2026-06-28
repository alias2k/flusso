//! The designer's JSON API: load the project, introspect the database, preview
//! a schema, validate against the live store, build a sample document from a
//! live row, and write the files back.
//!
//! Every operation works in the validated vocabulary — requests carry
//! [`IndexSchema`]/[`ConfigToml`] as JSON (so invalid identifiers are rejected
//! at deserialization), and the catalog/diagnostics types are the source
//! layer's own. The handlers in [`crate::server`] are thin wrappers over these
//! functions; the logic lives here so it stays testable without a server.

use std::collections::BTreeMap;
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

/// A full save: the edited config and every index schema to write.
#[derive(Debug, Deserialize)]
pub struct SaveRequest {
    /// The deployment config to write to `flusso.toml`.
    pub config: ConfigToml,
    /// Each index schema and the path to write it to (relative to the config).
    pub indexes: Vec<SaveSchema>,
}

/// One schema file to write.
#[derive(Debug, Deserialize)]
pub struct SaveSchema {
    /// Path to write, as it appears in the config (relative to the config dir).
    pub schema_path: PathBuf,
    /// The schema to render.
    pub schema: IndexSchema,
    /// Raw YAML to write verbatim instead of regenerating from `schema` — the
    /// raw-edit escape hatch. When `None`, codegen renders `schema`.
    #[serde(default)]
    pub raw: Option<String>,
}

/// What was written.
#[derive(Debug, Serialize)]
pub struct SaveResponse {
    /// Absolute paths written, in write order (config first).
    pub written: Vec<String>,
}

/// Render and write the config and every schema in `request` — but only the
/// files that actually change, so unchanged files keep their mtime (and we don't
/// reflow files the user didn't touch). Canonical regeneration — see [`codegen`].
pub fn save_project(config_path: &Path, request: SaveRequest) -> Result<SaveResponse> {
    let base_dir = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut written = Vec::with_capacity(request.indexes.len() + 1);

    let toml = codegen::config_to_toml(&request.config)?;
    if write_if_changed(config_path, &toml)? {
        written.push(config_path.display().to_string());
    }

    for index in &request.indexes {
        let yaml = match &index.raw {
            Some(raw) => raw.clone(),
            None => codegen::schema_to_yaml(&index.schema)?,
        };
        let resolved = resolve(&base_dir, &index.schema_path);
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        if write_if_changed(&resolved, &yaml)? {
            written.push(resolved.display().to_string());
        }
    }

    Ok(SaveResponse { written })
}

/// One file's on-disk text vs what a save would write.
#[derive(Debug, Serialize)]
pub struct FileDiff {
    /// Absolute path of the file.
    pub path: String,
    /// Current on-disk text (empty when the file doesn't exist yet).
    pub current: String,
    /// Text a save would write.
    pub next: String,
    /// Whether the save would change the file.
    pub changed: bool,
}

/// Compute, without writing anything, what a save of `request` would change on
/// disk — the config plus every index file.
pub fn diff_project(config_path: &Path, request: SaveRequest) -> Result<Vec<FileDiff>> {
    let base_dir = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut diffs = Vec::with_capacity(request.indexes.len() + 1);

    let next = codegen::config_to_toml(&request.config)?;
    let current = std::fs::read_to_string(config_path).unwrap_or_default();
    diffs.push(FileDiff {
        changed: current != next,
        path: config_path.display().to_string(),
        current,
        next,
    });

    for index in &request.indexes {
        let next = match &index.raw {
            Some(raw) => raw.clone(),
            None => codegen::schema_to_yaml(&index.schema)?,
        };
        let resolved = resolve(&base_dir, &index.schema_path);
        let current = std::fs::read_to_string(&resolved).unwrap_or_default();
        diffs.push(FileDiff {
            changed: current != next,
            path: resolved.display().to_string(),
            current,
            next,
        });
    }

    Ok(diffs)
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
    /// Whether the database was reachable; when false, `error` says why and
    /// `diagnostics` is empty.
    pub db_reachable: bool,
    /// Why the database could not be reached/queried.
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

/// Validate `request`'s schemas against the live database. A DB connection
/// failure is reported in [`ValidateResponse::error`], not as an error result.
pub async fn validate(request: ValidateRequest) -> ValidateResponse {
    match validate_inner(request).await {
        Ok(diagnostics) => ValidateResponse {
            diagnostics,
            db_reachable: true,
            error: None,
        },
        Err(e) => ValidateResponse {
            db_reachable: false,
            error: Some(format!("{e:#}")),
            ..Default::default()
        },
    }
}

async fn validate_inner(request: ValidateRequest) -> Result<Vec<DiagnosticDto>> {
    let config = Config::from(request.config);
    let connection_url = config
        .source
        .resolve_connection_url()
        .context("resolving the source connection URL")?;

    let indexes: BTreeMap<IndexName, IndexSchema> = request
        .indexes
        .into_iter()
        .map(|index| (index.name, index.schema))
        .collect();
    let spec = Arc::new(SourceSpec::new(indexes));

    let documents = PgDocumentBuilder::connect(connection_url.as_ref(), Arc::clone(&spec))
        .await
        .context("connecting to the database")?;
    let diagnostics = validate_indexes(&spec, &documents)
        .await
        .context("validating schemas against the database")?;

    Ok(diagnostics.into_iter().map(diagnostic_dto).collect())
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<serde_json::Value>,
    /// Whether the database was reachable; when false, `error` says why.
    pub db_reachable: bool,
    /// Why no document could be produced even though the DB was reachable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Why the database could not be reached/queried.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Why a reachable database still produced no sample document.
enum SampleOutcome {
    Document(serde_json::Value),
    /// The root table has no rows to sample.
    EmptyTable,
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
        Ok(SampleOutcome::EmptyTable) => SampleResponse {
            db_reachable: true,
            note: Some("the root table has no rows to sample".to_owned()),
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
    // return `None`, so distinguishing it here keeps the empty-table note clean).
    if request.schema.primary_key.is_none() {
        return Ok(SampleOutcome::NoPrimaryKey);
    }

    let config = Config::from(request.config);
    let connection_url = config
        .source
        .resolve_connection_url()
        .context("resolving the source connection URL")?;

    let mut indexes: BTreeMap<IndexName, IndexSchema> = BTreeMap::new();
    indexes.insert(request.name.clone(), request.schema);
    let spec = Arc::new(SourceSpec::new(indexes));

    let documents = PgDocumentBuilder::connect(connection_url.as_ref(), Arc::clone(&spec))
        .await
        .context("connecting to the database")?;
    let body = documents
        .sample_document(&request.name)
        .await
        .context("building a sample document")?;
    Ok(match body.as_ref().map(sinks_core::to_json) {
        Some(document) => SampleOutcome::Document(document),
        None => SampleOutcome::EmptyTable,
    })
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

/// Write `contents` to `path` only if it differs from what's there; returns
/// whether a write happened.
fn write_if_changed(path: &Path, contents: &str) -> Result<bool> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(false);
    }
    std::fs::write(path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

/// Resolve a schema path against the config directory (absolute paths as-is).
fn resolve(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}
