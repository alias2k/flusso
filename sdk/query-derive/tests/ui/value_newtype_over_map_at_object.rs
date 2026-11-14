// A `FlussoValue` newtype over a map type stands where the map type does: at a
// `map` or `json` field, never at an `object` with fields of its own
// (issue #294).
use std::collections::HashMap;

use flusso_query::{FlussoMap, FlussoRoot, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Codes(HashMap<String, String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Wrapped(Codes);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    email: String,
    #[serde(rename = "billingAddress")]
    billing_address: Wrapped,
}

fn main() {}
