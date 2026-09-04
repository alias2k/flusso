// The Rust enum is marked `exhaustive` but covers only part of the schema's
// declared set. (Unmarked, a subset is legal — see tests/fragment.rs.)
use flusso_query::{FieldSpec, FlussoFragment, FlussoValue, KindTag};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword, exhaustive)]
enum OrderStatus {
    Pending,
    Shipped,
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
