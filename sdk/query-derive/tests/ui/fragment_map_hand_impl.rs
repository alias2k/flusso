// A hand-written `impl FlussoMap<K>` satisfies a *root*'s deferred map bound,
// but a fragment validates through `FlussoValueMeta` const data that only the
// derives emit — so the type must use `#[derive(FlussoMap)]` (any struct shape
// works). The error's note must point there.
use std::collections::BTreeMap;

use flusso_query::{FlussoFragment, FlussoMap, kind};

// Multilingual text stored as a flat JSONB map: language keys plus `fallback`.
#[derive(serde::Deserialize)]
struct Translation {
    fallback: Option<String>,
    #[serde(flatten)]
    langs: BTreeMap<String, String>,
}

// The marker alone — what the derive would emit *without* the metadata a
// fragment's check reads.
impl FlussoMap<kind::Text> for Translation {}

#[derive(serde::Deserialize, FlussoFragment)]
struct Localized {
    name: Translation,
}

fn main() {}
