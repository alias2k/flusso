// serde reads `mail`; the derive would check `email`. It would compile, and every
// real document would fail to deserialize the field.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(rename = "mail")]
    #[flusso(rename = "email")]
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct SplitRename {
    id: i32,
    #[serde(rename(serialize = "email", deserialize = "mail"))]
    #[flusso(rename = "email")]
    email: String,
}

fn main() {}
