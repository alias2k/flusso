// serde reads the field's own name (or its `#[serde(rename)]`), so a
// `#[flusso(rename)]` with no `alias` beside it is redundant or names a key
// serde never reads (issue #262).
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Unseen {
    id: i32,
    #[flusso(rename = "email")]
    mail: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct UnseenInFragment {
    #[flusso(rename = "email")]
    mail: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Repeated {
    id: i32,
    #[serde(rename = "fullName")]
    #[flusso(rename = "fullName")]
    full_name: Option<String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct RepeatedSplit {
    id: i32,
    #[serde(rename(serialize = "mail", deserialize = "email"))]
    #[flusso(rename = "email")]
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
#[serde(rename_all = "camelCase")]
struct RepeatedRenameAll {
    id: i32,
    #[flusso(rename = "fullName")]
    full_name: Option<String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct NotAnAlias {
    id: i32,
    #[serde(alias = "mail")]
    #[flusso(rename = "email")]
    address: String,
}

fn main() {}
