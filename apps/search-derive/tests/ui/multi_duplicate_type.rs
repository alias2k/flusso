use flusso_search::{FlussoDocument, FlussoMultiDocument};

#[derive(serde::Deserialize, FlussoDocument)]
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
