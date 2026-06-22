//! Map field handles: dynamic-key objects whose values share one leaf kind.
//!
//! A `map` field (e.g. translations `{"en": …, "it": …}`) has runtime-determined
//! keys but a compile-time-known value kind. That split is the whole point:
//! `.key(runtime_str)` returns a **fully-typed** leaf handle of the declared kind
//! — [`Text`] for a [`TextMap`], [`Keyword`] for a [`KeywordMap`], [`Number<T>`]
//! for a [`NumberMap`], [`Date`] for a [`DateMap`] — so a specific key is queried
//! with full type safety while keys stay open-ended.
//!
//! Three operators are shared by every map handle:
//!
//! - [`key`](TextMap::key) — a specific key → a typed leaf handle.
//! - [`has_key`](TextMap::has_key) — a presence check on one key.
//! - [`exists`](TextMap::exists) — a presence check on the whole field.
//!
//! [`TextMap`] additionally offers [`search`](TextMap::search): full-text across
//! *every* key at once, with optional per-key preference — the common
//! cross-language case, without enumerating keys or silently missing one.
//!
//! ```
//! use flusso_query::{AsQuery, Root, TextMap};
//!
//! // A specific key — a fully-typed `Text` leaf.
//! let q = TextMap::<Root>::at("title").key("it").matches("ciao").to_value();
//! assert_eq!(q["match"]["title.it"], serde_json::json!("ciao"));
//!
//! // Cross-key search, preferring Italian then English.
//! let q = TextMap::<Root>::at("title")
//!     .search("ciao")
//!     .prefer("it", 3.0)
//!     .prefer("en", 2.0)
//!     .to_value();
//! assert_eq!(q["multi_match"]["type"], serde_json::json!("best_fields"));
//! ```

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Common, Date, Keyword, Number, Text, common_opts, exists_q, wrap};
use crate::query::{AsQuery, Query, Root};

/// Define a concrete map handle over leaf kind `$Leaf`. Each carries a field
/// path and scope `S`, exposes `key`/`has_key`/`exists`, and `key` returns a
/// fully-typed `$Leaf<S>` leaf handle.
macro_rules! map_handle {
    ($(#[$meta:meta])* $Name:ident => $Leaf:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $Name<S = Root> {
            path: String,
            _scope: PhantomData<fn() -> S>,
        }

        impl<S> $Name<S> {
            pub fn at(path: impl Into<String>) -> Self {
                Self {
                    path: path.into(),
                    _scope: PhantomData,
                }
            }

            #[doc = concat!("A specific runtime key → a fully-typed `", $kind, "` leaf handle, \
                queried like any other ", $kind, " field.")]
            pub fn key(&self, key: impl AsRef<str>) -> $Leaf<S> {
                $Leaf::at(format!("{}.{}", self.path, key.as_ref()))
            }

            /// The map holds the given key with a non-null value.
            pub fn has_key(&self, key: impl AsRef<str>) -> Query<S> {
                exists_q(&format!("{}.{}", self.path, key.as_ref()))
            }

            /// The map field itself is present (has at least one key).
            pub fn exists(&self) -> Query<S> {
                exists_q(&self.path)
            }
        }
    };
}

map_handle!(
    /// A dynamic-key object whose values are analyzed full text (`map` with a
    /// `text`/`identifier` value kind). [`key`](Self::key) yields a [`Text`]
    /// leaf; [`search`](Self::search) runs full text across every key.
    TextMap => Text, "text"
);
map_handle!(
    /// A dynamic-key object whose values are exact strings (`map` with a
    /// `keyword`/`enum`/`uuid` value kind). [`key`](Self::key) yields a
    /// [`Keyword`] leaf for exact match. No `search` — exact-match maps use
    /// `key(..).eq(..)` / `has_key(..)`, consistent with the leaf split.
    KeywordMap => Keyword, "keyword"
);
map_handle!(
    /// A dynamic-key object whose values are dates (`map` with a
    /// `date`/`timestamp` value kind). [`key`](Self::key) yields a [`Date`]
    /// leaf for range/exact operators (`gte`/`between`/`eq`/…).
    DateMap => Date, "date"
);

/// A dynamic-key object whose values are numbers (`map` with a numeric value
/// kind — `short`…`double`, `decimal`). [`key`](Self::key) yields a
/// [`Number<T>`] leaf for range/exact operators (`gt`/`between`/`eq`/…); `T` is
/// the numeric type the schema's value kind implies (e.g. `i64` for `long`,
/// `f64` for `double`). `has_key`/`exists` are presence checks. No `search` —
/// numbers aren't full text.
#[derive(Debug, Clone)]
pub struct NumberMap<T, S = Root> {
    path: String,
    _marker: PhantomData<fn() -> (T, S)>,
}

impl<T, S> NumberMap<T, S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// The map holds the given key with a non-null value.
    pub fn has_key(&self, key: impl AsRef<str>) -> Query<S> {
        exists_q(&format!("{}.{}", self.path, key.as_ref()))
    }

    /// The map field itself is present (has at least one key).
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

impl<T, S> NumberMap<T, S>
where
    T: Into<serde_json::Value> + Copy,
{
    /// A specific runtime key → a fully-typed [`Number<T>`] leaf handle,
    /// queried like any other numeric field.
    pub fn key(&self, key: impl AsRef<str>) -> Number<T, S> {
        Number::at(format!("{}.{}", self.path, key.as_ref()))
    }
}

impl<S> TextMap<S> {
    /// Full-text search across *every* key at once, with optional per-key
    /// preference. Returns a [`MapSearch`] builder; add [`prefer`](MapSearch::prefer)
    /// to weight a key (e.g. the user's locale).
    pub fn search(&self, query: impl Into<String>) -> MapSearch<S> {
        MapSearch::new(&self.path, query.into())
    }
}

/// A cross-key full-text query over a [`TextMap`]: one analyzed `query` matched
/// against every key, with optional per-key preference. Renders a `multi_match`
/// of `type: best_fields` over the preferred keys (each `field^weight`) plus the
/// wildcard `path.*` fallback, so the best-scoring key wins without
/// double-counting. [`only_preferred`](Self::only_preferred) drops the fallback.
#[derive(Debug, Clone)]
pub struct MapSearch<S = Root> {
    path: String,
    query: String,
    preferred: Vec<String>,
    include_all: bool,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> MapSearch<S> {
    fn new(path: &str, query: String) -> Self {
        Self {
            path: path.to_string(),
            query,
            preferred: Vec::new(),
            include_all: true,
            opts: Map::new(),
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Prefer a key, weighting its score by `weight` (`path.key^weight`). Add
    /// several to rank keys (e.g. the user's locale highest).
    #[must_use]
    pub fn prefer(mut self, key: impl AsRef<str>, weight: f32) -> Self {
        self.preferred
            .push(format!("{}.{}^{weight}", self.path, key.as_ref()));
        self
    }

    /// Search only the preferred keys — drop the `path.*` fallback that
    /// otherwise also searches every other key.
    #[must_use]
    pub fn only_preferred(mut self) -> Self {
        self.include_all = false;
        self
    }

    fn set(mut self, key: &str, value: Value) -> Self {
        self.opts.insert(key.to_string(), value);
        self
    }

    /// Combine analyzed terms with `"AND"` or `"OR"` (default `"OR"`).
    #[must_use]
    pub fn operator(self, operator: impl Into<String>) -> Self {
        self.set("operator", Value::String(operator.into()))
    }

    /// Edit distance for analyzed terms — `"AUTO"` or an integer-as-string.
    #[must_use]
    pub fn fuzziness(self, fuzziness: impl Into<String>) -> Self {
        self.set("fuzziness", Value::String(fuzziness.into()))
    }

    /// How many of the analyzed terms must match (e.g. `"75%"`, `"2"`).
    #[must_use]
    pub fn minimum_should_match(self, value: impl Into<String>) -> Self {
        self.set("minimum_should_match", Value::String(value.into()))
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for MapSearch<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut fields: Vec<Value> = self.preferred.iter().cloned().map(Value::String).collect();
        if self.include_all {
            fields.push(Value::String(format!("{}.*", self.path)));
        }
        let mut body = self.opts;
        body.insert("query".to_string(), Value::String(self.query));
        body.insert("fields".to_string(), Value::Array(fields));
        // `best_fields` takes the max score per field, so the same term matching
        // several keys isn't double-counted.
        body.entry("type")
            .or_insert_with(|| Value::String("best_fields".to_string()));
        self.common.write(&mut body);
        Some(wrap("multi_match", body))
    }
}
