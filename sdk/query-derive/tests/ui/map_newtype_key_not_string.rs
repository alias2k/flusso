// The keys of a `FlussoMap` newtype's map are document keys too (issue #294).
use std::collections::HashMap;

use flusso_query::FlussoMap;

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Codes(HashMap<i64, String>);

fn main() {}
