use std::collections::HashMap;

use flusso_query::FlussoMap;

// A map has no variant set to cover — `exhaustive` belongs on a FlussoValue
// enum only.
#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text, exhaustive)]
struct Translations(HashMap<String, String>);

fn main() {}
