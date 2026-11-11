// `blurb` is a `text` field with `subfields: false` while the sink keeps its
// auto subfields on: the field is mapped bare, so its handle has no subfield
// views — asking for one is a compile error, not a query on a missing field.
use flusso_query::FlussoRoot;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "stored")]
struct Listing {
    id: String,
    blurb: Option<String>,
}

fn main() {
    let _ = Listing::blurb().keyword();
    let _ = Listing::blurb().keyword_lowercase();
}
