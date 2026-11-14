// A `FlussoFragment` newtype that reads an array is an array of sub-documents,
// for a `nested` field. Any other array it could read is refused where the
// newtype is declared (issue #289): it would fit no field but a `json` one,
// and a single-valued field would read it as one value and fail on read.
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoMap, FlussoValue};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Orders(Vec<Order>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
enum Status {
    Paid,
}

#[derive(serde::Deserialize, FlussoValue)]
struct Tags(Vec<Status>);

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Labels(HashMap<String, String>);

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Label(String);

/// An array of values is a `FlussoValue` newtype's.
#[derive(serde::Deserialize, FlussoFragment)]
struct Codes(Vec<String>);

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct NamedCodes {
    codes: Vec<i64>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Statuses(Vec<Status>);

#[derive(serde::Deserialize, FlussoFragment)]
struct LabelList(Vec<Label>);

#[derive(serde::Deserialize, FlussoFragment)]
struct OverTags(Tags);

/// An array of maps, or of `serde_json::Value`, is opaque JSON.
#[derive(serde::Deserialize, FlussoFragment)]
struct Maps(Vec<Labels>);

#[derive(serde::Deserialize, FlussoFragment)]
struct HashMaps(Vec<HashMap<String, String>>);

#[derive(serde::Deserialize, FlussoFragment)]
struct Values(Vec<serde_json::Value>);

/// A nullable array is never a `nested` array.
#[derive(serde::Deserialize, FlussoFragment)]
struct MaybeOrders(Option<Vec<Order>>);

#[derive(serde::Deserialize, FlussoFragment)]
struct MaybeBatch(Option<Orders>);

/// An array of arrays.
#[derive(serde::Deserialize, FlussoFragment)]
struct Batches(Vec<Orders>);

fn main() {}
