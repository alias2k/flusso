use std::collections::BTreeMap;
use std::fmt;

use rust_decimal::Decimal;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum GenericValue {
    Null,
    Bool(bool),
    Int(i64),
    Decimal(Decimal),
    String(String),
    Array(Vec<GenericValue>),
    Map(BTreeMap<String, GenericValue>),
}

/// Serializes to the **natural** JSON shape — `5`, `"x"`, `true`, `null`,
/// `[…]`, `{…}` — not serde's externally-tagged enum form (`{"Int": 5}`). This
/// is what makes a serialized `Config` or `IndexMapping` read like the data it
/// describes; a value's variant is evident from its JSON shape.
impl Serialize for GenericValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            GenericValue::Null => serializer.serialize_none(),
            GenericValue::Bool(value) => serializer.serialize_bool(*value),
            GenericValue::Int(value) => serializer.serialize_i64(*value),
            // `Decimal` has an inherent `serialize` (to bytes) that shadows the
            // trait method, so call the trait method explicitly.
            GenericValue::Decimal(value) => Serialize::serialize(value, serializer),
            GenericValue::String(value) => serializer.serialize_str(value),
            GenericValue::Array(items) => items.serialize(serializer),
            GenericValue::Map(map) => map.serialize(serializer),
        }
    }
}

/// Deserializes from the same **natural** shape it serializes to, by inspecting
/// the value's form rather than expecting serde's tagged-enum encoding. This
/// requires a self-describing format (JSON, MessagePack, …); it is what lets a
/// compiled config round-trip. A `Decimal` is written as a string by its serde
/// integration, so a decimal literal round-trips as a [`String`](GenericValue::String)
/// — a documented edge for the rare decimal `constant` / `default`.
impl<'de> Deserialize<'de> for GenericValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GenericValueVisitor;

        impl<'de> Visitor<'de> for GenericValueVisitor {
            type Value = GenericValue;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any JSON-like value")
            }

            fn visit_bool<E>(self, v: bool) -> Result<GenericValue, E> {
                Ok(GenericValue::Bool(v))
            }

            fn visit_i64<E>(self, v: i64) -> Result<GenericValue, E> {
                Ok(GenericValue::Int(v))
            }

            fn visit_u64<E>(self, v: u64) -> Result<GenericValue, E> {
                match i64::try_from(v) {
                    Ok(i) => Ok(GenericValue::Int(i)),
                    Err(_) => Ok(GenericValue::Decimal(Decimal::from(v))),
                }
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<GenericValue, E> {
                Decimal::try_from(v)
                    .map(GenericValue::Decimal)
                    .map_err(E::custom)
            }

            fn visit_str<E>(self, v: &str) -> Result<GenericValue, E> {
                Ok(GenericValue::String(v.to_owned()))
            }

            fn visit_string<E>(self, v: String) -> Result<GenericValue, E> {
                Ok(GenericValue::String(v))
            }

            fn visit_none<E>(self) -> Result<GenericValue, E> {
                Ok(GenericValue::Null)
            }

            fn visit_unit<E>(self) -> Result<GenericValue, E> {
                Ok(GenericValue::Null)
            }

            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<GenericValue, D::Error> {
                GenericValue::deserialize(d)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<GenericValue, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(GenericValue::Array(items))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GenericValue, A::Error> {
                let mut out = BTreeMap::new();
                while let Some((key, value)) = map.next_entry()? {
                    out.insert(key, value);
                }
                Ok(GenericValue::Map(out))
            }
        }

        deserializer.deserialize_any(GenericValueVisitor)
    }
}
