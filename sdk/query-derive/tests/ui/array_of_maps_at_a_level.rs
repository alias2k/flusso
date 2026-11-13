// An array of maps is opaque JSON: a `nested` array wants a struct per element,
// an `object` with fields a struct, and a leaf is not that shape, on a root and
// in a fragment (issue #265).
use std::collections::{BTreeMap, HashMap};

use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: Vec<HashMap<String, serde_json::Value>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserTree {
    id: i32,
    orders: Vec<BTreeMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserMap {
    id: i32,
    orders: HashMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserObject {
    id: i32,
    #[serde(rename = "billingAddress")]
    billing_address: Vec<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserLeaf {
    id: i32,
    email: Vec<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Orders {
    orders: Vec<HashMap<String, serde_json::Value>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromFragment {
    id: i32,
    #[serde(flatten)]
    orders: Orders,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OrderMap {
    orders: BTreeMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromMapFragment {
    id: i32,
    #[serde(flatten)]
    orders: OrderMap,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Billing {
    #[serde(rename = "billingAddress")]
    billing_address: Vec<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromObjectFragment {
    id: i32,
    #[serde(flatten)]
    billing: Billing,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Email {
    email: Vec<HashMap<String, String>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromLeafFragment {
    id: i32,
    #[serde(flatten)]
    email: Email,
}

fn main() {}
