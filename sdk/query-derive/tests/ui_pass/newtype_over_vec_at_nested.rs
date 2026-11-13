// A `#[serde(transparent)]` (or tuple) fragment newtype over a `Vec` reads a
// JSON array, so it goes at a `nested` field like the `Vec` itself, on a root
// and in a fragment, and its element is checked there (issue #265).
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Orders(Vec<Order>);

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct OrderList {
    orders: Vec<Order>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Tuple(Vec<Order>);

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Wrapped(Orders);

#[derive(serde::Deserialize)]
struct Plain {}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: Orders,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserNamed {
    id: i32,
    orders: OrderList,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserTuple {
    id: i32,
    orders: Tuple,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserWrapped {
    id: i32,
    orders: Wrapped,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Parts {
    orders: Orders,
    #[serde(rename = "billingAddress")]
    #[flusso(opaque)]
    billing_address: Plain,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromFragment {
    id: i32,
    #[serde(flatten)]
    parts: Parts,
}

fn main() {}
