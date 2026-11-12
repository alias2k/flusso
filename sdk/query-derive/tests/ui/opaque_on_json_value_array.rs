// The error names the type as written: a `Vec` of `serde_json::Value`.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(opaque)]
    orders: Vec<serde_json::Value>,
}

fn main() {}
