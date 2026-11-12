// A flattened field's keys live at the enclosing level: it has no key of its
// own for `rename` to change.
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
    #[flusso(rename = "contact")]
    contact: Contact,
}

fn main() {}
