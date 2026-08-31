// The reported trap, post-#108: a `keyword`-tagged wrapper on a `text` map.
// The schema's `values:` kind is const-readable, so the error names the exact
// tag to put on the type instead of a generic "matching value kind".
use std::collections::HashMap;

use flusso_query::{FieldSpec, FlussoFragment, FlussoMap, KindTag};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: HashMap<String, String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct PaymentMethod {
    name: Translation,
}

// The schema: `name` at the embedding path is a `text` map.
const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "name",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: Some(KindTag::Text),
    children: &[],
}];

const _: () = PaymentMethod::__flusso_check(LEVEL);

fn main() {}
