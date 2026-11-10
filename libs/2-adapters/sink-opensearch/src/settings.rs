//! User-supplied index settings: the `index_settings` table on a sink, and the
//! same table per index under `[[index]]`'s `sinks.<name>`.
//!
//! [`IndexSettings`] is a flat map of dotted OpenSearch setting keys. It is
//! normalized and checked as it deserializes, so a bad table fails at
//! `build`/`check`/`run`, never at index creation:
//!
//! - nested tables flatten to dotted keys (`mapping = { total_fields = { limit
//!   = 5000 } }` is `index.mapping.total_fields.limit`);
//! - a key without the `index.` prefix gets it, as OpenSearch itself reads it;
//! - a key flusso owns is rejected: `index.refresh_interval` (the sink's
//!   `refresh_interval` option sets it), anything under `index.analysis` (the
//!   `flusso_*` analyzers) or `index.blocks` (flusso must write), and
//!   `index.hidden` (flusso lists its generations by wildcard);
//! - the same key written twice, a key that is both a value and a table, an
//!   empty table, and `null` are rejected.
//!
//! Layers merge with [`IndexSettings::layered`]: the upper layer's key replaces
//! the lower's, along with any key nested under or above it. The index body
//! builder lays the typed shard counts under the sink-wide settings, those
//! under the per-index ones, and flusso's owned keys over all of them.
//!
//! ```
//! use kernel::Options;
//! use sink_opensearch::OpensearchIndexConfig;
//!
//! let options: Options = toml::from_str(r#"
//!     index_settings = { mapping = { total_fields = { limit = 5000 } } }
//! "#).unwrap();
//! let config: OpensearchIndexConfig = options.deserialize_into().unwrap();
//! let limit = config.index_settings.get("index.mapping.total_fields.limit");
//! assert_eq!(limit, Some(&serde_json::json!(5000)));
//!
//! let owned: Options = toml::from_str(r#"index_settings = { refresh_interval = "1s" }"#).unwrap();
//! let error = owned.deserialize_into::<OpensearchIndexConfig>().unwrap_err().to_string();
//! assert!(error.contains("index.refresh_interval"), "{error}");
//! ```

use std::borrow::Cow;
use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The prefix every index-level setting key carries.
const INDEX_PREFIX: &str = "index.";

/// Setting keys flusso sets itself, with the reason given when a user sets one.
const OWNED: &[(&str, &str)] = &[
    (
        "index.refresh_interval",
        "flusso seeds with refresh off and then applies the sink's `refresh_interval` option; set that instead",
    ),
    (
        "index.analysis",
        "flusso defines the `flusso_*` analyzers and normalizers itself",
    ),
    (
        "index.blocks",
        "a block stops flusso's writes, alias swaps, and settings updates",
    ),
    (
        "index.hidden",
        "flusso finds its generations by wildcard, which skips hidden indexes",
    ),
];

/// A flat map of OpenSearch index settings, keyed by their full dotted name
/// (`index.mapping.total_fields.limit`). See the module docs for how a table
/// is normalized and what it rejects.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "BTreeMap<String, Value>", into = "BTreeMap<String, Value>")]
pub struct IndexSettings(BTreeMap<String, Value>);

impl IndexSettings {
    /// One setting by its full dotted key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// Whether no setting is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// These settings with `upper` laid over them: an upper key replaces the
    /// same key and any key nested under or above it.
    #[must_use]
    pub fn layered(&self, upper: &IndexSettings) -> IndexSettings {
        let mut merged = self.0.clone();
        for (key, value) in &upper.0 {
            merged.retain(|existing, _| !overlaps(existing, key));
            merged.insert(key.clone(), value.clone());
        }
        IndexSettings(merged)
    }

    /// The settings as the nested JSON object OpenSearch's `settings` takes.
    pub(crate) fn to_nested(&self) -> Map<String, Value> {
        let mut root = Map::new();
        for (key, value) in &self.0 {
            insert_nested(&mut root, key, value.clone());
        }
        root
    }

    /// Settings with no validation, for the typed options flusso maps itself.
    pub(crate) fn from_trusted(entries: impl IntoIterator<Item = (String, Value)>) -> Self {
        IndexSettings(entries.into_iter().collect())
    }
}

/// Two dotted keys name the same setting, or one is a table holding the other.
fn overlaps(a: &str, b: &str) -> bool {
    a == b || is_under(a, b) || is_under(b, a)
}

fn is_under(key: &str, table: &str) -> bool {
    key.strip_prefix(table)
        .is_some_and(|rest| rest.starts_with('.'))
}

/// Walk `key`'s segments, creating tables, and set the leaf. A key that is
/// both a value and a table never reaches here: deserialization rejects it and
/// [`IndexSettings::layered`] keeps only one of the two.
fn insert_nested(root: &mut Map<String, Value>, key: &str, value: Value) {
    let mut segments = key.split('.').peekable();
    let mut table = root;
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            table.insert(segment.to_owned(), value);
            return;
        }
        let entry = table
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let Value::Object(next) = entry else { return };
        table = next;
    }
}

impl TryFrom<BTreeMap<String, Value>> for IndexSettings {
    type Error = String;

    fn try_from(table: BTreeMap<String, Value>) -> Result<Self, String> {
        let mut flat = BTreeMap::new();
        for (key, value) in table {
            flatten(&key, value, &mut flat)?;
        }
        for a in flat.keys() {
            if let Some(b) = flat.keys().find(|b| is_under(b, a)) {
                return Err(format!(
                    "index setting `{a}` is both a value and a table (`{b}`)"
                ));
            }
        }
        for key in flat.keys() {
            if let Some((owned, why)) = OWNED.iter().find(|(owned, _)| overlaps(key, owned)) {
                return Err(format!(
                    "index setting `{key}` is owned by flusso (`{owned}`): {why}"
                ));
            }
        }
        Ok(IndexSettings(flat))
    }
}

/// Flatten one table entry into `out` under its full `index.`-prefixed key.
fn flatten(key: &str, value: Value, out: &mut BTreeMap<String, Value>) -> Result<(), String> {
    if key.is_empty() || key.split('.').any(str::is_empty) {
        return Err(format!("index setting key `{key}` has an empty segment"));
    }
    match value {
        Value::Object(table) if table.is_empty() => {
            Err(format!("index setting `{key}` is an empty table"))
        }
        Value::Object(table) => {
            for (child, value) in table {
                flatten(&format!("{key}.{child}"), value, out)?;
            }
            Ok(())
        }
        Value::Null => Err(format!("index setting `{key}` is null")),
        value => {
            let full = if key.starts_with(INDEX_PREFIX) {
                key.to_owned()
            } else {
                format!("{INDEX_PREFIX}{key}")
            };
            if out.insert(full.clone(), value).is_some() {
                return Err(format!("index setting `{full}` is given twice"));
            }
            Ok(())
        }
    }
}

impl From<IndexSettings> for BTreeMap<String, Value> {
    fn from(settings: IndexSettings) -> Self {
        settings.0
    }
}

impl JsonSchema for IndexSettings {
    fn schema_name() -> Cow<'static, str> {
        "IndexSettings".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "object",
            "additionalProperties": true,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(value: Value) -> Result<IndexSettings, String> {
        serde_json::from_value::<IndexSettings>(value).map_err(|e| e.to_string())
    }

    #[test]
    fn nested_and_dotted_keys_normalize_to_full_dotted_keys() {
        let settings = parse(json!({
            "mapping.total_fields.limit": 5000,
            "index": { "mapping": { "nested_fields": { "limit": 100 } } },
            "max_result_window": 20000,
        }))
        .unwrap();
        assert_eq!(
            BTreeMap::from(settings),
            BTreeMap::from([
                ("index.mapping.nested_fields.limit".to_owned(), json!(100)),
                ("index.mapping.total_fields.limit".to_owned(), json!(5000)),
                ("index.max_result_window".to_owned(), json!(20000)),
            ])
        );
    }

    #[test]
    fn owned_keys_are_rejected_with_the_alternative() {
        let error = parse(json!({ "refresh_interval": "1s" })).unwrap_err();
        assert!(error.contains("`index.refresh_interval`"), "{error}");
        assert!(error.contains("`refresh_interval` option"), "{error}");
        let error = parse(json!({ "analysis": { "analyzer": { "x": { "type": "standard" } } } }))
            .unwrap_err();
        assert!(error.contains("index.analysis"), "{error}");
        let error = parse(json!({ "index.analysis.filter.f.type": "lowercase" })).unwrap_err();
        assert!(error.contains("owned by flusso"), "{error}");
    }

    #[test]
    fn duplicate_conflicting_null_and_empty_keys_are_rejected() {
        let error = parse(json!({ "mapping.x": 1, "index.mapping.x": 2 })).unwrap_err();
        assert!(error.contains("given twice"), "{error}");
        let error = parse(json!({ "mapping": 1, "mapping.x": 2 })).unwrap_err();
        assert!(error.contains("both a value and a table"), "{error}");
        let error = parse(json!({ "mapping.x": null })).unwrap_err();
        assert!(error.contains("is null"), "{error}");
        let error = parse(json!({ "mapping": {} })).unwrap_err();
        assert!(error.contains("empty table"), "{error}");
        let error = parse(json!({ "mapping..x": 1 })).unwrap_err();
        assert!(error.contains("empty segment"), "{error}");
    }

    #[test]
    fn an_upper_layer_replaces_the_same_key_and_overlapping_tables() {
        let lower = parse(json!({ "a": 1, "b.c": 2, "d": 3 })).unwrap();
        let upper = parse(json!({ "a": 10, "b": 20, "d.e": 30 })).unwrap();
        assert_eq!(
            BTreeMap::from(lower.layered(&upper)),
            BTreeMap::from([
                ("index.a".to_owned(), json!(10)),
                ("index.b".to_owned(), json!(20)),
                ("index.d.e".to_owned(), json!(30)),
            ])
        );
    }

    #[test]
    fn nests_back_into_the_settings_shape() {
        let settings =
            parse(json!({ "mapping.total_fields.limit": 5000, "number_of_shards": 2 })).unwrap();
        assert_eq!(
            Value::Object(settings.to_nested()),
            json!({ "index": { "mapping": { "total_fields": { "limit": 5000 } }, "number_of_shards": 2 } })
        );
    }

    #[test]
    fn serializes_back_to_an_equal_table() {
        let settings = parse(json!({ "mapping": { "total_fields.limit": 5000 } })).unwrap();
        let round = parse(serde_json::to_value(&settings).unwrap()).unwrap();
        assert_eq!(round, settings);
    }
}
