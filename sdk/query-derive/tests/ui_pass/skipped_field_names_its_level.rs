// Serde reads no key for a `#[serde(skip)]` field, so `#[flusso(rename)]`
// alone may name the level its `scope` renames.
use flusso_query::{FlussoFragment, FlussoRoot, Sortable};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    total: f64,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(skip)]
    #[flusso(skip, rename = "orders", scope = "Buys")]
    cached: (),
}

fn main() {
    let _ = flusso_user_query::Buys::total().desc();
}
