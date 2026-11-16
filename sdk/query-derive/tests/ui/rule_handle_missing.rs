// A rule reads only fields Rust decides exactly as the index does. Analyzed
// text, a 32-bit float, a field with a mapping option of its own (a
// normalizer), a map and a `nested` array have no rule handle.
use flusso_query::FlussoRoot;

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
}

fn main() {
    let doc = Contract::doc();
    let _ = doc.title();
    let _ = doc.rating();
    let _ = doc.code();
    let _ = doc.labels();
    let _ = doc.invoices();
}
