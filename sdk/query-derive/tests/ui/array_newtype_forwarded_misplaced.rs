use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue};

// An untagged newtype over an array newtype reads the array too: refused where
// a single value or an element goes, on a root, in a fragment and flattened.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Outer(Codes);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct AtAPlainField {
    id: Outer,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct PlainLine {
    sku: Outer,
    codes: Vec<String>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct InAFragment {
    id: String,
    lines: Vec<PlainLine>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct ArrayOfArrays {
    sku: String,
    codes: Vec<Outer>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct ElementInAFragment {
    id: String,
    lines: Vec<ArrayOfArrays>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Flat {
    id: Outer,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct Flattened {
    #[serde(flatten)]
    flat: Flat,
}

fn main() {}
