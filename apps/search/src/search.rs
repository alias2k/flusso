//! The [`Search`] builder and the typed [`SearchResponse`] / [`Hit`] results.

use std::marker::PhantomData;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::Client;
use crate::error::Result;
use crate::handles::{NestedProjection, Sort};
use crate::query::{AsQuery, BoolBuilder, Root};

/// A document type bound to a flusso-maintained index — the trait that
/// `#[derive(FlussoDocument)]` implements.
///
/// The derive supplies [`INDEX`](Self::INDEX) and [`SCHEMA_HASH`](Self::SCHEMA_HASH)
/// (the physical index is `{INDEX}_{SCHEMA_HASH}`, exactly what the OpenSearch
/// sink writes); [`search`](Self::search) and [`get`](Self::get) are provided.
/// `DeserializeOwned` is required so search hits and fetched documents decode.
pub trait FlussoDocument: DeserializeOwned {
    /// The logical index name this binding queries.
    const INDEX: &'static str;

    /// The schema hash this binding was generated from (the physical-index suffix).
    const SCHEMA_HASH: &'static str;

    /// Start a typed search against this index.
    fn search(client: &Client) -> Search<'_, Self> {
        Search::new(client, Self::INDEX, Self::SCHEMA_HASH)
    }

    /// Fetch one document by id; `None` when absent.
    fn get(
        client: &Client,
        id: impl std::fmt::Display,
    ) -> impl std::future::Future<Output = Result<Option<Self>>> {
        client.get_one::<Self>(Self::INDEX, Self::SCHEMA_HASH, id)
    }
}

/// A typed search against one index.
///
/// Built from `Search::new(client, index)` (the derive will generate a
/// `Type::search(client)` that calls this). Clauses accumulate into a bool
/// query: `query`/`should` score, `filter`/`must_not` don't. Finish with
/// [`Search::send`] for a page of hits, [`Search::ids`] for a page of bare
/// document ids, or [`Search::count`] for just the number of matches.
#[derive(Debug)]
pub struct Search<'a, T> {
    client: &'a Client,
    index: String,
    hash: String,
    bool_query: BoolBuilder,
    raw: Option<Value>,
    sort: Vec<Sort>,
    from: Option<u64>,
    size: Option<u64>,
    nested: Vec<NestedProjection>,
    _marker: PhantomData<fn() -> T>,
}

impl<'a, T> Search<'a, T> {
    /// Start a search against `index` using `client`.
    pub fn new(client: &'a Client, index: impl Into<String>, hash: impl Into<String>) -> Self {
        Self {
            client,
            index: index.into(),
            hash: hash.into(),
            bool_query: BoolBuilder::default(),
            raw: None,
            sort: Vec::new(),
            from: None,
            size: None,
            nested: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// A scoring clause (`bool.must`). Accepts any root-scope [`AsQuery`]; an
    /// absent one (e.g. a `None` optional) adds nothing.
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

    /// Append a sort key.
    #[must_use]
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort.push(sort);
        self
    }

    /// Offset of the first hit to return.
    #[must_use]
    pub fn from(mut self, from: u64) -> Self {
        self.from = Some(from);
        self
    }

    /// Maximum number of hits to return.
    #[must_use]
    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Replace the query body with a raw OpenSearch query DSL value. The
    /// pressure-release valve for anything the typed builder can't express;
    /// results still deserialize into `T`.
    #[must_use]
    pub fn raw(mut self, query: Value) -> Self {
        self.raw = Some(query);
        self
    }

    /// Shape a nested array in the results (built via `Nested::matching` /
    /// `Nested::project`). Each hit's `source.<path>` is replaced with the
    /// matching subset; this does **not** change which parents match.
    #[must_use]
    pub fn filter_nested(mut self, projection: NestedProjection) -> Self {
        self.nested.push(projection);
        self
    }

    /// The accumulated query alone: the raw override, the bool clauses, or
    /// `match_all` when nothing was added. Shared by [`body`](Self::body) and
    /// [`count_body`](Self::count_body).
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
        let query = self.query_value();

        // `filter_nested` projections collect `inner_hits` without filtering
        // parents: they sit in `should` of a bool whose `must` holds the real
        // query, so (with `must` present) they're optional and only attach hits.
        let query = if self.nested.is_empty() {
            query
        } else {
            let mut bool_body = Map::new();
            bool_body.insert("must".to_string(), Value::Array(vec![query]));
            let shoulds = self.nested.iter().map(NestedProjection::to_value).collect();
            bool_body.insert("should".to_string(), Value::Array(shoulds));
            let mut outer = Map::new();
            outer.insert("bool".to_string(), Value::Object(bool_body));
            Value::Object(outer)
        };

        let mut root = Map::new();
        root.insert("query".to_string(), query);
        self.insert_page_params(&mut root);
        Value::Object(root)
    }

    /// Add the page-shaping keys (`sort` / `from` / `size`) to a request body.
    /// Shared by [`body`](Self::body) and [`ids_body`](Self::ids_body).
    fn insert_page_params(&self, root: &mut Map<String, Value>) {
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
    }

    /// The request body [`count`](Self::count) will POST to `_count`: just the
    /// query. Sort, `from`/`size`, and `filter_nested` projections are dropped —
    /// `_count` accepts none of them, and none of them changes which documents
    /// match. Pure — useful for tests and debugging.
    #[must_use]
    pub fn count_body(&self) -> Value {
        let mut root = Map::new();
        root.insert("query".to_string(), self.query_value());
        Value::Object(root)
    }

    /// The request body [`ids`](Self::ids) will POST to `_search`: the query
    /// plus sort and pagination, with `_source: false` so hits carry only
    /// their `_id` and nothing is fetched from stored source. `filter_nested`
    /// projections are dropped — they shape returned sources, and there are
    /// none. Pure — useful for tests and debugging.
    #[must_use]
    pub fn ids_body(&self) -> Value {
        let mut root = Map::new();
        root.insert("query".to_string(), self.query_value());
        self.insert_page_params(&mut root);
        root.insert("_source".to_string(), Value::Bool(false));
        Value::Object(root)
    }

    /// Execute the search and return only the matching document ids (the root
    /// primary keys, stringified by OpenSearch) — no sources are fetched, so
    /// this is the cheap way to feed another lookup (e.g. load the rows from
    /// Postgres). Sort, [`from`](Self::from), and [`size`](Self::size) apply
    /// as in [`send`](Self::send); the page's ids are returned in order.
    #[tracing::instrument(
        name = "search.ids",
        skip_all,
        fields(index = %self.index, returned = tracing::field::Empty),
        err,
    )]
    pub async fn ids(self) -> Result<Vec<String>> {
        let body = self.ids_body();
        let response = self.client.search(&self.index, &self.hash, &body).await?;
        let raw: RawIdsResponse = serde_json::from_value(response)?;
        let ids: Vec<String> = raw.hits.hits.into_iter().map(|hit| hit.id).collect();
        tracing::Span::current().record("returned", ids.len());
        tracing::debug!(returned = ids.len(), "ids search completed");
        Ok(ids)
    }

    /// Execute the query as a count: how many documents match, without fetching
    /// (or scoring) any hits — cheaper than [`send`](Self::send) when only the
    /// total is needed. Sort, pagination, and nested projections are ignored
    /// (see [`count_body`](Self::count_body)).
    #[tracing::instrument(
        name = "search.count",
        skip_all,
        fields(index = %self.index, count = tracing::field::Empty),
        err,
    )]
    pub async fn count(self) -> Result<u64> {
        let body = self.count_body();
        let response = self.client.count(&self.index, &self.hash, &body).await?;
        let raw: RawCount = serde_json::from_value(response)?;
        tracing::Span::current().record("count", raw.count);
        tracing::debug!(count = raw.count, "count completed");
        Ok(raw.count)
    }
}

impl<T> Search<'_, T>
where
    T: DeserializeOwned,
{
    /// Execute the search and decode the hits into `SearchResponse<T>`.
    #[tracing::instrument(
        name = "search.send",
        skip_all,
        fields(
            index = %self.index,
            from = ?self.from,
            size = ?self.size,
            total = tracing::field::Empty,
            took_ms = tracing::field::Empty,
        ),
        err,
    )]
    pub async fn send(self) -> Result<SearchResponse<T>> {
        let body = self.body();
        let mut response = self.client.search(&self.index, &self.hash, &body).await?;
        if !self.nested.is_empty() {
            let paths: Vec<&str> = self.nested.iter().map(NestedProjection::path).collect();
            merge_inner_hits(&mut response, &paths);
        }
        let page = SearchResponse::from_value(response)?;
        let span = tracing::Span::current();
        span.record("total", page.total);
        span.record("took_ms", page.took.as_millis() as u64);
        tracing::debug!(
            total = page.total,
            hits = page.hits.len(),
            "search completed"
        );
        Ok(page)
    }
}

/// Replace each `paths` array in every hit's `_source` with that path's
/// `inner_hits` subset, so the typed source carries the filtered nested array.
pub(crate) fn merge_inner_hits(response: &mut Value, paths: &[&str]) {
    let Some(hits) = response
        .get_mut("hits")
        .and_then(|hits| hits.get_mut("hits"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    for hit in hits {
        let inner = match hit.get("inner_hits") {
            Some(inner) => inner.clone(),
            None => continue,
        };
        let Some(source) = hit.get_mut("_source").and_then(Value::as_object_mut) else {
            continue;
        };
        for path in paths {
            let subset: Vec<Value> = inner
                .get(*path)
                .and_then(|hit| hit.get("hits"))
                .and_then(|hits| hits.get("hits"))
                .and_then(Value::as_array)
                .map(|hits| {
                    hits.iter()
                        .filter_map(|h| h.get("_source").cloned())
                        .collect()
                })
                .unwrap_or_default();
            source.insert((*path).to_string(), Value::Array(subset));
        }
    }
}

/// A page of search results.
#[derive(Debug)]
pub struct SearchResponse<T> {
    /// Total matches across the whole index, not the page size.
    pub total: u64,
    /// The top score in this page, if scored.
    pub max_score: Option<f32>,
    /// The hits in this page.
    pub hits: Vec<Hit<T>>,
    /// How long OpenSearch reported the query took.
    pub took: Duration,
}

impl<T> SearchResponse<T>
where
    T: DeserializeOwned,
{
    /// Decode an OpenSearch `_search` response body into a typed page.
    pub fn from_value(value: Value) -> Result<Self> {
        let raw: RawResponse<T> = serde_json::from_value(value)?;
        let hits = raw
            .hits
            .hits
            .into_iter()
            .map(|hit| Hit {
                id: hit.id,
                score: hit.score.unwrap_or(0.0),
                source: hit.source,
            })
            .collect();
        Ok(Self {
            total: raw.hits.total.value,
            max_score: raw.hits.max_score,
            hits,
            took: Duration::from_millis(raw.took),
        })
    }
}

/// One search hit: the typed document plus its envelope metadata.
#[derive(Debug)]
pub struct Hit<T> {
    /// The document id (root primary key, stringified by OpenSearch).
    pub id: String,
    /// The relevance score (`0.0` when the query didn't score).
    pub score: f32,
    /// The fully-typed document.
    pub source: T,
}

// ---- wire types ------------------------------------------------------------

#[derive(Deserialize)]
struct RawResponse<T> {
    #[serde(default)]
    took: u64,
    hits: RawHits<T>,
}

#[derive(Deserialize)]
struct RawHits<T> {
    total: RawTotal,
    #[serde(default)]
    max_score: Option<f32>,
    hits: Vec<RawHit<T>>,
}

#[derive(Deserialize)]
struct RawTotal {
    value: u64,
}

/// The `_count` response envelope (`{ "count": N, "_shards": … }`).
#[derive(Deserialize)]
struct RawCount {
    count: u64,
}

/// A `_search` response read for its hit ids only (`_source: false`, so hits
/// carry no source to decode).
#[derive(Deserialize)]
struct RawIdsResponse {
    hits: RawIdsHits,
}

#[derive(Deserialize)]
struct RawIdsHits {
    hits: Vec<RawIdHit>,
}

#[derive(Deserialize)]
struct RawIdHit {
    #[serde(rename = "_id")]
    id: String,
}

#[derive(Deserialize)]
struct RawHit<T> {
    #[serde(rename = "_id")]
    id: String,
    #[serde(rename = "_score", default)]
    score: Option<f32>,
    #[serde(rename = "_source")]
    source: T,
}
