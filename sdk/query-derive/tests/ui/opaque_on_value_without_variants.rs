// At a root `keyword`/`text` field `opaque` turns off the variant checks, so
// on a value type with no variants it turns off nothing.
use flusso_query::{FlussoRoot, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Email(String);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum Code {
    Free,
}

// `email` declares no variants: only an `exhaustive` type is checked there.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[flusso(opaque)]
    email: Email,
}

// `tier` declares variants, but `Email` has none to check.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Tiered {
    id: i32,
    #[flusso(opaque)]
    tier: Email,
}

// A non-`exhaustive` enum where the schema declares no variants.
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Coded {
    id: i32,
    #[flusso(opaque)]
    email: Code,
}

fn main() {}
