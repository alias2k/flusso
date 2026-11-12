// `opaque` on a fragment field keeps what it keeps on a root: the array shape,
// the kind of a type with flusso metadata, and the rejection where it turns off
// nothing.
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(serde::Deserialize)]
struct Plain {}

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Code(String);

#[derive(serde::Deserialize, FlussoFragment)]
struct VecAtAnObject {
    #[flusso(opaque)]
    #[serde(rename = "billingAddress")]
    billing: Vec<Plain>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OneAtANestedArray {
    #[flusso(opaque)]
    orders: Plain,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct KeywordAtALong {
    #[flusso(opaque)]
    #[serde(rename = "orderCount")]
    order_count: Code,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct NothingToTurnOff {
    #[flusso(opaque)]
    email: Code,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct AtAMap {
    #[flusso(opaque)]
    title: Plain,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Root0 {
    id: i32,
    #[serde(flatten)]
    part: VecAtAnObject,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Root1 {
    id: i32,
    #[serde(flatten)]
    part: OneAtANestedArray,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Root2 {
    id: i32,
    #[serde(flatten)]
    part: KeywordAtALong,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Root3 {
    id: i32,
    #[serde(flatten)]
    part: NothingToTurnOff,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct Root4 {
    sku: String,
    #[serde(flatten)]
    part: AtAMap,
}

fn main() {}
