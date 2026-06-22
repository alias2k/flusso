//! Scalar value fields: the [`Bool`] exact-match field and the ordered
//! [`Number`] / [`Date`] fields with their range operators.
//!
//! The value operators return small builders ([`EqQuery`], [`TermsQuery`],
//! [`RangeQuery`]) that carry the universal `boost` / `name` modifiers (and, for
//! ranges, `format` / `time_zone` / `relation`) and render lazily through
//! [`AsQuery`] — so they drop straight into a clause, with or
//! without options.

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Common, Sort, SortOrder, common_opts, exists_q, single, wrap};
use crate::query::{AsQuery, Query, Root};

/// An exact-match (`term`) clause for a non-string value (number, bool, date),
/// carrying the universal `boost` / `name` modifiers.
#[derive(Debug, Clone)]
pub struct EqQuery<S = Root> {
    path: String,
    value: Value,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> EqQuery<S> {
    fn new(path: &str, value: Value) -> Self {
        Self {
            path: path.to_string(),
            value,
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for EqQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        if self.common.is_empty() {
            Some(single("term", &self.path, self.value))
        } else {
            let mut body = Map::new();
            body.insert("value".to_string(), self.value);
            self.common.write(&mut body);
            Some(single("term", &self.path, Value::Object(body)))
        }
    }
}

/// A multi-value (`terms`) clause, carrying `boost` / `name`. Shared by the
/// keyword and numeric `any_of` operators.
#[derive(Debug, Clone)]
pub struct TermsQuery<S = Root> {
    path: String,
    values: Vec<Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> TermsQuery<S> {
    pub(crate) fn new(path: &str, values: Vec<Value>) -> Self {
        Self {
            path: path.to_string(),
            values,
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for TermsQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        // `terms` carries `boost` / `_name` beside the field, not inside it.
        let mut body = Map::new();
        body.insert(self.path, Value::Array(self.values));
        self.common.write(&mut body);
        Some(wrap("terms", body))
    }
}

/// A `range` clause with bounds plus the universal `boost` / `name` and the
/// range-specific `format` / `time_zone` / `relation` modifiers.
#[derive(Debug, Clone)]
pub struct RangeQuery<S = Root> {
    path: String,
    bounds: Vec<(&'static str, Value)>,
    extra: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> RangeQuery<S> {
    pub(crate) fn new(path: &str, bounds: Vec<(&'static str, Value)>) -> Self {
        Self {
            path: path.to_string(),
            bounds,
            extra: Map::new(),
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Date math / numeric `format` for the bounds (`date` fields).
    #[must_use]
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.extra
            .insert("format".to_string(), Value::String(format.into()));
        self
    }

    /// Time zone applied to the bounds (`date` fields), e.g. `"+01:00"`.
    #[must_use]
    pub fn time_zone(mut self, time_zone: impl Into<String>) -> Self {
        self.extra
            .insert("time_zone".to_string(), Value::String(time_zone.into()));
        self
    }

    /// How the range relates to range-typed field values
    /// (`INTERSECTS` / `CONTAINS` / `WITHIN`).
    #[must_use]
    pub fn relation(mut self, relation: impl Into<String>) -> Self {
        self.extra
            .insert("relation".to_string(), Value::String(relation.into()));
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for RangeQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.extra;
        for (key, value) in self.bounds {
            body.insert(key.to_string(), value);
        }
        self.common.write(&mut body);
        Some(single("range", &self.path, Value::Object(body)))
    }
}

/// A boolean field.
#[derive(Debug, Clone)]
pub struct Bool<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Bool<S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Exact match.
    pub fn eq(&self, value: bool) -> EqQuery<S> {
        EqQuery::new(&self.path, Value::Bool(value))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

/// A numeric field. `T` is the Rust scalar; `S` is the scope (defaults to
/// [`Root`], so `Number<i64>` is a root-scope handle).
#[derive(Debug, Clone)]
pub struct Number<T, S = Root> {
    path: String,
    _marker: PhantomData<fn() -> (T, S)>,
}

impl<T, S> Number<T, S>
where
    T: Into<Value> + Copy,
{
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// Exact match.
    pub fn eq(&self, value: T) -> EqQuery<S> {
        EqQuery::new(&self.path, value.into())
    }

    /// Match any of the given values.
    pub fn any_of(&self, values: impl IntoIterator<Item = T>) -> TermsQuery<S> {
        let array = values.into_iter().map(Into::into).collect();
        TermsQuery::new(&self.path, array)
    }

    /// Strictly less than `value`.
    pub fn lt(&self, value: T) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lt", value.into())])
    }

    /// Less than or equal to `value`.
    pub fn lte(&self, value: T) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lte", value.into())])
    }

    /// Strictly greater than `value`.
    pub fn gt(&self, value: T) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gt", value.into())])
    }

    /// Greater than or equal to `value`.
    pub fn gte(&self, value: T) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gte", value.into())])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: T, high: T) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gte", low.into()), ("lte", high.into())])
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    pub fn asc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Asc)
    }

    pub fn desc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Desc)
    }
}

/// A `date`/`timestamp` field. Bounds are ISO-8601 strings.
#[derive(Debug, Clone)]
pub struct Date<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Date<S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Exact match.
    pub fn eq(&self, value: impl Into<String>) -> EqQuery<S> {
        EqQuery::new(&self.path, Value::String(value.into()))
    }

    /// Match any of the given dates.
    pub fn any_of(&self, values: impl IntoIterator<Item = impl Into<String>>) -> TermsQuery<S> {
        let array = values
            .into_iter()
            .map(|v| Value::String(v.into()))
            .collect();
        TermsQuery::new(&self.path, array)
    }

    /// Strictly before `value`.
    pub fn lt(&self, value: impl Into<String>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lt", Value::String(value.into()))])
    }

    /// At or before `value`.
    pub fn lte(&self, value: impl Into<String>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lte", Value::String(value.into()))])
    }

    /// Strictly after `value`.
    pub fn gt(&self, value: impl Into<String>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gt", Value::String(value.into()))])
    }

    /// At or after `value`.
    pub fn gte(&self, value: impl Into<String>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gte", Value::String(value.into()))])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: impl Into<String>, high: impl Into<String>) -> RangeQuery<S> {
        RangeQuery::new(
            &self.path,
            vec![
                ("gte", Value::String(low.into())),
                ("lte", Value::String(high.into())),
            ],
        )
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    pub fn asc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Asc)
    }

    pub fn desc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Desc)
    }
}
