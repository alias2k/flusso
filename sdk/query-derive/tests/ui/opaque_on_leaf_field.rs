// `email` is a `String`: a built-in leaf has no sub-shape for `opaque` to opt
// out of.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(opaque)]
    email: String,
}

fn main() {}
