//! Discovering `flusso.toml` at compile time and resolving the named index's
//! mapping — no database, the same resolution `flusso build` performs.

use std::path::{Path, PathBuf};

use schema::{IndexMapping, IndexName, MappingType, ResolvedField};

/// The query **scope** a struct's handles live in (see `query::Root`).
///
/// `Root` for the document root and for any `object`/`one_to_one` reached only
/// through other objects (flattened, dotted, no wrapper). `SelfTagged` for a
/// `nested` element: its handles carry the struct's own type as their scope, so
/// they must be lifted with `Nested::any`/`all`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The document root scope.
    Root,
    /// This struct's own type (a `nested` element introduces its own scope).
    SelfTagged,
}

/// A resolved index plus the files whose changes should retrigger a rebuild.
pub(crate) struct Resolved {
    pub(crate) mapping: IndexMapping,
    /// Absolute paths to fold in via `include_bytes!` so edits rebuild.
    pub(crate) tracked: Vec<PathBuf>,
}

impl Resolved {
    /// The resolved fields at `path` (dotted, e.g. `orders.items`), or the root
    /// fields when `path` is `None`. `Err` names where the walk broke.
    pub(crate) fn fields_at(&self, path: Option<&str>) -> Result<&[ResolvedField], String> {
        let Some(path) = path else {
            return Ok(&self.mapping.fields);
        };
        let mut fields = self.mapping.fields.as_slice();
        let mut walked = String::new();
        for segment in path.split('.') {
            let next = fields.iter().find(|f| f.name.as_ref() == segment);
            match next {
                Some(field) if !field.children.is_empty() => {
                    fields = &field.children;
                    if !walked.is_empty() {
                        walked.push('.');
                    }
                    walked.push_str(segment);
                }
                Some(_) => {
                    return Err(format!(
                        "`path = \"{path}\"`: `{segment}` is a leaf field with no nested fields"
                    ));
                }
                None => {
                    let scope = if walked.is_empty() {
                        format!("index `{}`", self.mapping.index.as_ref())
                    } else {
                        format!("`{walked}`")
                    };
                    return Err(format!(
                        "`path = \"{path}\"`: no field `{segment}` in {scope}"
                    ));
                }
            }
        }
        Ok(fields)
    }

    /// The query scope for the struct bound at `path` (see [`Scope`]).
    ///
    /// `None`/object-only path → [`Scope::Root`]; a path whose final segment is
    /// a `nested` array → [`Scope::SelfTagged`]. An `object` *under* a `nested`
    /// can't be expressed in the "objects-direct" scope model, so it's an error.
    /// Assumes `path` has already validated via [`Resolved::fields_at`].
    pub(crate) fn scope_at(&self, path: Option<&str>) -> Result<Scope, String> {
        let Some(path) = path else {
            return Ok(Scope::Root);
        };
        let segments: Vec<&str> = path.split('.').collect();
        let mut fields = self.mapping.fields.as_slice();
        let mut nested_ancestor = false;
        for (i, segment) in segments.iter().enumerate() {
            let Some(field) = fields.iter().find(|f| f.name.as_ref() == *segment) else {
                return Err(format!("`path = \"{path}\"`: no field `{segment}`"));
            };
            let is_nested = matches!(field.mapping.mapping_type, MappingType::Nested);
            let is_object = matches!(field.mapping.mapping_type, MappingType::Object);
            if i + 1 == segments.len() {
                if is_nested {
                    return Ok(Scope::SelfTagged);
                }
                if is_object && nested_ancestor {
                    return Err(format!(
                        "`path = \"{path}\"`: `{segment}` is an object inside a `nested` array \
                         — querying object sub-fields within a nested scope isn't supported yet"
                    ));
                }
                return Ok(Scope::Root);
            }
            nested_ancestor |= is_nested;
            fields = &field.children;
        }
        Ok(Scope::Root)
    }
}

/// Find `flusso.toml`, load + resolve it, and return the requested index.
pub(crate) fn resolve(index: &str, config_override: Option<&str>) -> Result<Resolved, String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| "CARGO_MANIFEST_DIR is unset — cannot locate flusso.toml".to_owned())?;
    let config_path = find_config(Path::new(&manifest), config_override)?;

    let config = schema::load(&config_path)
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

    Ok(Resolved { mapping, tracked })
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
