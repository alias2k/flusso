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

/// A `Vec` of a value type reads no sub-documents.
#[derive(serde::Serialize, serde::Deserialize, flusso_query::FlussoValue)]
#[flusso(keyword)]
enum Status {
    Paid,
}

/// One more newtype does not hide the array of arrays.
#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Wrapped(Orders);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserVecOfWrapped {
    id: i32,
    orders: Vec<Wrapped>,
}

/// A value type is no sub-document, wrapped or not.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserVecOfValues {
    id: i32,
    orders: Vec<Status>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserVecOfStrings {
    id: i32,
    orders: Vec<String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct WrappedStatus(Status);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserVecOfWrappedStatus {
    id: i32,
    orders: Vec<WrappedStatus>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct VecOfWrappedStatus {
    orders: Vec<WrappedStatus>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromVecOfWrappedStatus {
    id: i32,
    #[serde(flatten)]
    orders: VecOfWrappedStatus,
}

/// A newtype fragment over a primitive is a value too.
#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Label(String);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserVecOfLabels {
    id: i32,
    orders: Vec<Label>,
}

/// `opaque` in a fragment keeps the check a root keeps.
#[derive(serde::Deserialize, FlussoFragment)]
struct OpaqueVecOfValues {
    #[flusso(opaque)]
    orders: Vec<Status>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromOpaqueVecOfValues {
    id: i32,
    #[serde(flatten)]
    orders: OpaqueVecOfValues,
}

fn main() {}
