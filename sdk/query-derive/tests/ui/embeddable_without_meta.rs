// `embeddable!` pairs with a hand-written `FlussoValueMeta` (issue #265).
#[derive(serde::Serialize, serde::Deserialize)]
struct Code(String);

flusso_query::embeddable!(Code);

fn main() {}
