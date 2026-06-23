//! String field handles: the exact [`Keyword`] and the analyzed [`Text`], plus
//! the cross-field [`multi_match`].
//!
//! Every operator returns a small per-query builder ([`TermQuery`],
//! [`WildcardQuery`], [`MatchQuery`], …) carrying that query's options plus the
//! universal `boost` / `name`. Builders render lazily through
//! [`AsQuery`], so they drop straight into a clause — with no
//! options (the DSL shorthand) or with them (the expanded object form).

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{
    Common, FlussoValue, Fuzziness, MinimumShouldMatch, MultiMatchType, Operator, Sort, SortOrder,
    Sortable, TermsQuery, ZeroTermsQuery, common_opts, exists_q, keyed_value_query, kind, wrap,
};
use crate::FlussoDocument;
use crate::query::{AsQuery, Query, Root};

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

/// An exact-match (`term`) clause on a string field, with optional
/// `case_insensitive` plus the universal `boost` / `name`.
#[derive(Debug, Clone)]
pub struct TermQuery<S = Root> {
    path: String,
    value: Value,
    case_insensitive: Option<bool>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> TermQuery<S> {
    fn new(path: &str, value: Value) -> Self {
        Self {
            path: path.to_string(),
            value,
            case_insensitive: None,
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Match regardless of case.
    #[must_use]
    pub fn case_insensitive(mut self) -> Self {
        self.case_insensitive = Some(true);
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for TermQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut opts = Map::new();
        if let Some(ci) = self.case_insensitive {
            opts.insert("case_insensitive".to_string(), Value::Bool(ci));
        }
        self.common.write(&mut opts);
        Some(keyed_value_query(
            "term", &self.path, "value", self.value, opts,
        ))
    }
}

/// A `prefix` clause, with `case_insensitive` / `rewrite` plus `boost` / `name`.
#[derive(Debug, Clone)]
pub struct PrefixQuery<S = Root> {
    path: String,
    value: String,
    case_insensitive: Option<bool>,
    rewrite: Option<String>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> PrefixQuery<S> {
    fn new(path: &str, value: String) -> Self {
        Self {
            path: path.to_string(),
            value,
            case_insensitive: None,
            rewrite: None,
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Match regardless of case.
    #[must_use]
    pub fn case_insensitive(mut self) -> Self {
        self.case_insensitive = Some(true);
        self
    }

    /// The multi-term `rewrite` method (e.g. `"constant_score"`).
    #[must_use]
    pub fn rewrite(mut self, rewrite: impl Into<String>) -> Self {
        self.rewrite = Some(rewrite.into());
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for PrefixQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut opts = Map::new();
        if let Some(ci) = self.case_insensitive {
            opts.insert("case_insensitive".to_string(), Value::Bool(ci));
        }
        if let Some(rewrite) = self.rewrite {
            opts.insert("rewrite".to_string(), Value::String(rewrite));
        }
        self.common.write(&mut opts);
        Some(keyed_value_query(
            "prefix",
            &self.path,
            "value",
            Value::String(self.value),
            opts,
        ))
    }
}

/// A `wildcard` clause, with `case_insensitive` / `rewrite` plus `boost` / `name`.
#[derive(Debug, Clone)]
pub struct WildcardQuery<S = Root> {
    path: String,
    value: String,
    case_insensitive: Option<bool>,
    rewrite: Option<String>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> WildcardQuery<S> {
    fn new(path: &str, value: String) -> Self {
        Self {
            path: path.to_string(),
            value,
            case_insensitive: None,
            rewrite: None,
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Match regardless of case.
    #[must_use]
    pub fn case_insensitive(mut self) -> Self {
        self.case_insensitive = Some(true);
        self
    }

    /// The multi-term `rewrite` method.
    #[must_use]
    pub fn rewrite(mut self, rewrite: impl Into<String>) -> Self {
        self.rewrite = Some(rewrite.into());
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for WildcardQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut opts = Map::new();
        if let Some(ci) = self.case_insensitive {
            opts.insert("case_insensitive".to_string(), Value::Bool(ci));
        }
        if let Some(rewrite) = self.rewrite {
            opts.insert("rewrite".to_string(), Value::String(rewrite));
        }
        self.common.write(&mut opts);
        Some(keyed_value_query(
            "wildcard",
            &self.path,
            "value",
            Value::String(self.value),
            opts,
        ))
    }
}

/// A `regexp` clause, with `case_insensitive` / `flags` /
/// `max_determinized_states` plus `boost` / `name`.
#[derive(Debug, Clone)]
pub struct RegexpQuery<S = Root> {
    path: String,
    value: String,
    case_insensitive: Option<bool>,
    flags: Option<String>,
    max_determinized_states: Option<u32>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> RegexpQuery<S> {
    fn new(path: &str, value: String) -> Self {
        Self {
            path: path.to_string(),
            value,
            case_insensitive: None,
            flags: None,
            max_determinized_states: None,
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Match regardless of case.
    #[must_use]
    pub fn case_insensitive(mut self) -> Self {
        self.case_insensitive = Some(true);
        self
    }

    /// Enabled Lucene regex operators (e.g. `"INTERSECTION|COMPLEMENT|EMPTY"`).
    #[must_use]
    pub fn flags(mut self, flags: impl Into<String>) -> Self {
        self.flags = Some(flags.into());
        self
    }

    /// Cap on the automaton size compiled from the pattern.
    #[must_use]
    pub fn max_determinized_states(mut self, max: u32) -> Self {
        self.max_determinized_states = Some(max);
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for RegexpQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut opts = Map::new();
        if let Some(ci) = self.case_insensitive {
            opts.insert("case_insensitive".to_string(), Value::Bool(ci));
        }
        if let Some(flags) = self.flags {
            opts.insert("flags".to_string(), Value::String(flags));
        }
        if let Some(max) = self.max_determinized_states {
            opts.insert("max_determinized_states".to_string(), Value::from(max));
        }
        self.common.write(&mut opts);
        Some(keyed_value_query(
            "regexp",
            &self.path,
            "value",
            Value::String(self.value),
            opts,
        ))
    }
}

/// A `fuzzy` clause, with `fuzziness` / `prefix_length` / `max_expansions` /
/// `transpositions` plus `boost` / `name`.
#[derive(Debug, Clone)]
pub struct FuzzyQuery<S = Root> {
    path: String,
    value: String,
    fuzziness: Option<Value>,
    prefix_length: Option<u32>,
    max_expansions: Option<u32>,
    transpositions: Option<bool>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> FuzzyQuery<S> {
    fn new(path: &str, value: String) -> Self {
        Self {
            path: path.to_string(),
            value,
            fuzziness: None,
            prefix_length: None,
            max_expansions: None,
            transpositions: None,
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    /// Maximum edit distance ([`Fuzziness::Auto`] is the usual choice).
    #[must_use]
    pub fn fuzziness(mut self, fuzziness: Fuzziness) -> Self {
        self.fuzziness = Some(fuzziness.to_value());
        self
    }

    /// Leading characters that must match exactly.
    #[must_use]
    pub fn prefix_length(mut self, prefix_length: u32) -> Self {
        self.prefix_length = Some(prefix_length);
        self
    }

    /// Cap on the variations the term expands into.
    #[must_use]
    pub fn max_expansions(mut self, max_expansions: u32) -> Self {
        self.max_expansions = Some(max_expansions);
        self
    }

    /// Whether adjacent-character transpositions count as one edit.
    #[must_use]
    pub fn transpositions(mut self, transpositions: bool) -> Self {
        self.transpositions = Some(transpositions);
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for FuzzyQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut opts = Map::new();
        if let Some(fuzziness) = self.fuzziness {
            opts.insert("fuzziness".to_string(), fuzziness);
        }
        if let Some(prefix_length) = self.prefix_length {
            opts.insert("prefix_length".to_string(), Value::from(prefix_length));
        }
        if let Some(max_expansions) = self.max_expansions {
            opts.insert("max_expansions".to_string(), Value::from(max_expansions));
        }
        if let Some(transpositions) = self.transpositions {
            opts.insert("transpositions".to_string(), Value::Bool(transpositions));
        }
        self.common.write(&mut opts);
        Some(keyed_value_query(
            "fuzzy",
            &self.path,
            "value",
            Value::String(self.value),
            opts,
        ))
    }
}

/// Type-state marker: this `text`/`keyword` handle's field carries flusso's
/// auto subfields, so its subfield accessors (`.keyword()` / `.text()` /
/// `.keyword_lowercase()`) — and the sugar built on them (`Text::any_of`,
/// `Text::asc`) — are in scope. The default for a hand-written handle; the
/// derive stamps it on a field only when every OpenSearch sink has
/// `auto_subfields` on and the field declares no custom `fields`.
#[derive(Debug)]
pub enum WithSubfields {}

/// Type-state marker: this handle's field has **no** auto subfields, so the
/// subfield accessors don't exist — calling one is a compile error, not a 400.
/// The derive stamps it when subfields aren't provisioned; subfield leaves
/// (`.keyword()`) also carry it, since a subfield has no further subfields.
#[derive(Debug)]
pub enum NoSubfields {}

/// An exact, aggregatable string field (`keyword`, `enum`, `uuid`). `Sub` is a
/// [`WithSubfields`]/[`NoSubfields`] type-state marker gating the subfield
/// accessors.
#[derive(Debug, Clone)]
pub struct Keyword<S = Root, Sub = WithSubfields> {
    path: String,
    _marker: PhantomData<fn() -> (S, Sub)>,
}

impl<S, Sub> Keyword<S, Sub> {
    fn handle(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// Exact match. Accepts a `String`/`&str`, or any `#[derive(FlussoValue)]`
    /// keyword enum/newtype — matched against its serde string form
    /// (`Account::tier().eq(AccountTier::Pro)`).
    pub fn eq(&self, value: impl FlussoValue<kind::Keyword>) -> TermQuery<S> {
        TermQuery::new(&self.path, keyword_term(&value))
    }

    /// Match any of the given values (`String`/`&str` or keyword `FlussoValue` types).
    pub fn any_of(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Keyword>>,
    ) -> TermsQuery<S> {
        let array = values.into_iter().map(|v| keyword_term(&v)).collect();
        TermsQuery::new(&self.path, array)
    }

    /// Prefix match.
    pub fn prefix(&self, value: impl Into<String>) -> PrefixQuery<S> {
        PrefixQuery::new(&self.path, value.into())
    }

    /// Wildcard match — `?` matches one character, `*` matches any run.
    pub fn wildcard(&self, pattern: impl Into<String>) -> WildcardQuery<S> {
        WildcardQuery::new(&self.path, pattern.into())
    }

    /// Regular-expression match (Lucene regex syntax, anchored to the whole term).
    pub fn regexp(&self, pattern: impl Into<String>) -> RegexpQuery<S> {
        RegexpQuery::new(&self.path, pattern.into())
    }

    /// Fuzzy term match — tolerates typos within the default `AUTO` distance.
    pub fn fuzzy(&self, value: impl Into<String>) -> FuzzyQuery<S> {
        FuzzyQuery::new(&self.path, value.into())
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

impl<S: FlussoDocument, Sub> Sortable for Keyword<S, Sub> {
    fn asc(&self) -> Sort {
        Sort::field::<S>(&self.path, SortOrder::Asc)
    }
    fn desc(&self) -> Sort {
        Sort::field::<S>(&self.path, SortOrder::Desc)
    }
}

impl<S> Keyword<S, WithSubfields> {
    pub fn at(path: impl Into<String>) -> Self {
        Self::handle(path)
    }

    /// The full-text `.text` subfield flusso auto-creates on a `keyword` field
    /// (analyzed with `flusso_code`), so a keyword is still searchable in a
    /// search box. Only in scope when the field carries auto subfields.
    pub fn text(&self) -> Text<S, NoSubfields> {
        Text::leaf(format!("{}.text", self.path))
    }

    /// The case/accent-insensitive `.keyword_lowercase` subfield flusso
    /// auto-creates — for case-insensitive exact match and sort. Only in scope
    /// when the field carries auto subfields.
    pub fn keyword_lowercase(&self) -> Keyword<S, NoSubfields> {
        Keyword::leaf(format!("{}.keyword_lowercase", self.path))
    }
}

impl<S> Keyword<S, NoSubfields> {
    /// Construct a handle for a field known to have no auto subfields (a
    /// subfield leaf, or a field the derive resolved as un-subfielded).
    pub fn leaf(path: impl Into<String>) -> Self {
        Self::handle(path)
    }
}

/// A `match`-family clause (`match`, `match_phrase`, `match_phrase_prefix`,
/// `match_bool_prefix`): the analyzed `query` value plus whichever options the
/// kind supports, all written under the field as an object. The `kind`
/// selects the wrapper and which setters are meaningful; unset options are
/// simply omitted.
#[derive(Debug, Clone)]
pub struct MatchQuery<S = Root> {
    wrapper: &'static str,
    path: String,
    value: String,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> MatchQuery<S> {
    fn new(wrapper: &'static str, path: &str, value: String) -> Self {
        Self {
            wrapper,
            path: path.to_string(),
            value,
            opts: Map::new(),
            common: Common::default(),
            _scope: PhantomData,
        }
    }

    fn set(mut self, key: &str, value: Value) -> Self {
        self.opts.insert(key.to_string(), value);
        self
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

    /// Phrase `slop` — allowed positional gap (phrase / phrase-prefix).
    #[must_use]
    pub fn slop(self, slop: u32) -> Self {
        self.set("slop", Value::from(slop))
    }

    /// Behavior when analysis yields no terms ([`ZeroTermsQuery::None`] or
    /// [`ZeroTermsQuery::All`]).
    #[must_use]
    pub fn zero_terms_query(self, value: ZeroTermsQuery) -> Self {
        self.set(
            "zero_terms_query",
            Value::String(value.as_str().to_string()),
        )
    }

    /// Ignore format errors (e.g. analyzing text for a numeric subfield).
    #[must_use]
    pub fn lenient(self, lenient: bool) -> Self {
        self.set("lenient", Value::Bool(lenient))
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for MatchQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut opts = self.opts;
        self.common.write(&mut opts);
        Some(keyed_value_query(
            self.wrapper,
            &self.path,
            "query",
            Value::String(self.value),
            opts,
        ))
    }
}

/// An analyzed full-text field (`text`, `identifier`). No exact `eq`. `Sub` is
/// a [`WithSubfields`]/[`NoSubfields`] type-state marker gating the subfield
/// accessors (and the `any_of` / `asc` sugar built on them).
#[derive(Debug, Clone)]
pub struct Text<S = Root, Sub = WithSubfields> {
    path: String,
    boost: Option<f32>,
    _marker: PhantomData<fn() -> (S, Sub)>,
}

impl<S, Sub> Text<S, Sub> {
    fn handle(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            boost: None,
            _marker: PhantomData,
        }
    }

    /// Weight this field for [`multi_match`] (`field^weight`). Has no effect on
    /// this handle's own `matches` / `match_phrase` clauses, which carry their
    /// own `boost`.
    #[must_use]
    pub fn boosted(mut self, weight: f32) -> Self {
        self.boost = Some(weight);
        self
    }

    /// The field's path as listed in a [`multi_match`] `fields` array —
    /// `field^weight` when [`boosted`](Self::boosted), else the bare path.
    pub(crate) fn field_spec(&self) -> String {
        match self.boost {
            Some(weight) => format!("{}^{weight}", self.path),
            None => self.path.clone(),
        }
    }

    /// Analyzed match.
    pub fn matches(&self, value: impl Into<String>) -> MatchQuery<S> {
        MatchQuery::new("match", &self.path, value.into())
    }

    /// Analyzed phrase match (terms in order).
    pub fn match_phrase(&self, value: impl Into<String>) -> MatchQuery<S> {
        MatchQuery::new("match_phrase", &self.path, value.into())
    }

    /// Analyzed phrase-prefix match (search-as-you-type).
    pub fn match_phrase_prefix(&self, value: impl Into<String>) -> MatchQuery<S> {
        MatchQuery::new("match_phrase_prefix", &self.path, value.into())
    }

    /// Bool-prefix match — every term a `term` except the last, which is a
    /// prefix (the other half of search-as-you-type).
    pub fn match_bool_prefix(&self, value: impl Into<String>) -> MatchQuery<S> {
        MatchQuery::new("match_bool_prefix", &self.path, value.into())
    }

    /// Analyzed match tolerant of typos — sugar for
    /// `matches(v).fuzziness(Fuzziness::Auto)`.
    pub fn matches_fuzzy(&self, value: impl Into<String>) -> MatchQuery<S> {
        self.matches(value).fuzziness(Fuzziness::Auto)
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

impl<S> Text<S, WithSubfields> {
    pub fn at(path: impl Into<String>) -> Self {
        Self::handle(path)
    }

    /// Exact match against **any** of the given values, on the auto `.keyword`
    /// subfield. A `terms` query on the analyzed field would match raw tokens,
    /// which is rarely intended; this targets the exact subfield instead. Only
    /// in scope when the field carries auto subfields.
    pub fn any_of(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Keyword>>,
    ) -> TermsQuery<S> {
        self.keyword().any_of(values)
    }

    /// The exact `.keyword` subfield flusso auto-creates on a `text` field —
    /// for exact `eq` / `any_of`, `wildcard`, `prefix`, and exact sort. (A
    /// wildcard belongs here, not on the analyzed handle, which matches tokens
    /// not the whole value.) Only in scope when the field carries auto subfields.
    pub fn keyword(&self) -> Keyword<S, NoSubfields> {
        Keyword::leaf(format!("{}.keyword", self.path))
    }

    /// The case/accent-insensitive `.keyword_lowercase` subfield — for
    /// case-insensitive exact match and sort. Only in scope when the field
    /// carries auto subfields.
    pub fn keyword_lowercase(&self) -> Keyword<S, NoSubfields> {
        Keyword::leaf(format!("{}.keyword_lowercase", self.path))
    }

}

/// Sorting a `text` field goes through its case/accent-insensitive
/// `.keyword_lowercase` subfield (the analyzed field itself isn't sortable), so
/// it's [`Sortable`] only when the field carries auto subfields.
impl<S: FlussoDocument> Sortable for Text<S, WithSubfields> {
    fn asc(&self) -> Sort {
        self.keyword_lowercase().asc()
    }
    fn desc(&self) -> Sort {
        self.keyword_lowercase().desc()
    }
}

impl<S> Text<S, NoSubfields> {
    /// Construct a handle for a field known to have no auto subfields (a
    /// subfield leaf, or a field the derive resolved as un-subfielded).
    pub fn leaf(path: impl Into<String>) -> Self {
        Self::handle(path)
    }
}

/// A cross-field full-text query over several [`Text`] fields in the same scope.
/// Returns a [`MultiMatchQuery`] builder; weight individual fields with
/// [`Text::boosted`].
pub fn multi_match<S, Sub>(
    query: impl Into<String>,
    fields: impl IntoIterator<Item = Text<S, Sub>>,
) -> MultiMatchQuery<S> {
    MultiMatchQuery {
        query: query.into(),
        fields: fields.into_iter().map(|f| f.field_spec()).collect(),
        opts: Map::new(),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `multi_match` clause: one analyzed `query` over several `fields`, with the
/// `type` / `operator` / `fuzziness` / `tie_breaker` / `minimum_should_match`
/// options plus `boost` / `name`.
#[derive(Debug, Clone)]
pub struct MultiMatchQuery<S = Root> {
    query: String,
    fields: Vec<String>,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> MultiMatchQuery<S> {
    fn set(mut self, key: &str, value: Value) -> Self {
        self.opts.insert(key.to_string(), value);
        self
    }

    /// The scoring [`MultiMatchType`] (default `BestFields`).
    #[must_use]
    pub fn match_type(self, match_type: MultiMatchType) -> Self {
        self.set("type", Value::String(match_type.as_str().to_string()))
    }

    /// Combine analyzed terms with [`Operator::And`] or [`Operator::Or`].
    #[must_use]
    pub fn operator(self, operator: Operator) -> Self {
        self.set("operator", Value::String(operator.as_str().to_string()))
    }

    /// Edit distance ([`Fuzziness::Auto`] is the usual choice).
    #[must_use]
    pub fn fuzziness(self, fuzziness: Fuzziness) -> Self {
        self.set("fuzziness", fuzziness.to_value())
    }

    /// `tie_breaker` for `best_fields` — how much non-winning fields contribute.
    #[must_use]
    pub fn tie_breaker(self, tie_breaker: f32) -> Self {
        self.set("tie_breaker", Value::from(tie_breaker))
    }

    /// How many of the analyzed terms must match
    /// (e.g. `2`, `MinimumShouldMatch::percent(75)`).
    #[must_use]
    pub fn minimum_should_match(self, value: impl Into<MinimumShouldMatch>) -> Self {
        self.set("minimum_should_match", value.into().to_value())
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for MultiMatchQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.opts;
        body.insert("query".to_string(), Value::String(self.query));
        body.insert(
            "fields".to_string(),
            Value::Array(self.fields.into_iter().map(Value::String).collect()),
        );
        self.common.write(&mut body);
        Some(wrap("multi_match", body))
    }
}
