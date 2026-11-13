// serde passes over a skipped field of a `#[serde(transparent)]` value type, so
// the derive never reads a `#[flusso(…)]` there (issue #265).
use flusso_query::FlussoValue;

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(transparent)]
struct Email {
    value: String,
    #[serde(skip)]
    #[flusso(keyword)]
    verified: bool,
}

fn main() {}
