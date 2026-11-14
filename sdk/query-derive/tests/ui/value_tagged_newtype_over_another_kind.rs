// A tagged `FlussoValue` newtype reads one string, and a Rust number reads
// none, so `i64` is refused (issue #294).
use flusso_query::FlussoValue;

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Count(i64);

fn main() {}
