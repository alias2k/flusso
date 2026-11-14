// A tagged `FlussoValue` newtype stands for any string of its tag, so a type
// that reads only its own kind's values (a decimal, a date) is refused, with
// both fixes: wrap a `String`, or drop the tag to carry that kind (issue #294).
use flusso_query::{Decimal, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Amount(Decimal);

fn main() {}
