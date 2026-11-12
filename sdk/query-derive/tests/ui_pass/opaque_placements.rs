use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue, kind};

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

/// A hand-written `FlussoValue` carries no `FlussoValueMeta`, so at a root
/// `keyword` field `opaque` is what spares it the variant checks' read.
#[derive(serde::Serialize, serde::Deserialize)]
struct HandWritten(String);
impl FlussoValue<kind::Keyword> for HandWritten {}

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    #[flusso(opaque)]
    status: LooseStatus,
    #[flusso(opaque)]
    shipping: Option<Plain>,
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

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct HandWrittenEmail {
    id: i32,
    #[flusso(opaque)]
    email: HandWritten,
}

/// A type of the user's own named like a built-in leaf: taken for its own where
/// no built-in leaf could sit, so `opaque` works there.
#[derive(serde::Deserialize)]
struct Date {}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct NamedLikeALeaf {
    id: i32,
    #[flusso(opaque)]
    #[serde(rename = "billingAddress")]
    billing_address: Date,
    #[serde(flatten)]
    #[flusso(opaque)]
    rest: Date,
}

/// A hand-written `FlussoValue` at a `long` field: a fragment reads a field
/// type's metadata to check its kind, and `opaque` spares it that read.
#[derive(serde::Serialize, serde::Deserialize)]
struct HandWrittenCount(i64);
impl FlussoValue<kind::Long> for HandWrittenCount {}

/// In a fragment, `opaque` turns off at each placement what it turns off on a
/// root, so a root's placements work here too.
#[derive(serde::Deserialize, FlussoFragment)]
struct UserParts {
    #[flusso(opaque)]
    tier: LooseTier,
    #[flusso(opaque)]
    email: HandWritten,
    #[flusso(opaque)]
    #[serde(rename = "billingAddress")]
    billing_address: Plain,
    #[flusso(opaque)]
    orders: Vec<Plain>,
    #[flusso(opaque)]
    #[serde(rename = "orderCount")]
    order_count: HandWrittenCount,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserFromParts {
    id: i32,
    #[serde(flatten)]
    parts: UserParts,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct ListingParts {
    #[flusso(opaque)]
    snapshot: Option<Plain>,
    #[flusso(opaque)]
    variants: Vec<Plain>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "stored")]
struct ListingFromParts {
    id: String,
    #[serde(flatten)]
    parts: ListingParts,
}

fn main() {}
