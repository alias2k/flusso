//! `flusso-client` — a typed query client for a flusso-maintained OpenSearch index.
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
//! # Not yet built (see CLIENT.md for the endgame)
//!
//! - The `#[derive(FlussoDocument)]` proc-macro. Today the document struct and
//!   its field handles are written **by hand**; the derive will generate exactly
//!   the calls this crate exposes. See the integration tests for the shape.
//! - `filter_nested` / inner-hits, scope-tagged `Query<S>` child-merge & lift,
//!   and the `Option<Q>` optional-filter primitive.
//!
//! # Example (hand-written until the derive lands)
//!
//! ```no_run
//! use flusso_client::{Client, Keyword, Number, Nested};
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
//!     fn search(client: &Client) -> flusso_client::Search<'_, User> {
//!         flusso_client::Search::new(client, "users")
//!     }
//! }
//!
//! # async fn run() -> flusso_client::Result<()> {
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
    Binary, Bool, Date, Geo, GeoPoint, Json, Keyword, Nested, Number, Sort, SortOrder, Text,
    multi_match,
};
pub use query::{AsQuery, Query};
pub use search::{Hit, Search, SearchResponse};
