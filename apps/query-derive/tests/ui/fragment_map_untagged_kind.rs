// `#[derive(FlussoMap)]` defaults to `keyword` (same default as `FlussoValue`),
// so a translations wrapper that forgets `#[flusso(text)]` carries
// MAP_VALUES = [Keyword] and fails against the `text` map it was written for —
// the easy mistake when switching from a hand-written `impl FlussoMap<Text>`.
use std::collections::HashMap;

use flusso_query::{FieldSpec, FlussoFragment, FlussoMap, KindTag};

#[derive(serde::Deserialize, FlussoMap)]
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
