use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize)]
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
