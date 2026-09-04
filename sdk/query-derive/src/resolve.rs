//! Discovering `flusso.toml` at compile time and resolving the named index's
//! mapping — no database, the same resolution `flusso build` performs.
//!
//! Only the **root** level is resolved here. Every deeper level is reached by
//! the recursive codegen walking `ResolvedField::children`, so there is no path
//! to parse and no scope to infer.

use std::path::{Path, PathBuf};

use config::{IndexMapping, IndexName, Sink};

/// One path level for codegen: the field name plus whether it's a `nested`
/// boundary (vs a flattened object). Mirrors `flusso_query::Segment`.
#[derive(Debug, Clone)]
pub(crate) struct PathSegment {
    pub(crate) name: String,
    pub(crate) nested: bool,
}

/// A resolved index plus the files whose changes should retrigger a rebuild.
pub(crate) struct Resolved {
    pub(crate) mapping: IndexMapping,
    /// Whether every OpenSearch sink has `auto_subfields` on — so the auto
    /// `.keyword`/`.text`/`.keyword_lowercase` subfields are guaranteed present
    /// in whichever cluster the query client reads. Conservative across a
    /// multi-sink fan-out: false if *any* OpenSearch sink has it off. Gates the
    /// subfield accessors on generated `text`/`keyword` handles.
    pub(crate) auto_subfields: bool,
    /// Absolute paths to fold in via `include_bytes!` so edits rebuild.
    pub(crate) tracked: Vec<PathBuf>,
}

/// Find `flusso.toml`, load + resolve it, and return the requested index.
pub(crate) fn resolve(index: &str, config_override: Option<&str>) -> Result<Resolved, String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| "CARGO_MANIFEST_DIR is unset — cannot locate flusso.toml".to_owned())?;
    let config_path = find_config(Path::new(&manifest), config_override)?;

    let config = config::load(&config_path)
        .map_err(|error| format!("loading `{}`: {error}", config_path.display()))?;

    let key = IndexName::try_new(index.to_owned())
        .map_err(|error| format!("`{index}` is not a valid index name: {error}"))?;

    let index_entry = config.indexes.get(&key).ok_or_else(|| {
        let mut available: Vec<&str> = config.indexes.keys().map(IndexName::as_ref).collect();
        available.sort_unstable();
        format!(
            "index `{index}` is not defined in `{}` (found: {})",
            config_path.display(),
            available.join(", "),
        )
    })?;

    let mapping = index_entry.schema.resolve(key);
    let tracked = tracked_files(&config_path);

    // Indexes fan out to every configured sink (there's no per-index sink
    // selection), so the subfields are guaranteed only if every OpenSearch sink
    // provisions them. Stdout sinks don't create indexes — ignore them. No
    // OpenSearch sink (nothing to query) → leave the permissive default.
    let auto_subfields = config
        .sinks
        .values()
        .filter_map(|sink| match sink {
            Sink::Opensearch(os) => Some(os.auto_subfields),
            Sink::Stdout(_) => None,
        })
        .all(|on| on);

    Ok(Resolved {
        mapping,
        auto_subfields,
        tracked,
    })
}

/// Walk up from `start` to find `flusso.toml`, honoring an explicit override
/// (the `config = "…"` attribute) or the `FLUSSO_CONFIG` env var.
fn find_config(start: &Path, config_override: Option<&str>) -> Result<PathBuf, String> {
    if let Some(over) = config_override.map(str::to_owned).or_else(env_config) {
        let candidate = resolve_relative(start, &over);
        return if candidate.is_file() {
            Ok(candidate)
        } else {
            Err(format!(
                "configured flusso.toml not found at `{}`",
                candidate.display()
            ))
        };
    }

    for current in start.ancestors() {
        let candidate = current.join("flusso.toml");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "could not find `flusso.toml` searching up from `{}` — set `FLUSSO_CONFIG` or \
         `#[flusso(config = \"…\")]`",
        start.display()
    ))
}

fn env_config() -> Option<String> {
    std::env::var("FLUSSO_CONFIG")
        .ok()
        .filter(|s| !s.is_empty())
}

fn resolve_relative(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// `flusso.toml` plus every schema file it references (resolved relative to the
/// config), so editing either retriggers compilation.
fn tracked_files(config_path: &Path) -> Vec<PathBuf> {
    let mut tracked = vec![config_path.to_path_buf()];
    let dir = config_path.parent().unwrap_or(Path::new("."));
    let text = std::fs::read_to_string(config_path).unwrap_or_default();
    let table = toml::from_str::<toml::Value>(&text).unwrap_or(toml::Value::Boolean(false));
    if let Some(indexes) = table.get("index").and_then(toml::Value::as_array) {
        for entry in indexes {
            if let Some(schema) = entry.get("schema").and_then(toml::Value::as_str) {
                tracked.push(dir.join(schema));
            }
        }
    }
    tracked
}
