// A fragment newtype over `HashMap<String, serde_json::Value>` goes only where
// that map goes: a `json` field. A root refuses it at a `map`; a fragment
// agrees (issue #294).
use std::collections::HashMap;

use flusso_query::{FieldSpec, FlussoFragment, KindTag};

#[derive(serde::Deserialize, FlussoFragment)]
struct Anything(HashMap<String, serde_json::Value>);

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized {
    title: Anything,
}

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
