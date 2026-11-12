// A flattened map collects the keys no other field reads; it is never checked,
// so `opaque` has nothing to turn off.
use std::collections::HashMap;

use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
    #[serde(flatten)]
    #[flusso(opaque)]
    rest: HashMap<String, serde_json::Value>,
}

fn main() {}
