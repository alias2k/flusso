// A flattened group is checked against the ENCLOSING level. `orders` is a nested
// array at the root of `users`, so a group claiming a field from inside it fails
// here — proving the level handed down is the parent's, not a child's.
use flusso_query::{FlussoRoot, FlussoFragment};

#[derive(serde::Deserialize, FlussoFragment)]
struct Wrong {
    // `status` lives at `users.orders`, not at the root.
    status: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(flatten)]
    wrong: Wrong,
}

fn main() {}
