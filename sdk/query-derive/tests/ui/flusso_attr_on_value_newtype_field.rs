// The kind tag of a `FlussoValue` newtype goes on the type; nothing on its
// field is read.
use flusso_query::FlussoValue;

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Code(#[flusso(keyword)] String);

fn main() {}
