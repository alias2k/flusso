// A combined search has nowhere to take a root's context from, so a root with
// one cannot be a variant: its read-time fields would not be computed.
use flusso_query::{FlussoMultiDocument, FlussoRoot};

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
}

#[derive(FlussoMultiDocument)]
enum Anything {
    Contract(Contract),
    User(User),
}

fn main() {}
