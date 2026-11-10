//! Map field handles: dynamic-key objects whose values share one leaf kind.
//!
//! A `map` field (e.g. translations `{"en": …, "it": …}`) has runtime-determined
//! keys but a compile-time-known value kind. That split is the whole point:
//! `.key(runtime_str)` returns a **fully-typed** leaf handle of the declared kind
//! — [`Text`] for a [`TextMap`], [`Keyword`] for a [`KeywordMap`], [`Number`]
//! for a [`NumberMap`], [`Date`] for a [`DateMap`] — so a specific key is queried
//! with full type safety while keys stay open-ended.
//!
//! The sink maps every key exactly like a plain field of the value kind, so a
//! key handle is a plain-field handle: the same operators, subfield accessors
//! and sort. A string map carries the same [`WithSubfields`]/[`NoSubfields`]
//! type-state as a plain `text`/`keyword` field and passes it to its keys.
//!
//! Three operators are shared by every map handle:
//!
//! - [`key`](TextMap::key) — a specific key → a typed leaf handle.
//! - [`has_key`](TextMap::has_key) — a presence check on one key.
//! - [`exists`](TextMap::exists) — a presence check on the whole field.
//!
//! [`TextMap`] additionally offers [`search`](TextMap::search): full-text across
//! *every* key at once, with optional per-key preference — the common
//! cross-language case, without enumerating keys or silently missing one. Both
//! string maps offer [`contains`](TextMap::contains): part of a value, across
//! every key, case- and accent-insensitively.
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
//!
//! // Part of a word, in any key: "excav" finds "Excavator".
//! let q = TextMap::<Root>::at("title").contains("excav").to_value();
//! assert_eq!(q["query_string"]["query"], serde_json::json!("*excav*"));
//! ```

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::sort::MapSortValueKind;
use super::{
    Common, Date, Fuzziness, KEYWORD_LOWERCASE_SUFFIX, Keyword, MapKeySort, MinimumShouldMatch,
    MultiMatchType, NoSubfields, Number, Operator, Text, WithSubfields, common_opts, exists_q,
    wrap,
};
use crate::query::{AsQuery, Query, Root};

/// The key-agnostic presence checks every map handle shares.
macro_rules! presence_ops {
    () => {
        /// The map holds the given key with a non-null value.
        pub fn has_key(&self, key: impl AsRef<str>) -> Query<S> {
            exists_q(&format!("{}.{}", self.path, key.as_ref()))
        }

        /// The map field itself is present (has at least one key).
        pub fn exists(&self) -> Query<S> {
            exists_q(&self.path)
        }
    };
}

/// Define a string map handle. `Sub` is the [`WithSubfields`]/[`NoSubfields`]
/// type-state of its keys, exactly as for a plain field of the value kind: the
/// derive stamps `WithSubfields` when every OpenSearch sink has
/// `auto_subfields` on.
macro_rules! string_map_handle {
    ($(#[$meta:meta])* $Name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $Name<S = Root, Sub = WithSubfields> {
            path: String,
            _marker: PhantomData<fn() -> (S, Sub)>,
        }

        impl<S, Sub> $Name<S, Sub> {
            fn handle(path: impl Into<String>) -> Self {
                Self {
                    path: path.into(),
                    _marker: PhantomData,
                }
            }

            presence_ops!();
        }

        impl<S> $Name<S, WithSubfields> {
            /// A handle for a map whose keys carry flusso's auto subfields.
            pub fn at(path: impl Into<String>) -> Self {
                Self::handle(path)
            }

            /// Case- and accent-insensitive substring match across **every**
            /// key — `contains("excav")` finds "Excavator". Returns a
            /// [`MapContains`] builder; see it for the rendered shape.
            pub fn contains(&self, value: impl AsRef<str>) -> MapContains<S> {
                MapContains::new(&self.path, value.as_ref())
            }
        }

        impl<S> $Name<S, NoSubfields> {
            /// A handle for a map whose keys have no auto subfields.
            pub fn leaf(path: impl Into<String>) -> Self {
                Self::handle(path)
            }
        }
    };
}

string_map_handle!(
    /// A dynamic-key object whose values are analyzed full text (`map` with a
    /// `text`/`identifier` value kind). [`key`](Self::key) yields a [`Text`]
    /// leaf; [`search`](Self::search) runs full text across every key;
    /// [`contains`](Self::contains) matches part of a value across every key;
    /// [`sort_key`](Self::sort_key) orders by a key, with `.or(..)` fallback.
    TextMap
);
string_map_handle!(
    /// A dynamic-key object whose values are exact strings (`map` with a
    /// `keyword`/`enum`/`uuid` value kind). [`key`](Self::key) yields a
    /// [`Keyword`] leaf for exact match; [`contains`](Self::contains) matches
    /// part of a value across every key; [`sort_key`](Self::sort_key) orders by
    /// a key, with `.or(..)` fallback.
    KeywordMap
);

/// A dynamic-key object whose values are dates (`map` with a
/// `date`/`timestamp` value kind). [`key`](Self::key) yields a [`Date`]
/// leaf for range/exact operators (`gte`/`between`/`eq`/…).
/// [`sort_key`](Self::sort_key) orders by a key, with `.or(..)` fallback.
#[derive(Debug, Clone)]
pub struct DateMap<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> DateMap<S> {
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    presence_ops!();
}

impl<S, Sub> TextMap<S, Sub> {
    /// A specific runtime key → a [`Text`] leaf, mapped and queried exactly like
    /// a plain `text` field: the same subfield accessors and, with subfields,
    /// the same `.asc()`/`.desc()` (on `.keyword_lowercase`).
    pub fn key(&self, key: impl AsRef<str>) -> Text<S, Sub> {
        Text::handle(format!("{}.{}", self.path, key.as_ref()))
    }

    /// Full-text search across *every* key at once, with optional per-key
    /// preference. Returns a [`MapSearch`] builder; add [`prefer`](MapSearch::prefer)
    /// to weight a key (e.g. the user's locale).
    pub fn search(&self, query: impl Into<String>) -> MapSearch<S> {
        MapSearch::new(&self.path, query.into())
    }
}

impl<S> TextMap<S, WithSubfields> {
    /// Sort by this key, with optional fallback — `sort_key("it").or("en")`
    /// orders by `it`, else `en` (language fallback), case- and
    /// accent-insensitively. Returns a [`MapKeySort`]; pass it to
    /// [`SortBuilder::by`](crate::SortBuilder::by) or `.asc()`/`.desc()`.
    /// Needs the auto subfields, like sorting a plain `text` field.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), key, MapSortValueKind::Folded)
    }
}

impl<S, Sub> KeywordMap<S, Sub> {
    /// A specific runtime key → a [`Keyword`] leaf, mapped and queried exactly
    /// like a plain `keyword` field (`eq` is an exact `term`; sorts directly).
    pub fn key(&self, key: impl AsRef<str>) -> Keyword<S, Sub> {
        Keyword::handle(format!("{}.{}", self.path, key.as_ref()))
    }
}

impl<S> KeywordMap<S, WithSubfields> {
    /// Sort by this key, with optional fallback (`sort_key("a").or("b")`),
    /// case- and accent-insensitively. Returns a [`MapKeySort`]; see it for the
    /// rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), key, MapSortValueKind::Folded)
    }
}

impl<S> KeywordMap<S, NoSubfields> {
    /// Sort by this key, with optional fallback (`sort_key("a").or("b")`),
    /// case-insensitively (no accent folding without the subfields). Returns a
    /// [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), key, MapSortValueKind::Exact)
    }
}

impl<S> DateMap<S> {
    /// A specific runtime key → a fully-typed [`Date`] leaf for range/exact
    /// operators. A `date` map key is doc-valued on its bare path, so the leaf
    /// sorts directly; [`sort_key`](Self::sort_key) adds ordered key fallback.
    pub fn key(&self, key: impl AsRef<str>) -> Date<S> {
        Date::at(format!("{}.{}", self.path, key.as_ref()))
    }

    /// Sort by this key, with optional fallback (`sort_key("eu").or("us")`), by
    /// epoch millis. Returns a [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), key, MapSortValueKind::Date)
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
    /// its bare path, so the leaf sorts directly; [`sort_key`](Self::sort_key)
    /// adds ordered key fallback.
    pub fn key(&self, key: impl AsRef<str>) -> Number<K, S> {
        Number::at(format!("{}.{}", self.path, key.as_ref()))
    }

    /// Sort by this key, with optional fallback (`sort_key("usd").or("eur")`).
    /// Returns a [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), key, MapSortValueKind::Number)
    }

    presence_ops!();
}

/// A cross-key full-text query over a [`TextMap`]: one analyzed `query` matched
/// against every key, with optional per-key preference. Renders a `multi_match`
/// over the preferred keys (each `field^weight`) plus the wildcard `path.*`
/// fallback. The type defaults to `best_fields`, so the best-scoring key wins
/// without double-counting; [`match_type`](Self::match_type) picks another
/// (`BoolPrefix` matches the start of a word, as typed in a search box).
/// [`only_preferred`](Self::only_preferred) drops the fallback.
#[derive(Debug, Clone)]
pub struct MapSearch<S = Root> {
    keys: KeyFields,
    query: String,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> MapSearch<S> {
    fn new(path: &str, query: String) -> Self {
        Self {
            keys: KeyFields::new(path, ""),
            query,
            opts: Map::new(),
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Prefer a key, weighting its score by `weight` (`path.key^weight`). Add
    /// several to rank keys (e.g. the user's locale highest).
    #[must_use]
    pub fn prefer(mut self, key: impl AsRef<str>, weight: f32) -> Self {
        self.keys.prefer(key.as_ref(), weight);
        self
    }

    /// Search only the preferred keys — drop the `path.*` fallback that
    /// otherwise also searches every other key.
    #[must_use]
    pub fn only_preferred(mut self) -> Self {
        self.keys.include_all = false;
        self
    }

    fn set(mut self, key: &str, value: Value) -> Self {
        self.opts.insert(key.to_string(), value);
        self
    }

    /// The `multi_match` type (default [`MultiMatchType::BestFields`]).
    #[must_use]
    pub fn match_type(self, match_type: MultiMatchType) -> Self {
        self.set("type", Value::String(match_type.as_str().to_string()))
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
        let mut body = self.opts;
        body.insert("query".to_string(), Value::String(self.query));
        body.insert("fields".to_string(), self.keys.into_value());
        // `best_fields` takes the max score per field, so the same term matching
        // several keys isn't double-counted.
        body.entry("type")
            .or_insert_with(|| Value::String("best_fields".to_string()));
        self.common.write(&mut body);
        Some(wrap("multi_match", body))
    }
}

/// The characters `query_string` gives a meaning; each is escaped with a backslash so
/// the value is matched literally (whitespace too, so it stays one term).
fn is_query_string_syntax(c: char) -> bool {
    c.is_whitespace() || "\\+-=&|><!(){}[]^\"~*?:/".contains(c)
}

/// A case- and accent-insensitive substring match across every key of a
/// string map (`TextMap::contains` / `KeywordMap::contains`). Renders a
/// `query_string` of `*<value>*` — the value escaped, so it is matched
/// literally — over every key's `.keyword_lowercase` subfield
/// (`path.*.keyword_lowercase`, plus each [`prefer`](Self::prefer)'d key with
/// its weight). The subfield's normalizer folds the pattern like the indexed
/// value. A value longer than the subfield's `ignore_above` (256) is not
/// indexed there, so it never matches — the same rule as a plain field.
///
/// ```
/// use flusso_query::{AsQuery, Root, TextMap};
///
/// let q = TextMap::<Root>::at("title").contains("excav").to_value();
/// assert_eq!(q["query_string"]["query"], serde_json::json!("*excav*"));
/// assert_eq!(
///     q["query_string"]["fields"],
///     serde_json::json!(["title.*.keyword_lowercase"])
/// );
/// ```
#[derive(Debug, Clone)]
pub struct MapContains<S = Root> {
    keys: KeyFields,
    value: String,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> MapContains<S> {
    fn new(path: &str, value: &str) -> Self {
        Self {
            keys: KeyFields::new(path, KEYWORD_LOWERCASE_SUFFIX),
            value: value.to_string(),
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Prefer a key, weighting its score by `weight`.
    #[must_use]
    pub fn prefer(mut self, key: impl AsRef<str>, weight: f32) -> Self {
        self.keys.prefer(key.as_ref(), weight);
        self
    }

    /// Match only the preferred keys — drop the `path.*` fallback.
    #[must_use]
    pub fn only_preferred(mut self) -> Self {
        self.keys.include_all = false;
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for MapContains<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut pattern = String::from("*");
        for c in self.value.chars() {
            if is_query_string_syntax(c) {
                pattern.push('\\');
            }
            pattern.push(c);
        }
        pattern.push('*');
        let mut body = Map::new();
        body.insert("query".to_string(), Value::String(pattern));
        body.insert("fields".to_string(), self.keys.into_value());
        self.common.write(&mut body);
        Some(wrap("query_string", body))
    }
}

/// The `fields` list of a cross-key query: each preferred key with its weight,
/// then the `path.*` fallback unless dropped. `suffix` targets a subfield of
/// every key (`.keyword_lowercase`), or the key itself when empty.
#[derive(Debug, Clone)]
struct KeyFields {
    path: String,
    suffix: &'static str,
    preferred: Vec<String>,
    include_all: bool,
}

impl KeyFields {
    fn new(path: &str, suffix: &'static str) -> Self {
        Self {
            path: path.to_string(),
            suffix,
            preferred: Vec::new(),
            include_all: true,
        }
    }

    fn prefer(&mut self, key: &str, weight: f32) {
        self.preferred
            .push(format!("{}.{key}{}^{weight}", self.path, self.suffix));
    }

    fn into_value(self) -> Value {
        let mut fields: Vec<Value> = self.preferred.into_iter().map(Value::String).collect();
        if self.include_all {
            fields.push(Value::String(format!("{}.*{}", self.path, self.suffix)));
        }
        Value::Array(fields)
    }
}
