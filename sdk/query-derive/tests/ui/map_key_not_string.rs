// A map's keys are document keys, so they read as strings: `HashMap<i64, _>`
// fails on the first key that is not a number (issue #294).
use std::collections::HashMap;

use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    codes: Option<HashMap<i64, String>>,
}

fn main() {}
