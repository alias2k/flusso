use flusso_query::FlussoDocument;

// `orders` is a `nested` element: it implements `FlussoDocument` (for its PATH)
// but not `FlussoIndex`, so it has no `query()` — only the root can start a search.
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "orders")]
struct UserOrder {
    status: String,
}

fn main() {
    let _ = UserOrder::query();
}
