// `title` is a `map`: a wrapper there is checked by its `FlussoMap` bound,
// which `opaque` does not turn off, so `opaque` does nothing.
use std::collections::HashMap;

use flusso_query::{FlussoMap, FlussoRoot};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translations(HashMap<String, String>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    #[flusso(opaque)]
    title: Translations,
}

fn main() {}
