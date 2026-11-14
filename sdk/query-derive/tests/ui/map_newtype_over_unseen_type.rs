// A `FlussoMap` newtype wraps a map the derive can see into: a `Box<String>`
// reads a string, and a plain struct names keys a map may lack (issue #294).
use flusso_query::FlussoMap;

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Boxed(Box<String>);

#[derive(serde::Deserialize)]
struct Plain {
    it: String,
}

#[derive(serde::Deserialize, FlussoMap)]
#[flusso(keyword)]
struct Wrapped(Plain);

fn main() {}
