// A map may hold both a field's key and its alias, and serde then fails with
// `duplicate field` (issue #294).
use flusso_query::FlussoMap;

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Title {
    #[serde(alias = "italian")]
    it: Option<String>,
}

fn main() {}
