// A fragment field and a fragment newtype read map keys as strings, as a root
// does (issue #294).
use std::collections::HashMap;

use flusso_query::FlussoFragment;

#[derive(serde::Deserialize, FlussoFragment)]
struct Codes(HashMap<i64, String>);

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized {
    title: HashMap<i64, String>,
}

fn main() {}
