//! Sub-document and opaque field handles: the flattened [`Object`], and the
//! non-searchable [`Binary`] / [`Json`] fields (existence, or a raw clause).

use std::marker::PhantomData;

use serde_json::Value;

use super::exists_q;
use crate::query::{Query, Root};

/// An `object` sub-document — a `group` or a to-one (`belongs_to`/`has_one`) join. Objects are
/// **flattened**, so their sub-fields are queried by their own scope-`S`
/// dotted-path handles directly (`Account::tier()`); this handle is for the
/// object itself. `S` is the enclosing scope (`Root` at the top level).
#[derive(Debug, Clone)]
pub struct Object<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Object<S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// The object is present — most useful on a nullable to-one join.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

/// A `binary` field — base64-encoded, not searchable. Only existence.
#[derive(Debug, Clone)]
pub struct Binary<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Binary<S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

/// An untyped `object`/`json` field. The escape hatch: existence, or a raw
/// clause spliced in verbatim.
#[derive(Debug, Clone)]
pub struct Json<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Json<S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Splice a raw OpenSearch query clause in verbatim.
    pub fn raw(&self, clause: Value) -> Query<S> {
        Query::leaf(clause)
    }
}
