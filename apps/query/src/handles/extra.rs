//! Standalone query types that aren't tied to one field handle: document-id
//! lookup ([`ids`]), the user-facing full-text strings ([`query_string`],
//! [`simple_query_string`], [`combined_fields`]), and the advanced-relevance
//! queries ([`script`], [`script_score`], [`distance_feature`], [`rank_feature`],
//! [`more_like_this`]). Each returns a builder implementing
//! [`AsQuery`](crate::AsQuery).

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Common, Text, common_opts, wrap};
use crate::query::{AsQuery, Query, Root};

fn string_array(values: impl IntoIterator<Item = impl Into<String>>) -> Vec<Value> {
    values
        .into_iter()
        .map(|v| Value::String(v.into()))
        .collect()
}

fn field_specs<S>(fields: impl IntoIterator<Item = Text<S>>) -> Vec<Value> {
    fields
        .into_iter()
        .map(|f| Value::String(f.field_spec()))
        .collect()
}

/// Match documents by a list of `_id` values — typed get-many-by-id.
pub fn ids<S>(values: impl IntoIterator<Item = impl Into<String>>) -> IdsQuery<S> {
    IdsQuery {
        values: string_array(values),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// An `ids` clause.
#[derive(Debug, Clone)]
pub struct IdsQuery<S = Root> {
    values: Vec<Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> IdsQuery<S> {
    common_opts!(common);
}

impl<S> AsQuery<S> for IdsQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert("values".to_string(), Value::Array(self.values));
        self.common.write(&mut body);
        Some(wrap("ids", body))
    }
}

/// Full Lucene query-string syntax (power users; can error on malformed input).
/// Target fields with [`default_field`](QueryStringQuery::default_field) or
/// [`fields`](QueryStringQuery::fields).
pub fn query_string<S>(query: impl Into<String>) -> QueryStringQuery<S> {
    QueryStringQuery {
        query: query.into(),
        opts: Map::new(),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `query_string` clause.
#[derive(Debug, Clone)]
pub struct QueryStringQuery<S = Root> {
    query: String,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> QueryStringQuery<S> {
    /// The single field searched when the query text names none.
    #[must_use]
    pub fn default_field(mut self, field: impl Into<String>) -> Self {
        self.opts
            .insert("default_field".to_string(), Value::String(field.into()));
        self
    }

    /// The fields searched (each may carry a `^weight` via [`Text::boosted`]).
    #[must_use]
    pub fn fields(mut self, fields: impl IntoIterator<Item = Text<S>>) -> Self {
        self.opts
            .insert("fields".to_string(), Value::Array(field_specs(fields)));
        self
    }

    /// Default boolean operator between terms (`"OR"` default / `"AND"`).
    #[must_use]
    pub fn default_operator(mut self, operator: impl Into<String>) -> Self {
        self.opts.insert(
            "default_operator".to_string(),
            Value::String(operator.into()),
        );
        self
    }

    /// Override the analyzer applied to the query text.
    #[must_use]
    pub fn analyzer(mut self, analyzer: impl Into<String>) -> Self {
        self.opts
            .insert("analyzer".to_string(), Value::String(analyzer.into()));
        self
    }

    /// Ignore format errors (e.g. text against a numeric field).
    #[must_use]
    pub fn lenient(mut self, lenient: bool) -> Self {
        self.opts
            .insert("lenient".to_string(), Value::Bool(lenient));
        self
    }

    /// Set any other `query_string` parameter verbatim.
    #[must_use]
    pub fn param(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.opts.insert(key.into(), value.into());
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for QueryStringQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.opts;
        body.insert("query".to_string(), Value::String(self.query));
        self.common.write(&mut body);
        Some(wrap("query_string", body))
    }
}

/// Lenient search-bar syntax over chosen fields — never errors on bad input.
pub fn simple_query_string<S>(query: impl Into<String>) -> SimpleQueryStringQuery<S> {
    SimpleQueryStringQuery {
        query: query.into(),
        opts: Map::new(),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `simple_query_string` clause.
#[derive(Debug, Clone)]
pub struct SimpleQueryStringQuery<S = Root> {
    query: String,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> SimpleQueryStringQuery<S> {
    /// The fields searched (each may carry a `^weight` via [`Text::boosted`]).
    #[must_use]
    pub fn fields(mut self, fields: impl IntoIterator<Item = Text<S>>) -> Self {
        self.opts
            .insert("fields".to_string(), Value::Array(field_specs(fields)));
        self
    }

    /// Default boolean operator between terms (`"OR"` default / `"AND"`).
    #[must_use]
    pub fn default_operator(mut self, operator: impl Into<String>) -> Self {
        self.opts.insert(
            "default_operator".to_string(),
            Value::String(operator.into()),
        );
        self
    }

    /// Override the analyzer applied to the query text.
    #[must_use]
    pub fn analyzer(mut self, analyzer: impl Into<String>) -> Self {
        self.opts
            .insert("analyzer".to_string(), Value::String(analyzer.into()));
        self
    }

    /// Enabled syntax features (e.g. `"AND|OR|PREFIX"`).
    #[must_use]
    pub fn flags(mut self, flags: impl Into<String>) -> Self {
        self.opts
            .insert("flags".to_string(), Value::String(flags.into()));
        self
    }

    /// How many terms must match (e.g. `"75%"`, `"2"`).
    #[must_use]
    pub fn minimum_should_match(mut self, value: impl Into<String>) -> Self {
        self.opts.insert(
            "minimum_should_match".to_string(),
            Value::String(value.into()),
        );
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for SimpleQueryStringQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.opts;
        body.insert("query".to_string(), Value::String(self.query));
        self.common.write(&mut body);
        Some(wrap("simple_query_string", body))
    }
}

/// Term-centric full-text across several fields, treating them as one combined
/// field (`combined_fields`).
pub fn combined_fields<S>(
    query: impl Into<String>,
    fields: impl IntoIterator<Item = Text<S>>,
) -> CombinedFieldsQuery<S> {
    CombinedFieldsQuery {
        query: query.into(),
        fields: field_specs(fields),
        opts: Map::new(),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `combined_fields` clause.
#[derive(Debug, Clone)]
pub struct CombinedFieldsQuery<S = Root> {
    query: String,
    fields: Vec<Value>,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> CombinedFieldsQuery<S> {
    /// Combine analyzed terms with `"AND"` or `"OR"`.
    #[must_use]
    pub fn operator(mut self, operator: impl Into<String>) -> Self {
        self.opts
            .insert("operator".to_string(), Value::String(operator.into()));
        self
    }

    /// How many terms must match (e.g. `"75%"`, `"2"`).
    #[must_use]
    pub fn minimum_should_match(mut self, value: impl Into<String>) -> Self {
        self.opts.insert(
            "minimum_should_match".to_string(),
            Value::String(value.into()),
        );
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for CombinedFieldsQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.opts;
        body.insert("query".to_string(), Value::String(self.query));
        body.insert("fields".to_string(), Value::Array(self.fields));
        self.common.write(&mut body);
        Some(wrap("combined_fields", body))
    }
}

/// Keep documents for which a painless `source` returns true (a non-scoring
/// filter).
pub fn script<S>(source: impl Into<String>) -> ScriptQuery<S> {
    ScriptQuery {
        source: source.into(),
        params: None,
        lang: None,
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `script` clause.
#[derive(Debug, Clone)]
pub struct ScriptQuery<S = Root> {
    source: String,
    params: Option<Value>,
    lang: Option<String>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> ScriptQuery<S> {
    /// Bound parameters available to the script as `params`.
    #[must_use]
    pub fn params(mut self, params: Value) -> Self {
        self.params = Some(params);
        self
    }

    /// Scripting language (default `"painless"`).
    #[must_use]
    pub fn lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = Some(lang.into());
        self
    }

    common_opts!(common);
}

fn script_object(source: String, params: Option<Value>, lang: Option<String>) -> Value {
    let mut script = Map::new();
    script.insert("source".to_string(), Value::String(source));
    if let Some(params) = params {
        script.insert("params".to_string(), params);
    }
    if let Some(lang) = lang {
        script.insert("lang".to_string(), Value::String(lang));
    }
    Value::Object(script)
}

impl<S> AsQuery<S> for ScriptQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert(
            "script".to_string(),
            script_object(self.source, self.params, self.lang),
        );
        self.common.write(&mut body);
        Some(wrap("script", body))
    }
}

/// Recompute `query`'s score with a painless `source` (`script_score`).
pub fn script_score<S>(query: impl AsQuery<S>, source: impl Into<String>) -> ScriptScoreQuery<S> {
    ScriptScoreQuery {
        query: query
            .into_query()
            .map_or_else(super::match_all_value, |q| q.to_value()),
        source: source.into(),
        params: None,
        min_score: None,
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `script_score` clause.
#[derive(Debug, Clone)]
pub struct ScriptScoreQuery<S = Root> {
    query: Value,
    source: String,
    params: Option<Value>,
    min_score: Option<f32>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> ScriptScoreQuery<S> {
    /// Bound parameters available to the script as `params`.
    #[must_use]
    pub fn params(mut self, params: Value) -> Self {
        self.params = Some(params);
        self
    }

    /// Exclude hits whose recomputed score is below this.
    #[must_use]
    pub fn min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for ScriptScoreQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert("query".to_string(), self.query);
        body.insert(
            "script".to_string(),
            script_object(self.source, self.params, None),
        );
        if let Some(min_score) = self.min_score {
            body.insert("min_score".to_string(), Value::from(min_score));
        }
        self.common.write(&mut body);
        Some(wrap("script_score", body))
    }
}

/// Boost by proximity of a `field` (date or geo) to `origin`, decaying over
/// `pivot` (`distance_feature`).
pub fn distance_feature<S>(
    field: impl Into<String>,
    origin: impl Into<Value>,
    pivot: impl Into<String>,
) -> DistanceFeatureQuery<S> {
    DistanceFeatureQuery {
        field: field.into(),
        origin: origin.into(),
        pivot: pivot.into(),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `distance_feature` clause.
#[derive(Debug, Clone)]
pub struct DistanceFeatureQuery<S = Root> {
    field: String,
    origin: Value,
    pivot: String,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> DistanceFeatureQuery<S> {
    common_opts!(common);
}

impl<S> AsQuery<S> for DistanceFeatureQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert("field".to_string(), Value::String(self.field));
        body.insert("origin".to_string(), self.origin);
        body.insert("pivot".to_string(), Value::String(self.pivot));
        self.common.write(&mut body);
        Some(wrap("distance_feature", body))
    }
}

/// Boost by a `rank_feature` / `rank_features` field's stored value. The
/// default saturation function applies unless one is chosen.
pub fn rank_feature<S>(field: impl Into<String>) -> RankFeatureQuery<S> {
    RankFeatureQuery {
        field: field.into(),
        function: None,
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `rank_feature` clause.
#[derive(Debug, Clone)]
pub struct RankFeatureQuery<S = Root> {
    field: String,
    function: Option<(&'static str, Value)>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> RankFeatureQuery<S> {
    /// The `saturation` function with an explicit `pivot`.
    #[must_use]
    pub fn saturation(mut self, pivot: f32) -> Self {
        let mut body = Map::new();
        body.insert("pivot".to_string(), Value::from(pivot));
        self.function = Some(("saturation", Value::Object(body)));
        self
    }

    /// The `log` function with a `scaling_factor`.
    #[must_use]
    pub fn log(mut self, scaling_factor: f32) -> Self {
        let mut body = Map::new();
        body.insert("scaling_factor".to_string(), Value::from(scaling_factor));
        self.function = Some(("log", Value::Object(body)));
        self
    }

    /// The `sigmoid` function with `pivot` and `exponent`.
    #[must_use]
    pub fn sigmoid(mut self, pivot: f32, exponent: f32) -> Self {
        let mut body = Map::new();
        body.insert("pivot".to_string(), Value::from(pivot));
        body.insert("exponent".to_string(), Value::from(exponent));
        self.function = Some(("sigmoid", Value::Object(body)));
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for RankFeatureQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert("field".to_string(), Value::String(self.field));
        if let Some((name, function)) = self.function {
            body.insert(name.to_string(), function);
        }
        self.common.write(&mut body);
        Some(wrap("rank_feature", body))
    }
}

/// Find documents similar to `like` text(s) across `fields` (`more_like_this`).
pub fn more_like_this<S>(
    fields: impl IntoIterator<Item = Text<S>>,
    like: impl IntoIterator<Item = impl Into<String>>,
) -> MoreLikeThisQuery<S> {
    MoreLikeThisQuery {
        fields: field_specs(fields),
        like: string_array(like),
        opts: Map::new(),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `more_like_this` clause.
#[derive(Debug, Clone)]
pub struct MoreLikeThisQuery<S = Root> {
    fields: Vec<Value>,
    like: Vec<Value>,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> MoreLikeThisQuery<S> {
    /// Ignore source terms occurring fewer than this many times.
    #[must_use]
    pub fn min_term_freq(mut self, min_term_freq: u32) -> Self {
        self.opts
            .insert("min_term_freq".to_string(), Value::from(min_term_freq));
        self
    }

    /// Cap on the terms selected from the source text.
    #[must_use]
    pub fn max_query_terms(mut self, max_query_terms: u32) -> Self {
        self.opts
            .insert("max_query_terms".to_string(), Value::from(max_query_terms));
        self
    }

    /// How many selected terms must match (e.g. `"30%"`).
    #[must_use]
    pub fn minimum_should_match(mut self, value: impl Into<String>) -> Self {
        self.opts.insert(
            "minimum_should_match".to_string(),
            Value::String(value.into()),
        );
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for MoreLikeThisQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.opts;
        body.insert("fields".to_string(), Value::Array(self.fields));
        body.insert("like".to_string(), Value::Array(self.like));
        self.common.write(&mut body);
        Some(wrap("more_like_this", body))
    }
}
