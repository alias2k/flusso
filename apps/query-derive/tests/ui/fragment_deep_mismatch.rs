// A mismatch two levels down: the fragment embeds a fragment. The primary span
// is the embedding, with a note chain through each `__flusso_check`.
use flusso_query::{FieldSpec, FlussoFragment, KindTag};

#[derive(serde::Deserialize, FlussoFragment)]
struct Address {
    geo: Geo,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Geo {
    lat: f64,
}

const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "geo",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: None,
    // `lat` is a keyword here, which an `f64` cannot hold.
    children: &[FieldSpec {
        name: "lat",
        kind: KindTag::Keyword,
        nullable: false,
        array: false,
        variants: &[],
        map_values: None,
        children: &[],
    }],
}];

const _: () = Address::__flusso_check(LEVEL);

fn main() {}
