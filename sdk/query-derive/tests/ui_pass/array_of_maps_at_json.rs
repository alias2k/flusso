// An array of maps is opaque JSON, which a `json` field takes, on a root and in
// a fragment (issue #265).
use std::collections::{BTreeMap, HashMap};

use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct Shape {
    id: String,
    blob: Option<Vec<HashMap<String, serde_json::Value>>>,
    settings: Vec<BTreeMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Blobs {
    blob: Option<Vec<HashMap<String, serde_json::Value>>>,
    settings: Vec<BTreeMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct FromFragment {
    id: String,
    #[serde(flatten)]
    blobs: Blobs,
}

fn main() {}
