// Only the root binds to an index. A fragment describes a shape with no
// location, so it has no `query`/`get` entry point — it cannot start a search.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: Vec<Order>,
}

fn main() {
    // The root can.
    let _ = User::query();
    // The fragment cannot: no index, so no entry point.
    let _ = Order::query();
}
