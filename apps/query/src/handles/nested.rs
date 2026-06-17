//! Handles for `nested` arrays of objects: [`Nested`] (lifting element queries
//! into the enclosing scope) and [`NestedProjection`] (shaping the returned
//! array via `inner_hits`).

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Sort, exists_q, match_all_value};
use crate::query::{AsQuery, Query, Root};

/// `{ "nested": { "path": "<path>", "query": <query> } }`.
fn nested_value(path: &str, query: Value) -> Value {
    let mut body = Map::new();
    body.insert("path".to_string(), Value::String(path.to_string()));
    body.insert("query".to_string(), query);
    let mut outer = Map::new();
    outer.insert("nested".to_string(), Value::Object(body));
    Value::Object(outer)
}

/// `{ "bool": { "<clause>": [ … ] } }`.
fn bool_value(clause: &str, items: Vec<Value>) -> Value {
    let mut body = Map::new();
    body.insert(clause.to_string(), Value::Array(items));
    let mut outer = Map::new();
    outer.insert("bool".to_string(), Value::Object(body));
    Value::Object(outer)
}

/// A `nested` array of objects. `E` is the **enclosing** scope (where queries
/// over this array land — `Root` at the top level, the parent element type when
/// deeper); `C` is the **child** scope (the element type). Lifting a child query
/// (`Query<C>`) through `any`/`all` produces a `Query<E>`.
#[derive(Debug, Clone)]
pub struct Nested<E = Root, C = serde_json::Value> {
    path: String,
    _marker: PhantomData<fn() -> (E, C)>,
}

impl<E, C> Nested<E, C> {
    /// Build a handle for the nested array at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// Parents with **at least one** element matching `query`.
    pub fn any(&self, query: impl AsQuery<C>) -> Query<E> {
        let inner = query
            .into_query()
            .map_or_else(match_all_value, |q| q.to_value());
        Query::leaf(nested_value(&self.path, inner))
    }

    /// Parents where **every** element matches `query` ("no element fails it").
    pub fn all(&self, query: impl AsQuery<C>) -> Query<E> {
        let inner = query
            .into_query()
            .map_or_else(match_all_value, |q| q.to_value());
        let fails = bool_value("must_not", vec![inner]);
        let nested = nested_value(&self.path, fails);
        Query::leaf(bool_value("must_not", vec![nested]))
    }

    /// The nested array has at least one element.
    pub fn exists(&self) -> Query<E> {
        exists_q(&self.path)
    }

    /// Shape the **returned** array: keep elements matching `query` (with the
    /// builder's sort/size). Pass to [`crate::Search::filter_nested`].
    pub fn matching(&self, query: impl AsQuery<C>) -> NestedProjection {
        NestedProjection {
            path: self.path.clone(),
            query: query.into_query().map(|q| q.to_value()),
            sort: Vec::new(),
            size: None,
            from: None,
        }
    }

    /// Like [`matching`](Self::matching) with no predicate — every element.
    pub fn project(&self) -> NestedProjection {
        NestedProjection {
            path: self.path.clone(),
            query: None,
            sort: Vec::new(),
            size: None,
            from: None,
        }
    }
}

/// A request to shape one nested array in the results (via `inner_hits`).
#[derive(Debug, Clone)]
pub struct NestedProjection {
    path: String,
    query: Option<Value>,
    sort: Vec<Sort>,
    size: Option<u64>,
    from: Option<u64>,
}

impl NestedProjection {
    /// Order the returned elements.
    #[must_use]
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort.push(sort);
        self
    }

    /// Cap how many elements are returned per parent.
    #[must_use]
    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Offset within each parent's matching elements.
    #[must_use]
    pub fn from(mut self, from: u64) -> Self {
        self.from = Some(from);
        self
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    /// The `{ "nested": { path, query, inner_hits } }` clause (inner_hits named
    /// after the path, for retrieval).
    pub(crate) fn to_value(&self) -> Value {
        let query = self.query.clone().unwrap_or_else(match_all_value);
        let mut inner_hits = Map::new();
        inner_hits.insert("name".to_string(), Value::String(self.path.clone()));
        if let Some(size) = self.size {
            inner_hits.insert("size".to_string(), Value::from(size));
        }
        if let Some(from) = self.from {
            inner_hits.insert("from".to_string(), Value::from(from));
        }
        if !self.sort.is_empty() {
            let keys = self.sort.iter().map(Sort::to_value).collect();
            inner_hits.insert("sort".to_string(), Value::Array(keys));
        }
        let mut nested = Map::new();
        nested.insert("path".to_string(), Value::String(self.path.clone()));
        nested.insert("query".to_string(), query);
        nested.insert("inner_hits".to_string(), Value::Object(inner_hits));
        let mut outer = Map::new();
        outer.insert("nested".to_string(), Value::Object(nested));
        Value::Object(outer)
    }
}
