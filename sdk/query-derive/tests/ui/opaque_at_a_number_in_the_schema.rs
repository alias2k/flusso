// `orderCount` is a `long`: a newtype there is checked by its `FlussoValue`
// bound, which `opaque` does not turn off, so `opaque` does nothing.
use flusso_query::{FlussoRoot, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Count(i64);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(opaque)]
    #[serde(rename = "orderCount")]
    order_count: Count,
}

fn main() {}
