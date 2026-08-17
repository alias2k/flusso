// A map wrapper carries a value kind, so it can only stand in for a `map`
// field (or opaque JSON) — `billingAddress` is a structured object with named
// fields, and the root must reject the wrapper there just as a fragment would.
use std::collections::HashMap;

use flusso_query::{FlussoMap, FlussoRoot};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translations(HashMap<String, String>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    email: String,
    #[flusso(rename = "billingAddress")]
    billing_address: Translations,
}

fn main() {}
