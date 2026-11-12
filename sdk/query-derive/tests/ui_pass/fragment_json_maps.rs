// A `HashMap`/`BTreeMap` of `serde_json::Value` at an opaque `json` field
// compiles in a fragment as it does on a root (issue #262).
use std::collections::{BTreeMap, HashMap};

use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct BTreeBlobs {
    blob: Option<BTreeMap<String, serde_json::Value>>,
    settings: BTreeMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct HashBlobs {
    blob: Option<HashMap<String, serde_json::Value>>,
    settings: HashMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct Shape {
    id: String,
    blob: Option<BTreeMap<String, serde_json::Value>>,
    settings: HashMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct FromBTree {
    id: String,
    #[serde(flatten)]
    blobs: BTreeBlobs,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "shapes")]
struct FromHash {
    id: String,
    #[serde(flatten)]
    blobs: HashBlobs,
}

fn main() {}
