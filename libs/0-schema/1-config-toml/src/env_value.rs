use std::fmt;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};

/// A config value given either literally or as `{ env = "VAR" }`. Parsing keeps
/// the distinction; the core model carries it as a [`Secret`](schema_core::Secret)
/// and resolves it at runtime.
///
/// `Deserialize` is written by hand (rather than `#[serde(untagged)]`) so a wrong
/// shape reports what was actually expected — "a string or a `{ env = "VAR" }`
/// reference" — instead of serde's opaque "data did not match any variant of
/// untagged enum".
#[derive(Debug, Clone)]
pub enum EnvOrValue {
    Env { env: String },
    Value(String),
}

/// The accepted shapes, named once so every error message stays consistent.
const EXPECTED: &str = "a string value or an env reference `{ env = \"VAR\" }`";

impl Serialize for EnvOrValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            EnvOrValue::Value(value) => serializer.serialize_str(value),
            EnvOrValue::Env { env } => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("env", env)?;
                map.end()
            }
        }
    }
}

struct EnvOrValueVisitor;

impl<'de> Visitor<'de> for EnvOrValueVisitor {
    type Value = EnvOrValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(EXPECTED)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(EnvOrValue::Value(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(EnvOrValue::Value(value))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let Some(key) = map.next_key::<String>()? else {
            return Err(de::Error::custom(format!(
                "expected {EXPECTED}, found an empty table"
            )));
        };
        if key != "env" {
            return Err(de::Error::custom(format!(
                "unknown key `{key}` — expected {EXPECTED}"
            )));
        }
        let env: String = map.next_value()?;
        if let Some(extra) = map.next_key::<String>()? {
            return Err(de::Error::custom(format!(
                "unexpected key `{extra}` in env reference — write it as `{{ env = \"VAR\" }}`"
            )));
        }
        Ok(EnvOrValue::Env { env })
    }
}

impl<'de> serde::Deserialize<'de> for EnvOrValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(EnvOrValueVisitor)
    }
}
