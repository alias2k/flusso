// A map type carries its declared value kind, so embedding it at a map of a
// different kind is a compile error — not merely "the field is object-ish".
use std::collections::HashMap;

use flusso_query::{FieldSpec, FlussoFragment, FlussoMap, KindTag};

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(text)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: HashMap<String, String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized {
    title: Translation,
}

// The schema says this map holds `keyword` values.
const LEVEL: &[FieldSpec] = &[FieldSpec {
    name: "title",
    kind: KindTag::Object,
    nullable: false,
    array: false,
    variants: &[],
    map_values: Some(KindTag::Keyword),
    children: &[],
}];

const _: () = Localized::__flusso_check(LEVEL);

fn main() {}
