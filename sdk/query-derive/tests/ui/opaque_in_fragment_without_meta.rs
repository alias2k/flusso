// A fragment reads a leaf type's kind from its flusso metadata, as a root
// checks its bound, so `opaque` no longer lets a type without it through at a
// leaf (issue #265).
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue, kind};

#[derive(serde::Serialize, serde::Deserialize)]
struct HandWritten(String);
impl FlussoValue<kind::Keyword> for HandWritten {}

#[derive(serde::Serialize, serde::Deserialize)]
struct HandWrittenCount(i64);
impl FlussoValue<kind::Long> for HandWrittenCount {}

#[derive(serde::Deserialize, FlussoFragment)]
struct EmailPart {
    #[flusso(opaque)]
    email: HandWritten,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(flatten)]
    email: EmailPart,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct CountPart {
    #[flusso(opaque)]
    #[serde(rename = "orderCount")]
    order_count: HandWrittenCount,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct UserCount {
    id: i32,
    #[serde(flatten)]
    count: CountPart,
}

fn main() {}
