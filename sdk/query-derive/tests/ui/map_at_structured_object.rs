// `profile` is a `has_one` with fields of its own: a map type is not its shape,
// on a root or in a fragment (issue #262).
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct Shape {
    id: String,
    profile: Option<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Profiled {
    profile: Option<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct FromFragment {
    id: String,
    #[serde(flatten)]
    profiled: Profiled,
}

fn main() {}
