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

use super::{
    Common, FlussoValue, RangeRelation, Sort, SortOrder, common_opts, exists_q, kind, single, wrap,
};
use crate::query::{AsQuery, Query, Root};

/// The JSON value for a typed date input, taken from its serde serialization
/// (`String`/`&str` pass straight through; `chrono` types serialize to their
/// ISO-8601 string). Mirrors `keyword_term` — a non-string serialization falls
/// back to its display form rather than failing.
fn date_value(value: &impl FlussoValue<kind::Date>) -> Value {
    match serde_json::to_value(value) {
        Ok(Value::String(string)) => Value::String(string),
        Ok(other) => Value::String(other.to_string()),
        Err(_) => Value::String(String::new()),
    }
}

/// The JSON value for a numeric input, from its serde serialization. The
/// primitives serialize straight to a JSON number; `rust_decimal::Decimal`
/// serializes to a string (the workspace's `serde-with-str`), so parse it back
/// to a number — the field is numeric, so a clean number is what it queries.
/// Generic over the numeric kind `K` (`Byte`…`Decimal`).
fn number_value<K>(value: &impl FlussoValue<K>) -> Value {
    match serde_json::to_value(value) {
        Ok(Value::String(string)) => string
            .parse::<serde_json::Number>()
            .map_or(Value::String(string), Value::Number),
        Ok(other) => other,
        Err(_) => Value::Null,
    }
}

/// The JSON value for a boolean input, from its serde serialization (`bool` →
/// `Value::Bool`; a bool newtype serializes through to the same).
fn bool_value(value: &impl FlussoValue<kind::Bool>) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

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
    /// ([`RangeRelation::Intersects`] / `Contains` / `Within`).
    #[must_use]
    pub fn relation(mut self, relation: RangeRelation) -> Self {
        self.extra.insert(
            "relation".to_string(),
            Value::String(relation.as_str().to_string()),
        );
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

    /// Exact match. Accepts a `bool`, or a `#[derive(FlussoValue)]` bool newtype.
    pub fn eq(&self, value: impl FlussoValue<kind::Bool>) -> EqQuery<S> {
        EqQuery::new(&self.path, bool_value(&value))
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

/// A numeric field. `K` is the numeric kind ([`kind::Byte`]…[`kind::Decimal`]),
/// `S` the scope. Value operators accept any value of that kind — the matching
/// primitive, a losslessly-widening one (`i32` on a `Long`/`Double`/`Decimal`
/// field), `rust_decimal::Decimal` (`decimal` feature, on a `Decimal` field), or
/// a `#[derive(FlussoValue)]` numeric newtype — so a custom money/quantity type
/// queries with no cast. A lossy value is a compile error (a float on an integer
/// field, an `i64` on a `Short`).
#[derive(Debug, Clone)]
pub struct Number<K, S = Root> {
    path: String,
    _marker: PhantomData<fn() -> (K, S)>,
}

impl<K, S> Number<K, S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// Exact match.
    pub fn eq(&self, value: impl FlussoValue<K>) -> EqQuery<S> {
        EqQuery::new(&self.path, number_value(&value))
    }

    /// Match any of the given values.
    pub fn any_of(&self, values: impl IntoIterator<Item = impl FlussoValue<K>>) -> TermsQuery<S> {
        let array = values.into_iter().map(|v| number_value(&v)).collect();
        TermsQuery::new(&self.path, array)
    }

    /// Strictly less than `value`.
    pub fn lt(&self, value: impl FlussoValue<K>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lt", number_value(&value))])
    }

    /// Less than or equal to `value`.
    pub fn lte(&self, value: impl FlussoValue<K>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lte", number_value(&value))])
    }

    /// Strictly greater than `value`.
    pub fn gt(&self, value: impl FlussoValue<K>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gt", number_value(&value))])
    }

    /// Greater than or equal to `value`.
    pub fn gte(&self, value: impl FlussoValue<K>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gte", number_value(&value))])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: impl FlussoValue<K>, high: impl FlussoValue<K>) -> RangeQuery<S> {
        RangeQuery::new(
            &self.path,
            vec![("gte", number_value(&low)), ("lte", number_value(&high))],
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

    /// Exact match. Accepts a `String`/`&str`, or — with the `chrono` feature —
    /// a `NaiveDate` / `NaiveDateTime` / `DateTime<Utc>`.
    pub fn eq(&self, value: impl FlussoValue<kind::Date>) -> EqQuery<S> {
        EqQuery::new(&self.path, date_value(&value))
    }

    /// Match any of the given dates (`String`/`&str` or `chrono` date types).
    pub fn any_of(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Date>>,
    ) -> TermsQuery<S> {
        let array = values.into_iter().map(|v| date_value(&v)).collect();
        TermsQuery::new(&self.path, array)
    }

    /// Strictly before `value`.
    pub fn lt(&self, value: impl FlussoValue<kind::Date>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lt", date_value(&value))])
    }

    /// At or before `value`.
    pub fn lte(&self, value: impl FlussoValue<kind::Date>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("lte", date_value(&value))])
    }

    /// Strictly after `value`.
    pub fn gt(&self, value: impl FlussoValue<kind::Date>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gt", date_value(&value))])
    }

    /// At or after `value`.
    pub fn gte(&self, value: impl FlussoValue<kind::Date>) -> RangeQuery<S> {
        RangeQuery::new(&self.path, vec![("gte", date_value(&value))])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(
        &self,
        low: impl FlussoValue<kind::Date>,
        high: impl FlussoValue<kind::Date>,
    ) -> RangeQuery<S> {
        RangeQuery::new(
            &self.path,
            vec![("gte", date_value(&low)), ("lte", date_value(&high))],
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
