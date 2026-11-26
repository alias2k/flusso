// A `Vec` at a field that holds one value is refused, on a root and in a
// fragment. Where the schema can declare the field `array: true` the message
// leads with that (issue #485): the element type alone compiles, then fails
// to read every document that holds a list.
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

fn main() {}
