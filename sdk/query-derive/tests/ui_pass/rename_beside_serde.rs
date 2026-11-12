use flusso_query::FlussoRoot;

// `#[flusso(rename)]` names the key when serde may read one of several: it
// names one of the field's `alias`es.

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Aliased {
    id: i32,
    #[serde(rename = "mail", alias = "email")]
    #[flusso(rename = "email")]
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct AliasedName {
    id: i32,
    #[serde(alias = "email")]
    #[flusso(rename = "email")]
    mail: String,
}

fn main() {}
