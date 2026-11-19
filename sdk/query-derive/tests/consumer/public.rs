//! A library that exports every kind of derived type, documented, under the
//! strictest lints a consumer turns on: whatever the derives add to a public
//! API must pass `missing_docs`, `unreachable_pub` and the rest too.

use std::collections::HashMap;

#[expect(
    clippy::wildcard_imports,
    reason = "brings every name the derives' output spells out in full"
)]
use flusso_query::rules::*;
#[expect(
    clippy::wildcard_imports,
    reason = "brings every name the derives' output spells out in full"
)]
use flusso_query::*;
use serde::Deserialize;

/// A user, read from the `users` index.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(index = "users")]
pub struct User {
    /// The id.
    pub id: i32,
    /// The email.
    pub email: String,
    /// The tier.
    pub tier: Tier,
    /// The orders.
    pub orders: Vec<Order>,
    /// Where bills go.
    #[serde(rename = "billingAddress")]
    pub billing_address: Address,
    /// Where parcels go.
    #[serde(rename = "shippingAddress")]
    pub shipping_address: Address,
}

/// A user's tier.
#[derive(Debug, Clone, Copy, serde::Serialize, Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
pub enum Tier {
    /// Free.
    Free,
    /// Pro.
    Pro,
    /// Enterprise.
    Enterprise,
}

/// An email address.
#[derive(Debug, Clone, serde::Serialize, Deserialize, FlussoValue)]
pub struct Email(pub String);

/// One order.
#[derive(Debug, Deserialize, FlussoFragment)]
pub struct Order {
    /// The status.
    pub status: String,
}

/// An address.
#[derive(Debug, Deserialize, FlussoFragment)]
pub struct Address {
    /// The city, beside the rest.
    #[serde(flatten)]
    pub city: City,
}

/// The city of an address.
#[derive(Debug, Deserialize, FlussoFragment)]
pub struct City {
    /// The city.
    pub city: String,
}

/// Translations of one text.
#[derive(Debug, Deserialize, FlussoMap)]
#[flusso(keyword)]
pub struct Translations(pub HashMap<String, String>);

/// A product, read from the `products` index.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(index = "products")]
pub struct Product {
    /// The SKU.
    pub sku: String,
}

/// A hit of a search over users and products.
#[derive(Debug, FlussoMultiDocument)]
pub enum Hit {
    /// A user.
    User(User),
    /// A product.
    Product(Product),
}

/// What a request brings that no document holds.
#[derive(Debug, Clone)]
pub struct Viewing {
    /// Who reads.
    pub viewer: String,
}

/// A contract, with fields decided when it is read.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
pub struct Contract {
    /// The id.
    pub id: String,
    /// Whether the reader owns it.
    #[flusso(rules = is_mine)]
    pub is_mine: bool,
    /// The title, as shown.
    #[flusso(computed = label)]
    pub label: String,
}

fn is_mine(cx: &Viewing) -> Rules<bool> {
    Rules::when(Contract::doc().owner_id().eq(cx.viewer.as_str()))
        .then(true)
        .otherwise(false)
}

/// What [`Contract::label`] is computed from.
#[derive(Debug, Deserialize)]
pub struct Titled {
    /// The stored title.
    pub title: String,
}

fn label(doc: &Titled, _: &Viewing) -> String {
    doc.title.clone()
}
