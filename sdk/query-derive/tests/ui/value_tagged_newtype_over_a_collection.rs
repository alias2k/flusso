// A tagged `FlussoValue` newtype stands for one value of its tag, so it cannot
// wrap a map, a collection or a `Box` of one: the first document fails with
// `expected a map` (issue #294).
use std::collections::HashMap;

use flusso_query::FlussoValue;

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Codes(HashMap<String, String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Boxed(Box<HashMap<String, String>>);

fn main() {}
