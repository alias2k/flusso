//! `flusso schema` — the editor-assist schemas and the Reference option tables,
//! rendered from the registered adapters' descriptions.
//!
//! - `schema config` prints the complete JSON Schema for `flusso.toml`: the
//!   base derived from the config entities, with each port entry replaced by
//!   one alternative per registered adapter (its `type` constant plus its own
//!   option schema). The committed copy lives in the config crate
//!   (`libs/1-config/config.schema.json`) and a test here fails when it drifts;
//!   `just schema-gen` refreshes it.
//! - `schema index` prints the hand-curated `*.schema.yml` schema as embedded.
//! - `schema docs [--out DIR]` renders one Markdown option table per adapter,
//!   which the Reference pages `{{#include}}`; the same drift test guards them.

use std::io::Write;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, ValueEnum};
use kernel::{AdapterDescription, Port};
use serde_json::{Map, Value, json};

use crate::adapters;

#[derive(Debug, Args)]
pub(crate) struct SchemaArgs {
    /// Which artifact to print: the `flusso.toml` schema, the `*.schema.yml`
    /// schema, or the adapters' Reference option tables.
    #[arg(value_enum, env = "FLUSSO_SCHEMA")]
    which: SchemaKind,

    /// For `docs`: write one `<port>-<kind>.md` per adapter into this directory
    /// instead of printing them all to stdout.
    #[arg(long, env = "FLUSSO_SCHEMA_OUT")]
    out: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    Config,
    Index,
    Docs,
}

pub(crate) fn execute(args: SchemaArgs) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    match args.which {
        SchemaKind::Config => writeln!(out, "{}", config_schema_json()?)?,
        SchemaKind::Index => writeln!(out, "{}", config::INDEX_SCHEMA.trim_end())?,
        SchemaKind::Docs => match args.out {
            Some(dir) => {
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
                for (file, table) in adapter_docs() {
                    let path = dir.join(&file);
                    std::fs::write(&path, table)
                        .with_context(|| format!("writing {}", path.display()))?;
                }
            }
            None => {
                for (file, table) in adapter_docs() {
                    writeln!(out, "<!-- {file} -->\n{table}")?;
                }
            }
        },
    }
    Ok(())
}

/// The complete `flusso.toml` schema, pretty-printed with a trailing newline:
/// the bytes committed at `libs/1-config/config.schema.json`.
pub(crate) fn config_schema_json() -> anyhow::Result<String> {
    let schema = assemble_config_schema();
    Ok(serde_json::to_string_pretty(&schema)?)
}

/// The base schema of the config entities with every port entry replaced by
/// the registered adapters' alternatives.
pub(crate) fn assemble_config_schema() -> Value {
    let mut root = schemars::generate::SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<config::toml::ConfigToml>()
        .to_value();
    let mut definitions = root
        .get("definitions")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    definitions.remove("PortEntry");

    let alternatives = |port: Port, definitions: &mut Map<String, Value>| -> Value {
        let refs: Vec<Value> = adapters::descriptions()
            .iter()
            .filter(|d| d.port == port)
            .map(|d| {
                let name = format!("{}_{}", d.port, d.kind);
                let entry = adapter_entry_schema(d, definitions);
                definitions.insert(name.clone(), entry);
                json!({ "$ref": format!("#/definitions/{name}") })
            })
            .collect();
        json!({ "oneOf": refs })
    };

    let source = alternatives(Port::Source, &mut definitions);
    let stream = alternatives(Port::Stream, &mut definitions);
    let sink = alternatives(Port::Sink, &mut definitions);

    if let Some(properties) = root.get_mut("properties").and_then(Value::as_object_mut) {
        for (key, schema) in [("source", source), ("stream", stream)] {
            if let Some(slot) = properties.get_mut(key) {
                let description = slot.get("description").cloned();
                *slot = schema;
                if let (Some(description), Some(object)) = (description, slot.as_object_mut()) {
                    object.insert("description".to_owned(), description);
                }
            }
        }
        if let Some(sinks) = properties.get_mut("sinks").and_then(Value::as_object_mut) {
            sinks.insert("additionalProperties".to_owned(), sink);
            sinks.insert(
                "propertyNames".to_owned(),
                json!({ "$ref": "#/definitions/SinkName" }),
            );
        }
    }
    if let Some(object) = root.as_object_mut() {
        object.insert("definitions".to_owned(), Value::Object(definitions));
    }
    root
}

/// One adapter's entry schema: its option schema with the `type` constant
/// added and required, and its own definitions hoisted into the root under
/// `<port>_<kind>_<Name>`. Definitions every adapter shares by identity
/// ([`SHARED_DEFINITIONS`]) keep their name, so the root has one of each.
fn adapter_entry_schema(
    description: &AdapterDescription,
    definitions: &mut Map<String, Value>,
) -> Value {
    let mut schema = description.schema.clone().to_value();
    let prefix = format!("{}_{}_", description.port, description.kind);
    let local: Map<String, Value> = schema
        .as_object_mut()
        .and_then(|o| o.remove("definitions"))
        .and_then(|d| d.as_object().cloned())
        .unwrap_or_default();
    for (name, definition) in local {
        let target = if SHARED_DEFINITIONS.contains(&name.as_str()) {
            name
        } else {
            format!("{prefix}{name}")
        };
        let mut definition = definition;
        rewrite_refs(&mut definition, &renames_placeholder(&prefix));
        definitions.insert(target, definition);
    }
    rewrite_refs(&mut schema, &renames_placeholder(&prefix));
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
        object.remove("title");
        object.insert("title".to_owned(), Value::String(description.kind.clone()));
        let properties = object
            .entry("properties")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(properties) = properties.as_object_mut() {
            let mut typed = Map::new();
            typed.insert(
                "type".to_owned(),
                json!({ "type": "string", "const": description.kind, "description": format!("Selects the {} {} adapter.", description.kind, description.port) }),
            );
            typed.extend(properties.clone());
            *properties = typed;
        }
        let required = object
            .entry("required")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(required) = required.as_array_mut() {
            required.insert(0, Value::String("type".to_owned()));
        }
        object.insert("additionalProperties".to_owned(), Value::Bool(false));
    }
    schema
}

/// Definitions every adapter shares by identity, kept unprefixed so the
/// assembled schema has one of each.
const SHARED_DEFINITIONS: &[&str] = &["Secret"];

/// The `$ref` rewrite for one adapter: `#/definitions/X` → prefixed, except
/// shared definitions.
fn renames_placeholder(prefix: &str) -> impl Fn(&str) -> Option<String> + '_ {
    move |reference: &str| {
        let name = reference.strip_prefix("#/definitions/")?;
        if SHARED_DEFINITIONS.contains(&name) {
            return None;
        }
        Some(format!("#/definitions/{prefix}{name}"))
    }
}

fn rewrite_refs(value: &mut Value, rename: &dyn Fn(&str) -> Option<String>) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(reference)) = object.get_mut("$ref")
                && let Some(renamed) = rename(reference)
            {
                *reference = renamed;
            }
            for child in object.values_mut() {
                rewrite_refs(child, rename);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_refs(item, rename);
            }
        }
        _ => {}
    }
}

/// One Markdown option table per registered adapter, as `(file name, content)`:
/// the Reference pages include these, so an option's description has exactly
/// one home, its doc comment.
pub(crate) fn adapter_docs() -> Vec<(String, String)> {
    adapters::descriptions()
        .iter()
        .map(|d| (format!("{}-{}.md", d.port, d.kind), render_table(d)))
        .collect()
}

fn render_table(description: &AdapterDescription) -> String {
    let schema = description.schema.as_value();
    let definitions = schema.get("definitions").and_then(Value::as_object);
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let entry = match description.port.singleton_entry() {
        Some(entry) => format!("`[{entry}]`"),
        None => "`[sinks.<name>]`".to_owned(),
    };
    let mut out = format!(
        "<!-- Generated by `flusso schema docs` from the {} adapter's config type; edit the doc comments there, then run `just schema-gen`. -->\n",
        description.kind
    );
    out.push_str("| Key | Type | Default | Meaning |\n| --- | --- | --- | --- |\n");
    out.push_str(&format!(
        "| `type` | `\"{}\"` | — | Required. Selects this adapter for {entry}. |\n",
        description.kind
    ));
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (key, property) in properties {
            let ty = render_type(property, definitions);
            let default = match property.get("default") {
                Some(Value::String(s)) => format!("`\"{s}\"`"),
                Some(Value::Null) => "none".to_owned(),
                Some(other) => format!("`{other}`"),
                None if required.contains(&key.as_str()) => "—".to_owned(),
                None => "none".to_owned(),
            };
            let mut meaning = String::new();
            if required.contains(&key.as_str()) {
                meaning.push_str("Required. ");
            }
            meaning.push_str(&one_line(
                property
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ));
            if description.secrets.iter().any(|s| s == key) {
                let var = kernel::override_var(
                    description.port.singleton_entry().unwrap_or("<NAME>"),
                    &description.kind,
                    key,
                );
                meaning.push_str(&format!(" `{var}` overrides or supplies it."));
            }
            out.push_str(&format!(
                "| `{key}` | {ty} | {default} | {} |\n",
                meaning.trim()
            ));
        }
    }
    out
}

/// A property's type in the Reference vocabulary (`string`, `bool`, enum
/// tokens, `string or { env }`, …).
fn render_type(property: &Value, definitions: Option<&Map<String, Value>>) -> String {
    // schemars wraps a `$ref` in a one-element `allOf` when the field carries
    // its own description; the type is the referenced one.
    if let Some([inner]) = property
        .get("allOf")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    {
        return render_type(inner, definitions);
    }
    if let Some(reference) = property.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or(reference);
        return match name {
            "Secret" => "string or `{ env }`".to_owned(),
            other => definitions
                .and_then(|d| d.get(other))
                .map(|d| render_type(d, definitions))
                .unwrap_or_else(|| format!("`{other}`")),
        };
    }
    if let Some(tokens) = property.get("enum").and_then(Value::as_array) {
        return tokens
            .iter()
            .filter_map(Value::as_str)
            .map(|t| format!("`{t}`"))
            .collect::<Vec<_>>()
            .join(" \\| ");
    }
    if let Some(alternatives) = property
        .get("anyOf")
        .or_else(|| property.get("oneOf"))
        .and_then(Value::as_array)
    {
        // schemars renders a unit-variant enum as one `const` alternative per
        // variant (so each keeps its doc comment); show them as tokens.
        let tokens: Vec<String> = alternatives
            .iter()
            .filter_map(|a| a.get("const").and_then(Value::as_str))
            .map(|t| format!("`{t}`"))
            .collect();
        if !tokens.is_empty() && tokens.len() == alternatives.len() {
            return tokens.join(" \\| ");
        }
        let rendered: Vec<String> = alternatives
            .iter()
            .filter(|a| a.get("type").and_then(Value::as_str) != Some("null"))
            .map(|a| render_type(a, definitions))
            .collect();
        return rendered.join(" or ");
    }
    match property.get("type") {
        Some(Value::String(t)) => scalar_type(t, property),
        Some(Value::Array(types)) => types
            .iter()
            .filter_map(Value::as_str)
            .filter(|t| *t != "null")
            .map(|t| scalar_type(t, property))
            .collect::<Vec<_>>()
            .join(" or "),
        _ => "value".to_owned(),
    }
}

fn scalar_type(json_type: &str, property: &Value) -> String {
    match json_type {
        "boolean" => "bool".to_owned(),
        "integer" => match property.get("minimum").and_then(Value::as_i64) {
            Some(min) if min > 0 => format!("int ≥ {min}"),
            _ => "int".to_owned(),
        },
        "number" => "float".to_owned(),
        "array" => "list".to_owned(),
        "object" => "table".to_owned(),
        other => other.to_owned(),
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf()
    }

    /// The committed editor schema is what `flusso schema config` prints.
    #[test]
    fn committed_config_schema_is_current() {
        let generated = config_schema_json().unwrap() + "\n";
        assert_eq!(
            config::CONFIG_SCHEMA,
            generated,
            "libs/1-config/config.schema.json is stale — run `just schema-gen` and commit the result"
        );
    }

    /// The committed Reference tables are what `flusso schema docs` renders.
    #[test]
    fn committed_adapter_docs_are_current() {
        let dir = repo_root().join("docs/src/reference/generated");
        for (file, table) in adapter_docs() {
            let committed = std::fs::read_to_string(dir.join(&file)).unwrap_or_default();
            assert_eq!(
                committed, table,
                "docs/src/reference/generated/{file} is stale — run `just schema-gen` and commit the result"
            );
        }
    }

    #[test]
    fn assembled_schema_has_one_alternative_per_adapter() {
        let schema = assemble_config_schema();
        let source = schema.pointer("/properties/source/oneOf").unwrap();
        assert_eq!(source.as_array().unwrap().len(), 1);
        let sinks = schema
            .pointer("/properties/sinks/additionalProperties/oneOf")
            .unwrap();
        assert_eq!(sinks.as_array().unwrap().len(), 2);
        let opensearch = schema.pointer("/definitions/sink_opensearch").unwrap();
        assert_eq!(
            opensearch.pointer("/properties/type/const"),
            Some(&json!("opensearch"))
        );
        assert_eq!(
            opensearch.get("additionalProperties"),
            Some(&Value::Bool(false))
        );
        assert!(
            opensearch
                .pointer("/properties/url/allOf/0/$ref")
                .and_then(Value::as_str)
                .is_some_and(|r| r.ends_with("/Secret"))
        );
        assert!(schema.pointer("/definitions/Secret").is_some());
        assert!(schema.pointer("/definitions/PortEntry").is_none());
        assert_eq!(
            schema.pointer("/properties/sinks/propertyNames/$ref"),
            Some(&json!("#/definitions/SinkName"))
        );
    }

    #[test]
    fn tables_start_with_the_type_row_and_mark_required_keys() {
        let docs = adapter_docs();
        let (_, opensearch) = docs
            .iter()
            .find(|(file, _)| file == "sink-opensearch.md")
            .unwrap();
        assert!(opensearch.contains("| `type` | `\"opensearch\"` | — | Required."));
        assert!(
            opensearch.contains("| `url` | string or `{ env }` | — | Required."),
            "{opensearch}"
        );
        assert!(opensearch.contains("`<NAME>_OPENSEARCH_URL` overrides"));
        assert!(opensearch.contains("| `batch_size` | int | `1000` |"));
        let (_, postgres) = docs
            .iter()
            .find(|(file, _)| file == "source-postgres.md")
            .unwrap();
        assert!(postgres.contains("`SOURCE_POSTGRES_CONNECTION_URL` overrides"));
        assert!(postgres.contains("`disable` \\| `prefer`"), "{postgres}");
    }
}
