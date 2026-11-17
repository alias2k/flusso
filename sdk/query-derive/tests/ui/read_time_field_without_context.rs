// A read-time field is decided from request-time values, so the root must
// name their type.
use flusso_query::FlussoRoot;
use flusso_query::rules::Rules;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts")]
struct Contract {
    id: String,
    #[flusso(rules = is_signed)]
    is_signed: bool,
    #[flusso(computed = shout)]
    shout: String,
}

fn is_signed(_: &()) -> Rules<bool> {
    unimplemented!()
}

fn shout(_: &serde_json::Value, _: &()) -> String {
    unimplemented!()
}

fn main() {}
