// The Rust enum declares a variant the schema never lists, so it can never
// match a document. (A *subset* is legal and compiles — see tests/fragment.rs.)
use flusso_query::{FieldSpec, FlussoFragment, FlussoValue, KindTag};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum OrderStatus {
    Pending,
    Refunded,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Order {
    status: OrderStatus,
}

const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "status",
    kind: KindTag::Keyword,
    nullable: false,
    array: false,
    variants: &["pending", "shipped", "delivered"],
    map_values: None,
    children: &[],
}];

const _: () = Order::__flusso_check(LEVEL);

fn main() {}
