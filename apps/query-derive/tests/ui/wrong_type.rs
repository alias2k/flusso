use flusso_query::FlussoDocument;

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
struct User {
    id: i32,
    // `email` is a `keyword` (String) in the schema, not an integer.
    email: i32,
}

fn main() {}
