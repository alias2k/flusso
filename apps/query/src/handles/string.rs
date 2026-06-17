//! String field handles: the exact [`Keyword`] and the analyzed [`Text`], plus
//! the cross-field [`multi_match`].

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{FlussoValue, Sort, SortOrder, exists_q, kind, single};
use crate::query::{Query, Root};

/// The keyword term for a value, taken from its serde serialization — so a
/// `#[derive(FlussoValue)]` enum/newtype matches exactly the string it stores
/// in the document. `String`/`&str` pass straight through; the non-string
/// fallback only fires for a hand-written [`trait@FlussoValue`] impl that breaks the
/// "serializes to a string" contract the derive enforces.
fn keyword_term(value: &impl serde::Serialize) -> Value {
    match serde_json::to_value(value) {
        Ok(Value::String(string)) => Value::String(string),
        Ok(other) => Value::String(other.to_string()),
        Err(_) => Value::String(String::new()),
    }
}

// ---- Keyword ---------------------------------------------------------------

/// An exact, aggregatable string field (`keyword`, `enum`, `uuid`).
#[derive(Debug, Clone)]
pub struct Keyword<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Keyword<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Exact match. Accepts a `String`/`&str`, or any `#[derive(FlussoValue)]`
    /// keyword enum/newtype — matched against its serde string form
    /// (`Account::tier().eq(AccountTier::Pro)`).
    pub fn eq(&self, value: impl FlussoValue<kind::Keyword> + serde::Serialize) -> Query<S> {
        single("term", &self.path, keyword_term(&value))
    }

    /// Match any of the given values (`String`/`&str` or keyword `FlussoValue` types).
    pub fn in_(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Keyword> + serde::Serialize>,
    ) -> Query<S> {
        let array = values.into_iter().map(|v| keyword_term(&v)).collect();
        single("terms", &self.path, Value::Array(array))
    }

    /// Prefix match.
    pub fn prefix(&self, value: impl Into<String>) -> Query<S> {
        single("prefix", &self.path, Value::String(value.into()))
    }

    /// Wildcard match — `?` matches one character, `*` matches any run.
    pub fn wildcard(&self, pattern: impl Into<String>) -> Query<S> {
        single("wildcard", &self.path, Value::String(pattern.into()))
    }

    /// Regular-expression match (Lucene regex syntax, anchored to the whole term).
    pub fn regexp(&self, pattern: impl Into<String>) -> Query<S> {
        single("regexp", &self.path, Value::String(pattern.into()))
    }

    /// Fuzzy term match — tolerates typos within the default `AUTO` distance.
    pub fn fuzzy(&self, value: impl Into<String>) -> Query<S> {
        single("fuzzy", &self.path, Value::String(value.into()))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Sort ascending on this field.
    pub fn asc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Asc)
    }

    /// Sort descending on this field.
    pub fn desc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Desc)
    }
}

// ---- Text ------------------------------------------------------------------

/// An analyzed full-text field (`text`, `identifier`). No exact `eq`.
#[derive(Debug, Clone)]
pub struct Text<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Text<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Analyzed match.
    pub fn matches(&self, value: impl Into<String>) -> Query<S> {
        single("match", &self.path, Value::String(value.into()))
    }

    /// Analyzed phrase match (terms in order).
    pub fn match_phrase(&self, value: impl Into<String>) -> Query<S> {
        single("match_phrase", &self.path, Value::String(value.into()))
    }

    /// Analyzed phrase-prefix match (search-as-you-type).
    pub fn match_phrase_prefix(&self, value: impl Into<String>) -> Query<S> {
        single(
            "match_phrase_prefix",
            &self.path,
            Value::String(value.into()),
        )
    }

    /// Analyzed match tolerant of typos — a `match` with `fuzziness: AUTO`.
    pub fn matches_fuzzy(&self, value: impl Into<String>) -> Query<S> {
        let mut params = Map::new();
        params.insert("query".to_string(), Value::String(value.into()));
        params.insert("fuzziness".to_string(), Value::String("AUTO".to_string()));
        single("match", &self.path, Value::Object(params))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

/// A cross-field full-text query over several [`Text`] fields in the same scope.
pub fn multi_match<S>(
    query: impl Into<String>,
    fields: impl IntoIterator<Item = Text<S>>,
) -> Query<S> {
    let paths = fields
        .into_iter()
        .map(|field| Value::String(field.path))
        .collect();
    let mut body = Map::new();
    body.insert("query".to_string(), Value::String(query.into()));
    body.insert("fields".to_string(), Value::Array(paths));
    let mut outer = Map::new();
    outer.insert("multi_match".to_string(), Value::Object(body));
    Query::leaf(Value::Object(outer))
}
