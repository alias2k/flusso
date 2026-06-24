//! Map field handles: dynamic-key objects whose values share one leaf kind.
//!
//! A `map` field (e.g. translations `{"en": …, "it": …}`) has runtime-determined
//! keys but a compile-time-known value kind. That split is the whole point:
//! `.key(runtime_str)` returns a **fully-typed** leaf handle of the declared kind
//! — [`Text`] for a [`TextMap`], [`Keyword`] for a [`KeywordMap`], [`Number`]
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

use super::sort::MapSortValueKind;
use super::{
    Common, Date, Fuzziness, Keyword, MapKey, MapKeySort, MapSortKey, MinimumShouldMatch, Number,
    Operator, Text, common_opts, exists_q, wrap,
};
use crate::FlussoDocument;
use crate::query::{AsQuery, Query, Root};

/// Collect preferred-key strings for a `sort_by` call.
fn sort_keys(keys: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<String> {
    keys.into_iter()
        .map(|key| key.as_ref().to_string())
        .collect()
}

/// Define a concrete map handle. Each carries a field path and scope `S` and
/// exposes the key-agnostic `has_key`/`exists`; per-handle `key` (a fully-typed
/// leaf) and `sort_by` (a key-fallback [`MapKeySort`]) are defined alongside.
macro_rules! map_handle {
    ($(#[$meta:meta])* $Name:ident) => {
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
    /// leaf; [`search`](Self::search) runs full text across every key;
    /// [`sort_by`](Self::sort_by) orders by a key with fallback.
    TextMap
);
map_handle!(
    /// A dynamic-key object whose values are exact strings (`map` with a
    /// `keyword`/`enum`/`uuid` value kind). [`key`](Self::key) yields a
    /// [`Keyword`] leaf for exact match. No `search` — exact-match maps use
    /// `key(..).eq(..)` / `has_key(..)`, consistent with the leaf split.
    /// [`sort_by`](Self::sort_by) orders by a key with fallback.
    KeywordMap
);
map_handle!(
    /// A dynamic-key object whose values are dates (`map` with a
    /// `date`/`timestamp` value kind). [`key`](Self::key) yields a [`Date`]
    /// leaf for range/exact operators (`gte`/`between`/`eq`/…).
    /// [`sort_by`](Self::sort_by) orders by a key with fallback.
    DateMap
);

impl<S> TextMap<S> {
    /// A specific runtime key → a fully-typed [`Text`] leaf, queried like any
    /// other text field. It carries the [`MapKey`] marker, so it is **not**
    /// directly sortable — order a text map by key with
    /// [`sort_by`](Self::sort_by), which is correct at query time and supports
    /// key fallback.
    pub fn key(&self, key: impl AsRef<str>) -> Text<S, MapKey> {
        Text::map_key(format!("{}.{}", self.path, key.as_ref()))
    }

    /// Sort by an ordered list of preferred keys (language fallback: by `it`,
    /// else `en`, …). Returns a [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_by(&self, keys: impl IntoIterator<Item = impl AsRef<str>>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), sort_keys(keys), MapSortValueKind::String)
    }
}

impl<S: FlussoDocument> MapSortKey for TextMap<S> {
    type Scope = S;
    fn map_key_sort(&self, keys: Vec<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), keys, MapSortValueKind::String)
    }
}

impl<S> KeywordMap<S> {
    /// A specific runtime key → a fully-typed [`Keyword`] leaf for exact match.
    /// It carries the [`MapKey`] marker, so it is **not** directly sortable —
    /// order a keyword map by key with [`sort_by`](Self::sort_by).
    pub fn key(&self, key: impl AsRef<str>) -> Keyword<S, MapKey> {
        Keyword::map_key(format!("{}.{}", self.path, key.as_ref()))
    }

    /// Sort by an ordered list of preferred keys (fallback). Returns a
    /// [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_by(&self, keys: impl IntoIterator<Item = impl AsRef<str>>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), sort_keys(keys), MapSortValueKind::String)
    }
}

impl<S: FlussoDocument> MapSortKey for KeywordMap<S> {
    type Scope = S;
    fn map_key_sort(&self, keys: Vec<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), keys, MapSortValueKind::String)
    }
}

impl<S> DateMap<S> {
    /// A specific runtime key → a fully-typed [`Date`] leaf for range/exact
    /// operators. A `date` map key is doc-valued on its bare path, so the leaf
    /// sorts directly; [`sort_by`](Self::sort_by) adds ordered key fallback.
    pub fn key(&self, key: impl AsRef<str>) -> Date<S> {
        Date::at(format!("{}.{}", self.path, key.as_ref()))
    }

    /// Sort by an ordered list of preferred keys (fallback), by epoch millis.
    /// Returns a [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_by(&self, keys: impl IntoIterator<Item = impl AsRef<str>>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), sort_keys(keys), MapSortValueKind::Date)
    }
}

impl<S: FlussoDocument> MapSortKey for DateMap<S> {
    type Scope = S;
    fn map_key_sort(&self, keys: Vec<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), keys, MapSortValueKind::Date)
    }
}

/// A dynamic-key object whose values are numbers (`map` with a numeric value
/// kind — `short`…`double`, `decimal`). [`key`](Self::key) yields a [`Number`]
/// leaf for range/exact operators (`gt`/`between`/`eq`/…). `has_key`/`exists`
/// are presence checks. No `search` — numbers aren't full text.
#[derive(Debug, Clone)]
pub struct NumberMap<K, S = Root> {
    path: String,
    _marker: PhantomData<fn() -> (K, S)>,
}

impl<K, S> NumberMap<K, S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// A specific runtime key → a [`Number`] leaf handle of value kind `K`,
    /// queried like any other numeric field. A numeric map key is doc-valued on
    /// its bare path, so the leaf sorts directly; [`sort_by`](Self::sort_by)
    /// adds ordered key fallback.
    pub fn key(&self, key: impl AsRef<str>) -> Number<K, S> {
        Number::at(format!("{}.{}", self.path, key.as_ref()))
    }

    /// Sort by an ordered list of preferred keys (fallback). Returns a
    /// [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_by(&self, keys: impl IntoIterator<Item = impl AsRef<str>>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), sort_keys(keys), MapSortValueKind::Number)
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

impl<K, S: FlussoDocument> MapSortKey for NumberMap<K, S> {
    type Scope = S;
    fn map_key_sort(&self, keys: Vec<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), keys, MapSortValueKind::Number)
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

    /// Combine analyzed terms with [`Operator::And`] or [`Operator::Or`]
    /// (default `Or`).
    #[must_use]
    pub fn operator(self, operator: Operator) -> Self {
        self.set("operator", Value::String(operator.as_str().to_string()))
    }

    /// Edit distance for analyzed terms ([`Fuzziness::Auto`] is the usual choice).
    #[must_use]
    pub fn fuzziness(self, fuzziness: Fuzziness) -> Self {
        self.set("fuzziness", fuzziness.to_value())
    }

    /// How many of the analyzed terms must match
    /// (e.g. `2`, `MinimumShouldMatch::percent(75)`).
    #[must_use]
    pub fn minimum_should_match(self, value: impl Into<MinimumShouldMatch>) -> Self {
        self.set("minimum_should_match", value.into().to_value())
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
