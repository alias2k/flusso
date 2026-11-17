// A rule field generates `<name>(&cx)`, which cannot be an entry point's name.
use flusso_query::FlussoRoot;
use flusso_query::rules::Rules;

struct Viewing;

fn flag(_: &Viewing) -> Rules<bool> {
    unimplemented!()
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
    #[flusso(rules = flag)]
    query: bool,
}

fn main() {}
