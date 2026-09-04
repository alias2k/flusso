// Embedding is checked by default: the type must be a fragment (or a value
// type). A plain un-derived struct is rejected — use `#[flusso(opaque)]` to
// opt out deliberately.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize)]
struct Plain {
    status: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: Vec<Plain>,
}

fn main() {}
