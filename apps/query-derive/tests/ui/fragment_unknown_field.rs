// A fragment declares a field the schema doesn't have at the embedding path.
// The error surfaces where the level is handed in, naming the field.
use flusso_query::{FieldSpec, FlussoFragment, KindTag};

#[derive(serde::Deserialize, FlussoFragment)]
struct Address {
    city: String,
    town: String,
}

const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "city",
    kind: KindTag::Keyword,
    nullable: false,
    array: false,
    variants: &[],
    map_values: None,
    children: &[],
}];

const _: () = Address::__flusso_check(LEVEL);

fn main() {}
