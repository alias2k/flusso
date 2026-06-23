//! The [`Query`] type: an OpenSearch query clause, tagged with the **scope** it
//! was built in, composed with `and` / `or` / `not`.
//!
//! Scope `S` is the query context a handle belongs to. Root fields and flattened
//! object / to-one-join sub-fields are all [`Root`]; a `nested` array introduces
//! its own scope (the element type), so a nested query must be lifted
//! ([`Nested::any`](crate::Nested::any)/[`all`](crate::Nested::all)) before it can
//! join a `Root` query — the compiler enforces it.

use std::marker::PhantomData;

use serde_json::{Map, Value};

use crate::handles::MinimumShouldMatch;

/// The default query scope — the document root. Root fields and flattened
/// object / to-one-join sub-fields share it.
///
/// It is a [`FlussoDocument`](crate::FlussoDocument) with an empty `PATH`: a
/// root/flattened-object field sits under no `nested` boundary, so it sorts
/// plainly. (A `nested` array's element struct is its own scope, with a
/// non-empty `PATH`.)
#[derive(Debug, Clone, Copy)]
pub struct Root;

impl crate::FlussoDocument for Root {
    const PATH: &'static [crate::Segment] = &[];
}

/// A composable query clause in scope `S`.
///
/// Handles produce a `Query<S>` (`User::email().eq(…)` → `Query<Root>`,
/// `Order::status().eq(…)` → `Query<Order>`). `and`/`or`/`not` and the
/// [`crate::Search`] clauses only combine the **same** scope; a nested query is
/// lifted to its parent scope through the nested handle.
#[derive(Debug, Clone)]
pub struct Query<S = Root> {
    inner: Inner,
    _scope: PhantomData<fn() -> S>,
}

/// The scope-free internal representation (the scope is purely a type-level tag).
#[derive(Debug, Clone)]
enum Inner {
    Leaf(Value),
    Bool(BoolInner),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BoolInner {
    must: Vec<Inner>,
    filter: Vec<Inner>,
    should: Vec<Inner>,
    must_not: Vec<Inner>,
    minimum_should_match: Option<Value>,
    boost: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
enum Clause {
    Must,
    Filter,
    Should,
    MustNot,
}

impl BoolInner {
    pub(crate) fn is_empty(&self) -> bool {
        self.must.is_empty()
            && self.filter.is_empty()
            && self.should.is_empty()
            && self.must_not.is_empty()
    }

    fn push(&mut self, clause: Clause, inner: Inner) {
        match clause {
            Clause::Must => self.must.push(inner),
            Clause::Filter => self.filter.push(inner),
            Clause::Should => self.should.push(inner),
            Clause::MustNot => self.must_not.push(inner),
        }
    }

    /// True when only `clause`'s list is populated, so a new entry can be
    /// appended without changing the query's meaning.
    fn is_pure(&self, clause: Clause) -> bool {
        match clause {
            Clause::Must => {
                self.filter.is_empty() && self.should.is_empty() && self.must_not.is_empty()
            }
            Clause::Filter => {
                self.must.is_empty() && self.should.is_empty() && self.must_not.is_empty()
            }
            Clause::Should => {
                self.must.is_empty() && self.filter.is_empty() && self.must_not.is_empty()
            }
            Clause::MustNot => {
                self.must.is_empty() && self.filter.is_empty() && self.should.is_empty()
            }
        }
    }

    pub(crate) fn to_value(&self) -> Value {
        let mut body = Map::new();
        insert_clause(&mut body, "must", &self.must);
        insert_clause(&mut body, "filter", &self.filter);
        insert_clause(&mut body, "should", &self.should);
        insert_clause(&mut body, "must_not", &self.must_not);
        if let Some(msm) = &self.minimum_should_match {
            body.insert("minimum_should_match".to_string(), msm.clone());
        }
        if let Some(boost) = self.boost {
            body.insert("boost".to_string(), Value::from(boost));
        }
        let mut outer = Map::new();
        outer.insert("bool".to_string(), Value::Object(body));
        Value::Object(outer)
    }
}

fn insert_clause(target: &mut Map<String, Value>, key: &str, clauses: &[Inner]) {
    if clauses.is_empty() {
        return;
    }
    let array = clauses.iter().map(Inner::to_value).collect();
    target.insert(key.to_string(), Value::Array(array));
}

impl Inner {
    fn to_value(&self) -> Value {
        match self {
            Inner::Leaf(value) => value.clone(),
            Inner::Bool(bool_inner) => bool_inner.to_value(),
        }
    }
}

/// Combine `a` and `b` under `clause`, flattening when `a` is already a pure
/// bool for that clause (so `x.and(y).and(z)` is one bool with three `must`).
fn combine(a: Inner, b: Inner, clause: Clause) -> Inner {
    if let Inner::Bool(mut bool_inner) = a {
        if bool_inner.is_pure(clause) {
            bool_inner.push(clause, b);
            return Inner::Bool(bool_inner);
        }
        let mut combined = BoolInner::default();
        combined.push(clause, Inner::Bool(bool_inner));
        combined.push(clause, b);
        return Inner::Bool(combined);
    }
    let mut combined = BoolInner::default();
    combined.push(clause, a);
    combined.push(clause, b);
    Inner::Bool(combined)
}

/// Combine two optional clauses under `clause`, treating an absent side as the
/// identity (so `Some(a).or(None)` is just `a`). Both absent → `match_all`.
/// Backs the [`AsQuery`] `and`/`or` combinators, which any builder inherits.
fn combine_opt<S>(a: Option<Query<S>>, b: Option<Query<S>>, clause: Clause) -> Query<S> {
    match (a, b) {
        (Some(a), Some(b)) => Query::wrap(combine(a.inner, b.inner, clause)),
        (Some(only), None) | (None, Some(only)) => only,
        (None, None) => Query::match_all(),
    }
}

impl<S> Query<S> {
    /// Wrap a leaf clause value. Crate-internal: handles call this.
    pub(crate) fn leaf(value: Value) -> Self {
        Query {
            inner: Inner::Leaf(value),
            _scope: PhantomData,
        }
    }

    /// A `match_all` clause — the identity when combining absent clauses.
    pub(crate) fn match_all() -> Self {
        Query::leaf(crate::handles::match_all_value())
    }

    fn wrap(inner: Inner) -> Self {
        Query {
            inner,
            _scope: PhantomData,
        }
    }

    /// `self AND other`, within the same scope.
    #[must_use]
    pub fn and(self, other: impl AsQuery<S>) -> Query<S> {
        match other.into_query() {
            Some(other) => Query::wrap(combine(self.inner, other.inner, Clause::Must)),
            None => self,
        }
    }

    /// `self OR other`, within the same scope.
    #[must_use]
    pub fn or(self, other: impl AsQuery<S>) -> Query<S> {
        match other.into_query() {
            Some(other) => Query::wrap(combine(self.inner, other.inner, Clause::Should)),
            None => self,
        }
    }

    /// `NOT self`.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Query<S> {
        Query::wrap(Inner::Bool(BoolInner {
            must_not: vec![self.inner],
            ..BoolInner::default()
        }))
    }

    /// Set `boost` on this clause. On a `bool` (an `and`/`or`/`not` result) it
    /// becomes the bool's `boost`; a leaf clause is wrapped in a `bool` whose
    /// single `must` is the leaf (prefer the leaf builder's own `boost` there).
    #[must_use]
    pub fn boost(mut self, boost: f32) -> Query<S> {
        self.inner = match self.inner {
            Inner::Bool(mut bool_inner) => {
                bool_inner.boost = Some(boost);
                Inner::Bool(bool_inner)
            }
            leaf => Inner::Bool(BoolInner {
                must: vec![leaf],
                boost: Some(boost),
                ..BoolInner::default()
            }),
        };
        self
    }

    /// Set `minimum_should_match` on a `should`-group. Use this on an `or`
    /// chain (or `Search::min_should_match`) so the optional clauses become a
    /// real constraint — without it, `should` beside `must`/`filter` only
    /// scores. Accepts a count (`1`) or [`MinimumShouldMatch::percent`] / `raw`.
    /// A leaf clause is wrapped as a single `should`.
    #[must_use]
    pub fn min_should_match(mut self, value: impl Into<MinimumShouldMatch>) -> Query<S> {
        let value = value.into().to_value();
        self.inner = match self.inner {
            Inner::Bool(mut bool_inner) => {
                bool_inner.minimum_should_match = Some(value);
                Inner::Bool(bool_inner)
            }
            leaf => Inner::Bool(BoolInner {
                should: vec![leaf],
                minimum_should_match: Some(value),
                ..BoolInner::default()
            }),
        };
        self
    }

    /// Render to the OpenSearch query DSL.
    #[must_use]
    pub fn to_value(&self) -> Value {
        self.inner.to_value()
    }

    /// The scope-free inner clause. Crate-internal — `Search` collects these.
    pub(crate) fn into_inner(self) -> InnerClause {
        InnerClause(self.inner)
    }
}

/// An opaque scope-free clause, handed from a [`Query`] to [`crate::Search`].
pub(crate) struct InnerClause(Inner);

/// A bool builder over scope-free clauses, used by [`crate::Search`] (root scope).
#[derive(Debug, Clone, Default)]
pub(crate) struct BoolBuilder {
    bool_inner: BoolInner,
}

impl BoolBuilder {
    pub(crate) fn push_must(&mut self, clause: InnerClause) {
        self.bool_inner.push(Clause::Must, clause.0);
    }
    pub(crate) fn push_filter(&mut self, clause: InnerClause) {
        self.bool_inner.push(Clause::Filter, clause.0);
    }
    pub(crate) fn push_should(&mut self, clause: InnerClause) {
        self.bool_inner.push(Clause::Should, clause.0);
    }
    pub(crate) fn push_must_not(&mut self, clause: InnerClause) {
        self.bool_inner.push(Clause::MustNot, clause.0);
    }
    pub(crate) fn set_min_should_match(&mut self, value: Value) {
        self.bool_inner.minimum_should_match = Some(value);
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.bool_inner.is_empty()
    }
    pub(crate) fn to_value(&self) -> Value {
        self.bool_inner.to_value()
    }
}

/// Anything that can become a query clause in scope `S`. A clause may be absent
/// ([`into_query`](AsQuery::into_query) returns `None`) — that's what makes an
/// `Option<Query<S>>` a first-class optional filter.
///
/// The leaf-query builders ([`TermQuery`](crate::TermQuery),
/// [`WildcardQuery`](crate::WildcardQuery), [`MatchQuery`](crate::MatchQuery), …)
/// implement this, so they drop straight into [`Search`](crate::Search) clauses
/// and into `and`/`or`/`not` with no explicit `.build()`. The combinators here
/// are *provided* methods; on a [`Query`] the inherent ones win, so a builder
/// gains `and`/`or`/`not`/`to_value` for free while `Query`'s behavior is
/// unchanged.
pub trait AsQuery<S> {
    /// The clause this produces, or `None` to contribute nothing.
    fn into_query(self) -> Option<Query<S>>;

    /// `self AND other`. An absent side is the identity.
    #[must_use]
    fn and(self, other: impl AsQuery<S>) -> Query<S>
    where
        Self: Sized,
    {
        combine_opt(self.into_query(), other.into_query(), Clause::Must)
    }

    /// `self OR other`. An absent side is the identity.
    #[must_use]
    fn or(self, other: impl AsQuery<S>) -> Query<S>
    where
        Self: Sized,
    {
        combine_opt(self.into_query(), other.into_query(), Clause::Should)
    }

    /// `NOT self` (negating an absent clause matches everything).
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    fn not(self) -> Query<S>
    where
        Self: Sized,
    {
        self.into_query().map_or_else(Query::match_all, Query::not)
    }

    /// Render this clause to the OpenSearch query DSL. An absent clause renders
    /// as `match_all`. Handy for tests and debugging.
    #[must_use]
    fn to_value(&self) -> Value
    where
        Self: Sized + Clone,
    {
        self.clone()
            .into_query()
            .map_or_else(crate::handles::match_all_value, |q| q.to_value())
    }
}

impl<S> AsQuery<S> for Query<S> {
    fn into_query(self) -> Option<Query<S>> {
        Some(self)
    }
}

impl<S, T: AsQuery<S>> AsQuery<S> for Option<T> {
    fn into_query(self) -> Option<Query<S>> {
        self.and_then(AsQuery::into_query)
    }
}
