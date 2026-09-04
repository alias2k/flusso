// `scope` becomes a type name, so an invalid one is rejected where it is
// written rather than producing code that won't parse.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(scope = "not a type")]
    orders: Vec<Order>,
}

fn main() {}
