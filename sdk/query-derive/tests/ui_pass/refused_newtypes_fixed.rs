// Each newtype `ui/fragment_newtype_reads_another_array.rs` refuses, written
// as its message says, compiles at the field it belongs to and reads
// (issue #289).
use std::collections::HashMap;

use flusso_query::{FlussoFragment, FlussoMap, FlussoRoot, FlussoValue};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Orders(Vec<Order>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
#[serde(rename_all = "snake_case")]
enum Tag {
    Urgent,
}

#[derive(serde::Deserialize, FlussoValue)]
struct Tags(Vec<Tag>);

#[derive(serde::Deserialize, FlussoValue)]
#[serde(transparent)]
struct Label(String);

/// `Codes(Vec<String>)`, `NamedCodes`, `Statuses`, `LabelList`, `OverTags`:
/// derive `FlussoValue` instead. `OverTags` needs no `Serialize`.
#[derive(serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Deserialize, FlussoValue)]
#[serde(transparent)]
struct NamedCodes {
    codes: Vec<i32>,
}

#[derive(serde::Deserialize, FlussoValue)]
struct Statuses(Vec<Tag>);

#[derive(serde::Deserialize, FlussoValue)]
struct LabelList(Vec<Label>);

#[derive(serde::Deserialize, FlussoValue)]
struct OverTags(Tags);

/// `MaybeOrders`, `MaybeBatch`: drop the `Option`. `Batches`: wrap the element.
#[derive(serde::Deserialize, FlussoFragment)]
struct MaybeOrders(Vec<Order>);

#[derive(serde::Deserialize, FlussoFragment)]
struct MaybeBatch(Orders);

#[derive(serde::Deserialize, FlussoFragment)]
struct Batches(Vec<Order>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct List {
    id: String,
    tags: Option<OverTags>,
    labels: LabelList,
    scores: Option<NamedCodes>,
    /// `Maps`, `HashMaps`, `Values`: the `Vec` itself at a `json` field.
    notes: Option<Vec<HashMap<String, serde_json::Value>>>,
}

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Note(HashMap<String, serde_json::Value>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct ListOfValues {
    id: String,
    tags: Option<Statuses>,
    labels: Codes,
    notes: Option<Vec<Note>>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    orders: MaybeOrders,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserBatch {
    id: i32,
    orders: MaybeBatch,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserBatches {
    id: i32,
    orders: Batches,
}

fn main() {
    let list = r#"{"id": "1", "tags": ["urgent"], "labels": ["a"], "scores": [1], "notes": [{"x": 1}]}"#;
    let read: List = serde_json::from_str(list).unwrap();
    assert_eq!(read.labels.0.len(), 1);
    assert_eq!(read.scores.unwrap().codes, [1]);
    assert_eq!(serde_json::from_str::<ListOfValues>(list).unwrap().labels.0, ["a"]);
    let user = r#"{"id": 1, "orders": [{"status": "paid"}]}"#;
    assert_eq!(serde_json::from_str::<User>(user).unwrap().orders.0.len(), 1);
    assert_eq!(serde_json::from_str::<UserBatch>(user).unwrap().orders.0.0.len(), 1);
    assert_eq!(serde_json::from_str::<UserBatches>(user).unwrap().orders.0.len(), 1);
    let empty = r#"{"id": 1, "orders": []}"#;
    assert!(serde_json::from_str::<User>(empty).unwrap().orders.0.is_empty());
}
