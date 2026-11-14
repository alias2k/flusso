// A tagged `FlussoValue` newtype stands for one value, so it cannot wrap a map
// type, which reads an object (issue #294).
use std::collections::HashMap;

use flusso_query::{FlussoMap, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Codes(HashMap<String, String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Code(Codes);

fn main() {}
