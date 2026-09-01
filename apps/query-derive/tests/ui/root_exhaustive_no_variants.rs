use flusso_query::{FlussoRoot, FlussoValue};

// `email` is a plain keyword with no declared `variants:` — an exhaustive enum
// there could never verify its guarantee (and a schema edit dropping
// `variants:` must not silently disarm it), so it is rejected.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword, exhaustive)]
enum Email {
    Work,
    Home,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    email: Email,
}

fn main() {}
