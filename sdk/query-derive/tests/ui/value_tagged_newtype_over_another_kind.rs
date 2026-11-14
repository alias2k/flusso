// A tagged `FlussoValue` newtype over a type with flusso metadata must wrap a
// value of its tag: `i64` reads a number, never a `keyword` (issue #294).
use flusso_query::FlussoValue;

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Count(i64);

fn main() {}
