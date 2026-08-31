// A string-tagged wrapper on a *number* map: no tag can name the per-type
// numeric kinds, so the error steers to a plain `HashMap`/`BTreeMap` instead
// of suggesting a tag that doesn't exist.
use std::collections::HashMap;

use flusso_query::{FieldSpec, FlussoFragment, FlussoMap, KindTag};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Prices {
    #[serde(flatten)]
    amounts: HashMap<String, String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Listing {
    prices: Prices,
}

// The schema: `prices` at the embedding path is a `double` map.
const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "prices",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: Some(KindTag::Double),
    children: &[],
}];

const _: () = Listing::__flusso_check(LEVEL);

fn main() {}
