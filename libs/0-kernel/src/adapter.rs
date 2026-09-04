//! What an adapter declares about itself: which port it implements, its kind
//! string, and the shape of its options.
//!
//! An adapter owns its configuration (ADR 0001). It writes one struct, derives
//! [`AdapterConfig`] on it, and from that single declaration the composition
//! root gets everything it renders: the editor JSON schema, the Reference
//! option tables, the designer's forms, and the environment variables that
//! override the adapter's secrets. Nothing in the kernel or the config layer
//! names the adapter.
//!
//! ```
//! use kernel::{AdapterConfig, Options, Port, Secret};
//! use schemars::JsonSchema;
//! use serde::{Deserialize, Serialize};
//!
//! /// Where the documents go.
//! #[derive(Debug, Serialize, Deserialize, JsonSchema)]
//! #[serde(deny_unknown_fields)]
//! struct DemoConfig {
//!     /// The cluster URL.
//!     url: Secret,
//!     /// Documents per request.
//!     #[serde(default = "default_batch")]
//!     batch_size: u32,
//! }
//! fn default_batch() -> u32 { 1000 }
//!
//! impl AdapterConfig for DemoConfig {
//!     const PORT: Port = Port::Sink;
//!     const KIND: &'static str = "demo";
//!     fn example() -> Self {
//!         Self { url: Secret::Value("https://search:9200".into()), batch_size: 500 }
//!     }
//! }
//!
//! let description = DemoConfig::description();
//! assert_eq!(description.kind, "demo");
//! // Every `Secret` field gets an override variable, named per entry.
//! let vars: Vec<String> = description.override_vars("primary").collect();
//! assert_eq!(vars, ["PRIMARY_DEMO_URL"]);
//!
//! let mut options = Options::empty();
//! options.insert("url", "https://other:9200");
//! let config = DemoConfig::from_options(options).unwrap();
//! assert_eq!(config.batch_size, 1000);
//! ```

use std::fmt;

use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::options::{Options, OptionsError};

/// The three contracts the engine drives. Every adapter implements exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Port {
    Source,
    Stream,
    Sink,
}

impl Port {
    /// The entry name of a singleton port in `flusso.toml`, also its `<NAME>`
    /// in override variables: `source` and `stream`. Sinks are named by the
    /// user, so this is `None` for them.
    pub fn singleton_entry(self) -> Option<&'static str> {
        match self {
            Port::Source => Some("source"),
            Port::Stream => Some("stream"),
            Port::Sink => None,
        }
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Port::Source => "source",
            Port::Stream => "stream",
            Port::Sink => "sink",
        })
    }
}

/// The schema-property marker a [`Secret`](crate::Secret) plants so a
/// description can find the fields that take an override variable.
pub const SECRET_MARKER: &str = "x-flusso-secret";

/// An adapter's configuration type: its port, its kind, and an example.
///
/// The provided [`description`](Self::description) and
/// [`from_options`](Self::from_options) are what the composition root uses;
/// adapters implement only the three required items, usually through
/// `#[derive(AdapterConfig)]`.
pub trait AdapterConfig: JsonSchema + Serialize + DeserializeOwned + Sized {
    /// Which port this adapter implements.
    const PORT: Port;

    /// The `type = "…"` token that selects this adapter in `flusso.toml`.
    const KIND: &'static str;

    /// A realistic, complete example, rendered into the docs and the schema.
    fn example() -> Self;

    /// Turn a port entry's options into this config, strictly.
    fn from_options(options: Options) -> Result<Self, OptionsError> {
        options.deserialize_into()
    }

    /// Everything the composition root renders from this declaration.
    fn description() -> AdapterDescription {
        // Draft-07 (`definitions`, no `$defs`): what TOML/YAML editor tooling
        // supports most widely, and the dialect the assembled config schema uses.
        let schema = schemars::generate::SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<Self>();
        let secrets = secret_paths(&schema);
        AdapterDescription {
            port: Self::PORT,
            kind: Self::KIND.to_owned(),
            schema,
            example: Options::from_serialize(&Self::example()).unwrap_or_default(),
            secrets,
        }
    }
}

/// The rendered facts about one adapter's options: the JSON schema (draft-07;
/// doc comments as descriptions, serde defaults as defaults), an example, and
/// the field paths that accept an override variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterDescription {
    /// Which port the adapter implements.
    pub port: Port,
    /// The adapter's `type` token.
    pub kind: String,
    /// The JSON schema of the options struct.
    pub schema: Schema,
    /// A complete example, as it would appear under the entry.
    pub example: Options,
    /// The `Secret` fields, as `.`-joined paths from the entry root.
    pub secrets: Vec<String>,
}

impl AdapterDescription {
    /// The override variable of every secret field for the entry named
    /// `entry` (a sink's name, or `source`/`stream` for the singletons).
    pub fn override_vars<'a>(&'a self, entry: &'a str) -> impl Iterator<Item = String> + 'a {
        self.secrets
            .iter()
            .map(move |path| override_var(entry, &self.kind, path))
    }
}

/// The environment variable that overrides one option of one port entry:
/// `<ENTRY>_<KIND>_<FIELD>`, uppercased, nested path segments joined with `_`.
///
/// ```
/// assert_eq!(kernel::override_var("primary", "opensearch", "url"), "PRIMARY_OPENSEARCH_URL");
/// assert_eq!(
///     kernel::override_var("source", "postgres", "connection_url.password"),
///     "SOURCE_POSTGRES_CONNECTION_URL_PASSWORD",
/// );
/// ```
pub fn override_var(entry: &str, kind: &str, field_path: &str) -> String {
    format!("{entry}_{kind}_{}", field_path.replace('.', "_")).to_uppercase()
}

/// Walk a schema's `properties` (through `$ref`s into `$defs`, `anyOf`/`oneOf`
/// alternatives, and `Option` wrappers) and collect the paths of every property
/// carrying [`SECRET_MARKER`].
fn secret_paths(schema: &Schema) -> Vec<String> {
    let root = schema.as_value();
    let defs = root
        .get("$defs")
        .or_else(|| root.get("definitions"))
        .and_then(serde_json::Value::as_object);
    let mut out = Vec::new();
    walk(root, defs, &mut Vec::new(), &mut out, 0);
    out.sort();
    out.dedup();
    out
}

fn walk(
    node: &serde_json::Value,
    defs: Option<&serde_json::Map<String, serde_json::Value>>,
    path: &mut Vec<String>,
    out: &mut Vec<String>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    if node.get(SECRET_MARKER).and_then(serde_json::Value::as_bool) == Some(true)
        && !path.is_empty()
    {
        out.push(path.join("."));
        return;
    }
    if let Some(reference) = node.get("$ref").and_then(serde_json::Value::as_str) {
        let name = reference.rsplit('/').next().unwrap_or(reference);
        if let Some(target) = defs.and_then(|d| d.get(name)) {
            walk(target, defs, path, out, depth + 1);
        }
        return;
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(alternatives) = node.get(key).and_then(serde_json::Value::as_array) {
            for alternative in alternatives {
                walk(alternative, defs, path, out, depth + 1);
            }
        }
    }
    if let Some(properties) = node
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (name, property) in properties {
            path.push(name.clone());
            walk(property, defs, path, out, depth + 1);
            path.pop();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Secret;

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Nested {
        password: Option<Secret>,
        host: String,
    }

    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Demo {
        url: Secret,
        #[serde(default)]
        token: Option<Secret>,
        connection: Nested,
        #[serde(default = "default_retries")]
        retries: u32,
    }

    fn default_retries() -> u32 {
        3
    }

    impl AdapterConfig for Demo {
        const PORT: Port = Port::Sink;
        const KIND: &'static str = "demo";

        fn example() -> Self {
            Self {
                url: Secret::Value("https://x".into()),
                token: None,
                connection: Nested {
                    password: Some(Secret::Env("PW".into())),
                    host: "db".into(),
                },
                retries: 5,
            }
        }
    }

    #[test]
    fn description_lists_secret_paths_including_nested_and_optional() {
        let description = Demo::description();
        assert_eq!(
            description.secrets,
            vec!["connection.password", "token", "url"]
        );
        let vars: Vec<String> = description.override_vars("primary").collect();
        assert_eq!(
            vars,
            [
                "PRIMARY_DEMO_CONNECTION_PASSWORD",
                "PRIMARY_DEMO_TOKEN",
                "PRIMARY_DEMO_URL"
            ]
        );
    }

    #[test]
    fn description_carries_defaults_and_docs_from_the_struct() {
        let description = Demo::description();
        let retries = description.schema.pointer("/properties/retries").unwrap();
        assert_eq!(retries.get("default"), Some(&serde_json::json!(3)));
        assert_eq!(
            description.example.get("retries").and_then(|v| v.as_i64()),
            Some(5)
        );
        assert_eq!(description.port, Port::Sink);
        assert_eq!(Port::Source.singleton_entry(), Some("source"));
        assert_eq!(Port::Sink.singleton_entry(), None);
        assert_eq!(Port::Stream.to_string(), "stream");
    }

    #[test]
    fn from_options_is_strict() {
        let mut options = Options::empty();
        options.insert("url", "https://x");
        options.insert("retriez", 1i64);
        let error = Demo::from_options(options).unwrap_err().to_string();
        assert!(error.contains("unknown field `retriez`"), "{error}");
    }
}
