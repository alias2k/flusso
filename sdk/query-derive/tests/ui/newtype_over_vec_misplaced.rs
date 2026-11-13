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

/// One more newtype does not hide the array of arrays.
#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Batches(Vec<Orders>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserBatches {
    id: i32,
    orders: Batches,
}

/// A `Vec` of a value type reads no sub-documents.
#[derive(serde::Serialize, serde::Deserialize, flusso_query::FlussoValue)]
#[flusso(keyword)]
enum Status {
    Paid,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Statuses(Vec<Status>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserStatuses {
    id: i32,
    orders: Statuses,
}

/// A wrapper around the `Vec` is not looked through.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserBoxed {
    id: i32,
    orders: Box<Vec<Order>>,
}

fn main() {}
