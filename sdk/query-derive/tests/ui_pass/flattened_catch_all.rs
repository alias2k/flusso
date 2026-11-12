use std::collections::{BTreeMap, HashMap};

use flusso_query::{FlussoFragment, FlussoRoot};

// serde's catch-all for leftover keys — a flattened map or `serde_json::Value` —
// has nothing to check, so it compiles bare on a fragment and on a root, with no
// `opaque` or `skip`.

#[derive(serde::Deserialize, FlussoFragment)]
struct OrderHashMap {
    status: String,
    #[serde(flatten)]
    rest: HashMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OrderBTreeMap {
    status: String,
    #[serde(flatten)]
    rest: BTreeMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OrderValue {
    status: String,
    #[serde(flatten)]
    rest: serde_json::Value,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OrderSkipped {
    status: String,
    #[serde(flatten)]
    #[flusso(skip)]
    rest: HashMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: Vec<OrderHashMap>,
    #[serde(flatten)]
    rest: HashMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserBTree {
    id: i32,
    orders: Vec<OrderBTreeMap>,
    #[serde(flatten)]
    rest: BTreeMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserValue {
    id: i32,
    orders: Vec<OrderValue>,
    #[serde(flatten)]
    rest: serde_json::Value,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserSkipped {
    id: i32,
    orders: Vec<OrderSkipped>,
}

fn main() {}
