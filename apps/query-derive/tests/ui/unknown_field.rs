use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    bogus: String,
}

fn main() {}
