// A hand-written `FlussoValue` sits in a fragment with its checks on once it
// implements `FlussoValueMeta` and calls `embeddable!` (issue #265).
use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue, FlussoValueMeta, KindTag, kind};

#[derive(serde::Serialize, serde::Deserialize)]
struct HandWritten(String);
impl FlussoValue<kind::Keyword> for HandWritten {}
impl FlussoValueMeta for HandWritten {
    const KINDS: &'static [KindTag] = &[KindTag::Keyword];
    const VARIANTS: &'static [&'static str] = &[];
}

#[derive(serde::Serialize, serde::Deserialize)]
struct HandWrittenCount(i64);
impl FlussoValue<kind::Long> for HandWrittenCount {}
impl FlussoValueMeta for HandWrittenCount {
    const KINDS: &'static [KindTag] = &[KindTag::Long];
    const VARIANTS: &'static [&'static str] = &[];
}

flusso_query::embeddable!(HandWritten, HandWrittenCount);

/// Declares `trial`, which the schema's `tier` does not list, so `opaque`
/// turns the variant check off.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum LooseTier {
    Free,
    Trial,
}
impl FlussoValue<kind::Keyword> for LooseTier {}
impl FlussoValueMeta for LooseTier {
    const KINDS: &'static [KindTag] = &[KindTag::Keyword];
    const VARIANTS: &'static [&'static str] = &["free", "trial"];
}
flusso_query::embeddable!(LooseTier);

#[derive(serde::Deserialize, FlussoFragment)]
struct UserParts {
    email: HandWritten,
    #[serde(rename = "orderCount")]
    order_count: HandWrittenCount,
    #[flusso(opaque)]
    tier: LooseTier,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct User {
    id: i32,
    #[serde(flatten)]
    parts: UserParts,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
struct Direct {
    id: i32,
    email: HandWritten,
    #[serde(rename = "orderCount")]
    order_count: HandWrittenCount,
}

fn main() {}
