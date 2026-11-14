// serde flattens only a struct or a map; anything else compiles and fails on
// every read, so a flattened array, array newtype or single value is refused,
// on a root and in a fragment (issue #289).
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Orders(Vec<Order>);

#[derive(serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Code(String);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct ArrayNewtypeOnARoot {
    id: i32,
    #[serde(flatten)]
    codes: Codes,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct ValueOnARoot {
    id: i32,
    #[serde(flatten)]
    code: Code,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct VecOnARoot {
    id: i32,
    #[serde(flatten)]
    orders: Vec<Order>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct StringOnARoot {
    id: i32,
    #[serde(flatten)]
    email: Option<String>,
}

/// In a fragment that is a `nested` array's element.
#[derive(serde::Deserialize, FlussoFragment)]
struct Element {
    #[serde(flatten)]
    orders: Orders,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct ArrayNewtypeInAnElement {
    id: i32,
    orders: Vec<Element>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Flat {
    #[serde(flatten)]
    code: Code,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct ValueInAFlattenedFragment {
    id: i32,
    #[serde(flatten)]
    flat: Flat,
}

/// `opaque` turns off the walk into the type, not what serde can flatten.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct OpaqueOnARoot {
    id: i32,
    #[serde(flatten)]
    #[flusso(opaque)]
    codes: Codes,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OpaqueFlat {
    #[serde(flatten)]
    #[flusso(opaque)]
    orders: Orders,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct OpaqueInAFragment {
    id: i32,
    #[serde(flatten)]
    flat: OpaqueFlat,
}

fn main() {}
