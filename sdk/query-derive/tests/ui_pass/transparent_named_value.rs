// A `#[serde(transparent)]` struct with one named field is a value newtype,
// like its tuple form.
use flusso_query::{FlussoRoot, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(transparent)]
struct Email {
    value: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    email: Email,
}

fn main() {
    let _ = User::email().eq(Email { value: "ada@x.com".into() });
}
