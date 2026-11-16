// A rule's condition is built from the rule handles behind `doc()`: an
// ordinary query can ask things Rust cannot decide on a document, so it is
// not a `Condition`.
use flusso_query::FlussoRoot;
use flusso_query::rules::Rules;

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
}

fn rules() -> Rules<bool> {
    Rules::when(Contract::owner_id().eq("u1"))
        .then(true)
        .otherwise(false)
}

fn main() {}
