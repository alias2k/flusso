use flusso_query::FlussoDocument;

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
struct User {
    id: i32,
    // `email` is `required` (non-null) in the schema — `Option` is wrong.
    email: Option<String>,
}

fn main() {}
