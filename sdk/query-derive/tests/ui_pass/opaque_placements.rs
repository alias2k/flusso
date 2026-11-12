use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

// Every placement where `#[flusso(opaque)]` turns a check off, so none of them
// may be rejected as an attribute that does nothing.

/// Derives nothing: reachable at an `object`, `nested` or `json` field only
/// because the field is `opaque`.
#[derive(serde::Deserialize)]
struct Plain {}

/// Declares `refunded`, which the schema's `status` does not list: the variant
/// check would reject it at a `keyword` field, and `opaque` turns that off.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum LooseStatus {
    Paid,
    Refunded,
}

/// Declares `trial`, which the schema's `tier` does not list.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum LooseTier {
    Free,
    Pro,
    Enterprise,
    Trial,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    #[flusso(opaque)]
    status: LooseStatus,
    #[flusso(opaque)]
    shipping: Plain,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(opaque)]
    tier: LooseTier,
    #[flusso(opaque)]
    #[serde(rename = "billingAddress")]
    billing_address: Plain,
    #[flusso(opaque)]
    #[serde(rename = "shippingAddress")]
    shipping_address: Plain,
    #[flusso(opaque)]
    orders: Vec<serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserOrders {
    id: i32,
    orders: Vec<Order>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "stored")]
struct Listing {
    id: String,
    #[flusso(opaque)]
    snapshot: Option<Plain>,
    #[flusso(opaque)]
    variants: Vec<Plain>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Flattened {
    id: i32,
    #[serde(flatten)]
    #[flusso(opaque)]
    rest: Plain,
}

fn main() {}
