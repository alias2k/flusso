// The value kind of a `FlussoMap` type goes on the type; its fields are never
// checked one by one.
use flusso_query::FlussoMap;

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translations {
    #[flusso(opaque)]
    fallback: String,
}

fn main() {}
