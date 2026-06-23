//! The [`Search`] builder and the typed [`SearchResponse`] / [`Hit`] results.

use std::marker::PhantomData;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::Client;
use crate::error::Result;
use crate::handles::{MinimumShouldMatch, NestedProjection, Sort};
use crate::path::Segment;
use crate::query::{AsQuery, BoolBuilder, Root};

/// A view onto a flusso-maintained index — the root document **or** any of its
/// `nested` element projections. Every struct `#[derive(FlussoDocument)]`
/// generates carries a [`PATH`](Self::PATH): the chain of container levels from
/// the index root down to this view, which a nesting-aware sort reads to render
/// the right `nested` clause. The root's `PATH` is empty.
///
/// The index-pointing operations (`query`/`get`, the index name + hash) live on
/// the [`FlussoIndex`] supertrait, emitted **only** for the root — so a child
/// projection cannot start a search.
pub trait FlussoDocument {
    /// This view's position from the index root, outermost first. Empty for the
    /// root and for any flattened-object scope (no `nested` boundary above it).
    const PATH: &'static [Segment];
}

/// The **root** document bound to a flusso-maintained index — the entry point for
/// queries. `#[derive(FlussoDocument)]` implements this only for the struct with
/// no `path` (the index root).
///
/// The derive supplies [`INDEX`](Self::INDEX) and [`SCHEMA_HASH`](Self::SCHEMA_HASH)
/// (the physical index is `{INDEX}_{SCHEMA_HASH}`, exactly what the OpenSearch
/// sink writes); [`query`](Self::query) and [`get`](Self::get) are provided.
/// `DeserializeOwned` is required so search hits and fetched documents decode.
pub trait FlussoIndex: FlussoDocument + DeserializeOwned {
    /// The logical index name this binding queries.
    const INDEX: &'static str;

    /// The schema hash this binding was generated from (the physical-index suffix).
    const SCHEMA_HASH: &'static str;

    /// The physical index this binding addresses — `{INDEX}_{SCHEMA_HASH}`,
    /// exactly what the sink writes. Useful for logging, admin, and
    /// hand-written [`FlussoMultiDocument`](crate::FlussoMultiDocument) impls
    /// dispatching hits by their `_index`.
    fn physical_index() -> String {
        format!("{}_{}", Self::INDEX, Self::SCHEMA_HASH)
    }

    /// Start a typed query against this index. No client is involved: the
    /// returned [`Search`] is a plain value — build it anywhere, store it,
    /// clone it, and hand a [`Client`] to a terminal
    /// ([`send`](Search::send) / [`ids`](Search::ids) / [`count`](Search::count))
    /// when it's time to run.
    fn query() -> Search<Self> {
        Search::new(Self::INDEX, Self::SCHEMA_HASH)
    }

    /// Fetch one document by id; `None` when absent.
    fn get(
        client: &Client,
        id: impl std::fmt::Display,
    ) -> impl std::future::Future<Output = Result<Option<Self>>> {
        client.get_one::<Self>(Self::INDEX, Self::SCHEMA_HASH, id)
    }
}

/// A typed query against one index — a plain, client-free value.
///
/// Built from [`FlussoDocument::query`] (or `Search::new(index, hash)` by
/// hand). Clauses accumulate into a bool query: `query`/`should` score,
/// `filter`/`must_not` don't. Because no client (and no lifetime) is
/// involved, a `Search` can be named, stored, cloned, and reused; a [`Client`]
/// appears only at the terminals — [`Search::send`] for a page of hits,
/// [`Search::ids`] for a page of bare document ids, [`Search::count`] for
/// just the number of matches (all `&self`, so running consumes nothing) —
/// or several searches go in one round-trip via [`Client::msearch`].
#[derive(Debug, Clone)]
pub struct Search<T> {
    index: String,
    hash: String,
    bool_query: BoolBuilder,
    raw: Option<Value>,
    sort: Vec<Sort>,
    from: Option<u64>,
    size: Option<u64>,
    nested: Vec<NestedProjection>,
    min_score: Option<f32>,
    track_total_hits: Option<Value>,
    track_scores: Option<bool>,
    search_after: Option<Vec<Value>>,
    collapse: Option<Value>,
    post_filter: Option<Value>,
    highlight: Option<Highlight>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Search<T> {
    /// Start a query against `index` (the logical name) and its schema `hash`.
    pub fn new(index: impl Into<String>, hash: impl Into<String>) -> Self {
        Self {
            index: index.into(),
            hash: hash.into(),
            bool_query: BoolBuilder::default(),
            raw: None,
            sort: Vec::new(),
            from: None,
            size: None,
            nested: Vec::new(),
            min_score: None,
            track_total_hits: None,
            track_scores: None,
            search_after: None,
            collapse: None,
            post_filter: None,
            highlight: None,
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

    /// Require at least this many `should` clauses to match. Beside `query` /
    /// `filter` clauses, `should` defaults to non-constraining (scoring only);
    /// setting this makes a top-level `should`-group a real filter. Accepts a
    /// count (`1`) or [`MinimumShouldMatch::percent`] / `raw`.
    #[must_use]
    pub fn min_should_match(mut self, value: impl Into<MinimumShouldMatch>) -> Self {
        self.bool_query
            .set_min_should_match(value.into().to_value());
        self
    }

    #[must_use]
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort.push(sort);
        self
    }

    /// Drop hits scoring below `min_score`.
    #[must_use]
    pub fn min_score(mut self, min_score: f32) -> Self {
        self.min_score = Some(min_score);
        self
    }

    /// Control how the hit total is counted. `true` counts exactly, `false`
    /// disables counting, an integer caps accuracy at that many (e.g. `10_000`).
    #[must_use]
    pub fn track_total_hits(mut self, track: impl Into<Value>) -> Self {
        self.track_total_hits = Some(track.into());
        self
    }

    /// Compute relevance scores even when sorting by a field.
    #[must_use]
    pub fn track_scores(mut self, track: bool) -> Self {
        self.track_scores = Some(track);
        self
    }

    /// Deep-paginate after the given sort values (the last hit's `sort` array
    /// from the previous page). Pair with a deterministic [`sort`](Self::sort).
    #[must_use]
    pub fn search_after(mut self, values: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.search_after = Some(values.into_iter().map(Into::into).collect());
        self
    }

    /// Collapse hits so only the top hit per `field` value is returned.
    #[must_use]
    pub fn collapse(mut self, field: impl Into<String>) -> Self {
        let mut body = Map::new();
        body.insert("field".to_string(), Value::String(field.into()));
        self.collapse = Some(Value::Object(body));
        self
    }

    /// A filter applied **after** scoring/aggregation — narrows the returned
    /// hits without affecting scores or aggregations.
    #[must_use]
    pub fn post_filter(mut self, query: impl AsQuery<Root>) -> Self {
        if let Some(query) = query.into_query() {
            self.post_filter = Some(query.to_value());
        }
        self
    }

    /// Attach match highlighting (see [`Highlight`]).
    #[must_use]
    pub fn highlight(mut self, highlight: Highlight) -> Self {
        self.highlight = Some(highlight);
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
        self.insert_search_level(&mut root, true);
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

    /// Add the search-level keys that shape *which* hits return and how the
    /// total/scores are reported (`min_score`, `track_total_hits`,
    /// `track_scores`, `search_after`, `collapse`, `post_filter`, and —
    /// `with_highlight` only — `highlight`). Shared by [`body`](Self::body) and
    /// [`ids_body`](Self::ids_body); `highlight` is skipped for ids (no source
    /// to highlight). `_count` gets none of these.
    fn insert_search_level(&self, root: &mut Map<String, Value>, with_highlight: bool) {
        if let Some(min_score) = self.min_score {
            root.insert("min_score".to_string(), Value::from(min_score));
        }
        if let Some(track) = &self.track_total_hits {
            root.insert("track_total_hits".to_string(), track.clone());
        }
        if let Some(track) = self.track_scores {
            root.insert("track_scores".to_string(), Value::Bool(track));
        }
        if let Some(values) = &self.search_after {
            root.insert("search_after".to_string(), Value::Array(values.clone()));
        }
        if let Some(collapse) = &self.collapse {
            root.insert("collapse".to_string(), collapse.clone());
        }
        if let Some(post_filter) = &self.post_filter {
            root.insert("post_filter".to_string(), post_filter.clone());
        }
        if with_highlight && let Some(highlight) = &self.highlight {
            root.insert("highlight".to_string(), highlight.to_value());
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
        self.insert_search_level(&mut root, false);
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
    pub async fn ids(&self, client: &Client) -> Result<Vec<String>> {
        let body = self.ids_body();
        let response = client.search_at(&self.physical_index(), &body).await?;
        let raw: RawIdsResponse = serde_json::from_value(response)?;
        let ids: Vec<String> = raw.hits.hits.into_iter().map(|hit| hit.id).collect();
        tracing::Span::current().record("returned", ids.len());
        tracing::debug!(returned = ids.len(), "ids search completed");
        Ok(ids)
    }

    /// The physical index this query addresses (`{index}_{hash}` — exactly
    /// what the sink writes). Crate-internal: [`Client::msearch`] renders it
    /// into each NDJSON header line.
    pub(crate) fn physical_index(&self) -> String {
        format!("{}_{}", self.index, self.hash)
    }

    /// The paths of the accumulated [`filter_nested`](Self::filter_nested)
    /// projections, for post-processing a response with [`merge_inner_hits`].
    pub(crate) fn nested_paths(&self) -> Vec<&str> {
        self.nested.iter().map(NestedProjection::path).collect()
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
    pub async fn count(&self, client: &Client) -> Result<u64> {
        let body = self.count_body();
        let response = client.count_at(&self.physical_index(), &body).await?;
        let raw: RawCount = serde_json::from_value(response)?;
        tracing::Span::current().record("count", raw.count);
        tracing::debug!(count = raw.count, "count completed");
        Ok(raw.count)
    }
}

impl<T> Search<T>
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
    pub async fn send(&self, client: &Client) -> Result<SearchResponse<T>> {
        let body = self.body();
        let mut response = client.search_at(&self.physical_index(), &body).await?;
        let paths = self.nested_paths();
        if !paths.is_empty() {
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

/// Match highlighting for a [`Search`] (the `highlight` block). Name the fields
/// to highlight and tune the tags / fragments; pass it to
/// [`Search::highlight`].
#[derive(Debug, Clone, Default)]
pub struct Highlight {
    fields: Map<String, Value>,
    opts: Map<String, Value>,
}

impl Highlight {
    /// An empty highlight config — add fields with [`field`](Self::field).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Highlight `field` with the default settings.
    #[must_use]
    pub fn field(mut self, field: impl Into<String>) -> Self {
        self.fields.insert(field.into(), Value::Object(Map::new()));
        self
    }

    /// Highlight `field` with explicit per-field settings (e.g. a custom
    /// `fragment_size` / `matched_fields`).
    #[must_use]
    pub fn field_with(mut self, field: impl Into<String>, settings: Value) -> Self {
        self.fields.insert(field.into(), settings);
        self
    }

    /// Tags wrapping each highlighted snippet's start.
    #[must_use]
    pub fn pre_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.opts.insert(
            "pre_tags".to_string(),
            Value::Array(tags.into_iter().map(|t| Value::String(t.into())).collect()),
        );
        self
    }

    /// Tags wrapping each highlighted snippet's end.
    #[must_use]
    pub fn post_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.opts.insert(
            "post_tags".to_string(),
            Value::Array(tags.into_iter().map(|t| Value::String(t.into())).collect()),
        );
        self
    }

    /// Character length of each highlighted fragment.
    #[must_use]
    pub fn fragment_size(mut self, fragment_size: u32) -> Self {
        self.opts
            .insert("fragment_size".to_string(), Value::from(fragment_size));
        self
    }

    /// Maximum number of fragments returned per field.
    #[must_use]
    pub fn number_of_fragments(mut self, number_of_fragments: u32) -> Self {
        self.opts.insert(
            "number_of_fragments".to_string(),
            Value::from(number_of_fragments),
        );
        self
    }

    /// Only highlight fields that the query matched (default `true`).
    #[must_use]
    pub fn require_field_match(mut self, require: bool) -> Self {
        self.opts
            .insert("require_field_match".to_string(), Value::Bool(require));
        self
    }

    fn to_value(&self) -> Value {
        let mut body = self.opts.clone();
        body.insert("fields".to_string(), Value::Object(self.fields.clone()));
        Value::Object(body)
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

/// The `_count` response envelope (`{ "count": N, "_shards": … }`) — shared
/// with the combined-search [`count`](crate::MultiSearch::count).
#[derive(Deserialize)]
pub(crate) struct RawCount {
    pub(crate) count: u64,
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
