//! Combined search: one query over several indexes, hits ranked together.
//!
//! Where [`Client::msearch`](crate::Client::msearch) runs *independent*
//! searches in one round-trip (separate result lists), a [`MultiSearch`] runs
//! **one** query across every index a [`FlussoMultiDocument`] union spans and
//! returns a single, blended, relevance-ranked result list. Each hit decodes
//! into the union variant matching its physical `_index` — the sink writes
//! exactly `{INDEX}_{SCHEMA_HASH}`, so dispatch is precise, no alias involved.
//!
//! The union enum is yours: one single-field variant per document type, named
//! after the search surface it serves. `#[derive(FlussoMultiDocument)]` (the
//! `derive` feature) writes the impl; without it, a hand-written impl is two
//! short members — see the trait docs.
//!
//! Root-scope queries already compose across document types ([`Query<Root>`]
//! carries no document type), so any handle mix works in the builder. A field
//! unmapped in one of the indexes simply doesn't match there — but **sorting**
//! on it errors on the OpenSearch side unless the sort carries an
//! `unmapped_type`; prefer sorting on fields all indexes share (or relevance).
//!
//! [`Query<Root>`]: crate::Query

use std::marker::PhantomData;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::Client;
use crate::error::Result;
use crate::handles::Sort;
use crate::query::{AsQuery, BoolBuilder, Root};
use crate::search::{Hit, RawCount, SearchResponse};

/// A union of [`FlussoDocument`](crate::FlussoDocument) types searched
/// together — one query, one blended result list, each hit decoded into the
/// variant matching its index.
///
/// `#[derive(FlussoMultiDocument)]` (the `derive` feature) implements it for
/// an enum with one single-field variant per document type. Without the
/// derive, the impl is written by hand — exactly what the derive generates:
///
/// ```no_run
/// use flusso_search::{FlussoDocument, FlussoMultiDocument, Error, Result};
/// use serde_json::Value;
/// # #[derive(serde::Deserialize)] struct User { email: String }
/// # impl FlussoDocument for User {
/// #     const INDEX: &'static str = "users";
/// #     const SCHEMA_HASH: &'static str = "xxxxxx";
/// # }
/// # #[derive(serde::Deserialize)] struct Order { status: String }
/// # impl FlussoDocument for Order {
/// #     const INDEX: &'static str = "orders";
/// #     const SCHEMA_HASH: &'static str = "yyyyyy";
/// # }
///
/// /// One item in the storefront's blended search — name it after the
/// /// surface it serves, like your document structs.
/// enum StoreItem {
///     User(User),
///     Order(Order),
/// }
///
/// impl FlussoMultiDocument for StoreItem {
///     const TARGETS: &'static [(&'static str, &'static str)] = &[
///         (User::INDEX, User::SCHEMA_HASH),
///         (Order::INDEX, Order::SCHEMA_HASH),
///     ];
///
///     fn decode(physical_index: &str, source: Value) -> Result<Self> {
///         if physical_index == User::physical_index() {
///             return Ok(Self::User(serde_json::from_value(source)?));
///         }
///         if physical_index == Order::physical_index() {
///             return Ok(Self::Order(serde_json::from_value(source)?));
///         }
///         Err(Error::UnexpectedIndex { index: physical_index.to_owned() })
///     }
/// }
/// ```
pub trait FlussoMultiDocument: Sized {
    /// The `(logical index, schema hash)` pair of every document type in the
    /// union, in variant order — each is that type's
    /// [`INDEX`](crate::FlussoDocument::INDEX) /
    /// [`SCHEMA_HASH`](crate::FlussoDocument::SCHEMA_HASH).
    const TARGETS: &'static [(&'static str, &'static str)];

    /// Decode one hit's `_source` into the right variant, dispatching on the
    /// hit's physical index name. A hit from an index no variant claims is
    /// [`Error::UnexpectedIndex`](crate::Error::UnexpectedIndex).
    fn decode(physical_index: &str, source: Value) -> Result<Self>;

    /// Start a typed query across all of this union's indexes. Like
    /// [`FlussoDocument::query`](crate::FlussoDocument::query), the returned
    /// builder is a plain client-free value.
    fn query() -> MultiSearch<Self> {
        MultiSearch::new()
    }
}

/// A typed query across every index of a [`FlussoMultiDocument`] union — the
/// blended counterpart of [`Search`](crate::Search), with the same clause
/// builder and the same client-free shape.
///
/// Hits come back in **one** relevance-ranked list; `from`/`size` page that
/// blended list, not each index. Terminals: [`send`](Self::send) for a typed
/// page of union values, [`count`](Self::count) for the total matches across
/// all the indexes.
#[derive(Debug, Clone)]
pub struct MultiSearch<U> {
    /// The comma-joined physical index list the request addresses.
    path: String,
    bool_query: BoolBuilder,
    raw: Option<Value>,
    sort: Vec<Sort>,
    from: Option<u64>,
    size: Option<u64>,
    _marker: PhantomData<fn() -> U>,
}

impl<U: FlussoMultiDocument> MultiSearch<U> {
    /// Start a query across the union's indexes (usually via
    /// [`FlussoMultiDocument::query`]).
    #[must_use]
    pub fn new() -> Self {
        let path = U::TARGETS
            .iter()
            .map(|(index, hash)| format!("{index}_{hash}"))
            .collect::<Vec<_>>()
            .join(",");
        Self {
            path,
            bool_query: BoolBuilder::default(),
            raw: None,
            sort: Vec::new(),
            from: None,
            size: None,
            _marker: PhantomData,
        }
    }

    /// A scoring clause (`bool.must`). Root-scope queries from *any* of the
    /// union's document types compose here; a field unmapped in one index
    /// simply doesn't match there. An absent clause adds nothing.
    #[must_use]
    pub fn query(mut self, query: impl AsQuery<Root>) -> Self {
        if let Some(query) = query.into_query() {
            self.bool_query.push_must(query.into_inner());
        }
        self
    }

    /// A non-scoring, cacheable clause (`bool.filter`). An absent clause adds
    /// nothing — so `filter(opt.map(|v| handle.eq(v)))` is a conditional filter.
    #[must_use]
    pub fn filter(mut self, query: impl AsQuery<Root>) -> Self {
        if let Some(query) = query.into_query() {
            self.bool_query.push_filter(query.into_inner());
        }
        self
    }

    /// An exclusion clause (`bool.must_not`). An absent clause excludes nothing.
    #[must_use]
    pub fn must_not(mut self, query: impl AsQuery<Root>) -> Self {
        if let Some(query) = query.into_query() {
            self.bool_query.push_must_not(query.into_inner());
        }
        self
    }

    /// An optional, scoring clause (`bool.should`). An absent clause adds nothing.
    #[must_use]
    pub fn should(mut self, query: impl AsQuery<Root>) -> Self {
        if let Some(query) = query.into_query() {
            self.bool_query.push_should(query.into_inner());
        }
        self
    }

    /// Append a sort key. It applies to the **blended** list, so the field
    /// must exist in every index of the union (or carry an `unmapped_type` in
    /// its options) — OpenSearch rejects a sort on a field one index lacks.
    /// Relevance (no sort) is always safe.
    #[must_use]
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort.push(sort);
        self
    }

    /// Offset of the first hit to return, in the blended list.
    #[must_use]
    pub fn from(mut self, from: u64) -> Self {
        self.from = Some(from);
        self
    }

    /// Maximum number of hits to return, across all the indexes combined.
    #[must_use]
    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Replace the query body with a raw OpenSearch query DSL value. The
    /// pressure-release valve, as on [`Search`](crate::Search); hits still
    /// decode into the union.
    #[must_use]
    pub fn raw(mut self, query: Value) -> Self {
        self.raw = Some(query);
        self
    }

    /// The comma-joined physical index list this query addresses — one
    /// `{index}_{hash}` per union variant. For logging and debugging.
    #[must_use]
    pub fn physical_path(&self) -> &str {
        &self.path
    }

    /// The accumulated query alone: the raw override, the bool clauses, or
    /// `match_all` when nothing was added.
    fn query_value(&self) -> Value {
        match &self.raw {
            Some(raw) => raw.clone(),
            None if self.bool_query.is_empty() => crate::handles::match_all_value(),
            None => self.bool_query.to_value(),
        }
    }

    /// The request body this search will POST to `_search`. Pure — useful for
    /// tests and debugging.
    #[must_use]
    pub fn body(&self) -> Value {
        let mut root = Map::new();
        root.insert("query".to_string(), self.query_value());
        if !self.sort.is_empty() {
            let keys = self.sort.iter().map(Sort::to_value).collect();
            root.insert("sort".to_string(), Value::Array(keys));
        }
        if let Some(from) = self.from {
            root.insert("from".to_string(), Value::from(from));
        }
        if let Some(size) = self.size {
            root.insert("size".to_string(), Value::from(size));
        }
        Value::Object(root)
    }

    /// The request body [`count`](Self::count) will POST to `_count`: just
    /// the query (as on [`Search::count_body`](crate::Search::count_body)).
    #[must_use]
    pub fn count_body(&self) -> Value {
        let mut root = Map::new();
        root.insert("query".to_string(), self.query_value());
        Value::Object(root)
    }

    /// Execute the search and decode the blended hits into the union.
    #[tracing::instrument(
        name = "search.multi",
        skip_all,
        fields(
            path = %self.path,
            from = ?self.from,
            size = ?self.size,
            total = tracing::field::Empty,
            took_ms = tracing::field::Empty,
        ),
        err,
    )]
    pub async fn send(&self, client: &Client) -> Result<SearchResponse<U>> {
        let body = self.body();
        let response = client.search_at(&self.path, &body).await?;
        let page = decode_response::<U>(response)?;
        let span = tracing::Span::current();
        span.record("total", page.total);
        span.record("took_ms", page.took.as_millis() as u64);
        tracing::debug!(
            total = page.total,
            hits = page.hits.len(),
            "combined search completed"
        );
        Ok(page)
    }

    /// Count the matches across all the union's indexes, without fetching
    /// any hits.
    #[tracing::instrument(
        name = "search.multi_count",
        skip_all,
        fields(path = %self.path, count = tracing::field::Empty),
        err,
    )]
    pub async fn count(&self, client: &Client) -> Result<u64> {
        let body = self.count_body();
        let response = client.count_at(&self.path, &body).await?;
        let raw: RawCount = serde_json::from_value(response)?;
        tracing::Span::current().record("count", raw.count);
        tracing::debug!(count = raw.count, "combined count completed");
        Ok(raw.count)
    }
}

impl<U: FlussoMultiDocument> Default for MultiSearch<U> {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode a combined `_search` response: the usual envelope, but each hit's
/// `_source` is dispatched by the hit's `_index` into the union.
pub(crate) fn decode_response<U: FlussoMultiDocument>(value: Value) -> Result<SearchResponse<U>> {
    let raw: RawMultiResponse = serde_json::from_value(value)?;
    let hits = raw
        .hits
        .hits
        .into_iter()
        .map(|hit| {
            Ok(Hit {
                id: hit.id,
                score: hit.score.unwrap_or(0.0),
                source: U::decode(&hit.index, hit.source)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(SearchResponse {
        total: raw.hits.total.value,
        max_score: raw.hits.max_score,
        hits,
        took: Duration::from_millis(raw.took),
    })
}

// ---- wire types ------------------------------------------------------------
//
// Mirrors `search.rs`'s response shapes, but keeps each hit's `_index` (the
// dispatch key) and defers `_source` to `Value` (decoded per variant).

#[derive(Deserialize)]
struct RawMultiResponse {
    #[serde(default)]
    took: u64,
    hits: RawMultiHits,
}

#[derive(Deserialize)]
struct RawMultiHits {
    total: RawMultiTotal,
    #[serde(default)]
    max_score: Option<f32>,
    hits: Vec<RawMultiHit>,
}

#[derive(Deserialize)]
struct RawMultiTotal {
    value: u64,
}

#[derive(Deserialize)]
struct RawMultiHit {
    #[serde(rename = "_index")]
    index: String,
    #[serde(rename = "_id")]
    id: String,
    #[serde(rename = "_score", default)]
    score: Option<f32>,
    #[serde(rename = "_source")]
    source: Value,
}
