//! `flusso-search` — a typed query client for a flusso-maintained search index.
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
//! - [`Client`] transport over OpenSearch (`connect`, `basic_auth`, search, get).
//! - Field handles ([`Keyword`], [`Text`], [`Bool`], [`Number`], [`Date`],
//!   [`Nested`], [`Binary`], [`Json`]) with operators that build a [`Query`].
//! - [`Query`] composition (`and` / `or` / `not`) and the [`Search`] bool-clause
//!   builder (`query` / `filter` / `must_not` / `should`, plus `sort` / `from` /
//!   `size` / `raw`).
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
//! - The `#[derive(FlussoDocument)]` proc-macro lives in `flusso-search-derive`
//!   (the `derive` feature). Without it, document structs + handles are written
//!   by hand — exactly the calls this crate exposes (see the integration tests).
//! - `filter_nested`'s `keep_source()` opt-out (it always replaces the array in
//!   `source` today) and a typed `hit.nested(handle)` accessor.
//!
//! # Example (hand-written until the derive lands)
//!
//! ```no_run
//! use flusso_search::{Client, Keyword, Number, Nested};
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
//!     fn order_count() -> Number<i64> { Number::at("orderCount") }
//!     fn search(client: &Client) -> flusso_search::Search<'_, User> {
//!         flusso_search::Search::new(client, "users", "xxxxxx")
//!     }
//! }
//!
//! # async fn run() -> flusso_search::Result<()> {
//! let client = Client::connect("https://localhost:9200")?;
//! let page = User::search(&client)
//!     .filter(User::email().eq("ada@example.com"))
//!     .filter(User::order_count().gte(5))
//!     .size(20)
//!     .send()
//!     .await?;
//! println!("{} matches", page.total);
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod handles;
mod query;
mod search;

#[cfg(test)]
mod tests;

pub use client::Client;
pub use error::{Error, Result};
pub use handles::{
    Binary, Bool, Date, FlussoValue, Geo, GeoPoint, Json, Keyword, Nested, NestedProjection,
    Number, Object, Sort, SortOrder, Text, kind, multi_match,
};
pub use query::{AsQuery, Query, Root};
pub use search::{FlussoDocument, Hit, Search, SearchResponse};

/// `#[derive(FlussoDocument)]` — generates the typed query surface for a
/// hand-written document struct (its field handles) and implements the
/// [`FlussoDocument`](trait@FlussoDocument) trait (`INDEX`/`SCHEMA_HASH` +
/// `search`/`get`). See [`CLIENT.md`](../../../CLIENT.md). Enabled by the
/// `derive` feature.
#[cfg(feature = "derive")]
pub use flusso_search_derive::FlussoDocument;

/// `#[derive(FlussoValue)]` — implements [`trait@FlussoValue`] for an enum or newtype
/// wrapper, so it may stand in for a field of the chosen kind (`#[flusso(keyword)]`
/// — the default — `#[flusso(text)]`, `#[flusso(number)]`, or `#[flusso(date)]`)
/// in a [`FlussoDocument`] struct. Enabled by the `derive` feature.
#[cfg(feature = "derive")]
pub use flusso_search_derive::FlussoValue;

/// `rust_decimal::Decimal`, re-exported for `decimal` fields. Enabled by the
/// `decimal` feature.
#[cfg(feature = "decimal")]
pub use rust_decimal::Decimal;

/// `chrono`, re-exported for `date`/`timestamp` fields. Enabled by the `chrono`
/// feature.
#[cfg(feature = "chrono")]
pub use chrono;
