// A `FlussoValue` newtype over a single value is a query value, which takes
// `Serialize`: without it, the field that uses it says so (issue #289; a
// newtype over an array newtype is no query value and needs none).
use flusso_query::{FlussoRoot, FlussoValue};

#[derive(serde::Deserialize, FlussoValue)]
struct Email(String);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    email: Email,
}

fn main() {}
