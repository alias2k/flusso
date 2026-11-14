// A std type with no flusso kind, in a fragment, at a field the derive models:
// the message names the type and says what to declare instead.
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(serde::Deserialize, FlussoFragment)]
struct Brand {
    k: String,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Boxed {
    bt: Option<Box<Brand>>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Chars {
    kwa: Vec<char>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Unsigned {
    kwa: Vec<u64>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct A {
    #[serde(flatten)]
    f: Boxed,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct B {
    #[serde(flatten)]
    f: Chars,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary")]
struct C {
    #[serde(flatten)]
    f: Unsigned,
}

fn main() {}
