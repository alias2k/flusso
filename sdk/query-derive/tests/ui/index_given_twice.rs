// A root binds one index: a second `index` is an error, not "last wins".
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
#[flusso(index = "users")]
struct User {
    id: i32,
}

fn main() {}
