// Every serde attribute the derive does not read is stepped over, so the ones
// it does read (`rename_all`, `rename`, `flatten`) are found wherever they sit.
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(bound = "", rename_all = "camelCase")]
struct Address {
    city: String,
    postal_code: Option<String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
#[serde(deny_unknown_fields, bound = "", rename_all = "camelCase")]
struct User {
    id: i32,
    full_name: Option<String>,
    billing_address: Address,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
#[serde(rename_all(serialize = "snake_case", deserialize = "camelCase"))]
struct UserRead {
    id: i32,
    full_name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword, exhaustive)]
enum Tier {
    Free,
    #[serde(alias = "p", rename = "pro")]
    Professional,
    #[serde(rename(deserialize = "enterprise"))]
    Business,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Tiered {
    id: i32,
    tier: Tier,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Contact {
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Flattened {
    id: i32,
    #[serde(bound = "", flatten)]
    contact: Contact,
}

fn main() {}
