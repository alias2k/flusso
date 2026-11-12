// A field has one key: a second `rename` is an error, not "last wins".
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(rename = "email", rename = "fullName")]
    #[serde(alias = "mail")]
    email: String,
}

fn main() {}
