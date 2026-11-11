//! Map field handles: dynamic-key objects whose values share one leaf kind.
//!
//! A `map` field (e.g. translations `{"en": …, "it": …}`) has runtime keys but
//! a compile-time value kind. The sink maps every key exactly like a plain
//! field of that kind, so a map handle offers two forms of the same surface:
//!
//! - **One key**: [`key`](TextMap::key) returns the plain-field handle of the
//!   value kind — [`Text`] for a [`TextMap`], [`Keyword`] for a [`KeywordMap`],
//!   [`Number`] for a [`NumberMap`], [`Date`] for a [`DateMap`].
//! - **Every key**: the map handle mirrors that plain handle's operators, with
//!   the same names and arguments, applied across all keys. `TextMap::matches`
//!   is `Text::matches` on any key, `NumberMap::between` is `Number::between` on
//!   any key, and so on.
//!
//! Every cross-key query takes two map-only options:
//!
//! - `prefer(key, weight)` weights one key (`path.key^weight`). Add several to
//!   rank keys, e.g. the user's locale first. In a filter it changes nothing.
//! - `only_preferred()` drops the all-keys fallback, so only the preferred keys
//!   are searched. With no preferred key it renders `match_none`: an empty
//!   `fields` would search every field of the index.
//!
//! # How a cross-key query renders
//!
//! Keys aren't known when the query is built, so a cross-key operator renders
//! as a query that takes a **field pattern**:
//!
//! - The analyzed family (`matches`, `match_phrase`, `match_phrase_prefix`,
//!   `match_bool_prefix`, `matches_fuzzy`) is a `multi_match` over `path.*`.
//!   That pattern also reaches each key's subfields (`.keyword`,
//!   `.keyword_lowercase`), so an exact whole-value hit scores higher. This is
//!   intended.
//! - Every other operator is a `query_string` over one explicit pattern, the
//!   value escaped so it's matched literally: `eq`/`any_of` are quoted terms,
//!   ranges are `[a TO b]`, `prefix` is `v*`, `regexp` is `/re/`, `fuzzy` is
//!   `v~`. A bare term starting with `AND`/`OR`/`NOT` gets its first letter
//!   escaped so it doesn't parse as an operator.
//!
//! A pattern that matches no field (a map with no keys yet) matches nothing.
//!
//! # Subfields
//!
//! A string map carries the same [`WithSubfields`]/[`NoSubfields`] type-state
//! as a plain field and passes it to its keys. With subfields, the map has the
//! plain handle's subfield accessors too: `TextMap::keyword()`,
//! `keyword_lowercase()`, `KeywordMap::text()`. Each returns a map handle over
//! that subfield of every key (`path.*.keyword_lowercase`).
//!
//! The exact operators (`eq`, `any_of`, `prefix`, `wildcard`, `regexp`,
//! `fuzzy`) on a [`KeywordMap`] render over `path.*` without subfields. With
//! them, `path.*` would also reach each key's analyzed `.text` and folded
//! `.keyword_lowercase`, so `eq("sale")` would match "big sale" or "Sale".
//! The sink gives every key of such a map an exact `.keyword` subfield, and
//! the operators render over `path.*.keyword` instead: the same exact match,
//! under the same names. [`KeywordMap::keyword`] is that view.
//!
//! ```
//! use flusso_query::{AsQuery, KeywordMap, NumberMap, Root, TextMap, kind};
//! use serde_json::json;
//!
//! // One key: a plain `Text` handle.
//! let q = TextMap::<Root>::at("title").key("it").matches("ciao").to_value();
//! assert_eq!(q["match"]["title.it"], json!("ciao"));
//!
//! // Every key, preferring Italian then English.
//! let q = TextMap::<Root>::at("title")
//!     .matches("ciao")
//!     .prefer("it", 3.0)
//!     .prefer("en", 2.0)
//!     .to_value();
//! assert_eq!(
//!     q["multi_match"]["fields"],
//!     json!(["title.it^3", "title.en^2", "title.*"])
//! );
//!
//! // A price in any currency.
//! let q = NumberMap::<kind::Double, Root>::at("prices").between(10, 50).to_value();
//! assert_eq!(q["query_string"]["query"], json!(r#"["10" TO "50"]"#));
//!
//! // Exact value, in any key.
//! let q = KeywordMap::<Root>::at("tags").eq("sale").to_value();
//! assert_eq!(q["query_string"]["fields"], json!(["tags.*.keyword"]));
//!
//! // Case-insensitive exact value, in any key.
//! let q = KeywordMap::<Root>::at("tags").keyword_lowercase().eq("sale").to_value();
//! assert_eq!(q["query_string"]["fields"], json!(["tags.*.keyword_lowercase"]));
//! ```

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::scalar::{date_value, number_value};
use super::sort::MapSortValueKind;
use super::string::keyword_term;
use super::{
    Common, Date, FlussoValue, Fuzziness, KEYWORD_LOWERCASE_SUFFIX, Keyword, MapKeySort,
    MinimumShouldMatch, MultiMatchType, NoSubfields, Number, Operator, Text, WithSubfields,
    ZeroTermsQuery, exists_q, kind, wrap,
};
use crate::query::{AsQuery, Query, Root};

/// The suffix of the exact `.keyword` subfield a `text` key, and a `keyword`
/// key with subfields, carries.
const KEYWORD_SUFFIX: &str = ".keyword";
/// The suffix of the analyzed `.text` subfield a `keyword` key carries.
const TEXT_SUFFIX: &str = ".text";

/// Where a map handle's keys live: `{path}.{key}{suffix}`. `suffix` is empty for
/// the keys themselves, or names one subfield of every key (a subfield view).
#[derive(Debug, Clone)]
struct MapPath {
    path: String,
    suffix: &'static str,
}

impl MapPath {
    fn new(path: impl Into<String>, suffix: &'static str) -> Self {
        Self {
            path: path.into(),
            suffix,
        }
    }

    fn view(&self, suffix: &'static str) -> Self {
        Self::new(self.path.clone(), suffix)
    }

    fn key(&self, key: &str) -> String {
        format!("{}.{key}{}", self.path, self.suffix)
    }

    fn has_key<S>(&self, key: &str) -> Query<S> {
        exists_q(&self.key(key))
    }

    /// The whole field, or every key's subfield for a view.
    fn exists<S>(&self) -> Query<S> {
        if self.suffix.is_empty() {
            exists_q(&self.path)
        } else {
            exists_q(&format!("{}.*{}", self.path, self.suffix))
        }
    }

    fn query_string(&self, query: Option<String>) -> CrossKey {
        CrossKey::new(
            "query_string",
            KeyFields::new(&self.path, self.suffix),
            query,
        )
    }

    fn multi_match(&self, query: String, match_type: MultiMatchType) -> CrossKey {
        let mut body = CrossKey::new(
            "multi_match",
            KeyFields::new(&self.path, self.suffix),
            Some(query),
        );
        body.set("type", Value::String(match_type.as_str().to_string()));
        body
    }

    /// `"a" OR "b"`; no values matches nothing, as an empty `terms` does.
    fn terms(&self, values: impl IntoIterator<Item = Value>) -> CrossKey {
        let terms: Vec<String> = values.into_iter().map(|v| exact(&literal(&v))).collect();
        self.query_string((!terms.is_empty()).then(|| terms.join(" OR ")))
    }

    /// `[low TO high]`, a bracket per bound: `[`/`]` inclusive, `{`/`}`
    /// exclusive, `*` unbounded.
    fn range(&self, low: Option<Bound>, high: Option<Bound>) -> CrossKey {
        let (open, low) = match low {
            Some(Bound { value, inclusive }) => {
                (if inclusive { '[' } else { '{' }, quoted(&literal(&value)))
            }
            None => ('[', "*".to_string()),
        };
        let (close, high) = match high {
            Some(Bound { value, inclusive }) => {
                (if inclusive { ']' } else { '}' }, quoted(&literal(&value)))
            }
            None => (']', "*".to_string()),
        };
        self.query_string(Some(format!("{open}{low} TO {high}{close}")))
    }

    fn sort_key<S>(&self, key: impl Into<String>, kind: MapSortValueKind) -> MapKeySort<S> {
        MapKeySort::new(self.path.clone(), key, kind).with_suffix(self.suffix)
    }
}

/// One end of a cross-key range.
#[derive(Debug)]
struct Bound {
    value: Value,
    inclusive: bool,
}

fn inclusive(value: Value) -> Option<Bound> {
    Some(Bound {
        value,
        inclusive: true,
    })
}

fn exclusive(value: Value) -> Option<Bound> {
    Some(Bound {
        value,
        inclusive: false,
    })
}

/// A value as `query_string` text: a string as-is, a number in JSON form.
fn literal(value: &Value) -> String {
    match value {
        Value::String(string) => string.clone(),
        other => other.to_string(),
    }
}

/// A quoted `query_string` term: matched as one exact value on a keyword,
/// numeric or date field.
fn quoted(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// One exact value: [`quoted`], or `//` for the empty string. An empty phrase
/// `""` analyzes to nothing and matches nothing; the empty regex matches
/// exactly the empty value, as a `term` does.
fn exact(value: &str) -> String {
    if value.is_empty() {
        EMPTY_VALUE.to_string()
    } else {
        quoted(value)
    }
}

/// The `query_string` regex that matches only the empty value.
const EMPTY_VALUE: &str = "//";

/// The characters `query_string` gives a meaning; each is escaped with a backslash so
/// the value is matched literally (whitespace too, so it stays one term).
fn is_query_string_syntax(c: char) -> bool {
    c.is_whitespace() || "\\+-=&|><!(){}[]^\"~*?:/".contains(c)
}

/// A bare `query_string` term, `value` escaped so it's matched literally.
fn escaped(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if is_query_string_syntax(c) {
            out.push('\\');
        }
        out.push(c);
    }
    not_an_operator(out)
}

/// A bare `query_string` wildcard term: `?` and `*` stay wildcards, a `\` and
/// the character after it are copied as one escape, and every other syntax
/// character is escaped. A trailing lone `\` is a literal backslash, as in a
/// `wildcard` query.
fn wildcard_term(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push('\\');
                out.push(chars.next().unwrap_or('\\'));
            }
            '*' | '?' => out.push(c),
            _ => {
                if is_query_string_syntax(c) {
                    out.push('\\');
                }
                out.push(c);
            }
        }
    }
    not_an_operator(out)
}

/// A bare term starting with `AND`, `OR` or `NOT` can parse as that operator
/// (`AND~` is a 400). Escaping its first letter keeps it a term: `\AND` is
/// `AND`.
fn not_an_operator(term: String) -> String {
    if ["AND", "OR", "NOT"].iter().any(|op| term.starts_with(op)) {
        format!("\\{term}")
    } else {
        term
    }
}

/// A `query_string` regex term, `/pattern/`. An unescaped `/` would end it, so
/// it is escaped; an existing escape (`\/`, `\\`) is kept as written.
fn regexp_term(pattern: &str) -> String {
    let mut out = String::from("/");
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push(c);
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '/' => out.push_str("\\/"),
            _ => out.push(c),
        }
    }
    out.push('/');
    out
}

/// The body every cross-key builder renders: `{ <wrapper>: { query, fields,
/// ...opts, boost, _name } }`, or `match_none` when there is no query.
#[derive(Debug, Clone)]
struct CrossKey {
    wrapper: &'static str,
    keys: KeyFields,
    query: Option<String>,
    opts: Map<String, Value>,
    common: Common,
}

impl CrossKey {
    fn new(wrapper: &'static str, keys: KeyFields, query: Option<String>) -> Self {
        Self {
            wrapper,
            keys,
            query,
            opts: Map::new(),
            common: Common::default(),
        }
    }

    fn set(&mut self, key: &str, value: Value) {
        self.opts.insert(key.to_string(), value);
    }

    /// No query, or no field to run it on, renders `match_none`. An empty
    /// `fields` would fall back to `index.query.default_field`: every field.
    fn render<S>(self) -> Query<S> {
        let (Some(query), Some(fields)) = (self.query, self.keys.into_value()) else {
            let mut body = Map::new();
            self.common.write(&mut body);
            return wrap("match_none", body);
        };
        let mut body = self.opts;
        body.insert("query".to_string(), Value::String(query));
        body.insert("fields".to_string(), fields);
        self.common.write(&mut body);
        wrap(self.wrapper, body)
    }
}

/// Define a cross-key builder: a [`CrossKey`] body plus the map-only
/// `prefer`/`only_preferred` and the universal `boost`/`name`.
macro_rules! cross_key_builder {
    ($(#[$meta:meta])* $Name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        pub struct $Name<S = Root> {
            body: CrossKey,
            _scope: PhantomData<fn() -> S>,
        }

        impl<S> $Name<S> {
            fn new(body: CrossKey) -> Self {
                Self {
                    body,
                    _scope: PhantomData,
                }
            }

            /// Prefer a key, weighting its score by `weight` (`path.key^weight`).
            /// Add several to rank keys. In a filter it changes nothing.
            #[must_use]
            pub fn prefer(mut self, key: impl AsRef<str>, weight: f32) -> Self {
                self.body.keys.prefer(key.as_ref(), weight);
                self
            }

            /// Query only the preferred keys: drop the `path.*` fallback. With
            /// no preferred key, the query matches nothing.
            #[must_use]
            pub fn only_preferred(mut self) -> Self {
                self.body.keys.include_all = false;
                self
            }

            /// Multiply this clause's relevance score by `boost`.
            #[must_use]
            pub fn boost(mut self, boost: f32) -> Self {
                self.body.common.set_boost(boost);
                self
            }

            /// Tag this clause with `_name`, surfaced in a hit's `matched_queries`.
            #[must_use]
            pub fn name(mut self, name: impl Into<String>) -> Self {
                self.body.common.set_name(name.into());
                self
            }
        }

        impl<S> AsQuery<S> for $Name<S> {
            fn into_query(self) -> Option<Query<S>> {
                Some(self.body.render())
            }
        }
    };
}

/// Give a cross-key builder an option setter, for builders that take options.
macro_rules! settable {
    ($($Name:ident),+) => {
        $(impl<S> $Name<S> {
            fn set(mut self, key: &str, value: Value) -> Self {
                self.body.set(key, value);
                self
            }
        })+
    };
}

cross_key_builder!(
    /// The analyzed family across every key of a [`TextMap`] (`matches`,
    /// `match_phrase`, `match_phrase_prefix`, `match_bool_prefix`,
    /// `matches_fuzzy`): a `multi_match` over the preferred keys plus `path.*`,
    /// typed `best_fields` / `phrase` / `phrase_prefix` / `bool_prefix`. The
    /// options are the plain [`MatchQuery`](super::MatchQuery)'s, plus
    /// `tie_breaker`.
    ///
    /// ```
    /// use flusso_query::{AsQuery, Root, TextMap};
    /// use serde_json::json;
    ///
    /// let q = TextMap::<Root>::at("title").match_phrase("scarpe rosse").only_preferred().prefer("it", 2.0);
    /// assert_eq!(
    ///     q.to_value(),
    ///     json!({ "multi_match": {
    ///         "query": "scarpe rosse", "type": "phrase", "fields": ["title.it^2"]
    ///     } })
    /// );
    /// ```
    MapMatch
);

impl<S> MapMatch<S> {
    /// The `multi_match` type, replacing the operator's own
    /// (`MultiMatchType::MostFields` sums the keys' scores).
    #[must_use]
    pub fn match_type(self, match_type: MultiMatchType) -> Self {
        self.set("type", Value::String(match_type.as_str().to_string()))
    }

    /// Edit distance for analyzed terms ([`Fuzziness::Auto`] is the usual choice).
    #[must_use]
    pub fn fuzziness(self, fuzziness: Fuzziness) -> Self {
        self.set("fuzziness", fuzziness.to_value())
    }

    /// Combine analyzed terms with [`Operator::And`] or [`Operator::Or`]
    /// (default `Or`).
    #[must_use]
    pub fn operator(self, operator: Operator) -> Self {
        self.set("operator", Value::String(operator.as_str().to_string()))
    }

    /// How many of the analyzed terms must match
    /// (e.g. `2`, `MinimumShouldMatch::percent(75)`).
    #[must_use]
    pub fn minimum_should_match(self, value: impl Into<MinimumShouldMatch>) -> Self {
        self.set("minimum_should_match", value.into().to_value())
    }

    /// Leading characters that must match exactly (fuzzy/prefix matching).
    #[must_use]
    pub fn prefix_length(self, prefix_length: u32) -> Self {
        self.set("prefix_length", Value::from(prefix_length))
    }

    /// Cap on terms a prefix / fuzzy term expands into.
    #[must_use]
    pub fn max_expansions(self, max_expansions: u32) -> Self {
        self.set("max_expansions", Value::from(max_expansions))
    }

    /// Override the search analyzer for this clause.
    #[must_use]
    pub fn analyzer(self, analyzer: impl Into<String>) -> Self {
        self.set("analyzer", Value::String(analyzer.into()))
    }

    /// Phrase `slop`: allowed positional gap (phrase / phrase-prefix).
    #[must_use]
    pub fn slop(self, slop: u32) -> Self {
        self.set("slop", Value::from(slop))
    }

    /// Behavior when analysis yields no terms.
    #[must_use]
    pub fn zero_terms_query(self, value: ZeroTermsQuery) -> Self {
        self.set(
            "zero_terms_query",
            Value::String(value.as_str().to_string()),
        )
    }

    /// Ignore format errors.
    #[must_use]
    pub fn lenient(self, lenient: bool) -> Self {
        self.set("lenient", Value::Bool(lenient))
    }

    /// How much the non-winning keys contribute to the score (`best_fields`).
    #[must_use]
    pub fn tie_breaker(self, tie_breaker: f32) -> Self {
        self.set("tie_breaker", Value::from(tie_breaker))
    }
}

cross_key_builder!(
    /// An exact value across every key (`eq`, `any_of`): a `query_string` of
    /// quoted terms (`"a" OR "b"`) over the keys. No values renders
    /// `match_none`.
    ///
    /// ```
    /// use flusso_query::{AsQuery, KeywordMap, NoSubfields, Root};
    /// use serde_json::json;
    ///
    /// let q = KeywordMap::<Root, NoSubfields>::leaf("tags").any_of(["sale", "new"]);
    /// assert_eq!(
    ///     q.to_value(),
    ///     json!({ "query_string": { "query": r#""sale" OR "new""#, "fields": ["tags.*"] } })
    /// );
    /// ```
    MapTerm
);

cross_key_builder!(
    /// A range across every key (`lt`, `lte`, `gt`, `gte`, `between`): a
    /// `query_string` range (`["10" TO "50"]`, `{"3" TO *]`) over the keys.
    /// Date bounds take date math (`"now-1d"`).
    MapRange
);

impl<S> MapRange<S> {
    /// Time zone applied to date bounds, e.g. `"+01:00"`.
    #[must_use]
    pub fn time_zone(self, time_zone: impl Into<String>) -> Self {
        self.set("time_zone", Value::String(time_zone.into()))
    }
}

cross_key_builder!(
    /// A prefix across every key: a `query_string` of the escaped value plus
    /// `*`.
    MapPrefix
);

cross_key_builder!(
    /// A wildcard pattern across every key: `?` matches one character, `*` any
    /// run; every other syntax character is escaped.
    MapWildcard
);

settable!(
    MapMatch,
    MapRange,
    MapPrefix,
    MapWildcard,
    MapRegexp,
    MapFuzzy
);

macro_rules! rewrite_opt {
    ($Name:ident) => {
        impl<S> $Name<S> {
            /// The multi-term `rewrite` method (e.g. `"constant_score"`).
            #[must_use]
            pub fn rewrite(self, rewrite: impl Into<String>) -> Self {
                self.set("rewrite", Value::String(rewrite.into()))
            }
        }
    };
}
rewrite_opt!(MapPrefix);
rewrite_opt!(MapWildcard);

cross_key_builder!(
    /// A regular expression across every key (Lucene syntax, anchored to the
    /// whole value): a `query_string` of `/pattern/`.
    MapRegexp
);

impl<S> MapRegexp<S> {
    /// Cap on the automaton size compiled from the pattern.
    #[must_use]
    pub fn max_determinized_states(self, max: u32) -> Self {
        self.set("max_determinized_states", Value::from(max))
    }
}

cross_key_builder!(
    /// A fuzzy term across every key: a `query_string` of the escaped value
    /// plus `~`, the edit distance from [`fuzziness`](Self::fuzziness)
    /// (default `AUTO`).
    MapFuzzy
);

impl<S> MapFuzzy<S> {
    /// Maximum edit distance ([`Fuzziness::Auto`] is the usual choice).
    #[must_use]
    pub fn fuzziness(self, fuzziness: Fuzziness) -> Self {
        self.set("fuzziness", fuzziness.to_value())
    }

    /// Leading characters that must match exactly.
    #[must_use]
    pub fn prefix_length(self, prefix_length: u32) -> Self {
        self.set("fuzzy_prefix_length", Value::from(prefix_length))
    }

    /// Cap on the variations the term expands into.
    #[must_use]
    pub fn max_expansions(self, max_expansions: u32) -> Self {
        self.set("fuzzy_max_expansions", Value::from(max_expansions))
    }

    /// Whether adjacent-character transpositions count as one edit.
    #[must_use]
    pub fn transpositions(self, transpositions: bool) -> Self {
        self.set("fuzzy_transpositions", Value::Bool(transpositions))
    }
}

cross_key_builder!(
    /// A case- and accent-insensitive substring match across every key of a
    /// string map (`TextMap::contains` / `KeywordMap::contains`). Renders a
    /// `query_string` of `*<value>*` (escaped, so matched literally) over every
    /// key's `.keyword_lowercase` subfield. The subfield's normalizer folds the
    /// pattern like the indexed value. A value longer than the subfield's
    /// `ignore_above` (256) is not indexed there, so it never matches.
    ///
    /// An empty or whitespace-only value renders `match_none`: `*` alone would
    /// match every document with any key.
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
    ///
    /// let none = TextMap::<Root>::at("title").contains("  ").to_value();
    /// assert_eq!(none, serde_json::json!({ "match_none": {} }));
    /// ```
    MapContains
);

fn contains_body(path: &MapPath, value: &str) -> CrossKey {
    let query = (!value.trim().is_empty()).then(|| format!("*{}*", escaped(value)));
    path.view(KEYWORD_LOWERCASE_SUFFIX).query_string(query)
}

/// The key-agnostic presence checks every map handle shares.
macro_rules! presence_ops {
    () => {
        /// The map holds the given key with a non-null value.
        pub fn has_key(&self, key: impl AsRef<str>) -> Query<S> {
            self.keys.has_key(key.as_ref())
        }

        /// The map field itself is present (has at least one key).
        pub fn exists(&self) -> Query<S> {
            self.keys.exists()
        }
    };
}

/// The ordered value operators `Number` and `Date` share, across every key.
macro_rules! ordered_ops {
    ($kind:ty, $to_value:ident, $what:literal) => {
        #[doc = concat!("Some key equals `value` (", $what, ").")]
        pub fn eq(&self, value: impl FlussoValue<$kind>) -> MapTerm<S> {
            MapTerm::new(self.keys.terms([$to_value(&value)]))
        }

        #[doc = concat!("Some key equals any of `values` (", $what, ").")]
        pub fn any_of(
            &self,
            values: impl IntoIterator<Item = impl FlussoValue<$kind>>,
        ) -> MapTerm<S> {
            MapTerm::new(self.keys.terms(values.into_iter().map(|v| $to_value(&v))))
        }

        /// Some key is strictly less than `value`.
        pub fn lt(&self, value: impl FlussoValue<$kind>) -> MapRange<S> {
            MapRange::new(self.keys.range(None, exclusive($to_value(&value))))
        }

        /// Some key is less than or equal to `value`.
        pub fn lte(&self, value: impl FlussoValue<$kind>) -> MapRange<S> {
            MapRange::new(self.keys.range(None, inclusive($to_value(&value))))
        }

        /// Some key is strictly greater than `value`.
        pub fn gt(&self, value: impl FlussoValue<$kind>) -> MapRange<S> {
            MapRange::new(self.keys.range(exclusive($to_value(&value)), None))
        }

        /// Some key is greater than or equal to `value`.
        pub fn gte(&self, value: impl FlussoValue<$kind>) -> MapRange<S> {
            MapRange::new(self.keys.range(inclusive($to_value(&value)), None))
        }

        /// Some key is in the inclusive range `[low, high]`.
        pub fn between(
            &self,
            low: impl FlussoValue<$kind>,
            high: impl FlussoValue<$kind>,
        ) -> MapRange<S> {
            MapRange::new(
                self.keys
                    .range(inclusive($to_value(&low)), inclusive($to_value(&high))),
            )
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
            keys: MapPath,
            _marker: PhantomData<fn() -> (S, Sub)>,
        }

        impl<S, Sub> $Name<S, Sub> {
            fn handle(keys: MapPath) -> Self {
                Self {
                    keys,
                    _marker: PhantomData,
                }
            }

            presence_ops!();
        }

        impl<S> $Name<S, WithSubfields> {
            /// A handle for a map whose keys carry flusso's auto subfields.
            pub fn at(path: impl Into<String>) -> Self {
                Self::handle(MapPath::new(path, ""))
            }

            /// Case- and accent-insensitive substring match across **every**
            /// key: `contains("excav")` finds "Excavator". An empty or
            /// whitespace-only value matches nothing. Returns a
            /// [`MapContains`] builder; see it for the rendered shape.
            pub fn contains(&self, value: impl AsRef<str>) -> MapContains<S> {
                MapContains::new(contains_body(&self.keys, value.as_ref()))
            }

            /// The case/accent-insensitive `.keyword_lowercase` subfield of
            /// every key, as a keyword map: `keyword_lowercase().eq("sale")`
            /// is a case-insensitive exact match in any key.
            pub fn keyword_lowercase(&self) -> KeywordMap<S, NoSubfields> {
                KeywordMap::handle(self.keys.view(KEYWORD_LOWERCASE_SUFFIX))
            }
        }

        impl<S> $Name<S, NoSubfields> {
            /// A handle for a map whose keys have no auto subfields.
            pub fn leaf(path: impl Into<String>) -> Self {
                Self::handle(MapPath::new(path, ""))
            }
        }
    };
}

string_map_handle!(
    /// A dynamic-key object whose values are analyzed full text (`map` with a
    /// `text`/`identifier` value kind). [`key`](Self::key) yields a [`Text`]
    /// leaf; the analyzed operators ([`matches`](Self::matches),
    /// [`match_phrase`](Self::match_phrase), …) run across every key. With
    /// subfields: [`any_of`](Self::any_of), the subfield views,
    /// [`contains`](Self::contains) and [`sort_key`](Self::sort_key).
    TextMap
);
string_map_handle!(
    /// A dynamic-key object whose values are exact strings (`map` with a
    /// `keyword`/`enum`/`uuid` value kind). [`key`](Self::key) yields a
    /// [`Keyword`] leaf. The exact operators (`eq`, `any_of`, `prefix`,
    /// `wildcard`, `regexp`, `fuzzy`) run across every key, over each key's
    /// exact `.keyword` subfield when it has subfields. The subfield views
    /// ([`text`](Self::text), [`keyword_lowercase`](Self::keyword_lowercase))
    /// and [`contains`](Self::contains) need subfields.
    /// [`sort_key`](Self::sort_key) orders by a key, with `.or(..)` fallback.
    KeywordMap
);

impl<S, Sub> TextMap<S, Sub> {
    /// A specific runtime key → a [`Text`] leaf, mapped and queried exactly like
    /// a plain `text` field: the same subfield accessors and, with subfields,
    /// the same `.asc()`/`.desc()` (on `.keyword_lowercase`).
    pub fn key(&self, key: impl AsRef<str>) -> Text<S, Sub> {
        Text::handle(self.keys.key(key.as_ref()))
    }

    /// Analyzed match in any key (`best_fields`: the best key wins).
    pub fn matches(&self, value: impl Into<String>) -> MapMatch<S> {
        MapMatch::new(
            self.keys
                .multi_match(value.into(), MultiMatchType::BestFields),
        )
    }

    /// Analyzed phrase match (terms in order) in any key.
    pub fn match_phrase(&self, value: impl Into<String>) -> MapMatch<S> {
        MapMatch::new(self.keys.multi_match(value.into(), MultiMatchType::Phrase))
    }

    /// Analyzed phrase-prefix match (search-as-you-type) in any key.
    ///
    /// Renders `lenient: true`: OpenSearch refuses a phrase prefix on a
    /// keyword field, and `path.*` reaches each key's keyword subfields, so
    /// they are skipped instead of failing the query.
    pub fn match_phrase_prefix(&self, value: impl Into<String>) -> MapMatch<S> {
        MapMatch::new(
            self.keys
                .multi_match(value.into(), MultiMatchType::PhrasePrefix),
        )
        .lenient(true)
    }

    /// Bool-prefix match in any key: every term a `term` except the last,
    /// which is a prefix.
    pub fn match_bool_prefix(&self, value: impl Into<String>) -> MapMatch<S> {
        MapMatch::new(
            self.keys
                .multi_match(value.into(), MultiMatchType::BoolPrefix),
        )
    }

    /// Analyzed match tolerant of typos in any key: sugar for
    /// `matches(v).fuzziness(Fuzziness::Auto)`.
    pub fn matches_fuzzy(&self, value: impl Into<String>) -> MapMatch<S> {
        self.matches(value).fuzziness(Fuzziness::Auto)
    }
}

impl<S> TextMap<S, WithSubfields> {
    /// Exact match against any of `values` in any key, on the `.keyword`
    /// subfield (like `Text::any_of`).
    pub fn any_of(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Keyword>>,
    ) -> MapTerm<S> {
        self.keyword().any_of(values)
    }

    /// The exact `.keyword` subfield of every key, as a keyword map: exact
    /// `eq` / `any_of` / `prefix` / `wildcard` across keys.
    pub fn keyword(&self) -> KeywordMap<S, NoSubfields> {
        KeywordMap::handle(self.keys.view(KEYWORD_SUFFIX))
    }

    /// Sort by this key, with optional fallback: `sort_key("it").or("en")`
    /// orders by `it`, else `en` (language fallback), case- and
    /// accent-insensitively. Returns a [`MapKeySort`]; pass it to
    /// [`SortBuilder::by`](crate::SortBuilder::by) or `.asc()`/`.desc()`.
    /// Needs the auto subfields, like sorting a plain `text` field.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        self.keys.sort_key(key, MapSortValueKind::Folded)
    }
}

impl<S, Sub> KeywordMap<S, Sub> {
    /// A specific runtime key → a [`Keyword`] leaf, mapped and queried exactly
    /// like a plain `keyword` field (`eq` is an exact `term`; sorts directly).
    pub fn key(&self, key: impl AsRef<str>) -> Keyword<S, Sub> {
        Keyword::handle(self.keys.key(key.as_ref()))
    }
}

impl<S> KeywordMap<S, WithSubfields> {
    /// The analyzed `.text` subfield of every key, as a text map: full text
    /// across keys (`text().matches(..)`).
    pub fn text(&self) -> TextMap<S, NoSubfields> {
        TextMap::handle(self.keys.view(TEXT_SUFFIX))
    }

    /// The exact `.keyword` subfield of every key, as a keyword map without
    /// subfields. The exact operators below run on it; `path.*` itself would
    /// also reach each key's `.text` and `.keyword_lowercase`.
    pub fn keyword(&self) -> KeywordMap<S, NoSubfields> {
        KeywordMap::handle(self.keys.view(KEYWORD_SUFFIX))
    }

    /// Some key equals `value` exactly. Accepts a `String`/`&str` or a
    /// `#[derive(FlussoValue)]` keyword type, matched by its serde string.
    ///
    /// ```
    /// use flusso_query::{AsQuery, KeywordMap, Root};
    /// use serde_json::json;
    ///
    /// let q = KeywordMap::<Root>::at("codes").eq("0049").prefer("ean", 2.0);
    /// assert_eq!(
    ///     q.to_value(),
    ///     json!({ "query_string": {
    ///         "query": r#""0049""#,
    ///         "fields": ["codes.ean.keyword^2", "codes.*.keyword"]
    ///     } })
    /// );
    /// ```
    pub fn eq(&self, value: impl FlussoValue<kind::Keyword>) -> MapTerm<S> {
        self.keyword().eq(value)
    }

    /// Some key equals any of `values` exactly.
    pub fn any_of(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Keyword>>,
    ) -> MapTerm<S> {
        self.keyword().any_of(values)
    }

    /// Some key starts with `value`.
    pub fn prefix(&self, value: impl Into<String>) -> MapPrefix<S> {
        self.keyword().prefix(value)
    }

    /// Some key matches the wildcard `pattern`: `?` matches one character,
    /// `*` matches any run, and `\` escapes the next character. An empty
    /// pattern matches nothing, as on one key.
    pub fn wildcard(&self, pattern: impl Into<String>) -> MapWildcard<S> {
        self.keyword().wildcard(pattern)
    }

    /// Some key matches the regular expression `pattern` (Lucene syntax,
    /// anchored to the whole value).
    pub fn regexp(&self, pattern: impl Into<String>) -> MapRegexp<S> {
        self.keyword().regexp(pattern)
    }

    /// Some key is within the default `AUTO` edit distance of `value`. An
    /// empty value matches only the empty value, as `AUTO` does on one key.
    pub fn fuzzy(&self, value: impl Into<String>) -> MapFuzzy<S> {
        self.keyword().fuzzy(value)
    }

    /// Sort by this key, with optional fallback (`sort_key("a").or("b")`),
    /// case- and accent-insensitively. Returns a [`MapKeySort`]; see it for the
    /// rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        self.keys.sort_key(key, MapSortValueKind::Folded)
    }
}

impl<S> KeywordMap<S, NoSubfields> {
    /// Some key equals `value` exactly. Accepts a `String`/`&str` or a
    /// `#[derive(FlussoValue)]` keyword type, matched by its serde string.
    pub fn eq(&self, value: impl FlussoValue<kind::Keyword>) -> MapTerm<S> {
        MapTerm::new(self.keys.terms([keyword_term(&value)]))
    }

    /// Some key equals any of `values` exactly.
    pub fn any_of(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Keyword>>,
    ) -> MapTerm<S> {
        MapTerm::new(
            self.keys
                .terms(values.into_iter().map(|v| keyword_term(&v))),
        )
    }

    /// Some key starts with `value`.
    pub fn prefix(&self, value: impl Into<String>) -> MapPrefix<S> {
        let query = format!("{}*", escaped(&value.into()));
        MapPrefix::new(self.keys.query_string(Some(query)))
    }

    /// Some key matches the wildcard `pattern`: `?` matches one character,
    /// `*` matches any run, and `\` escapes the next character. An empty
    /// pattern matches nothing, as on one key.
    pub fn wildcard(&self, pattern: impl Into<String>) -> MapWildcard<S> {
        let pattern = pattern.into();
        let query = (!pattern.is_empty()).then(|| wildcard_term(&pattern));
        MapWildcard::new(self.keys.query_string(query))
    }

    /// Some key matches the regular expression `pattern` (Lucene syntax,
    /// anchored to the whole value).
    pub fn regexp(&self, pattern: impl Into<String>) -> MapRegexp<S> {
        MapRegexp::new(self.keys.query_string(Some(regexp_term(&pattern.into()))))
    }

    /// Some key is within the default `AUTO` edit distance of `value`. An
    /// empty value matches only the empty value, as `AUTO` does on one key.
    pub fn fuzzy(&self, value: impl Into<String>) -> MapFuzzy<S> {
        let value = value.into();
        let query = if value.is_empty() {
            EMPTY_VALUE.to_string()
        } else {
            format!("{}~", escaped(&value))
        };
        MapFuzzy::new(self.keys.query_string(Some(query)))
    }

    /// Sort by this key, with optional fallback (`sort_key("a").or("b")`),
    /// case-insensitively (no accent folding without the subfields). Returns a
    /// [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        self.keys.sort_key(key, MapSortValueKind::Exact)
    }
}

/// A dynamic-key object whose values are dates (`map` with a
/// `date`/`timestamp` value kind). [`key`](Self::key) yields a [`Date`] leaf;
/// `eq` / `any_of` / the ranges run across every key.
/// [`sort_key`](Self::sort_key) orders by a key, with `.or(..)` fallback.
#[derive(Debug, Clone)]
pub struct DateMap<S = Root> {
    keys: MapPath,
    _scope: PhantomData<fn() -> S>,
}

impl<S> DateMap<S> {
    /// A handle for the date map at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            keys: MapPath::new(path, ""),
            _scope: PhantomData,
        }
    }

    /// A specific runtime key → a fully-typed [`Date`] leaf for range/exact
    /// operators. A `date` map key is doc-valued on its bare path, so the leaf
    /// sorts directly; [`sort_key`](Self::sort_key) adds ordered key fallback.
    pub fn key(&self, key: impl AsRef<str>) -> Date<S> {
        Date::at(self.keys.key(key.as_ref()))
    }

    /// Sort by this key, with optional fallback (`sort_key("eu").or("us")`), by
    /// epoch millis. Returns a [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        self.keys.sort_key(key, MapSortValueKind::Date)
    }

    ordered_ops!(
        kind::Date,
        date_value,
        "an ISO-8601 string, date math, or a `chrono` type"
    );

    presence_ops!();
}

/// A dynamic-key object whose values are numbers (`map` with a numeric value
/// kind — `short`…`double`, `decimal`). [`key`](Self::key) yields a [`Number`]
/// leaf; `eq` / `any_of` / the ranges run across every key.
///
/// `K` is the declared kind, as for a plain field: a `decimal` map is a
/// `NumberMap<kind::Decimal>`, so its keys take a `Decimal` or an integer,
/// never an `f64`.
#[derive(Debug, Clone)]
pub struct NumberMap<K, S = Root> {
    keys: MapPath,
    _marker: PhantomData<fn() -> (K, S)>,
}

impl<K, S> NumberMap<K, S> {
    /// A handle for the numeric map at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            keys: MapPath::new(path, ""),
            _marker: PhantomData,
        }
    }

    /// A specific runtime key → a [`Number`] leaf handle of value kind `K`,
    /// queried like any other numeric field. A numeric map key is doc-valued on
    /// its bare path, so the leaf sorts directly; [`sort_key`](Self::sort_key)
    /// adds ordered key fallback.
    pub fn key(&self, key: impl AsRef<str>) -> Number<K, S> {
        Number::at(self.keys.key(key.as_ref()))
    }

    /// Sort by this key, with optional fallback (`sort_key("usd").or("eur")`).
    /// Returns a [`MapKeySort`]; see it for the rendered shape.
    pub fn sort_key(&self, key: impl Into<String>) -> MapKeySort<S> {
        self.keys.sort_key(key, MapSortValueKind::Number)
    }

    ordered_ops!(K, number_value, "a value of the map's numeric kind");

    presence_ops!();
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

    /// `None` when there is no field: only preferred keys, and none preferred.
    fn into_value(self) -> Option<Value> {
        let mut fields: Vec<Value> = self.preferred.into_iter().map(Value::String).collect();
        if self.include_all {
            fields.push(Value::String(format!("{}.*{}", self.path, self.suffix)));
        }
        (!fields.is_empty()).then_some(Value::Array(fields))
    }
}
