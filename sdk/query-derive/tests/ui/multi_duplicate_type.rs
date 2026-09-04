use flusso_query::{FlussoRoot, FlussoMultiDocument};

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    email: String,
}

// The same payload type twice would make the `_index` dispatch ambiguous —
// the first variant would always win.
#[derive(FlussoMultiDocument)]
enum SearchItem {
    Active(User),
    Dormant(User),
}

fn main() {}
