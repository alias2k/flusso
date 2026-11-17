// A fragment has no context: read-time fields go on the root.
use flusso_query::FlussoFragment;
use flusso_query::rules::Rules;

fn flag(_: &()) -> Rules<bool> {
    unimplemented!()
}

fn text(_: &serde_json::Value, _: &()) -> String {
    unimplemented!()
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Billing {
    country: String,
    #[flusso(rules = flag)]
    domestic: bool,
    #[flusso(computed = text)]
    label: String,
}

fn main() {}
