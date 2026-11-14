// A fragment newtype over a `HashMap` is checked like the `HashMap` written at
// the field: the schema's map holds `keyword` values, and `bool` is not one
// (issue #294).
use std::collections::HashMap;

use flusso_query::{FieldSpec, FlussoFragment, KindTag};

#[derive(serde::Deserialize, FlussoFragment)]
struct Flags(HashMap<String, bool>);

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized {
    title: Flags,
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
