//! Sort keys: [`SortOrder`] and the scope-free [`Sort`] clause produced by
//! `.asc()` / `.desc()` on a sortable handle (or `Geo::distance_sort`).

use serde_json::{Map, Value};

/// Sort direction.
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

impl SortOrder {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }
}

/// A single sort key, produced by `.asc()` / `.desc()` on a sortable handle (or
/// `Geo::distance_sort`). Scope-free.
#[derive(Debug, Clone)]
pub struct Sort {
    value: Value,
}

impl Sort {
    /// A field/order sort: `{ "<field>": { "order": "asc"|"desc" } }`.
    pub(crate) fn new(field: &str, order: SortOrder) -> Self {
        let mut order_body = Map::new();
        order_body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        let mut outer = Map::new();
        outer.insert(field.to_string(), Value::Object(order_body));
        Self {
            value: Value::Object(outer),
        }
    }

    /// A pre-built sort clause (e.g. `_geo_distance`).
    pub(crate) fn raw(value: Value) -> Self {
        Self { value }
    }

    pub(crate) fn to_value(&self) -> Value {
        self.value.clone()
    }
}
