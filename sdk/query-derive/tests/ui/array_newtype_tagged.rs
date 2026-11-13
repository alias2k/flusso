use flusso_query::FlussoValue;

// The kind of a newtype over a `Vec` comes from its element.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Codes(Vec<String>);

fn main() {}
