//! Shapes shared by more than one index.
//!
//! A [`FlussoFragment`] names no index and no path, so one declaration covers
//! every place the shape appears. `LineItem` below is embedded at
//! `users.orders.items` *and* at `orders.items` — two different indexes — and
//! each root validates it against the mapping at its own path. If the two
//! schemas ever drift apart, one of those checks fails the build.
//!
//! Before fragments this had to be two identical structs, one per location,
//! with nothing keeping them in agreement.

use flusso_query::{Decimal, FlussoFragment, FlussoValue};
use serde::{Deserialize, Serialize};

/// One line of an order. Same shape in both indexes.
#[derive(Debug, Serialize, Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LineItem {
    pub(crate) product_id: i32,
    pub(crate) quantity: i32,
    pub(crate) unit_price: Decimal,
}

/// The order lifecycle, as both schemas declare it.
///
/// `FlussoValue` reports the variants as the document spells them, so a root
/// checks them against the schema's declared `variants:` — a variant the schema
/// does not list is a compile error, since it could never match a document.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
pub(crate) enum OrderStatus {
    Pending,
    Paid,
    Shipped,
    Delivered,
    Cancelled,
}
