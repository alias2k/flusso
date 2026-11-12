use flusso_query::{FlussoFragment, FlussoRoot};

// `Vec<serde_json::Value>` holds a `nested` array unchecked, on a root and in a
// fragment, with no `opaque`.

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: Vec<serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Orders {
    orders: Vec<serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserWithOrders {
    id: i32,
    #[serde(flatten)]
    orders: Orders,
}

fn main() {}
