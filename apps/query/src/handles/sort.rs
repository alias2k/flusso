//! Sort keys: [`SortOrder`], [`SortMode`], and the [`Sort`] builder produced by
//! `.asc()` / `.desc()` on a sortable handle (or `Geo::distance_sort`,
//! [`Sort::score`], [`Sort::script`]).
//!
//! A [`Sort`] carries the key it sorts on (a field path, `_score`,
//! `_geo_distance`, or `_script`) plus its options (`missing`, `mode`,
//! `unmapped_type`, …); `.missing_first()` / `.mode(..)` chain onto it, and it
//! renders to one entry in the `sort` array. The typed handle is always the
//! entry point — there is no public string-path sort.

use serde_json::{Map, Value};

use crate::query::AsQuery;

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

/// How a multi-valued field collapses to one sort value.
#[derive(Debug, Clone, Copy)]
pub enum SortMode {
    /// Smallest value.
    Min,
    /// Largest value.
    Max,
    /// Arithmetic mean (numeric fields).
    Avg,
    /// Sum (numeric fields).
    Sum,
    /// Median (numeric fields).
    Median,
}

impl SortMode {
    fn as_str(self) -> &'static str {
        match self {
            SortMode::Min => "min",
            SortMode::Max => "max",
            SortMode::Avg => "avg",
            SortMode::Sum => "sum",
            SortMode::Median => "median",
        }
    }
}

/// A single sort key. Produced by `.asc()` / `.desc()` on a sortable handle, by
/// [`Sort::score`] / [`Sort::script`], or by `Geo::distance_sort`; chain the
/// option setters (`missing_first`, `mode`, `unmapped_type`, …) onto it.
#[derive(Debug, Clone)]
pub struct Sort {
    key: String,
    body: Map<String, Value>,
}

impl Sort {
    /// A field/order sort: `{ "<field>": { "order": "asc"|"desc" } }`.
    pub(crate) fn new(field: &str, order: SortOrder) -> Self {
        let mut body = Map::new();
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        Self {
            key: field.to_string(),
            body,
        }
    }

    /// Sort by relevance `_score` (descending by default).
    #[must_use]
    pub fn score() -> Self {
        let mut sort = Self {
            key: "_score".to_string(),
            body: Map::new(),
        };
        sort.body
            .insert("order".to_string(), Value::String("desc".to_string()));
        sort
    }

    /// Sort by a computed script value. `script_type` is the emitted value type
    /// (`"number"` / `"string"`); `source` is the painless expression.
    #[must_use]
    pub fn script(
        script_type: impl Into<String>,
        source: impl Into<String>,
        order: SortOrder,
    ) -> Self {
        let mut script = Map::new();
        script.insert("source".to_string(), Value::String(source.into()));
        let mut body = Map::new();
        body.insert("type".to_string(), Value::String(script_type.into()));
        body.insert("script".to_string(), Value::Object(script));
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        Self {
            key: "_script".to_string(),
            body,
        }
    }

    /// A pre-built sort clause (e.g. `_geo_distance`).
    pub(crate) fn from_parts(key: String, body: Map<String, Value>) -> Self {
        Self { key, body }
    }

    /// Sort ascending.
    #[must_use]
    pub fn asc(mut self) -> Self {
        self.body
            .insert("order".to_string(), Value::String("asc".to_string()));
        self
    }

    /// Sort descending.
    #[must_use]
    pub fn desc(mut self) -> Self {
        self.body
            .insert("order".to_string(), Value::String("desc".to_string()));
        self
    }

    /// Place documents missing this field first.
    #[must_use]
    pub fn missing_first(mut self) -> Self {
        self.body
            .insert("missing".to_string(), Value::String("_first".to_string()));
        self
    }

    /// Place documents missing this field last.
    #[must_use]
    pub fn missing_last(mut self) -> Self {
        self.body
            .insert("missing".to_string(), Value::String("_last".to_string()));
        self
    }

    /// Substitute a literal value for documents missing this field.
    #[must_use]
    pub fn missing(mut self, value: impl Into<Value>) -> Self {
        self.body.insert("missing".to_string(), value.into());
        self
    }

    /// How a multi-valued field reduces to one sort value.
    #[must_use]
    pub fn mode(mut self, mode: SortMode) -> Self {
        self.body
            .insert("mode".to_string(), Value::String(mode.as_str().to_string()));
        self
    }

    /// Type to assume when the field is unmapped on some shard (instead of
    /// failing the search), e.g. `"long"`.
    #[must_use]
    pub fn unmapped_type(mut self, unmapped_type: impl Into<String>) -> Self {
        self.body.insert(
            "unmapped_type".to_string(),
            Value::String(unmapped_type.into()),
        );
        self
    }

    /// Numeric type to sort as (`"double"` / `"long"` / `"date"` /
    /// `"date_nanos"`), for cross-index type coercion.
    #[must_use]
    pub fn numeric_type(mut self, numeric_type: impl Into<String>) -> Self {
        self.body.insert(
            "numeric_type".to_string(),
            Value::String(numeric_type.into()),
        );
        self
    }

    /// Date `format` for a `date` field sort.
    #[must_use]
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.body
            .insert("format".to_string(), Value::String(format.into()));
        self
    }

    /// Sort by a field inside a `nested` array, scoped to `path`.
    #[must_use]
    pub fn nested(mut self, path: impl Into<String>) -> Self {
        let mut nested = Map::new();
        nested.insert("path".to_string(), Value::String(path.into()));
        self.body
            .insert("nested".to_string(), Value::Object(nested));
        self
    }

    /// Sort by a field inside a `nested` array scoped to `path`, considering
    /// only elements matching `filter`.
    #[must_use]
    pub fn nested_filtered<S>(mut self, path: impl Into<String>, filter: impl AsQuery<S>) -> Self {
        let mut nested = Map::new();
        nested.insert("path".to_string(), Value::String(path.into()));
        if let Some(query) = filter.into_query() {
            nested.insert("filter".to_string(), query.to_value());
        }
        self.body
            .insert("nested".to_string(), Value::Object(nested));
        self
    }

    pub(crate) fn to_value(&self) -> Value {
        let mut outer = Map::new();
        outer.insert(self.key.clone(), Value::Object(self.body.clone()));
        Value::Object(outer)
    }
}
