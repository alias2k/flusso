use flusso_query::{FlussoFragment, FlussoValue};

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

// A kind tag makes a single value, which an array newtype is not.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
struct Tagged(Codes);

#[derive(serde::Serialize, serde::Deserialize, FlussoFragment)]
struct Line {
    sku: String,
}

// A value's check walks into nothing, so it cannot stand for a sub-document.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct OverLine(Line);

fn main() {}
