// A `FlussoValue` enum reads its variants' spelling from serde; a `#[flusso]`
// on a variant is never read.
use flusso_query::FlussoValue;

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum Tier {
    Free,
    #[flusso(rename = "pro")]
    Professional,
}

fn main() {}
