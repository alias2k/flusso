// A `Vec` at a field that holds one value is refused, on a root and in a
// fragment. The message leads with declaring a column `array: true` (issue
// #485): the element type alone compiles, then fails to read every document
// that holds a list. An aggregate (`orderCount`) takes no `array: true`, so
// the message names that case too.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct OnARoot {
    id: i32,
    email: Vec<String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Leaves {
    email: Vec<String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromLeaves {
    id: i32,
    #[serde(flatten)]
    flat: Leaves,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct CountOnARoot {
    id: i32,
    #[serde(rename = "orderCount")]
    order_count: Vec<i64>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Counted {
    #[serde(rename = "orderCount")]
    order_count: Vec<i64>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromCounted {
    id: i32,
    #[serde(flatten)]
    flat: Counted,
}

fn main() {}
