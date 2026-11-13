use flusso_query::{FlussoFragment, FlussoValue};

// A `FlussoValue` newtype over a `Vec` holds leaf values: not arrays, not
// sub-documents (a value's check never walks into one), not objects.

#[derive(serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Deserialize, FlussoValue)]
struct NestedCodes(Vec<Codes>);

#[derive(serde::Deserialize, FlussoFragment)]
struct Bogus {
    nope: i64,
    sku: bool,
}

#[derive(serde::Deserialize, FlussoValue)]
struct Bogi(Vec<Bogus>);

#[derive(serde::Deserialize, FlussoFragment)]
#[serde(transparent)]
struct BogusList(Vec<Bogus>);

#[derive(serde::Deserialize, FlussoValue)]
struct BogusLists(Vec<BogusList>);

fn main() {}
