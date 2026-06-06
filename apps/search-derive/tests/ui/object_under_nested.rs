use flusso_search::FlussoDocument;

// `orders.shipping` is an `object` (one_to_one) inside the `orders` nested array.
// In the "objects-direct" scope model an object is untagged (`Root`), which can't
// express a scope that lives *inside* a nested element — so the derive rejects it.
#[derive(serde::Deserialize, FlussoDocument)]
#[flusso(index = "users", path = "orders.shipping")]
struct Shipping {
    carrier: String,
}

fn main() {}
