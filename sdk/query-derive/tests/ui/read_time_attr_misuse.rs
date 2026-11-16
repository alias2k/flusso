// `rules` and `computed` beside an attribute that has nothing left to act on,
// both at once, or given twice.
use flusso_query::FlussoRoot;
use flusso_query::rules::Rules;

struct Viewing;

fn flag(_: &Viewing) -> Rules<bool> {
    unimplemented!()
}

fn text(_: &serde_json::Value, _: &Viewing) -> bool {
    unimplemented!()
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Both {
    #[flusso(rules = flag, computed = text)]
    flag: bool,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Skipped {
    #[flusso(skip, rules = flag)]
    flag: bool,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Renamed {
    #[flusso(computed = text, rename = "state")]
    #[serde(alias = "state")]
    flag: bool,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Twice {
    #[flusso(rules = flag, rules = flag)]
    flag: bool,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing, context = Viewing)]
struct TwoContexts {
    id: String,
}

fn main() {}
