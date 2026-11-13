use flusso_query::{FlussoRoot, FlussoValue};

// An array column read as one element, or as a primitive.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
#[serde(rename_all = "snake_case")]
enum Tag {
    Urgent,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct Order {
    id: String,
    tags: Option<Tag>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct Scores {
    id: String,
    scores: Option<i32>,
}

fn main() {}
