// The Rust enum is marked `exhaustive`, but the schema field it lands on
// declares no `variants:` — there is nothing to cover, so the guarantee is
// rejected rather than silently accepted.
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
    variants: &[],
    map_values: None,
    children: &[],
}];

const _: () = Order::__flusso_check(LEVEL);

fn main() {}
