use flusso_query::FlussoRoot;

// `#[flusso(rename)]` names the key when the derive cannot tell which serde
// reads (an `alias` beside the `rename`), or repeats the one it can read.

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
struct SplitRename {
    id: i32,
    #[serde(rename(serialize = "mail", deserialize = "email"))]
    #[flusso(rename = "email")]
    email: String,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Agreeing {
    id: i32,
    #[serde(rename = "fullName")]
    #[flusso(rename = "fullName")]
    full_name: Option<String>,
}

fn main() {}
