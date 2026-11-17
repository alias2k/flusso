// A root with a context is queried and fetched with one: there is no way to
// read it without deciding its read-time fields.
use flusso_query::FlussoRoot;

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
}

fn main() {
    let _ = Contract::query();
    let _ = <Contract as FlussoRoot>::query();
}
