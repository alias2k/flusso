// An `exhaustive` enum promises to cover the schema's declared variants, so a
// field that declares none is refused wherever it sits: a `json` field, or as
// a map's values (issue #294).
use std::collections::HashMap;

use flusso_query::{FlussoRoot, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword, exhaustive)]
enum Code {
    A,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct Shape {
    id: String,
    settings: Code,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Product {
    sku: String,
    codes: Option<HashMap<String, Code>>,
}

fn main() {}
