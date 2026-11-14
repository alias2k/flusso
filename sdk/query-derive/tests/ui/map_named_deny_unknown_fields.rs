// A map holds any key, so `deny_unknown_fields` on a named-field `FlussoMap`
// type fails on the first key it does not name (issue #294).
use flusso_query::FlussoMap;

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
#[serde(deny_unknown_fields)]
struct Title {
    it: Option<String>,
}

fn main() {}
