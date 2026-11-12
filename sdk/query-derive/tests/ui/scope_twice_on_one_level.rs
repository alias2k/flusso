// Two fields naming the same level each carry a `scope`: only one name could
// win, so the second is rejected instead of silently replacing the first.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    total: f64,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(scope = "Purchases")]
    orders: Vec<Order>,
    #[serde(skip)]
    #[flusso(skip, rename = "orders", scope = "Buys")]
    cached: (),
}

fn main() {}
