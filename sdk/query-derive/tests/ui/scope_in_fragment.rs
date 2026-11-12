// A fragment generates no query surface — only the root does — so a `scope`
// on any of its fields would rename nothing, even at a nested level.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Shipping {
    carrier: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    #[flusso(scope = "State")]
    status: String,
    #[flusso(scope = "Delivery")]
    shipping: Option<Shipping>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: Vec<Order>,
}

fn main() {}
