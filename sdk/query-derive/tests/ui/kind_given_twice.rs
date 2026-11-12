// A value type has one kind: a conflicting or repeated tag is not "last wins".
use flusso_query::{FlussoMap, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
#[flusso(text)]
enum Tier {
    Free,
}

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text, text)]
struct Translations(std::collections::HashMap<String, String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword, exhaustive, exhaustive)]
enum Status {
    Paid,
}

fn main() {}
