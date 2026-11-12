// `email` is a keyword leaf: its handle lives in the root's own scope, so
// there is no generated namespace for `scope` to rename.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(scope = "Mail")]
    email: String,
}

fn main() {}
