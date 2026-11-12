// `opaque` on a fragment field turns off only the shape check: the field must
// still exist at the path, with the schema's nullability, exactly as on a root.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize)]
struct Plain {}

#[derive(serde::Deserialize, FlussoFragment)]
struct Address {
    city: String,
    #[flusso(opaque)]
    region: Plain,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    #[flusso(opaque)]
    status: Option<Plain>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(rename = "billingAddress")]
    billing: Address,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserOrders {
    id: i32,
    orders: Vec<Order>,
}

fn main() {}
