//! Compound / scoring queries that wrap other clauses rather than a field:
//! [`constant_score`], [`dis_max`], [`boosting`], and [`function_score`]. Each
//! returns a builder implementing [`AsQuery`], so it composes
//! exactly like a leaf query.

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Common, common_opts, wrap};
use crate::query::{AsQuery, Query, Root};

fn clause_value<S>(query: impl AsQuery<S>) -> Value {
    query
        .into_query()
        .map_or_else(super::match_all_value, |q| q.to_value())
}

/// Wrap a `filter` so every match scores the same fixed `boost` (default 1.0).
pub fn constant_score<S>(filter: impl AsQuery<S>) -> ConstantScoreQuery<S> {
    ConstantScoreQuery {
        filter: clause_value(filter),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `constant_score` clause. Set the fixed score via [`boost`](Self::boost).
#[derive(Debug, Clone)]
pub struct ConstantScoreQuery<S = Root> {
    filter: Value,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> ConstantScoreQuery<S> {
    common_opts!(common);
}

impl<S> AsQuery<S> for ConstantScoreQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert("filter".to_string(), self.filter);
        self.common.write(&mut body);
        Some(wrap("constant_score", body))
    }
}

/// Score by the single best-matching clause, optionally crediting the others
/// via [`tie_breaker`](DisMaxQuery::tie_breaker).
pub fn dis_max<S>(queries: impl IntoIterator<Item = impl AsQuery<S>>) -> DisMaxQuery<S> {
    DisMaxQuery {
        queries: queries.into_iter().map(clause_value).collect(),
        tie_breaker: None,
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `dis_max` clause.
#[derive(Debug, Clone)]
pub struct DisMaxQuery<S = Root> {
    queries: Vec<Value>,
    tie_breaker: Option<f32>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> DisMaxQuery<S> {
    /// How much the non-winning clauses contribute (0.0–1.0).
    #[must_use]
    pub fn tie_breaker(mut self, tie_breaker: f32) -> Self {
        self.tie_breaker = Some(tie_breaker);
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for DisMaxQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert("queries".to_string(), Value::Array(self.queries));
        if let Some(tie_breaker) = self.tie_breaker {
            body.insert("tie_breaker".to_string(), Value::from(tie_breaker));
        }
        self.common.write(&mut body);
        Some(wrap("dis_max", body))
    }
}

/// Keep documents matching `positive`, but demote (don't exclude) those that
/// also match `negative` by `negative_boost` (0.0–1.0).
pub fn boosting<S>(
    positive: impl AsQuery<S>,
    negative: impl AsQuery<S>,
    negative_boost: f32,
) -> BoostingQuery<S> {
    BoostingQuery {
        positive: clause_value(positive),
        negative: clause_value(negative),
        negative_boost,
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `boosting` clause.
#[derive(Debug, Clone)]
pub struct BoostingQuery<S = Root> {
    positive: Value,
    negative: Value,
    negative_boost: f32,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> BoostingQuery<S> {
    common_opts!(common);
}

impl<S> AsQuery<S> for BoostingQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = Map::new();
        body.insert("positive".to_string(), self.positive);
        body.insert("negative".to_string(), self.negative);
        body.insert(
            "negative_boost".to_string(),
            Value::from(self.negative_boost),
        );
        self.common.write(&mut body);
        Some(wrap("boosting", body))
    }
}

/// Recompute relevance for `query` via one or more scoring functions.
pub fn function_score<S>(query: impl AsQuery<S>) -> FunctionScoreQuery<S> {
    FunctionScoreQuery {
        query: clause_value(query),
        functions: Vec::new(),
        opts: Map::new(),
        common: Common::default(),
        _scope: PhantomData,
    }
}

/// A `function_score` clause. Add functions with [`weight`](Self::weight) /
/// [`function`](Self::function) and tune combination with `score_mode` /
/// `boost_mode` / `max_boost` / `min_score`.
#[derive(Debug, Clone)]
pub struct FunctionScoreQuery<S = Root> {
    query: Value,
    functions: Vec<Value>,
    opts: Map<String, Value>,
    common: Common,
    _scope: PhantomData<fn() -> S>,
}

impl<S> FunctionScoreQuery<S> {
    /// Add a constant `weight` function applied to every match.
    #[must_use]
    pub fn weight(mut self, weight: f32) -> Self {
        let mut function = Map::new();
        function.insert("weight".to_string(), Value::from(weight));
        self.functions.push(Value::Object(function));
        self
    }

    /// Add a constant `weight` function applied only to matches of `filter`.
    #[must_use]
    pub fn weight_when(mut self, weight: f32, filter: impl AsQuery<S>) -> Self {
        let mut function = Map::new();
        function.insert("weight".to_string(), Value::from(weight));
        function.insert("filter".to_string(), clause_value(filter));
        self.functions.push(Value::Object(function));
        self
    }

    /// Add a raw function entry (e.g. a `field_value_factor`, `gauss`, or
    /// `script_score` object) — the escape hatch for the long tail of function
    /// types, still composed into the typed clause.
    #[must_use]
    pub fn function(mut self, function: Value) -> Self {
        self.functions.push(function);
        self
    }

    /// How the functions combine (`"multiply"` default / `"sum"` / `"avg"` /
    /// `"first"` / `"max"` / `"min"`).
    #[must_use]
    pub fn score_mode(mut self, score_mode: impl Into<String>) -> Self {
        self.opts
            .insert("score_mode".to_string(), Value::String(score_mode.into()));
        self
    }

    /// How the function score combines with the query score (`"multiply"`
    /// default / `"replace"` / `"sum"` / `"avg"` / `"max"` / `"min"`).
    #[must_use]
    pub fn boost_mode(mut self, boost_mode: impl Into<String>) -> Self {
        self.opts
            .insert("boost_mode".to_string(), Value::String(boost_mode.into()));
        self
    }

    /// Cap on the combined function score.
    #[must_use]
    pub fn max_boost(mut self, max_boost: f32) -> Self {
        self.opts
            .insert("max_boost".to_string(), Value::from(max_boost));
        self
    }

    /// Drop hits scoring below this threshold.
    #[must_use]
    pub fn min_score(mut self, min_score: f32) -> Self {
        self.opts
            .insert("min_score".to_string(), Value::from(min_score));
        self
    }

    common_opts!(common);
}

impl<S> AsQuery<S> for FunctionScoreQuery<S> {
    fn into_query(self) -> Option<Query<S>> {
        let mut body = self.opts;
        body.insert("query".to_string(), self.query);
        if !self.functions.is_empty() {
            body.insert("functions".to_string(), Value::Array(self.functions));
        }
        self.common.write(&mut body);
        Some(wrap("function_score", body))
    }
}
