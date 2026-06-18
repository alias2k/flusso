//! Scalar value fields: the [`Bool`] exact-match field and the ordered
//! [`Number`] / [`Date`] fields with their range operators.

use std::marker::PhantomData;

use serde_json::Value;

use super::{Sort, SortOrder, exists_q, range_q, single};
use crate::query::{Query, Root};

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
    pub fn eq(&self, value: bool) -> Query<S> {
        single("term", &self.path, Value::Bool(value))
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
    pub fn eq(&self, value: T) -> Query<S> {
        single("term", &self.path, value.into())
    }

    /// Match any of the given values.
    pub fn in_(&self, values: impl IntoIterator<Item = T>) -> Query<S> {
        let array = values.into_iter().map(Into::into).collect();
        single("terms", &self.path, Value::Array(array))
    }

    /// Strictly less than `value`.
    pub fn lt(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("lt", value.into())])
    }

    /// Less than or equal to `value`.
    pub fn lte(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("lte", value.into())])
    }

    /// Strictly greater than `value`.
    pub fn gt(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("gt", value.into())])
    }

    /// Greater than or equal to `value`.
    pub fn gte(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("gte", value.into())])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: T, high: T) -> Query<S> {
        range_q(&self.path, vec![("gte", low.into()), ("lte", high.into())])
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
    pub fn eq(&self, value: impl Into<String>) -> Query<S> {
        single("term", &self.path, Value::String(value.into()))
    }

    /// Strictly before `value`.
    pub fn lt(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("lt", Value::String(value.into()))])
    }

    /// At or before `value`.
    pub fn lte(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("lte", Value::String(value.into()))])
    }

    /// Strictly after `value`.
    pub fn gt(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("gt", Value::String(value.into()))])
    }

    /// At or after `value`.
    pub fn gte(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("gte", Value::String(value.into()))])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: impl Into<String>, high: impl Into<String>) -> Query<S> {
        range_q(
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
