//! `flusso-query` — a typed query client for a flusso-maintained search index.
//!
//! Targets OpenSearch and Elasticsearch 7.x, which share the `_search` query DSL
//! this crate emits; any future backend divergence is handled on the [`Client`],
//! not by separate crates.
//!
//! This is the **runtime** layer described in [`CLIENT.md`](../../../CLIENT.md):
//! the transport, the field-handle / [`Query`] / [`Search`] builder, and the
//! typed [`SearchResponse`]. It is generic over the caller's document type `T`.
//!
//! # What this first cut covers
//!
//! - [`Client`] transport over OpenSearch (`connect`, `basic_auth`, search,
//!   msearch, get).
//! - Field handles ([`Keyword`], [`Text`], [`Bool`], [`Number`], [`Date`],
//!   [`Nested`], [`Binary`], [`Json`]) with operators that build a [`Query`].
//! - [`Query`] composition (`and` / `or` / `not`) and the [`Search`] bool-clause
//!   builder (`query` / `filter` / `must_not` / `should`, plus `sort` / `from` /
//!   `size` / `raw`). A [`Search`] is a plain client-free value — build it
//!   anywhere, store and reuse it; a [`Client`] appears only at the terminals:
//!   [`Search::send`] (a typed page), [`Search::ids`] (a page of bare document
//!   ids, `_source: false`), or [`Search::count`] (just the number of matches,
//!   via `_count`).
//! - Several searches in one round-trip: [`Client::msearch`] (a tuple of
//!   `&Search<T>`, mixed document types, one typed response per slot) and
//!   [`Client::msearch_all`] (a slice of one type).
//! - Combined (blended) search: [`FlussoMultiDocument`] — a caller-owned enum
//!   with one variant per document type — and its [`MultiSearch`] builder. One
//!   query across all the union's indexes, one relevance-ranked result list,
//!   each hit decoded into the variant matching its physical `_index`.
//! - Typed [`SearchResponse`] / [`Hit`].
//!
//! Also covered: optional filters (`Option<Q>` is a [`Query`]); object/to-one-join
//! handles ([`Object`]); shaping returned nested arrays ([`Search::filter_nested`]
//! with [`Nested::matching`], via `inner_hits`); and scope-tagged queries —
//! [`Query`]`<S>` carries the scope `S` it was built in ([`Root`] for the document
//! root and flattened objects, the element type for a `nested` array), so a nested
//! query must be lifted through [`Nested::any`]/[`Nested::all`] before it can join a
//! root query; the compiler enforces it.
//!
//! # Not yet built (see CLIENT.md for the endgame)
//!
//! - The `#[derive(FlussoDocument)]` and `#[derive(FlussoMultiDocument)]`
//!   proc-macros live in `flusso-query-derive` (the `derive` feature). Without
//!   them, document structs + handles and union impls are written by hand —
//!   exactly the calls this crate exposes (see the integration tests).
//! - `filter_nested`'s `keep_source()` opt-out (it always replaces the array in
//!   `source` today) and a typed `hit.nested(handle)` accessor.
//!
//! # Example (hand-written until the derive lands)
//!
//! ```no_run
//! use flusso_query::{Client, Keyword, Number, Nested, kind};
//!
//! #[derive(serde::Deserialize)]
//! struct User {
//!     email: String,
//!     #[serde(rename = "orderCount")]
//!     order_count: i64,
//! }
//!
//! impl User {
//!     fn email() -> Keyword { Keyword::at("email") }
//!     fn order_count() -> Number<kind::Long> { Number::at("orderCount") }
//!     fn query() -> flusso_query::Search<User> {
//!         flusso_query::Search::new("users", "xxxxxx")
//!     }
//! }
//!
//! # async fn run() -> flusso_query::Result<()> {
//! // A query is a plain value — no client involved while building it.
//! let busy = User::query()
//!     .filter(User::email().eq("ada@example.com"))
//!     .filter(User::order_count().gte(5))
//!     .size(20);
//!
//! // The client appears once, when it's time to run.
//! let client = Client::connect("https://localhost:9200")?;
//! let page = busy.send(&client).await?;
//! println!("{} matches", page.total);
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod handles;
mod msearch;
mod multi;
mod query;
mod search;

#[cfg(test)]
mod tests;

pub use client::Client;
pub use error::{Error, Result};
pub use handles::{
    Binary, Bool, BoostMode, BoostingQuery, CombinedFieldsQuery, ConstantScoreQuery, Date, DateMap,
    DisMaxQuery, Distance, DistanceFeatureQuery, DistanceType, DistanceUnit, EqQuery, FlussoMap,
    FlussoValue, FunctionScoreQuery, Fuzziness, FuzzyQuery, Geo, GeoDistanceQuery, GeoPoint,
    IdsQuery, Json, Keyword, KeywordMap, MapSearch, MatchQuery, MinimumShouldMatch,
    MoreLikeThisQuery, MultiMatchQuery, MultiMatchType, Nested, NestedProjection, NestedQuery,
    NestedScoreMode, NoSubfields, Number, NumberMap, NumericType, Object, Operator, PrefixQuery,
    QueryStringQuery, RangeQuery, RangeRelation, RankFeatureQuery, RegexpQuery, ScoreMode,
    ScriptQuery, ScriptScoreQuery, ScriptSortType, SimpleQueryStringQuery, Sort, SortMode,
    SortOrder, TermQuery, TermsQuery, Text, TextMap, ValidationMethod, WildcardQuery,
    WithSubfields, ZeroTermsQuery, boosting, combined_fields, constant_score, dis_max,
    distance_feature, function_score, ids, kind, more_like_this, multi_match, query_string,
    rank_feature, script, script_score, simple_query_string,
};
pub use msearch::MsearchBundle;
pub use multi::{FlussoMultiDocument, MultiSearch};
pub use query::{AsQuery, Query, Root};
pub use search::{FlussoDocument, Highlight, Hit, Search, SearchResponse};

/// `#[derive(FlussoDocument)]` — generates the typed query surface for a
/// hand-written document struct (its field handles) and implements the
/// [`FlussoDocument`](trait@FlussoDocument) trait (`INDEX`/`SCHEMA_HASH` +
/// `search`/`get`). See [`CLIENT.md`](../../../CLIENT.md). Enabled by the
/// `derive` feature.
#[cfg(feature = "derive")]
pub use flusso_query_derive::FlussoDocument;

/// `#[derive(FlussoValue)]` — implements [`trait@FlussoValue`] for an enum or newtype
/// wrapper, so it may stand in for a field of the chosen kind (`#[flusso(keyword)]`
/// — the default — `#[flusso(text)]`, `#[flusso(number)]`, or `#[flusso(date)]`)
/// in a [`FlussoDocument`] struct. Enabled by the `derive` feature.
#[cfg(feature = "derive")]
pub use flusso_query_derive::FlussoValue;

/// `#[derive(FlussoMap)]` — implements [`trait@FlussoMap`] for a newtype wrapper
/// over a `map` field, so it may stand in for a `map` of the chosen value kind
/// (`#[flusso(keyword)]` — the default — `#[flusso(text)]`, `#[flusso(number)]`,
/// or `#[flusso(date)]`) in a [`FlussoDocument`] struct. A bare
/// `HashMap<String, V>` needs no derive. Enabled by the `derive` feature.
#[cfg(feature = "derive")]
pub use flusso_query_derive::FlussoMap;

/// `#[derive(FlussoMultiDocument)]` — implements [`trait@FlussoMultiDocument`]
/// for an enum with one single-field variant per document type (the
/// combined-search union): the generated impl lists every variant's index and
/// decodes each hit into the variant matching its physical `_index`. Enabled
/// by the `derive` feature.
#[cfg(feature = "derive")]
pub use flusso_query_derive::FlussoMultiDocument;

// The multi-document derive's generated code deserializes variant payloads;
// routing it through this re-export keeps it on this crate's `serde_json`.
// Hidden: not API.
#[doc(hidden)]
pub use serde_json as __serde_json;

/// `rust_decimal::Decimal`, re-exported for `decimal` fields. Enabled by the
/// `decimal` feature.
#[cfg(feature = "decimal")]
pub use rust_decimal::Decimal;

/// `chrono`, re-exported for `date`/`timestamp` fields. Enabled by the `chrono`
/// feature.
#[cfg(feature = "chrono")]
pub use chrono;

/// `uuid`, re-exported for `keyword` id / foreign-key fields. With this feature
/// a `uuid::Uuid` field needs no `#[flusso(skip)]` and `id().eq(some_uuid)`
/// works without `.to_string()`. Enabled by the `uuid` feature.
#[cfg(feature = "uuid")]
pub use uuid;
