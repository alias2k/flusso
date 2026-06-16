use flusso_query::FlussoDocument;

#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users")]
struct User {
    id: i32,
    bogus: String,
}

fn main() {}
