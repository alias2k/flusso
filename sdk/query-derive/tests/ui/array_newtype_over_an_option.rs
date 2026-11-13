use flusso_query::FlussoValue;

// The `Option` goes on the field, not inside the array newtype.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Codes(Option<Vec<String>>);

fn main() {}
