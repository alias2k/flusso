// An array newtype reads a whole JSON array, so a field that holds one value
// refuses it, on a root and in a fragment, `opaque` or not (issue #289): a
// plain leaf, an `object`, a `map`, a `geo` point.
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(serde::Deserialize, FlussoFragment)]
struct Address {
    city: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct Addresses(Vec<Address>);

#[derive(serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct OpaqueObjectOnARoot {
    id: i32,
    #[serde(rename = "billingAddress")]
    #[flusso(opaque)]
    billing_address: Addresses,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct OpaqueObject {
    #[serde(rename = "billingAddress")]
    #[flusso(opaque)]
    billing_address: Addresses,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromOpaqueObject {
    id: i32,
    #[serde(flatten)]
    flat: OpaqueObject,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Leaves {
    email: Codes,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromLeaves {
    id: i32,
    #[serde(flatten)]
    flat: Leaves,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct AtAGeoPoint {
    location: Option<Codes>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct FromAtAGeoPoint {
    id: i32,
    #[serde(flatten)]
    flat: AtAGeoPoint,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct AtAMap {
    codes: Option<Addresses>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "products")]
struct FromAtAMap {
    sku: String,
    #[serde(flatten)]
    flat: AtAMap,
}

fn main() {}
