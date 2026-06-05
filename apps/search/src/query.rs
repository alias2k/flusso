//! The [`Query`] type: an OpenSearch query clause you compose with `and` / `or`
//! / `not`, built by the field handles in [`crate::handles`].

use serde_json::{Map, Value};

/// A composable query clause.
///
/// Handles produce a `Query` (e.g. `Keyword::at("email").eq("a@b.com")`), and
/// queries compose with [`Query::and`] / [`Query::or`] / [`Query::not`] into a
/// bool query. The [`crate::Search`] builder accepts a `Query` in any of its
/// clauses. Serialize one to the OpenSearch DSL with [`Query::to_value`].
#[derive(Debug, Clone)]
pub struct Query {
    inner: Inner,
}

#[derive(Debug, Clone)]
enum Inner {
    /// A single leaf clause, e.g. `{"term": {"email": "a@b.com"}}`.
    Leaf(Value),
    /// A `bool` query with its four clause lists.
    Bool(BoolQuery),
}

/// The four clause lists of an OpenSearch `bool` query. Used both inside a
/// composed [`Query`] and by [`crate::Search`] for its top-level bool.
#[derive(Debug, Clone, Default)]
pub(crate) struct BoolQuery {
    pub(crate) must: Vec<Query>,
    pub(crate) filter: Vec<Query>,
    pub(crate) should: Vec<Query>,
    pub(crate) must_not: Vec<Query>,
}

impl BoolQuery {
    /// True when no clause has been added — a search with an empty bool means
    /// "match everything".
    pub(crate) fn is_empty(&self) -> bool {
        self.must.is_empty()
            && self.filter.is_empty()
            && self.should.is_empty()
            && self.must_not.is_empty()
    }

    /// The `{"bool": { … }}` value, omitting empty clause lists.
    pub(crate) fn to_value(&self) -> Value {
        let mut bool_body = Map::new();
        insert_clause(&mut bool_body, "must", &self.must);
        insert_clause(&mut bool_body, "filter", &self.filter);
        insert_clause(&mut bool_body, "should", &self.should);
        insert_clause(&mut bool_body, "must_not", &self.must_not);

        let mut outer = Map::new();
        outer.insert("bool".to_string(), Value::Object(bool_body));
        Value::Object(outer)
    }
}

/// Append a clause list under `key` as a JSON array, skipping it when empty.
fn insert_clause(target: &mut Map<String, Value>, key: &str, clauses: &[Query]) {
    if clauses.is_empty() {
        return;
    }
    let array = clauses.iter().map(Query::to_value).collect();
    target.insert(key.to_string(), Value::Array(array));
}

impl Query {
    /// Wrap a leaf clause value. Crate-internal: handles call this.
    pub(crate) fn leaf(value: Value) -> Self {
        Self {
            inner: Inner::Leaf(value),
        }
    }

    /// Wrap a bool query. Crate-internal.
    pub(crate) fn from_bool(bool_query: BoolQuery) -> Self {
        Self {
            inner: Inner::Bool(bool_query),
        }
    }

    /// `self AND other`. Flattens when `self` is already a pure-`must` bool, so
    /// `a.and(b).and(c)` yields one bool with three `must` clauses. `other` is
    /// any [`AsQuery`]; an absent one (e.g. `None`) leaves `self` unchanged.
    #[must_use]
    pub fn and(self, other: impl AsQuery) -> Query {
        let Some(other) = other.into_query() else {
            return self;
        };
        match self.inner {
            Inner::Bool(mut b) if is_pure(&b, Clause::Must) => {
                b.must.push(other);
                Query::from_bool(b)
            }
            _ => Query::from_bool(BoolQuery {
                must: vec![self, other],
                ..BoolQuery::default()
            }),
        }
    }

    /// `self OR other`. Flattens repeated `or` into one `should` bool. An absent
    /// `other` leaves `self` unchanged.
    #[must_use]
    pub fn or(self, other: impl AsQuery) -> Query {
        let Some(other) = other.into_query() else {
            return self;
        };
        match self.inner {
            Inner::Bool(mut b) if is_pure(&b, Clause::Should) => {
                b.should.push(other);
                Query::from_bool(b)
            }
            _ => Query::from_bool(BoolQuery {
                should: vec![self, other],
                ..BoolQuery::default()
            }),
        }
    }

    /// `NOT self` — a bool with a single `must_not` clause.
    //  Named `not` to match the documented `and`/`or`/`not` trio; we deliberately
    //  keep it inherent (rather than `impl std::ops::Not`) so callers don't need
    //  the trait in scope to write `query.not()`.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Query {
        Query::from_bool(BoolQuery {
            must_not: vec![self],
            ..BoolQuery::default()
        })
    }

    /// Render to the OpenSearch query DSL.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match &self.inner {
            Inner::Leaf(value) => value.clone(),
            Inner::Bool(bool_query) => bool_query.to_value(),
        }
    }
}

/// Anything that can become a query clause.
///
/// This is the extension point every clause-taking method accepts (`impl
/// AsQuery`), rather than a concrete [`Query`]. A clause may be **absent** —
/// [`into_query`](AsQuery::into_query) returns `Option<Query>` — which is what
/// makes an optional value a first-class filter: a `None` contributes nothing,
/// in any clause (`must_not(None)` excludes nothing, `and(None)` is identity).
///
/// ```
/// use flusso_search::{AsQuery, Keyword};
///
/// // A plain query is present…
/// assert!(Keyword::at("email").eq("a@b.com").into_query().is_some());
/// // …an absent optional contributes nothing.
/// let missing: Option<flusso_search::Query> = None;
/// assert!(missing.into_query().is_none());
/// ```
pub trait AsQuery {
    /// The query clause this produces, or `None` to contribute nothing.
    fn into_query(self) -> Option<Query>;
}

impl AsQuery for Query {
    fn into_query(self) -> Option<Query> {
        Some(self)
    }
}

/// An optional clause: `Some` contributes its query, `None` contributes nothing.
impl<T: AsQuery> AsQuery for Option<T> {
    fn into_query(self) -> Option<Query> {
        self.and_then(AsQuery::into_query)
    }
}

/// Which clause list a flatten check is about.
#[derive(Debug, Clone, Copy)]
enum Clause {
    Must,
    Should,
}

/// A bool is "pure" for a clause when only that clause list is populated, so a
/// new entry can be appended without changing the query's meaning.
fn is_pure(bool_query: &BoolQuery, clause: Clause) -> bool {
    match clause {
        Clause::Must => {
            bool_query.filter.is_empty()
                && bool_query.should.is_empty()
                && bool_query.must_not.is_empty()
        }
        Clause::Should => {
            bool_query.must.is_empty()
                && bool_query.filter.is_empty()
                && bool_query.must_not.is_empty()
        }
    }
}
