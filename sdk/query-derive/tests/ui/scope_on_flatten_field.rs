// A `#[serde(flatten)]` field's keys live at the enclosing level, so the field
// names no level of its own for `scope` to rename.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Contact {
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(flatten)]
    #[flusso(scope = "Reach")]
    contact: Contact,
}

fn main() {}
