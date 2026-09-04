//! The neutral, ordered value tree a port entry carries between the config
//! layer and the adapter that understands it.
//!
//! The config crate reads `[source]`, `[stream]`, and `[sinks.<name>]` as a
//! `type` plus **whatever else is there**, without interpreting it. That
//! remainder is an [`Options`] map. The adapter for that `type` turns it into
//! its own config struct with [`Options::deserialize_into`], where serde's
//! `deny_unknown_fields` rejects a typo. The kernel therefore names no adapter
//! and no file format, yet every option still validates strictly, one layer
//! later.
//!
//! [`OptionValue`] is one node of the tree: a scalar, an array, or a map with
//! sorted keys. Sorted keys are what make a compiled lock byte-stable.
//!
//! ```
//! use kernel::Options;
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize)]
//! #[serde(deny_unknown_fields)]
//! struct Channel { capacity: usize }
//!
//! let options: Options = toml::from_str(r#"capacity = 512"#).unwrap();
//! let channel: Channel = options.clone().deserialize_into().unwrap();
//! assert_eq!(channel.capacity, 512);
//!
//! let typo: Options = toml::from_str(r#"capacty = 512"#).unwrap();
//! let error = typo.deserialize_into::<Channel>().unwrap_err().to_string();
//! assert!(error.contains("unknown field `capacty`"), "{error}");
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::{Deserialize, Deserializer, Serialize};

/// The options of one port entry: everything under `[source]`, `[stream]`, or
/// `[sinks.<name>]` except its `type`, keyed in sorted order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Options(pub BTreeMap<String, OptionValue>);

impl Options {
    /// An empty options map: the entry's `type` with no settings, so every
    /// adapter default applies.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether no option was given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Read one top-level option.
    pub fn get(&self, key: &str) -> Option<&OptionValue> {
        self.0.get(key)
    }

    /// Set one top-level option, replacing an existing value. Used by the
    /// composition root to lay a flag or environment override over the file.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<OptionValue>) {
        self.0.insert(key.into(), value.into());
    }

    /// Turn the tree into an adapter's typed config. The adapter's struct
    /// decides the strictness (`deny_unknown_fields`, defaults, required
    /// fields); an error names the offending field.
    pub fn deserialize_into<T: DeserializeOwned>(self) -> Result<T, OptionsError> {
        serde_json::from_value(OptionValue::Map(self.0).into_json()).map_err(OptionsError)
    }

    /// Build the tree from any serializable struct, typically an adapter's
    /// config: the reverse of [`deserialize_into`](Self::deserialize_into),
    /// used to render an adapter's example and to write a config back out.
    /// Fails if the value is not a map at the top level.
    pub fn from_serialize<T: Serialize>(value: &T) -> Result<Self, OptionsError> {
        match OptionValue::from_json(serde_json::to_value(value).map_err(OptionsError)?) {
            OptionValue::Map(map) => Ok(Self(map)),
            other => Err(OptionsError(de::Error::custom(format!(
                "expected a map of options, got {}",
                other.kind()
            )))),
        }
    }
}

impl FromIterator<(String, OptionValue)> for Options {
    fn from_iter<I: IntoIterator<Item = (String, OptionValue)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// One node of an options tree.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<OptionValue>),
    Map(BTreeMap<String, OptionValue>),
}

impl OptionValue {
    /// The node's kind, for error messages.
    pub fn kind(&self) -> &'static str {
        match self {
            OptionValue::Null => "null",
            OptionValue::Bool(_) => "a boolean",
            OptionValue::Integer(_) => "an integer",
            OptionValue::Float(_) => "a float",
            OptionValue::String(_) => "a string",
            OptionValue::Array(_) => "an array",
            OptionValue::Map(_) => "a table",
        }
    }

    /// The string inside, if this is a string node.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptionValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// The boolean inside, if this is a boolean node.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            OptionValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The integer inside, if this is an integer node.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            OptionValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// The map inside, if this is a table node.
    pub fn as_map(&self) -> Option<&BTreeMap<String, OptionValue>> {
        match self {
            OptionValue::Map(m) => Some(m),
            _ => None,
        }
    }

    fn into_json(self) -> serde_json::Value {
        match self {
            OptionValue::Null => serde_json::Value::Null,
            OptionValue::Bool(b) => serde_json::Value::Bool(b),
            OptionValue::Integer(i) => serde_json::Value::from(i),
            OptionValue::Float(f) => serde_json::Value::from(f),
            OptionValue::String(s) => serde_json::Value::String(s),
            OptionValue::Array(items) => {
                serde_json::Value::Array(items.into_iter().map(Self::into_json).collect())
            }
            OptionValue::Map(map) => serde_json::Value::Object(
                map.into_iter().map(|(k, v)| (k, v.into_json())).collect(),
            ),
        }
    }

    fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => OptionValue::Null,
            serde_json::Value::Bool(b) => OptionValue::Bool(b),
            serde_json::Value::Number(n) => match n.as_i64() {
                Some(i) => OptionValue::Integer(i),
                None => OptionValue::Float(n.as_f64().unwrap_or(f64::NAN)),
            },
            serde_json::Value::String(s) => OptionValue::String(s),
            serde_json::Value::Array(items) => {
                OptionValue::Array(items.into_iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(map) => OptionValue::Map(
                map.into_iter()
                    .map(|(k, v)| (k, Self::from_json(v)))
                    .collect(),
            ),
        }
    }
}

impl From<bool> for OptionValue {
    fn from(value: bool) -> Self {
        OptionValue::Bool(value)
    }
}

impl From<i64> for OptionValue {
    fn from(value: i64) -> Self {
        OptionValue::Integer(value)
    }
}

impl From<u32> for OptionValue {
    fn from(value: u32) -> Self {
        OptionValue::Integer(i64::from(value))
    }
}

impl From<usize> for OptionValue {
    fn from(value: usize) -> Self {
        OptionValue::Integer(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl From<&str> for OptionValue {
    fn from(value: &str) -> Self {
        OptionValue::String(value.to_owned())
    }
}

impl From<String> for OptionValue {
    fn from(value: String) -> Self {
        OptionValue::String(value)
    }
}

impl From<Options> for OptionValue {
    fn from(value: Options) -> Self {
        OptionValue::Map(value.0)
    }
}

impl Serialize for OptionValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            OptionValue::Null => serializer.serialize_unit(),
            OptionValue::Bool(b) => serializer.serialize_bool(*b),
            OptionValue::Integer(i) => serializer.serialize_i64(*i),
            OptionValue::Float(f) => serializer.serialize_f64(*f),
            OptionValue::String(s) => serializer.serialize_str(s),
            OptionValue::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            OptionValue::Map(map) => {
                let mut out = serializer.serialize_map(Some(map.len()))?;
                for (key, value) in map {
                    out.serialize_entry(key, value)?;
                }
                out.end()
            }
        }
    }
}

struct OptionValueVisitor;

impl<'de> Visitor<'de> for OptionValueVisitor {
    type Value = OptionValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a config value (string, number, boolean, array, or table)")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(OptionValue::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(OptionValue::Integer(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        i64::try_from(v)
            .map(OptionValue::Integer)
            .map_err(|_| de::Error::custom(format!("integer {v} is out of range")))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(OptionValue::Float(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(OptionValue::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(OptionValue::String(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(OptionValue::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(OptionValue::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(OptionValue::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut out = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, OptionValue>()? {
            out.insert(key, value);
        }
        Ok(OptionValue::Map(out))
    }
}

impl<'de> Deserialize<'de> for OptionValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(OptionValueVisitor)
    }
}

/// An options tree could not be turned into an adapter's config (or back): a
/// missing or unknown field, a wrong type, an out-of-range number. The message
/// names the field.
#[derive(Debug)]
pub struct OptionsError(serde_json::Error);

impl fmt::Display for OptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for OptionsError {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Sample {
        url: String,
        #[serde(default = "default_batch")]
        batch: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pipeline: Option<String>,
        #[serde(default)]
        tags: Vec<String>,
    }

    fn default_batch() -> u32 {
        1000
    }

    #[test]
    fn deserializes_with_defaults_and_nested_values() {
        let options: Options =
            toml::from_str("url = \"http://x\"\ntags = [\"a\", \"b\"]\n").unwrap();
        let sample: Sample = options.deserialize_into().unwrap();
        assert_eq!(
            sample,
            Sample {
                url: "http://x".into(),
                batch: 1000,
                pipeline: None,
                tags: vec!["a".into(), "b".into()],
            }
        );
    }

    #[test]
    fn unknown_field_is_named() {
        let options: Options = toml::from_str("url = \"http://x\"\nbatch_sizee = 3\n").unwrap();
        let error = options
            .deserialize_into::<Sample>()
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field `batch_sizee`"), "{error}");
    }

    #[test]
    fn missing_field_is_named() {
        let options: Options = toml::from_str("batch = 3\n").unwrap();
        let error = options
            .deserialize_into::<Sample>()
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing field `url`"), "{error}");
    }

    #[test]
    fn round_trips_through_serialize() {
        let sample = Sample {
            url: "http://x".into(),
            batch: 5,
            pipeline: Some("enrich".into()),
            tags: vec![],
        };
        let options = Options::from_serialize(&sample).unwrap();
        assert_eq!(options.get("batch"), Some(&OptionValue::Integer(5)));
        assert_eq!(
            options.get("pipeline").and_then(OptionValue::as_str),
            Some("enrich")
        );
        let back: Sample = options.deserialize_into().unwrap();
        assert_eq!(back, sample);
    }

    #[test]
    fn keys_serialize_sorted() {
        let mut options = Options::empty();
        options.insert("zeta", 1i64);
        options.insert("alpha", "a");
        options.insert("mid", true);
        let toml = toml::to_string(&options).unwrap();
        assert_eq!(toml, "alpha = \"a\"\nmid = true\nzeta = 1\n");
    }

    #[test]
    fn nested_tables_and_null_survive() {
        let options: Options = toml::from_str("[inner]\nkey = \"v\"\n").unwrap();
        let inner = options.get("inner").and_then(OptionValue::as_map).unwrap();
        assert_eq!(inner.get("key").and_then(OptionValue::as_str), Some("v"));
        assert_eq!(OptionValue::Null.kind(), "null");
    }

    #[test]
    fn from_serialize_rejects_non_maps() {
        let error = Options::from_serialize(&7).unwrap_err().to_string();
        assert!(error.contains("expected a map of options"), "{error}");
    }
}

/// An options map is "any table" to the schema: the adapter's own schema is
/// what constrains it, spliced in by the composition root per `type`.
impl schemars::JsonSchema for Options {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Options")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "object" })
    }
}

impl schemars::JsonSchema for OptionValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("OptionValue")
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!(true)
    }
}
