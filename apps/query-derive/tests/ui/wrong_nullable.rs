use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    // `email` is `required` (non-null) in the schema — `Option` is wrong.
    email: Option<String>,
}

fn main() {}
