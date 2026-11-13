// A newtype fragment over a `Vec` reads an array, so it goes where the `Vec`
// would; a type that reads no array does not go at a `nested` field (issue #265).
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Address {
    city: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Addresses(Vec<Address>);

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

/// `Option<Vec<…>>` reads `null` too, so it is no array newtype.
#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct MaybeOrders(Option<Vec<Order>>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(rename = "billingAddress")]
    billing_address: Addresses,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserOrder {
    id: i32,
    orders: Order,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserMaybe {
    id: i32,
    orders: MaybeOrders,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Billing {
    #[serde(rename = "billingAddress")]
    billing_address: Addresses,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromFragment {
    id: i32,
    #[serde(flatten)]
    billing: Billing,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OneOrder {
    orders: Order,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromOrderFragment {
    id: i32,
    #[serde(flatten)]
    orders: OneOrder,
}

/// `Orders` already reads the array, so `Vec<Orders>` is an array of arrays.
#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Orders(Vec<Order>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserArrays {
    id: i32,
    orders: Vec<Orders>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct ManyOrders {
    orders: Vec<Orders>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromArraysFragment {
    id: i32,
    #[serde(flatten)]
    orders: ManyOrders,
}

fn main() {}
