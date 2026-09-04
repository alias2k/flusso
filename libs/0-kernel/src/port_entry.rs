//! One configured port: which adapter (`type`) and its options.
//!
//! `[source]`, `[stream]`, and every `[sinks.<name>]` table in `flusso.toml`
//! is a [`PortEntry`]. The config layer reads the `type` and carries the rest
//! as an opaque [`Options`] map; the composition root looks the kind up in
//! its adapter registry and hands the options to that adapter's config type.
//! Serialized as the file writes it: `type = "…"` plus the options flattened
//! beside it, keys sorted, so the lock is byte-stable.
//!
//! ```
//! use kernel::PortEntry;
//!
//! let entry: PortEntry = toml::from_str(r#"
//!     type = "opensearch"
//!     url = "https://search:9200"
//!     batch_size = 500
//! "#).unwrap();
//! assert_eq!(entry.kind, "opensearch");
//! assert_eq!(entry.options.get("batch_size").and_then(|v| v.as_i64()), Some(500));
//! assert_eq!(toml::to_string(&entry).unwrap(), "type = \"opensearch\"\nbatch_size = 500\nurl = \"https://search:9200\"\n");
//! ```

use serde::{Deserialize, Serialize};

use crate::options::Options;

/// One port entry of a deployment: the adapter kind and its options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortEntry {
    /// The adapter that implements the port, as written in `type = "…"`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Everything else under the table, uninterpreted here.
    #[serde(flatten)]
    pub options: Options,
}

impl PortEntry {
    /// An entry of `kind` with no options, so every adapter default applies.
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            options: Options::empty(),
        }
    }

    /// An entry of `kind` with the given options.
    pub fn with_options(kind: impl Into<String>, options: Options) -> Self {
        Self {
            kind: kind.into(),
            options,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn type_alone_is_an_empty_options_map() {
        let entry: PortEntry = toml::from_str("type = \"stdout\"\n").unwrap();
        assert_eq!(entry, PortEntry::new("stdout"));
        assert!(entry.options.is_empty());
    }

    #[test]
    fn missing_type_is_an_error() {
        let error = toml::from_str::<PortEntry>("pretty = true\n").unwrap_err();
        assert!(error.to_string().contains("type"), "{error}");
    }

    #[test]
    fn nested_tables_round_trip() {
        let text = "type = \"postgres\"\n\n[connection_url]\nhost = \"db\"\nport = 5432\n";
        let entry: PortEntry = toml::from_str(text).unwrap();
        let again = toml::to_string(&entry).unwrap();
        assert_eq!(again, text);
    }
}

/// The uninterpreted shape: a `type` plus any table. The composition root
/// replaces this with one alternative per registered adapter when it assembles
/// the editor schema.
impl schemars::JsonSchema for PortEntry {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("PortEntry")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "object",
            "required": ["type"],
            "properties": {
                "type": { "type": "string", "description": "The adapter that implements this port." }
            },
            "additionalProperties": true
        })
    }
}
