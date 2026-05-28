use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenericValue {
    Bool(bool),
    Int(i64),
    Decimal(Decimal),
    String(String),
    Array(Vec<GenericValue>),
    Map(BTreeMap<String, GenericValue>),
}
